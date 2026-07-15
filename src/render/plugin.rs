use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::storage::ShaderBuffer;
use bevy::sprite_render::Material2dPlugin;

use super::material::BlackHoleMaterial;

/// Lower bound for `render_scale`. Used to clamp the offscreen target resolution
/// in both spawn and resize paths. The UI slider floor mirrors this literal.
const MIN_RENDER_SCALE: f32 = 0.25;

/// Marks the full-screen quad so the resize system can find and rescale it
/// when the window is resized (the mesh is built once at startup).
#[derive(Component)]
struct FullscreenQuad;

/// The offscreen Image the black-hole shader renders into (sub-resolution).
#[derive(Component)]
pub struct OffscreenTarget(pub Handle<Image>);

/// The camera that renders the black-hole quad into the offscreen Image.
#[derive(Component)]
pub struct OffscreenCamera;

/// The camera that draws the upscaled offscreen Image to the window.
#[derive(Component)]
pub struct UpscaleCamera;

/// The quad that displays the upscaled image.
#[derive(Component)]
struct UpscaleQuad;

/// Marker for any camera that must be nudged each frame (Bevy 0.19 #24448
/// workaround). All render cameras carry this.
#[derive(Component)]
pub struct Nudgable;

/// Stores the fraction of the offscreen resolution that this quad's target
/// fills. Used by resize_offscreen to rescale each quad independently.
/// (1.0, 1.0) = full offscreen res; (0.5, 0.5) = half; etc.
#[derive(Component)]
pub struct QuadScaleFactor(pub f32, pub f32);

/// Marks the composite quad (renders to the window, not an offscreen Image).
/// resize_offscreen rescales it against the window, not the offscreen res.
#[derive(Component)]
pub struct CompositeQuad;

/// The half-res Image the bright-pass writes into (bloom stage [2]).
#[derive(Component)]
pub struct BloomTarget0(pub Handle<Image>);

/// Camera + quad markers for the bright-pass (for rebuild queries).
#[derive(Component)]
pub struct BrightPassCamera;
#[derive(Component)]
pub struct BrightPassQuad;

/// Camera + quad markers for the blur passes (for rebuild queries).
#[derive(Component)]
pub struct BlurCamera;
#[derive(Component)]
pub struct BlurQuad;

/// Bloom pyramid textures bloom_1, bloom_2 (bloom_0 is BloomTarget0).
#[derive(Component)]
pub struct BloomTarget1(pub Handle<Image>);
#[derive(Component)]
pub struct BloomTarget2(pub Handle<Image>);

/// The final up-sampled bloom texture read by the composite pass.
#[derive(Component)]
pub struct BloomFinalTarget(pub Handle<Image>);

/// Tracks the bloom quality currently applied to the render pipeline.
/// When it differs from `params.bloom_quality`, a rebuild is triggered.
#[derive(Resource)]
pub struct AppliedBloomQuality(pub crate::params::BloomQuality);

/// Disables bevy_egui's auto-context creation so we can explicitly assign
/// `PrimaryEguiContext` to the composite (window) camera. Without this,
/// bevy_egui assigns to the first spawned camera — the offscreen camera,
/// which renders to an Rgba16Float Image that egui's Rgba8UnormSrgb pipeline
/// can't render into.
fn disable_egui_auto_context(mut settings: ResMut<bevy_egui::EguiGlobalSettings>) {
    settings.auto_create_primary_context = false;
}

