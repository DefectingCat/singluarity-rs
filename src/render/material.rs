use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use bevy::sprite_render::Material2d;

/// GPU uniform: params + camera packed into one struct bound at binding 0.
#[derive(Clone, ShaderType)]
pub struct BlackHoleUniforms {
    // Camera basis + eye (4 vec3s = must align; pad each to vec4 via the shader struct)
    pub eye: Vec3,
    pub _pad0: f32,
    pub forward: Vec3,
    pub _pad1: f32,
    pub right: Vec3,
    pub _pad2: f32,
    pub up: Vec3,
    pub fov: f32,
    // Resolution
    pub resolution: Vec2,
    pub time: f32,
    pub _pad3: f32,
    // Physics + disk
    pub rs: f32,
    pub disk_inner: f32,
    pub disk_outer: f32,
    pub disk_tilt: f32,
    pub disk_brightness: f32,
    pub disk_rotation_speed: f32,
    pub doppler_strength: f32,
    pub star_intensity: f32,
    pub skybox_intensity: f32,
    pub grid_density: f32,
    // Flags packed as u32 (bools aren't valid uniform scalar types in WGSL)
    pub doppler_enabled: u32,
    pub grid_enabled: u32,
    pub planet_count: u32,
    pub steps: u32,
    pub spin: f32,       // Phase 2: dimensionless Kerr spin χ = a/M ∈ [0,1].
    pub star_aa: u32,
    pub bloom_threshold: f32,
    pub bloom_strength: f32,
    pub exposure: f32,
    pub _pad5: f32,
}

impl Default for BlackHoleUniforms {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 30.0),
            _pad0: 0.0,
            forward: Vec3::new(0.0, 0.0, -1.0),
            _pad1: 0.0,
            right: Vec3::new(1.0, 0.0, 0.0),
            _pad2: 0.0,
            up: Vec3::new(0.0, 1.0, 0.0),
            fov: 1.0,
            resolution: Vec2::new(1280.0, 720.0),
            time: 0.0,
            _pad3: 0.0,
            rs: 1.0,
            disk_inner: 3.0,
            disk_outer: 15.0,
            disk_tilt: 0.45,
            disk_brightness: 1.0,
            disk_rotation_speed: 0.5,
            doppler_strength: 1.0,
            star_intensity: 1.0,
            skybox_intensity: 0.0,
            grid_density: 1.0,
            doppler_enabled: 1,
            grid_enabled: 0,
            planet_count: 0,
            steps: 300,
            spin: 0.0,
            star_aa: 1,
            bloom_threshold: 1.0,
            bloom_strength: 0.8,
            exposure: 1.0,
            _pad5: 0.0,
        }
    }
}

/// One planet's data, uploaded in a storage buffer (binding 3).
#[derive(Clone, Copy, ShaderType, Default)]
pub struct SphereData {
    pub center: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub emissive: u32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

pub const MAX_PLANETS: usize = 32;

#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct BlackHoleMaterial {
    #[uniform(0)]
    pub uniforms: BlackHoleUniforms,
    // Texture at binding 1 + its matching sampler at binding 2. The derive
    // requires the texture and sampler attributes to live on the same field.
    // `dimension = "cube"` is REQUIRED: skybox.wgsl declares this binding as
    // `texture_cube<f32>`. The derive defaults to D2, which made the bind-group
    // layout (D2) disagree with the shader (Cube) — the pipeline failed to
    // specialize and the fullscreen quad silently drew nothing, leaving only
    // the camera clear color. Matching the dimension to Cube lets the pipeline
    // compile; when no cubemap is set, Bevy binds its 1x1 cube fallback (gated
    // out by `skybox_intensity > 0` in the shader anyway).
    #[texture(1, dimension = "cube")]
    #[sampler(2)]
    pub skybox: Option<Handle<Image>>,
    #[storage(3, read_only)]
    pub planets: Handle<ShaderBuffer>,
}

impl Material2d for BlackHoleMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/black_hole.wgsl".into()
    }
}

/// Samples the sub-resolution offscreen render and blits it fullscreen.
/// Bound to a second Camera2d that draws after the offscreen camera.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct UpscaleMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub source: Handle<Image>,
}

impl Material2d for UpscaleMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/upscale.wgsl".into()
    }
}

/// Extracts luminance above a threshold from the HDR offscreen into a
/// half-res float texture (bloom stage [2]). Soft-knee, not hard cut.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BrightPassMaterial {
    #[uniform(0)]
    pub threshold: f32,
    #[texture(1)]
    #[sampler(2)]
    pub source: Handle<Image>,
}

impl Material2d for BrightPassMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/brightpass.wgsl".into()
    }
}

