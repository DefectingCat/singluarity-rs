use bevy::prelude::*;

/// Bloom pyramid depth (number of bloom textures).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BloomQuality {
    Off,     // no bloom, scene-only ACES composite
    Low,     // 1 level: brightpass → composite (soft halo)
    Medium,  // 2 levels: brightpass → 1 down → 1 up → composite
    #[default]
    High,    // 3 levels: brightpass → 2 down → 2 up → composite (full cinematic)
}

impl BloomQuality {
    pub fn levels(self) -> u32 {
        match self {
            BloomQuality::Off => 0,
            BloomQuality::Low => 1,
            BloomQuality::Medium => 2,
            BloomQuality::High => 3,
        }
    }
}

/// Disk volumetric rendering quality. Gates noise octave counts.
#[allow(dead_code)] // consumed starting Task 7 (tier dispatch) + Task 8 (egui)
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DiskQuality {
    Off,     // flat zero-thickness fallback (current appearance)
    Low,     // 3/2/2 octaves — web default
    Medium,  // 4/3/3 octaves
    #[default]
    High,    // 5/4/3 octaves — desktop default
}

impl DiskQuality {
    /// Returns (filament_octaves, density_octaves, warp_octaves).
    /// Off returns zeros; the shader dispatches to the flat path instead.
    #[allow(dead_code)] // read in Task 7's tier dispatch
    pub fn octaves(self) -> (u32, u32, u32) {
        match self {
            DiskQuality::Off => (0, 0, 0),
            DiskQuality::Low => (3, 2, 2),
            DiskQuality::Medium => (4, 3, 3),
            DiskQuality::High => (5, 4, 3),
        }
    }
}

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
    pub bloom_quality: BloomQuality,
    // Disk turbulence (Phase 3.1: volumetric disk)
    pub disk_half_thickness: f32,
    pub filament_freq: f32,
    pub filament_sharpness: f32,
    pub density_freq: f32,
    pub density_strength: f32,
    pub arm_count: f32,
    pub arm_tightness: f32,
    pub arm_strength: f32,
    pub disk_quality: DiskQuality,
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
            bloom_quality: if cfg!(target_arch = "wasm32") { BloomQuality::Low } else { BloomQuality::High },
            disk_half_thickness: if cfg!(target_arch = "wasm32") { 0.2 } else { 0.3 },
            filament_freq: 1.0,
            filament_sharpness: 2.0,
            density_freq: 0.8,
            density_strength: 1.0,
            arm_count: 2.0,
            arm_tightness: 2.0,
            arm_strength: 0.5,
            disk_quality: if cfg!(target_arch = "wasm32") { DiskQuality::Low } else { DiskQuality::High },
        }
    }
}
