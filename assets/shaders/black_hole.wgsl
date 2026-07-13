#import bevy_sprite::mesh2d_vertex_output::VertexOutput
// Bevy/naga_oil imports: each module file uses #define_import_path, then we
// import individual symbols via `namespace::name` (or `namespace::{a, b}`).
// Whole-file imports without `::` do NOT reliably bring functions into scope.
#import singularity::ray_gen::ray_direction
#import singularity::geodesic::{deriv, classify_ray}
#import singularity::stars::{hash13, star_color}
#import singularity::disk::{rot_x, disk_hit, disk_color}
#import singularity::planets::{SphereData, planets, planet_hit}
#import singularity::grid::{flamm_depth, grid_hit}
#import singularity::skybox::skybox_color

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
            // Escaped: add background along the (disk-local) final dir.
            // Rotate back to world for the sky/stars sample.
            let world_dir = normalize(rot_x(d, uniforms.disk_tilt));
            var bg = vec3<f32>(0.0);
            // Procedural stars are always layered in.
            bg += star_color(world_dir, uniforms.star_intensity);
            // Optional cubemap skybox (gated so we never sample the fallback 1x1 texture).
            if (uniforms.skybox_intensity > 0.0) {
                bg += skybox_color(world_dir) * uniforms.skybox_intensity;
            }
            accum_color += (1.0 - accum_alpha) * bg;
            accum_alpha = 1.0;
            break;
        }

        // RK4 step (single step), then test disk crossing on the segment.
        let k1 = deriv(pos, d);
        let k2 = deriv(pos + k1.dpos * dt * 0.5, normalize(d + k1.ddir * dt * 0.5));
        let k3 = deriv(pos + k2.dpos * dt * 0.5, normalize(d + k2.ddir * dt * 0.5));
        let k4 = deriv(pos + k3.dpos * dt, normalize(d + k3.ddir * dt));
        let new_pos = pos + (k1.dpos + 2.0*k2.dpos + 2.0*k3.dpos + k4.dpos) * dt / 6.0;
        let new_dir = normalize(d + (k1.ddir + 2.0*k2.ddir + 2.0*k3.ddir + k4.ddir) * dt / 6.0);

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

        let ph = planet_hit(prev, new_pos, new_dir);
        if (ph.w > 0.0) {
            accum_color += (1.0 - accum_alpha) * ph.xyz * ph.w;
            accum_alpha += (1.0 - accum_alpha) * ph.w;
            if (accum_alpha > 0.99) { break; }
        }

        if (uniforms.grid_enabled != 0u) {
            let g = grid_hit(prev, new_pos);
            if (g.x + g.y + g.z > 0.0) {
                accum_color += g; // additive
            }
        }

        prev = new_pos;
        pos = new_pos;
        d = new_dir;
    }

    return vec4<f32>(accum_color, 1.0);
}
