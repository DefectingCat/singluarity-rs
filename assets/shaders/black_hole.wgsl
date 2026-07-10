#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import "shaders/ray_gen.wgsl"
#import "shaders/geodesic_schwarzschild.wgsl"

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
    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    var uv = (in.uv * 2.0 - 1.0);
    uv.x *= aspect;
    let dir = ray_direction(uv);
    // Step size scales with how far we need to travel (~ distance/ steps).
    let dt = max(length(uniforms.eye.xyz), 20.0) / f32(uniforms.steps);
    let res = classify_ray(uniforms.eye.xyz, dir, uniforms.steps, dt);
    if (res.status == 1u) {
        // Captured = shadow.
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    // Escaped: dim grey for now (stars come in Task 11).
    return vec4<f32>(0.02, 0.02, 0.02, 1.0);
}
