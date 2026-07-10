#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import "shaders/ray_gen.wgsl"
#import "shaders/geodesic_schwarzschild.wgsl"
#import "shaders/stars.wgsl"
#import "shaders/disk.wgsl"

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

    // Work in disk-local space: rotate eye + dir by -disk_tilt around X so the
    // disk lies on y=0. (disk_hit/disk_color assume disk-local coords.)
    var pos = rot_x(uniforms.eye.xyz, -uniforms.disk_tilt);
    var d   = normalize(rot_x(dir, -uniforms.disk_tilt));

    let dt = max(length(uniforms.eye.xyz), 20.0) / f32(uniforms.steps);
    let steps = uniforms.steps;

    // Front-to-back compositing.
    var accum_color = vec3<f32>(0.0);
    var accum_alpha = 0.0;

    var prev = pos;
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let r = length(pos);
        if (r < uniforms.rs) {
            // Captured: whatever we've composited so far is the result.
            break;
        }
        if (r > 1000.0) {
            // Escaped: add background stars along the (disk-local) final dir.
            // Rotate back to world for the star sample.
            let world_dir = normalize(rot_x(d, uniforms.disk_tilt));
            let star = star_color(world_dir, uniforms.star_intensity);
            accum_color += (1.0 - accum_alpha) * star;
            accum_alpha = 1.0;
            break;
        }

        // RK4 step (single step), then test disk crossing on the segment.
        let (k1p, k1d) = deriv(pos, d);
        let (k2p, k2d) = deriv(pos + k1p * dt * 0.5, normalize(d + k1d * dt * 0.5));
        let (k3p, k3d) = deriv(pos + k2p * dt * 0.5, normalize(d + k2d * dt * 0.5));
        let (k4p, k4d) = deriv(pos + k3p * dt, normalize(d + k3d * dt));
        let new_pos = pos + (k1p + 2.0*k2p + 2.0*k3p + k4p) * dt / 6.0;
        let new_dir = normalize(d + (k1d + 2.0*k2d + 2.0*k3d + k4d) * dt / 6.0);

        if (disk_hit(prev, new_pos)) {
            // Approximate the crossing point by interpolating to y=0.
            let ty = prev.y / (prev.y - new_pos.y);
            let hit = mix(prev, new_pos, vec3<f32>(ty));
            let dc = disk_color(hit, new_dir);
            let a = 0.85; // disk is nearly opaque
            accum_color += (1.0 - accum_alpha) * dc * a;
            accum_alpha += (1.0 - accum_alpha) * a;
            if (accum_alpha > 0.99) { break; }
        }

        prev = new_pos;
        pos = new_pos;
        d = new_dir;
    }

    return vec4<f32>(accum_color, 1.0);
}
