use crate::state::{AppState, LoadingProgress, ShaderAssets};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraUpdateSystems, RenderTarget};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::erased_render_asset::prepare_erased_assets;
use bevy::render::render_asset::prepare_assets;
use bevy::render::render_resource::{Extent3d, TextureFormat};
use bevy::render::storage::ShaderBuffer;
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp};
use bevy::sprite_render::Material2dPlugin;
use bevy::transform::TransformSystems;
use bevy_asset_loader::prelude::*;
use bevy_egui::EguiPostUpdateSet;

use super::material::{BlackHoleMaterial, BlurMaterial, BrightPassMaterial, CompositeMaterial};

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

/// Orders the materials that sample render targets after those textures are prepared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
struct RenderTargetsPrepared;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<LoadingProgress>()
            .add_loading_state(
                LoadingState::new(AppState::Loading)
                    .continue_to_state(AppState::Running)
                    .load_collection::<ShaderAssets>(),
            )
            .init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::camera::OrbitInteraction>()
            .init_resource::<crate::camera::UiPointerActivity>()
            .init_resource::<crate::camera::WantsPointer>()
            .init_resource::<crate::params::BlackHoleParams>()
            .init_resource::<crate::scene::planets::PlanetSystemDirty>()
            .add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_plugins(Material2dPlugin::<crate::render::material::CompositeMaterial>::default())
            .add_plugins(Material2dPlugin::<
                crate::render::material::BrightPassMaterial,
            >::default())
            .add_plugins(Material2dPlugin::<crate::render::material::BlurMaterial>::default())
            .add_plugins(bevy_egui::EguiPlugin::default())
            .add_systems(bevy::prelude::PreStartup, disable_egui_auto_context)
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(Startup, crate::scene::planets::spawn_planet_system)
            .add_systems(
                Update,
                update_loading_progress.run_if(in_state(AppState::Loading)),
            )
            .add_systems(OnEnter(AppState::Running), on_enter_running)
            .add_systems(
                Update,
                (
                    crate::camera::orbit_controller.run_if(in_state(AppState::Running)),
                    nudge_camera,
                ),
            )
            .add_systems(Update, crate::scene::planets::spawn_planet_system)
            .add_systems(
                Update,
                crate::scene::planets::orbit_system.before(crate::scene::planets::upload_planets),
            )
            .add_systems(Update, crate::scene::planets::upload_planets)
            .add_systems(
                PostUpdate,
                (rebuild_bloom, resize_offscreen, mirror_params)
                    .chain()
                    .after(EguiPostUpdateSet::EndPass)
                    .before(CameraUpdateSystems)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(
                bevy_egui::EguiPrimaryContextPass,
                (
                    crate::ui::setup_egui_style,
                    crate::ui::loading::loading_ui_system.run_if(in_state(AppState::Loading)),
                    crate::ui::ui_system.run_if(in_state(AppState::Running)),
                ),
            );

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            // Resizing replaces the GPU texture view under the same image handle.
            render_app.configure_sets(
                Render,
                RenderTargetsPrepared
                    .after(prepare_assets::<GpuImage>)
                    .before(prepare_erased_assets::<MeshMaterial2d<BrightPassMaterial>>)
                    .before(prepare_erased_assets::<MeshMaterial2d<BlurMaterial>>)
                    .before(prepare_erased_assets::<MeshMaterial2d<CompositeMaterial>>),
            );
        }
    }
}

fn update_loading_progress(mut progress: ResMut<LoadingProgress>) {
    progress.warmup_frames += 1;
    let frames = progress.warmup_frames;
    if frames < 5 {
        progress.progress = (frames as f32 / 5.0) * 0.4;
        progress.status = "Loading WESL Shader Assets...".to_string();
    } else if frames < 15 {
        progress.progress = 0.4 + ((frames - 5) as f32 / 10.0) * 0.45;
        progress.status = "Initializing WebGPU Render Pipeline...".to_string();
    } else {
        progress.progress = 0.95;
        progress.status = "Warming up GPU Pipeline...".to_string();
    }
}

fn on_enter_running() {
    crate::web::remove_loading_screen();
}

