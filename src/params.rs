use bevy::prelude::*;

pub(crate) const MAX_DISK_CLUMPS: usize = 6;

/// Dimensionless spin and the corresponding ISCO and horizon radii (in Rs) for this frame.
#[derive(Resource, Clone, Copy)]
pub struct KerrGeometry {
    pub spin: f32,
    pub isco: f32,
    pub horizon: f32,
}

impl KerrGeometry {
    pub(crate) fn from_spin(spin: f32) -> Self {
        Self {
            spin,
            isco: crate::physics::kerr_isco(spin),
            horizon: crate::physics::kerr_horizon(spin),
        }
    }
}

impl FromWorld for KerrGeometry {
    fn from_world(world: &mut World) -> Self {
        Self::from_spin(world.resource::<BlackHoleParams>().spin)
    }
}

pub(crate) fn advance_spin(
    time: Res<Time<Real>>,
    params: Res<BlackHoleParams>,
    mut geometry: ResMut<KerrGeometry>,
) {
    let target = params.spin;
    if geometry.spin == target {
        return;
    }
    // Event Horizon uses a 0.4 s time constant and caps a stalled frame at 100 ms.
    let weight = -(-2.5 * time.delta_secs().min(0.1)).exp_m1();
    let next = geometry.spin + (target - geometry.spin) * weight;
    let settled = (target - next).abs() < 1e-5 * (1.0 + target.abs());
    *geometry = KerrGeometry::from_spin(if settled { target } else { next });
}

/// Bloom pyramid depth (number of bloom textures).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BloomQuality {
    Off,    // no bloom, scene-only ACES composite
    Low,    // 1 level: brightpass → composite (soft halo)
    Medium, // 2 levels: brightpass → 1 down → 1 up → composite
    #[default]
    High, // 3 levels: brightpass → 2 down → 2 up → composite (full cinematic)
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

/// Volumetric disk shading quality. Every tier retains the main rotating clumps.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DiskQuality {
    Off,    // flat fallback
    Low,    // gas clumps without fine filaments
    Medium, // gas clumps + filaments
    #[default]
    High, // finer volume integration and filament detail
}

impl DiskQuality {
    /// Tier as a u32 for the WGSL uniform selector.
    pub fn as_u32(self) -> u32 {
        match self {
            DiskQuality::Off => 0,
            DiskQuality::Low => 1,
            DiskQuality::Medium => 2,
            DiskQuality::High => 3,
        }
    }
}

/// Accretion disk color models for display and temperature exploration.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DiskColorMode {
    Gradient,  // mode 0: white-hot to orange radial ramp
    Blackbody, // mode 1: analytic blackbody + Kerr δ
    #[default]
    Cinematic, // mode 2: warm highlights with restrained beaming
}

impl DiskColorMode {
    pub fn as_u32(self) -> u32 {
        match self {
            DiskColorMode::Gradient => 0,
            DiskColorMode::Blackbody => 1,
            DiskColorMode::Cinematic => 2,
        }
    }
}

/// Supersampling around the lensed disk rings. Low uses two rays, with four
/// near the shadow. High uses four across the entire ring region.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum AaQuality {
    Off,
    Low,
    #[default]
    High,
}

impl AaQuality {
    /// Base sub-rays per pixel in the ring region, before shader adaptation.
    pub fn samples(self) -> u32 {
        match self {
            AaQuality::Off => 1,
            AaQuality::Low => 2,
            AaQuality::High => 4,
        }
    }
}

/// All tunable black-hole parameters. Edited by the egui panel (Task 17),
/// mirrored into BlackHoleUniforms each frame (Task 7).
#[derive(Resource, Clone)]
pub struct BlackHoleParams {
    // Physics (natural units, Rs = 1)
    pub rs: f32,
    // Accretion disk
    pub disk_inner: f32,
    pub disk_outer: f32,
    pub disk_tilt: f32, // radians, tilt of disk plane vs. camera
    pub disk_brightness: f32,
    pub disk_rotation_speed: f32, // animation time multiplier, 0 pauses the flow
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
    /// Target dimensionless Kerr spin χ. The scene follows it through `KerrGeometry`.
    pub spin: f32,
    // Quality (Phase 3: cinematic rendering)
    pub star_aa: bool,
    pub bloom_threshold: f32,
    pub bloom_strength: f32,
    pub exposure: f32,
    pub bloom_quality: BloomQuality,
    // Disk turbulence (Phase 3.1: volumetric disk)
    pub disk_half_thickness: f32, // H/R ratio (scale height / radius); NOT absolute

    pub filament_freq: f32,
    pub filament_sharpness: f32,
    pub density_freq: f32,
    pub density_strength: f32,
    pub arm_count: f32,
    pub arm_tightness: f32,
    pub arm_strength: f32,
    pub disk_quality: DiskQuality,
    // Disk color model + blackbody temp (Phase 3.2), relativistic jets.
    pub disk_color_mode: DiskColorMode,
    pub disk_temp: f32,
    pub jets_enabled: bool,
    pub jets_strength: f32,
    // Anti-aliasing (Phase 3.3): supersample count for the lensed-image rings.
    pub aa_quality: AaQuality,
    // Planets (Phase 3.4: Kerr orbiting planets)
    pub planets_enabled: bool,
    pub planet_count_target: u32,  // 0..=8
    pub planet_radius_factor: f32, // k, 乘到 kerr_isco(χ) 上
    pub planet_seed: u32,          // ChaCha8Rng 种子, 改了触发系统重生
    pub planet_time_scale: f32,    // 模拟时间放大 (进动很慢, 需放大才可见)
}

impl Default for BlackHoleParams {
    fn default() -> Self {
        Self {
            rs: 1.0,
            disk_inner: 3.0,
            disk_outer: 24.0,
            disk_tilt: 0.0,
            disk_brightness: 1.25,
            disk_rotation_speed: 1.0,
            doppler_enabled: true,
            doppler_strength: 0.72,
            steps: if cfg!(target_arch = "wasm32") {
                200
            } else {
                300
            },
            render_scale: if cfg!(target_arch = "wasm32") {
                0.5
            } else {
                0.75
            },
            star_intensity: 0.48,
            grid_enabled: false,
            grid_density: 1.0,
            skybox_intensity: 0.0, // procedural stars only by default
            planet_count: 0,
            spin: 0.0,
            star_aa: true,
            bloom_threshold: 0.95,
            bloom_strength: 0.42,
            exposure: 1.0,
            bloom_quality: if cfg!(target_arch = "wasm32") {
                BloomQuality::Low
            } else {
                BloomQuality::High
            },
            disk_half_thickness: 0.075,
            filament_freq: 1.0,
            filament_sharpness: 2.0,
            density_freq: 2.0,
            density_strength: 1.0,
            arm_count: 4.0,
            arm_tightness: 2.0,
            arm_strength: 0.8,
            disk_quality: if cfg!(target_arch = "wasm32") {
                DiskQuality::Low
            } else {
                DiskQuality::High
            },
            disk_color_mode: DiskColorMode::default(),
            disk_temp: 11000.0,
            jets_enabled: false,
            jets_strength: 1.0,
            // Web uses one ray everywhere. Desktop supersamples the rings.
            aa_quality: if cfg!(target_arch = "wasm32") {
                AaQuality::Off
            } else {
                AaQuality::Low
            },
            planets_enabled: false,
            planet_count_target: 6,
            planet_radius_factor: 1.3,
            planet_seed: 42,
            planet_time_scale: 50.0,
        }
    }
}
