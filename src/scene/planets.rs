use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;

use crate::render::material::{SphereData, MAX_PLANETS};

/// A planet rendered as a lensed sphere inside the geodesic shader.
#[derive(Component, Clone, Copy)]
pub struct Planet {
    pub center: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub emissive: bool,
}

/// Collects all Planet components, builds a fixed-size Vec<SphereData> (padded
/// to MAX_PLANETS), wraps it in a ShaderBuffer, and ensures every BlackHoleMaterial
/// points its `planets` handle at that buffer. Also updates planet_count in params.
///
/// NOTE: the material field is `Handle<ShaderBuffer>` (Bevy 0.19 AsBindGroup
/// requirement). We create one ShaderBuffer asset and have all materials share it.
pub fn upload_planets(
    planets: Query<&Planet>,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut materials: ResMut<Assets<crate::render::material::BlackHoleMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let mut data: Vec<SphereData> = planets
        .iter()
        .take(MAX_PLANETS)
        .map(|p| SphereData {
            center: p.center,
            radius: p.radius,
            color: p.color,
            emissive: p.emissive as u32,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        })
        .collect();
    // Pad to MAX_PLANETS so the buffer size is constant (avoids reallocation churn).
    data.resize(MAX_PLANETS, SphereData::default());
    params.planet_count = planets.iter().count().min(MAX_PLANETS) as u32;

    // Build (or rebuild) the ShaderBuffer and share its handle across materials.
    let buffer = ShaderBuffer::from(data);
    for (_, mat) in materials.iter_mut() {
        // Replace the handle each frame (simple, correct; cheap for one material).
        mat.planets = buffers.add(buffer.clone());
    }
}

/// Spawns a default test planet behind/above the hole so lensing is visible.
pub fn spawn_default_planet(mut commands: Commands) {
    commands.spawn(Planet {
        center: Vec3::new(0.0, 2.0, -25.0),
        radius: 2.0,
        color: Vec3::new(0.3, 0.5, 1.0),
        emissive: false,
    });
}
