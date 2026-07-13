use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::sprite_render::Material2dPlugin;

use super::material::BlackHoleMaterial;

/// Marks the full-screen quad so the resize system can find and rescale it
/// when the window is resized (the mesh is built once at startup).
#[derive(Component)]
struct FullscreenQuad;

pub struct BlackHolePlugin;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::camera::WantsPointer>()
            .init_resource::<crate::params::BlackHoleParams>()
            .add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_plugins(bevy_egui::EguiPlugin::default())
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(Startup, crate::scene::planets::spawn_default_planet)
            .add_systems(
                Update,
                (
                    crate::camera::orbit_controller,
                    mirror_params,
                    fit_quad_to_window,
                    nudge_camera,
                ),
            )
            .add_systems(Update, crate::scene::planets::upload_planets)
            // bevy_egui 0.41 requires UI systems to run inside the egui context
            // pass (fonts/ctx are initialized there); placing them in Update panics.
            .add_systems(bevy_egui::EguiPrimaryContextPass, crate::ui::ui_system);
    }
}

fn spawn_fullscreen_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    window: Query<&Window>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    // Camera2d's default projection is ScalingMode::WindowSize (1 world unit =
    // 1 pixel, view centered at origin spanning [-w/2,w/2]×[-h/2,h/2]). A unit
    // quad (2×2) scaled by (w/2, h/2) fills the screen. fit_quad_to_window
    // updates this on resize.
    let half_w = win.width() / 2.0;
    let half_h = win.height() / 2.0;
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
    let mut material = BlackHoleMaterial::default();
    material.planets = planets_buffer;
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(material)),
        Transform::default().with_scale(Vec3::new(half_w, half_h, 1.0)),
        FullscreenQuad,
    ));
    commands.spawn(Camera2d);
}

/// Rescale the full-screen quad to the live window size so it always fills the
/// camera's view (Camera2d default projection: 1 world unit = 1 pixel).
fn fit_quad_to_window(
    window: Query<&Window>,
    mut quad: Query<&mut Transform, With<FullscreenQuad>>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    let target = Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0);
    for mut transform in &mut quad {
        if transform.scale != target {
            transform.scale = target;
        }
    }
}

/// Workaround for Bevy 0.19 issue #24448: with a static camera the world stops
/// rendering after the first frame. Oscillate the camera transform by a
/// sub-pixel amount each frame so the view matrix changes and the render graph
/// keeps producing frames. Amplitude is far below one pixel, so the image is
/// visually stable. Remove when the upstream regression is fixed.
fn nudge_camera(time: Res<Time>, mut camera: Query<&mut Transform, With<Camera2d>>) {
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
        u.disk_inner = params.disk_inner;
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
    }
}
