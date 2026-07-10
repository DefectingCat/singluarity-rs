#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct BlackHoleUniforms {
    eye: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    resolution: vec2<f32>,
    time: f32,
    _pad3: f32,
    rs: f32,
    disk_inner: f32,
    disk_outer: f32,
    disk_tilt: f32,
    disk_brightness: f32,
    disk_rotation_speed: f32,
    doppler_strength: f32,
    star_intensity: f32,
    skybox_intensity: f32,
    grid_density: f32,
    doppler_enabled: u32,
    grid_enabled: u32,
    planet_count: u32,
    steps: u32,
    _pad4: f32,
    _pad5: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> uniforms: BlackHoleUniforms;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Prove the camera basis + uniforms flow: tint by eye direction.
    let n = normalize(uniforms.eye.xyz);
    return vec4<f32>(abs(n) * 0.5 + 0.3, 1.0);
}
