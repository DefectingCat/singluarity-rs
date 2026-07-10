use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

use super::material::BlackHoleMaterial;

pub struct BlackHolePlugin;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::params::BlackHoleParams>()
            .add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_plugins(bevy_egui::EguiPlugin::default())
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(
                Update,
                (crate::camera::orbit_controller, mirror_params),
            )
            // bevy_egui 0.41 requires UI systems to run inside the egui context
            // pass (fonts/ctx are initialized there); placing them in Update panics.
            .add_systems(bevy_egui::EguiPrimaryContextPass, crate::ui::ui_system);
    }
}

fn spawn_fullscreen_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    window: Query<&Window>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    let aspect = win.width() / win.height();
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0 * aspect, 2.0))),
        MeshMaterial2d(materials.add(BlackHoleMaterial::default())),
        Transform::default(),
    ));
    commands.spawn(Camera2d);
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
