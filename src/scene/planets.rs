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

/// Collects all Planet components, writes them into the shared MAX_PLANETS-sized
/// `ShaderBuffer` that the material already binds, and updates `planet_count`.
///
/// CRITICAL: we must NOT allocate a new `ShaderBuffer` (and a new handle) each
/// frame. The `#[storage(3, read_only)]` binding resolves the handle via
/// `RenderAssets<GpuShaderBuffer>::get(handle)` and returns
/// `AsBindGroupError::RetryNextUpdate` if the GPU asset for *that exact handle*
/// isn't ready yet. A freshly-added asset has no GPU asset yet, so reassigning
/// the handle every frame makes the fullscreen quad's draw get skipped every
/// frame — the screen shows only the camera clear color (grey).
///
/// Instead, mutate the existing asset in place. `GpuShaderBuffer::prepare_asset`
/// (bevy_render 0.19 `storage.rs`) sees the changed CPU data, reuses the same
/// GPU buffer, and `write_buffer`s the new contents — the handle stays stable,
/// the GPU asset stays ready, and the draw proceeds.
pub fn upload_planets(
    planets: Query<&Planet>,
    mut params: ResMut<crate::params::BlackHoleParams>,
    materials: Res<Assets<crate::render::material::BlackHoleMaterial>>,
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

    // Write into the existing buffer asset(s) the materials already reference.
    // The startup system pre-creates exactly one such buffer; we find it by the
    // materials' handles and mutate in place — never reallocate the handle.
    // set_data moves a Vec<T> (encase treats Vec<T> as a runtime-sized array),
    // matching the official bevy 0.19 storage_buffer example.
    for (_, mat) in materials.iter() {
        if let Some(mut buffer) = buffers.get_mut(&mat.planets) {
            buffer.set_data(data.clone());
        }
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