pub struct BlackHolePlugin;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::camera::WantsPointer>()
            .init_resource::<crate::params::BlackHoleParams>()
            .add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_plugins(Material2dPlugin::<crate::render::material::CompositeMaterial>::default())
            .add_plugins(Material2dPlugin::<crate::render::material::BrightPassMaterial>::default())
            .add_plugins(Material2dPlugin::<crate::render::material::BlurMaterial>::default())
            .add_plugins(bevy_egui::EguiPlugin::default())
            // The bloom pipeline spawns 7 Camera2d entities (offscreen + brightpass
            // + 4 blur + composite). bevy_egui's auto-context assigns PrimaryEguiContext
            // to the FIRST Added<Camera> — the offscreen camera, which renders to an
            // Rgba16Float Image. egui's pipeline expects the window's Rgba8UnormSrgb
            // surface → format mismatch crash. Disable auto-assignment and explicitly
            // tag the composite (window) camera with PrimaryEguiContext (see spawn).
            // Runs in PreStartup, before setup_primary_egui_context_system.
            .add_systems(bevy::prelude::PreStartup, disable_egui_auto_context)
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(Startup, crate::scene::planets::spawn_default_planet)
            .add_systems(
                Update,
                (
                    crate::camera::orbit_controller,
                    mirror_params,
                    resize_offscreen,
                    nudge_camera,
                ),
            )
            .add_systems(Update, crate::scene::planets::upload_planets)
            .add_systems(Update, rebuild_bloom)
            // bevy_egui 0.41 requires UI systems to run inside the egui context
            // pass (fonts/ctx are initialized there); placing them in Update panics.
            .add_systems(bevy_egui::EguiPrimaryContextPass, crate::ui::ui_system);
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_fullscreen_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    mut composite_materials: ResMut<Assets<crate::render::material::CompositeMaterial>>,
    mut bp_materials: ResMut<Assets<crate::render::material::BrightPassMaterial>>,
    mut blur_materials: ResMut<Assets<crate::render::material::BlurMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window>,
    params: Res<crate::params::BlackHoleParams>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    let scale = params.render_scale.clamp(MIN_RENDER_SCALE, 1.0);
    let w = ((win.width() * scale) as u32).max(1);
    let h = ((win.height() * scale) as u32).max(1);

    // Offscreen target at sub-resolution. new_target_texture sets RENDER_ATTACHMENT.
    let offscreen = images.add(Image::new_target_texture(
        w,
        h,
        TextureFormat::Rgba16Float,
        None,
    ));
    commands.spawn(OffscreenTarget(offscreen.clone()));

    // --- Black-hole quad (renders into the offscreen Image) ---
    // Camera2d's default projection is ScalingMode::WindowSize (1 world unit =
    // 1 pixel, view centered at origin spanning [-w/2,w/2]×[-h/2,h/2]). A unit
    // quad (2×2) scaled by (w/2, h/2) fills the offscreen Image.
    let half_w = w as f32 / 2.0;
    let half_h = h as f32 / 2.0;
    // CRITICAL: the planets storage binding (Handle<ShaderBuffer>) must point
    // at a REAL buffer asset, not Handle::default(). A default handle makes
    // AsBindGroup return RetryNextUpdate every frame, which silently skips the
    // quad's draw — the screen shows only the camera clear color. Pre-fill a
    // MAX_PLANETS-sized buffer of zeroed SphereData; upload_planets updates it.
    let planets_buffer = buffers.add(ShaderBuffer::from(vec![
        super::material::SphereData::default();
        super::material::MAX_PLANETS
    ]));
    let material = BlackHoleMaterial {
        planets: planets_buffer,
        ..Default::default()
    };
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(material)),
        Transform::default().with_scale(Vec3::new(half_w, half_h, 1.0)),
        FullscreenQuad,
        QuadScaleFactor(1.0, 1.0),
        RenderLayers::layer(0),
    ));
    // Offscreen camera: order -20 so it renders before the upscale camera.
    commands.spawn((
        Camera2d,
        Camera {
            order: -20,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.1, 0.1, 0.1)),
            ..default()
        },
        RenderTarget::Image(offscreen.clone().into()),
        Msaa::Off,
        OffscreenCamera,
        Nudgable,
        RenderLayers::layer(0),
    ));

    let levels = params.bloom_quality.levels();
    let mut bloom_final_handle: Option<Handle<Image>> = None;

    if levels > 0 {
        // NOTE: when bloom is enabled the full 3-level pyramid (High) always
        // runs here. Partial pyramid — Low = brightpass-only, Medium = 1 down +
        // 1 up — is a future enhancement; for now this is a simple Off vs On
        // gate. spawn_bloom_pipeline always emits the full pyramid when called.
        bloom_final_handle = spawn_bloom_pipeline(
            &mut commands,
            &mut images,
            &mut bp_materials,
            &mut blur_materials,
            &mut meshes,
            &offscreen,
            w,
            h,
        );
    }

    // Track the quality tier currently realized in the render graph. Must be
    // set here (not via init_resource) so it matches the tiered params default
    // (Off on web, High on desktop) rather than the enum's #[default].
    commands.insert_resource(AppliedBloomQuality(params.bloom_quality));

    // --- Composite quad (draws HDR scene + bloom to the window, ACES tone-mapped) ---
    // When bloom is off (BloomQuality::Off), bloom_final_handle is None and we
    // fall back to a 1x1 black texture: the composite samples black, yielding a
    // scene-only ACES composite (no bloom contribution).
    let bloom_handle = bloom_final_handle.unwrap_or_else(|| {
        // 1x1 black fallback — composite samples black, effectively scene-only ACES.
        // 8 zero bytes = one Rgba16Float pixel (16 bits/channel × 4).
        images.add(Image::new_fill(
            bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            &[0, 0, 0, 0, 0, 0, 0, 0],
            TextureFormat::Rgba16Float,
            bevy::asset::RenderAssetUsages::default(),
        ))
    });
    let composite_mat = composite_materials.add(crate::render::material::CompositeMaterial {
        uniform: crate::render::material::CompositeUniform {
            bloom_strength: 0.8, exposure: 1.0, _pad0: 0.0, _pad1: 0.0,
        },
        scene: offscreen.clone(),
        bloom: bloom_handle,
    });
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(composite_mat),
        Transform::default().with_scale(Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0)),
        UpscaleQuad,
        CompositeQuad,
        RenderLayers::layer(1),
    ));
    // The composite camera renders to the window (Rgba8UnormSrgb surface).
    // PrimaryEguiContext tells bevy_egui to render the UI onto THIS camera,
    // not the offscreen/bloom cameras (which render to Rgba16Float Images).
    commands.spawn((Camera2d, Msaa::Off, UpscaleCamera, Nudgable, bevy_egui::PrimaryEguiContext, RenderLayers::layer(1)));
}

