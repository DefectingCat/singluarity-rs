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

pub struct BlackHolePlugin;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::camera::WantsPointer>()
            .init_resource::<crate::params::BlackHoleParams>()
            .add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_plugins(Material2dPlugin::<crate::render::material::UpscaleMaterial>::default())
            .add_plugins(bevy_egui::EguiPlugin::default())
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
    mut upscale_materials: ResMut<Assets<crate::render::material::UpscaleMaterial>>,
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

    // --- Upscale quad (draws offscreen Image to the window) ---
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(upscale_materials.add(crate::render::material::UpscaleMaterial {
            source: offscreen.clone(),
        })),
        Transform::default().with_scale(Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0)),
        UpscaleQuad,
        CompositeQuad,
        RenderLayers::layer(1),
    ));
    commands.spawn((Camera2d, Msaa::Off, UpscaleCamera, Nudgable, RenderLayers::layer(1)));
}

/// Recreate the offscreen Image and rescale both quads on window resize,
/// honoring the live `render_scale` param.
#[allow(clippy::type_complexity)]
fn resize_offscreen(
    mut images: ResMut<Assets<Image>>,
    params: Res<crate::params::BlackHoleParams>,
    target: Query<&OffscreenTarget>,
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

fn mirror_params(
    camera: Res<crate::camera::OrbitCamera>,
    params: Res<crate::params::BlackHoleParams>,
    time: Res<Time>,
    window: Query<&Window>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
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
    }
}
