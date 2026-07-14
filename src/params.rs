use bevy::prelude::*;

/// All tunable black-hole parameters. Edited by the egui panel (Task 17),
/// mirrored into BlackHoleUniforms each frame (Task 7).
#[derive(Resource, Clone)]
#[allow(dead_code)] // spin is reserved for Phase 2 (Kerr); render_scale now wired in Phase 2
pub struct BlackHoleParams {
    // Physics (natural units, Rs = 1)
    pub rs: f32,
    // Accretion disk
    pub disk_inner: f32,
    pub disk_outer: f32,
    pub disk_tilt: f32,        // radians, tilt of disk plane vs. camera
    pub disk_brightness: f32,
    pub disk_rotation_speed: f32,
    pub doppler_enabled: bool,
    pub doppler_strength: f32,
    // Renderer
    pub steps: u32,
    pub render_scale: f32,
    pub star_intensity: f32,
    pub grid_enabled: bool,
    pub grid_density: f32,
    // Background
    pub skybox_intensity: f32,
    // Planets (count matches the storage buffer; Task 14)
    pub planet_count: u32,
    // Kerr (Phase 2; unused in Phase 1)
    pub spin: f32,
    // Quality (Phase 3: cinematic rendering)
    pub star_aa: bool,
    pub bloom_threshold: f32,
    pub bloom_strength: f32,
    pub exposure: f32,
}

impl Default for BlackHoleParams {
    fn default() -> Self {
        Self {
            rs: 1.0,
            disk_inner: 3.0,
            disk_outer: 15.0,
            disk_tilt: 0.45,       // ~25.8 deg
            disk_brightness: 1.0,
            disk_rotation_speed: 0.5,
            doppler_enabled: true,
            doppler_strength: 1.0,
            steps: if cfg!(target_arch = "wasm32") { 200 } else { 300 },
            render_scale: if cfg!(target_arch = "wasm32") { 0.5 } else { 0.75 },
            star_intensity: 1.0,
            grid_enabled: false,
            grid_density: 1.0,
            skybox_intensity: 0.0, // procedural stars only by default
            planet_count: 0,
            spin: 0.0,
            star_aa: if cfg!(target_arch = "wasm32") { false } else { true },
            bloom_threshold: 1.0,
            bloom_strength: 0.8,
            exposure: 1.0,
        }
    }
}