/// Spawns the bloom pipeline (brightpass + blur pyramid + bloom_final).
/// Called at startup and when bloom_quality changes at runtime.
///
/// Pragmatic scope: this always emits the full 3-level pyramid (High) when
/// called. Partial pyramids — Low (brightpass-only) and Medium (1 down + 1 up)
/// — are a future enhancement; the caller simply does not invoke this for
/// `BloomQuality::Off`. The `levels`-driven branching is intentionally absent.
///
/// Returns the `bloom_final` handle (the half-res texture the composite pass
/// samples). Caller must fall back to a 1x1 black texture when it returns None.
#[allow(clippy::too_many_arguments)]
fn spawn_bloom_pipeline(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    bp_materials: &mut Assets<crate::render::material::BrightPassMaterial>,
    blur_materials: &mut Assets<crate::render::material::BlurMaterial>,
    meshes: &mut Assets<Mesh>,
    offscreen: &Handle<Image>,
    w: u32,
    h: u32,
) -> Option<Handle<Image>> {
    // --- Bright-pass (bloom stage [2]): half-res float target ---
    let bw = ((w as f32 * 0.5) as u32).max(1);
    let bh = ((h as f32 * 0.5) as u32).max(1);
    let bloom0 = images.add(Image::new_target_texture(
        bw, bh, TextureFormat::Rgba16Float, None,
    ));
    commands.spawn(BloomTarget0(bloom0.clone()));
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(bp_materials.add(crate::render::material::BrightPassMaterial {
            uniform: crate::render::material::BrightPassUniform {
                threshold: 1.0, _pad0: 0.0, _pad1: 0.0, _pad2: 0.0,
            },
            source: offscreen.clone(),
        })),
        Transform::default().with_scale(Vec3::new(bw as f32 / 2.0, bh as f32 / 2.0, 1.0)),
        BrightPassQuad,
        QuadScaleFactor(0.5, 0.5),
        RenderLayers::layer(2),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            order: -19,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.0, 0.0, 0.0)),
            ..default()
        },
        RenderTarget::Image(bloom0.clone().into()),
        Msaa::Off,
        BrightPassCamera,
        Nudgable,
        RenderLayers::layer(2),
    ));

    // --- Blur pyramid (bloom stages [3]/[4]): bloom_1, bloom_2 + down/up passes ---
    let b1w = ((w as f32 * 0.25) as u32).max(1);
    let b1h = ((h as f32 * 0.25) as u32).max(1);
    let b2w = ((w as f32 * 0.125) as u32).max(1);
    let b2h = ((h as f32 * 0.125) as u32).max(1);
    let bloom1 = images.add(Image::new_target_texture(
        b1w, b1h, TextureFormat::Rgba16Float, None,
    ));
    let bloom2 = images.add(Image::new_target_texture(
        b2w, b2h, TextureFormat::Rgba16Float, None,
    ));
    commands.spawn(BloomTarget1(bloom1.clone()));
    commands.spawn(BloomTarget2(bloom2.clone()));

    // Down pass 0→1: samples bloom0 (half-res), writes bloom1 (quarter-res).
    let down01_texel = Vec2::new(1.0 / bw as f32, 1.0 / bh as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(blur_materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 0, texel_size: down01_texel, blend: 0.0, _pad0: 0.0,
            },
            source: bloom0.clone(),
        })),
        Transform::default().with_scale(Vec3::new(b1w as f32 / 2.0, b1h as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.25, 0.25),
        RenderLayers::layer(3),
    ));
    // Down pass 1→2: samples bloom1 (quarter), writes bloom2 (eighth).
    let down12_texel = Vec2::new(1.0 / b1w as f32, 1.0 / b1h as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(blur_materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 0, texel_size: down12_texel, blend: 0.0, _pad0: 0.0,
            },
            source: bloom1.clone(),
        })),
        Transform::default().with_scale(Vec3::new(b2w as f32 / 2.0, b2h as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.125, 0.125),
        RenderLayers::layer(4),
    ));
    // Up pass 2→1: samples bloom2 (eighth), writes bloom1 (quarter).
    let up21_texel = Vec2::new(1.0 / b2w as f32, 1.0 / b2h as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(blur_materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 1, texel_size: up21_texel, blend: 0.6, _pad0: 0.0,
            },
            source: bloom2.clone(),
        })),
        Transform::default().with_scale(Vec3::new(b1w as f32 / 2.0, b1h as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.25, 0.25),
        RenderLayers::layer(5),
    ));
    // Up pass 1→0: samples bloom1 (quarter), writes bloom_final (half).
    let bfw = bw;
    let bfh = bh;
    let bloom_final = images.add(Image::new_target_texture(
        bfw, bfh, TextureFormat::Rgba16Float, None,
    ));
    commands.spawn(BloomFinalTarget(bloom_final.clone()));
    let up10_texel = Vec2::new(1.0 / b1w as f32, 1.0 / b1h as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(blur_materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 1, texel_size: up10_texel, blend: 0.8, _pad0: 0.0,
            },
            source: bloom1.clone(),
        })),
        Transform::default().with_scale(Vec3::new(bfw as f32 / 2.0, bfh as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.5, 0.5),
        RenderLayers::layer(6),
    ));
    // Cameras: down01=-18, down12=-17, up21=-16, up10=-15.
    commands.spawn((
        Camera2d, Camera { order: -18, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom1.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(3),
    ));
    commands.spawn((
        Camera2d, Camera { order: -17, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom2.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(4),
    ));
    commands.spawn((
        Camera2d, Camera { order: -16, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom1.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(5),
    ));
    commands.spawn((
        Camera2d, Camera { order: -15, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom_final.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(6),
    ));

    Some(bloom_final.clone())
}

/// Recreate the offscreen Image and rescale both quads on window resize,
/// honoring the live `render_scale` param.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn resize_offscreen(
    mut images: ResMut<Assets<Image>>,
    params: Res<crate::params::BlackHoleParams>,
    target: Query<&OffscreenTarget>,
    bloom0: Query<&BloomTarget0>,
    bloom1: Query<&BloomTarget1>,
    bloom2: Query<&BloomTarget2>,
    bloom_final: Query<&BloomFinalTarget>,
    window: Query<&Window>,
    mut resized: MessageReader<bevy::window::WindowResized>,
    // Both queries borrow `&mut Transform`. Bevy's conflict checker does not
    // treat `With<T>` filters as disjoint access, so two such Query params would
    // trip B0001 — they must be grouped in a ParamSet (borrowed one at a time).
    mut quads: ParamSet<(
        // p0: offscreen + bloom quads — rescaled against offscreen resolution.
        Query<(&mut Transform, &QuadScaleFactor), Without<CompositeQuad>>,
        // p1: composite quad — rescaled against window resolution.
        Query<&mut Transform, With<CompositeQuad>>,
    )>,
) {
    if resized.read().next().is_none() {
        return;
    }
    let Ok(win) = window.single() else { return; };
    let scale = params.render_scale.clamp(MIN_RENDER_SCALE, 1.0);
    let w = ((win.width() * scale) as u32).max(1);
    let h = ((win.height() * scale) as u32).max(1);
    if let Ok(handle) = target.single() {
        let img = Image::new_target_texture(w, h, TextureFormat::Rgba16Float, None);
        // insert returns Result in 0.19 (Err if the asset is locked this frame).
        // Resize fires repeatedly while the user drags, so a dropped frame's
        // insert is harmless — it'll succeed on the next WindowResized.
        let _ = images.insert(handle.0.id(), img);
    }
    // Bloom pyramid (queries return empty when bloom is off).
    let bw = ((w as f32 * 0.5) as u32).max(1);
    let bh = ((h as f32 * 0.5) as u32).max(1);
    let b1w = ((w as f32 * 0.25) as u32).max(1);
    let b1h = ((h as f32 * 0.25) as u32).max(1);
    let b2w = ((w as f32 * 0.125) as u32).max(1);
    let b2h = ((h as f32 * 0.125) as u32).max(1);
    if let Ok(t) = bloom0.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(bw, bh, TextureFormat::Rgba16Float, None));
    }
    if let Ok(t) = bloom1.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(b1w, b1h, TextureFormat::Rgba16Float, None));
    }
    if let Ok(t) = bloom2.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(b2w, b2h, TextureFormat::Rgba16Float, None));
    }
    if let Ok(t) = bloom_final.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(bw, bh, TextureFormat::Rgba16Float, None));
    }
    // Rescale offscreen + bloom quads against the offscreen resolution.
    for (mut t, f) in &mut quads.p0() {
        t.scale = Vec3::new(w as f32 * f.0 / 2.0, h as f32 * f.1 / 2.0, 1.0);
    }
    // Rescale the composite quad against the window resolution.
    for mut t in &mut quads.p1() {
        t.scale = Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0);
    }
}

