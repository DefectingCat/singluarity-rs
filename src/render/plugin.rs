use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

use super::material::BlackHoleMaterial;

pub struct BlackHolePlugin;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(Update, update_time);
    }
}

fn spawn_fullscreen_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
) {
    commands.spawn(Camera2d);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(BlackHoleMaterial { time: 0.0 })),
        Transform::default(),
    ));
}

fn update_time(time: Res<Time>, mut materials: ResMut<Assets<BlackHoleMaterial>>) {
    for (_, mat) in materials.iter_mut() {
        mat.time = time.elapsed_secs();
    }
}