fn scaled_size(size: Vec2, scale: Vec2) -> UVec2 {
    (size * scale).as_uvec2().max(UVec2::ONE)
}

fn offscreen_size(window: &Window, scale: f32) -> UVec2 {
    scaled_size(
        window.size(),
        Vec2::splat(scale.clamp(MIN_RENDER_SCALE, 1.0)),
    )
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
    let UVec2 { x: w, y: h } = offscreen_size(win, params.render_scale);

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
        super::material::SphereData::default(
        );
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

    let bloom_handle = spawn_bloom_pipeline(
        &mut commands,
        &mut images,
        &mut bp_materials,
        &mut blur_materials,
        &mut meshes,
        &offscreen,
        UVec2::new(w, h),
        &params,
    );
    commands.insert_resource(AppliedBloomQuality(params.bloom_quality));
    let composite_mat = composite_materials.add(crate::render::material::CompositeMaterial {
        uniform: crate::render::material::CompositeUniform {
            bloom_strength: params.bloom_strength,
            exposure: params.exposure,
            _pad0: 0.0,
            _pad1: 0.0,
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
    commands.spawn((
        Camera2d,
        Msaa::Off,
        UpscaleCamera,
        Nudgable,
        bevy_egui::PrimaryEguiContext,
        RenderLayers::layer(1),
    ));
}

/// Builds the selected glare pyramid, returning the texture sampled by the composite.
#[allow(clippy::too_many_arguments)]
fn spawn_bloom_pipeline(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    bp_materials: &mut Assets<BrightPassMaterial>,
    blur_materials: &mut Assets<BlurMaterial>,
    meshes: &mut Assets<Mesh>,
    offscreen: &Handle<Image>,
    scene_size: UVec2,
    params: &crate::params::BlackHoleParams,
) -> Handle<Image> {
    let levels = params.bloom_quality.levels();
    if levels == 0 {
        return images.add(Image::new_fill(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[0; 8],
            TextureFormat::Rgba16Float,
            bevy::asset::RenderAssetUsages::default(),
        ));
    }

    let quad = meshes.add(Rectangle::new(2.0, 2.0));
    let half_size = (scene_size / 2).max(UVec2::ONE);
    let bright = make_glare_target(images, half_size);
    let bright_material = bp_materials.add(BrightPassMaterial {
        uniform: super::material::BrightPassUniform {
            threshold: params.bloom_threshold,
            ..default()
        },
        source: offscreen.clone(),
    });
    commands.spawn(BloomTarget0(bright.clone()));
    commands.spawn((
        Mesh2d(quad.clone()),
        MeshMaterial2d(bright_material),
        Transform::from_scale((half_size.as_vec2() / 2.0).extend(1.0)),
        BrightPassQuad,
        QuadScaleFactor(0.5, 0.5),
        RenderLayers::layer(2),
    ));
    commands.spawn((
        Camera2d,
        glare_camera(-19),
        RenderTarget::Image(bright.clone().into()),
        Msaa::Off,
        BrightPassCamera,
        Nudgable,
        RenderLayers::layer(2),
    ));
    if levels == 1 {
        return bright;
    }

    let quarter_size = (scene_size / 4).max(UVec2::ONE);
    let quarter = make_glare_target(images, quarter_size);
    commands.spawn(BloomTarget1(quarter.clone()));
    spawn_blur_pass(
        commands,
        blur_materials,
        &quad,
        &bright,
        &quarter,
        half_size,
        quarter_size,
        3,
        0.25,
        false,
    );

    if levels == 3 {
        let eighth_size = (scene_size / 8).max(UVec2::ONE);
        let eighth = make_glare_target(images, eighth_size);
        commands.spawn(BloomTarget2(eighth.clone()));
        spawn_blur_pass(
            commands,
            blur_materials,
            &quad,
            &quarter,
            &eighth,
            quarter_size,
            eighth_size,
            4,
            0.125,
            false,
        );
        spawn_blur_pass(
            commands,
            blur_materials,
            &quad,
            &eighth,
            &quarter,
            eighth_size,
            quarter_size,
            5,
            0.25,
            true,
        );
    }

    let bloom = make_glare_target(images, half_size);
    commands.spawn(BloomFinalTarget(bloom.clone()));
    spawn_blur_pass(
        commands,
        blur_materials,
        &quad,
        &quarter,
        &bloom,
        quarter_size,
        half_size,
        6,
        0.5,
        true,
    );
    bloom
}

fn make_glare_target(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    images.add(Image::new_target_texture(
        size.x,
        size.y,
        TextureFormat::Rgba16Float,
        None,
    ))
}

fn glare_camera(order: isize) -> Camera {
    Camera {
        order,
        clear_color: ClearColorConfig::Custom(Color::BLACK),
        ..default()
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_blur_pass(
    commands: &mut Commands,
    materials: &mut Assets<BlurMaterial>,
    quad: &Handle<Mesh>,
    source: &Handle<Image>,
    target: &Handle<Image>,
    source_size: UVec2,
    target_size: UVec2,
    layer: usize,
    scale: f32,
    upsample: bool,
) {
    let material = materials.add(BlurMaterial {
        uniform: super::material::BlurUniform {
            mode: u32::from(upsample),
            texel_size: source_size.as_vec2().recip(),
            blend: 0.85,
            _pad0: 0.0,
        },
        source: source.clone(),
    });
    commands.spawn((
        Mesh2d(quad.clone()),
        MeshMaterial2d(material),
        Transform::from_scale((target_size.as_vec2() / 2.0).extend(1.0)),
        BlurQuad,
        QuadScaleFactor(scale, scale),
        RenderLayers::layer(layer),
    ));
    commands.spawn((
        Camera2d,
        glare_camera(layer as isize - 21),
        RenderTarget::Image(target.clone().into()),
        Msaa::Off,
        BlurCamera,
        Nudgable,
        RenderLayers::layer(layer),
    ));
}

fn resize_target(images: &mut Assets<Image>, handle: &Handle<Image>, size: UVec2) {
    if let Some(mut image) = images.get_mut(handle)
        && image.size() != size
    {
        image.resize(Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        });
    }
}

/// Synchronizes the render targets with the live window size and render scale.
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
    mut quads: Query<(&mut Transform, &QuadScaleFactor), Without<CompositeQuad>>,
    mut composite_quads: Query<&mut Transform, With<CompositeQuad>>,
    mut cameras: Query<(&Camera, &RenderTarget, &mut Projection), With<Nudgable>>,
    mut blur_materials: ResMut<Assets<BlurMaterial>>,
) {
    let Ok(win) = window.single() else {
        return;
    };
    for transform in &mut composite_quads {
        transform
            .map_unchanged(|t| &mut t.scale)
            .set_if_neq((win.size() / 2.0).extend(1.0));
    }
    let Ok(target) = target.single() else {
        return;
    };
    let size = offscreen_size(win, params.render_scale);
    if images
        .get(&target.0)
        .is_none_or(|image| image.size() == size)
    {
        return;
    }

    resize_target(&mut images, &target.0, size);
    let half = (size / 2).max(UVec2::ONE);
    let quarter = (size / 4).max(UVec2::ONE);
    let eighth = (size / 8).max(UVec2::ONE);
    if let Ok(target) = bloom0.single() {
        resize_target(&mut images, &target.0, half);
    }
    if let Ok(target) = bloom1.single() {
        resize_target(&mut images, &target.0, quarter);
    }
    if let Ok(target) = bloom2.single() {
        resize_target(&mut images, &target.0, eighth);
    }
    if let Ok(target) = bloom_final.single() {
        resize_target(&mut images, &target.0, half);
    }

    for (transform, factor) in &mut quads {
        let size = scaled_size(size.as_vec2(), Vec2::new(factor.0, factor.1));
        transform
            .map_unchanged(|t| &mut t.scale)
            .set_if_neq((size.as_vec2() / 2.0).extend(1.0));
    }
    for (_, material) in blur_materials.iter_mut() {
        if let Some(source) = images.get(&material.source) {
            material.uniform.texel_size = source.size_f32().recip();
        }
    }
    // Bevy emits image asset events after updating cameras. Mark projections
    // here so their cached target sizes match the resized textures this frame.
    for (camera, target, mut projection) in &mut cameras {
        if let RenderTarget::Image(target) = target
            && let Some(image) = images.get(&target.handle)
            && camera.physical_target_size() != Some(image.size())
        {
            projection.set_changed();
        }
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
fn nudge_camera(time: Res<Time>, mut camera: Query<&mut Transform, With<Nudgable>>) {
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
    bloom_entities: Query<
        Entity,
        Or<(
            With<BrightPassCamera>,
            With<BlurCamera>,
            With<BrightPassQuad>,
            With<BlurQuad>,
        )>,
    >,
    bloom_targets: Query<
        Entity,
        Or<(
            With<BloomTarget0>,
            With<BloomTarget1>,
            With<BloomTarget2>,
            With<BloomFinalTarget>,
        )>,
    >,
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
    let Ok(win) = window.single() else {
        return;
    };
    let Ok(offscreen) = offscreen_target.single() else {
        return;
    };
    for entity in bloom_entities.iter().chain(bloom_targets.iter()) {
        commands.entity(entity).despawn();
    }
    let bloom_handle = spawn_bloom_pipeline(
        &mut commands,
        &mut images,
        &mut bp_materials,
        &mut blur_materials,
        &mut meshes,
        &offscreen.0,
        offscreen_size(win, params.render_scale),
        &params,
    );
    for (_, mat) in composite_materials.iter_mut() {
        mat.bloom = bloom_handle.clone();
    }
    commands.insert_resource(AppliedBloomQuality(params.bloom_quality));
}

#[allow(clippy::too_many_arguments)]
fn mirror_params(
    camera: Res<crate::camera::OrbitCamera>,
    params: Res<crate::params::BlackHoleParams>,
    time: Res<Time>,
    mut disk_time: Local<f64>,
    window: Query<&Window>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    mut brightpass_materials: ResMut<Assets<crate::render::material::BrightPassMaterial>>,
    mut composite_materials: ResMut<Assets<crate::render::material::CompositeMaterial>>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    let resolution = win.size().max(Vec2::ONE);
    let (eye, forward, right, up) = camera.basis();
    // Integrate the rate so slowing or pausing the flow never resets its phase.
    *disk_time += time.delta_secs_f64() * f64::from(params.disk_rotation_speed.max(0.0));
    for (_, mat) in materials.iter_mut() {
        let u = &mut mat.uniforms;
        u.eye = eye;
        u.forward = forward;
        u.right = right;
        u.up = up;
        u.fov = camera.vertical_fov(resolution.x / resolution.y);
        u.resolution = resolution;
        u.time = time.elapsed_secs();
        u.rs = params.rs;
        // disk_inner is spin-derived (Kerr ISCO); the params.disk_inner field is ignored.
        u.disk_inner = crate::physics::kerr_isco(params.spin);
        u.disk_outer = params.disk_outer;
        u.disk_tilt = params.disk_tilt;
        u.disk_brightness = params.disk_brightness;
        u.disk_time = *disk_time as f32;
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
        u.aa_samples = params.aa_quality.samples();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{BlackHoleParams, BloomQuality};
    use crate::ui::preset::{Preset, apply};
    use bevy::asset::{AssetEvent, AssetEventSystems};
    use bevy::ecs::schedule::common_conditions::run_once;
    use bevy::render::{camera::camera_system, texture::ManualTextureViews};
    use bevy::window::{PrimaryWindow, WindowCreated, WindowResized, WindowScaleFactorChanged};

    fn target_size(world: &World, handle: &Handle<Image>) -> UVec2 {
        world
            .resource::<Assets<Image>>()
            .get(handle)
            .unwrap()
            .size()
    }

    #[test]
    fn render_scale_resizes_targets_without_window_events() {
        let mut params = BlackHoleParams::default();
        apply(Preset::Cinematic, &mut params);
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default())
            .init_asset::<Image>()
            .init_asset::<Mesh>()
            .init_asset::<ShaderBuffer>()
            .init_asset::<BlackHoleMaterial>()
            .init_asset::<CompositeMaterial>()
            .init_asset::<BrightPassMaterial>()
            .init_asset::<BlurMaterial>()
            .init_resource::<ManualTextureViews>()
            .add_message::<WindowCreated>()
            .add_message::<WindowResized>()
            .add_message::<WindowScaleFactorChanged>()
            .insert_resource(params)
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(
                PostUpdate,
                (
                    (rebuild_bloom, resize_offscreen)
                        .chain()
                        .after(EguiPostUpdateSet::EndPass)
                        .before(CameraUpdateSystems),
                    camera_system
                        .in_set(CameraUpdateSystems)
                        .before(AssetEventSystems),
                ),
            );
        app.world_mut().spawn((
            Window {
                resolution: (1280, 720).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.update();
        let world = app.world_mut();
        let scene = world
            .query::<&OffscreenTarget>()
            .single(world)
            .unwrap()
            .0
            .clone();
        assert_eq!(target_size(world, &scene), UVec2::new(960, 540));

        app.add_systems(
            PostUpdate,
            (|mut params: ResMut<BlackHoleParams>| params.render_scale = 0.5)
                .in_set(EguiPostUpdateSet::EndPass)
                .run_if(run_once),
        );
        app.update();
        let world = app.world_mut();
        let bloom = world
            .query::<&BloomTarget0>()
            .single(world)
            .unwrap()
            .0
            .clone();
        assert_eq!(target_size(world, &scene), UVec2::new(640, 360));
        assert_eq!(target_size(world, &bloom), UVec2::new(320, 180));
        let blur_materials = world.resource::<Assets<BlurMaterial>>();
        let (_, first_blur) = blur_materials
            .iter()
            .find(|(_, material)| material.source == bloom)
            .unwrap();
        assert_eq!(
            first_blur.uniform.texel_size,
            Vec2::new(1.0 / 320.0, 1.0 / 180.0)
        );
        assert_eq!(
            world
                .query_filtered::<&Camera, With<OffscreenCamera>>()
                .single(world)
                .unwrap()
                .physical_target_size(),
            Some(UVec2::new(640, 360))
        );
        assert_eq!(
            world
                .query_filtered::<&Transform, With<CompositeQuad>>()
                .single(world)
                .unwrap()
                .scale,
            Vec3::new(640.0, 360.0, 1.0)
        );

        apply(
            Preset::Cinematic,
            &mut app.world_mut().resource_mut::<BlackHoleParams>(),
        );
        app.update();
        assert_eq!(target_size(app.world(), &scene), UVec2::new(960, 540));
        apply(
            Preset::Performance,
            &mut app.world_mut().resource_mut::<BlackHoleParams>(),
        );
        app.update();
        let world = app.world_mut();
        let bloom = world.query::<&BloomTarget0>().single(world).unwrap();
        assert_eq!(target_size(world, &scene), UVec2::new(640, 360));
        assert_eq!(target_size(world, &bloom.0), UVec2::new(320, 180));

        let mut image_events = world
            .resource::<Messages<AssetEvent<Image>>>()
            .get_cursor_current();
        world.resource_mut::<BlackHoleParams>().render_scale = 0.5001;
        app.update();
        let events = app.world().resource::<Messages<AssetEvent<Image>>>();
        assert!(
            !image_events
                .read(events)
                .any(|event| matches!(event, AssetEvent::Modified { .. }))
        );

        for (quality, extract, blur) in [
            (BloomQuality::Off, 0, 0),
            (BloomQuality::Medium, 1, 2),
            (BloomQuality::High, 1, 4),
            (BloomQuality::Low, 1, 0),
        ] {
            app.world_mut()
                .resource_mut::<BlackHoleParams>()
                .bloom_quality = quality;
            app.update();
            let world = app.world_mut();
            assert_eq!(
                world.query::<&BrightPassCamera>().iter(world).count(),
                extract
            );
            assert_eq!(world.query::<&BlurCamera>().iter(world).count(), blur);
            let composites = world.resource::<Assets<CompositeMaterial>>();
            let bloom = &composites.iter().next().unwrap().1.bloom;
            assert_eq!(
                target_size(world, bloom),
                if extract == 0 {
                    UVec2::ONE
                } else {
                    UVec2::new(320, 180)
                },
            );
        }
    }
}