/// Workaround for Bevy 0.19 issue #24448: with a static camera the world stops
/// rendering after the first frame. Oscillate the camera transform by a
/// sub-pixel amount each frame so the view matrix changes and the render graph
/// keeps producing frames. Amplitude is far below one pixel, so the image is
/// visually stable. Applies to BOTH the offscreen camera (the most static
/// entity — its freeze would make the upscale camera re-sample a stale
/// texture every frame → frozen view) and the upscale camera (the one
/// rendering to the window). Remove when the upstream regression is fixed.
#[allow(clippy::type_complexity)]
fn nudge_camera(
    time: Res<Time>,
    mut camera: Query<&mut Transform, With<Nudgable>>,
) {
    let nudge = (time.elapsed_secs() * 5.0).sin() * 1e-3;
    for mut t in &mut camera {
        t.translation.x = nudge;
    }
}

/// Detects a bloom_quality change and rebuilds the bloom pipeline.
/// Heavy (despawns and respawns all bloom entities + their cameras/targets)
/// but only fires when the user changes the dropdown in the Quality panel.
///
/// `applied` is read as `Res` (immutable borrow) and re-written via
/// `commands.insert_resource` (a queued command applied later in the frame),
/// so the read/write do not conflict in Bevy's system parameter borrow checker.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn rebuild_bloom(
    params: Res<crate::params::BlackHoleParams>,
    applied: Res<AppliedBloomQuality>,
    mut commands: Commands,
    bloom_entities: Query<Entity, Or<(
        With<BrightPassCamera>, With<BlurCamera>,
        With<BrightPassQuad>, With<BlurQuad>,
    )>>,
    bloom_targets: Query<Entity, Or<(
        With<BloomTarget0>, With<BloomTarget1>,
        With<BloomTarget2>, With<BloomFinalTarget>,
    )>>,
    mut composite_materials: ResMut<Assets<crate::render::material::CompositeMaterial>>,
    offscreen_target: Query<&OffscreenTarget>,
    window: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
    mut bp_materials: ResMut<Assets<crate::render::material::BrightPassMaterial>>,
    mut blur_materials: ResMut<Assets<crate::render::material::BlurMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if params.bloom_quality == applied.0 {
        return;
    }
    // Despawn all bloom entities (cameras + quads).
    for e in bloom_entities.iter() {
        commands.entity(e).despawn();
    }
    // Despawn all bloom target markers (they are spawned as bare entities).
    for e in bloom_targets.iter() {
        commands.entity(e).despawn();
    }
    // Re-spawn at the new quality (or not at all, if Off).
    let Ok(win) = window.single() else { return; };
    let scale = params.render_scale.clamp(MIN_RENDER_SCALE, 1.0);
    let w = ((win.width() * scale) as u32).max(1);
    let h = ((win.height() * scale) as u32).max(1);
    let Ok(offscreen_t) = offscreen_target.single() else { return; };
    let offscreen = offscreen_t.0.clone();
    let new_bloom = spawn_bloom_pipeline(
        &mut commands,
        &mut images,
        &mut bp_materials,
        &mut blur_materials,
        &mut meshes,
        &offscreen,
        w,
        h,
    );
    // Update the composite material's bloom handle. When the new quality is
    // Off, spawn_bloom_pipeline returns None and we fall back to a 1x1 black
    // texture so the composite samples black (scene-only ACES).
    let bloom_handle = new_bloom.unwrap_or_else(|| {
        images.add(Image::new_fill(
            bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            &[0, 0, 0, 0, 0, 0, 0, 0],
            TextureFormat::Rgba16Float,
            bevy::asset::RenderAssetUsages::default(),
        ))
    });
    for (_, mat) in composite_materials.iter_mut() {
        mat.bloom = bloom_handle.clone();
    }
    commands.insert_resource(AppliedBloomQuality(params.bloom_quality));
}

fn mirror_params(
    camera: Res<crate::camera::OrbitCamera>,
    params: Res<crate::params::BlackHoleParams>,
    time: Res<Time>,
    window: Query<&Window>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    mut brightpass_materials: ResMut<Assets<crate::render::material::BrightPassMaterial>>,
    mut composite_materials: ResMut<Assets<crate::render::material::CompositeMaterial>>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    // Only update when something changed.
    let (eye, forward, right, up) = camera.basis();
    for (_, mat) in materials.iter_mut() {
        let u = &mut mat.uniforms;
        u.eye = eye;
        u.forward = forward;
        u.right = right;
        u.up = up;
        u.fov = camera.fov;
        u.resolution = Vec2::new(win.width(), win.height());
        u.time = time.elapsed_secs();
        u.rs = params.rs;
        // disk_inner is spin-derived (Kerr ISCO); the params.disk_inner field is ignored.
        u.disk_inner = crate::physics::kerr_isco(params.spin);
        u.disk_outer = params.disk_outer;
        u.disk_tilt = params.disk_tilt;
        u.disk_brightness = params.disk_brightness;
        u.disk_rotation_speed = params.disk_rotation_speed;
        u.doppler_strength = params.doppler_strength;
        u.star_intensity = params.star_intensity;
        u.skybox_intensity = params.skybox_intensity;
        u.grid_density = params.grid_density;
        u.doppler_enabled = params.doppler_enabled as u32;
        u.grid_enabled = params.grid_enabled as u32;
        u.planet_count = params.planet_count;
        u.steps = params.steps;
        u.spin = params.spin;
        u.star_aa = params.star_aa as u32;
        u.bloom_threshold = params.bloom_threshold;
        u.bloom_strength = params.bloom_strength;
        u.exposure = params.exposure;
        u.disk_half_thickness = params.disk_half_thickness;
        u.filament_freq = params.filament_freq;
        u.filament_sharpness = params.filament_sharpness;
        u.density_freq = params.density_freq;
        u.density_strength = params.density_strength;
        u.arm_count = params.arm_count;
        u.arm_tightness = params.arm_tightness;
        u.arm_strength = params.arm_strength;
        u.disk_quality = params.disk_quality.as_u32();
        u.disk_color_mode = params.disk_color_mode.as_u32();
        u.disk_temp = params.disk_temp;
        u.jets_enabled = params.jets_enabled as u32;
        u.jets_strength = params.jets_strength;
    }
    // Update brightpass threshold (live-tunable).
    for (_, mat) in brightpass_materials.iter_mut() {
        mat.uniform.threshold = params.bloom_threshold;
    }
    // Update composite material uniforms (bloom strength + exposure live-tunable).
    for (_, mat) in composite_materials.iter_mut() {
        mat.uniform.bloom_strength = params.bloom_strength;
        mat.uniform.exposure = params.exposure;
    }
}
