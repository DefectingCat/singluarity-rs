// All shader logic is inlined into this single file.
//
// HISTORY: the shader used to be split across several naga_oil modules
// (ray_gen, geodesic, stars, disk, planets, grid, skybox) that each
// `#define_import_path singularity::...` and were pulled into this file via
// `#import singularity::...`. That compiled without error, but calling ANY
// imported function at runtime produced no fragment output — the fullscreen
// quad silently drew nothing and only the camera clear color (grey) showed.
// (Local functions worked; only cross-module imports broke.) Rather than chase
// the naga_oil composition bug, every function is inlined here. The standalone
// module files still exist on disk but are no longer imported.
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
    spin: f32,
    _pad5: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> uniforms: BlackHoleUniforms;

// ---------- planets storage (binding 3) ----------
struct SphereData {
    center: vec4<f32>,   // xyz = center (world space), w = radius
    color: vec4<f32>,    // xyz = color, w = emissive flag
};
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<storage, read> planets: array<SphereData>;

// ---------- optional cubemap skybox (bindings 1 & 2) ----------
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var skybox: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var skybox_sampler: sampler;

// ====================== inlined helpers ======================

// Rotate a vector around the X axis by angle a.
fn rot_x(v: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
}

// --- ray_gen ---
// `fov` is packed into the `.w` of `up` (Rust lays out `up: Vec3` + `fov: f32`
// as one vec4 block).
fn ray_direction(uv: vec2<f32>) -> vec3<f32> {
    let tan_half_fov = tan(uniforms.up.w * 0.5);
    let dir =
        normalize(uniforms.forward.xyz)
        + uniforms.right.xyz * (uv.x * tan_half_fov)
        + uniforms.up.xyz    * (uv.y * tan_half_fov);
    return normalize(dir);
}

// --- stars ---
fn hash13(p: vec3<f32>) -> f32 {
    var q = vec3<f32>(dot(p, vec3<f32>(127.1, 311.7, 74.7)),
                      dot(p, vec3<f32>(269.5, 183.3, 246.1)),
                      dot(p, vec3<f32>(113.5, 271.9, 124.6)));
    let h = fract(sin(q) * 43758.5453);
    return h.x;
}

fn star_color(dir: vec3<f32>, intensity: f32) -> vec3<f32> {
    let scale = 80.0;
    let cell = floor(dir * scale);
    let h = hash13(cell);
    let threshold = 0.985;
    if (h > threshold) {
        let b = (h - threshold) / (1.0 - threshold);
        let col = mix(vec3<f32>(0.6, 0.7, 1.0), vec3<f32>(1.0, 0.9, 0.7), b);
        let f = abs(dir * scale - cell);
        let d = max(f.x, max(f.y, f.z));
        let falloff = smoothstep(0.5, 0.0, d);
        return col * b * falloff * 3.0 * intensity;
    }
    return vec3<f32>(0.0);
}

// --- skybox ---
fn skybox_color(dir: vec3<f32>) -> vec3<f32> {
    return textureSample(skybox, skybox_sampler, dir).rgb;
}

// --- geodesic ---
struct Deriv {
    dpos: vec3<f32>,
    ddir: vec3<f32>,
}

fn deriv(pos: vec3<f32>, dir: vec3<f32>) -> Deriv {
    let r = length(pos);
    let rs = uniforms.rs;
    let h = cross(pos, dir);
    let h2 = dot(h, h);
    let r5 = max(r * r * r * r * r, 1e-6);
    let dpos = dir;
    let accel = -1.5 * rs * h2 / r5 * pos;
    return Deriv(dpos, accel);
}

// --- disk ---
fn disk_hit(prev: vec3<f32>, cur: vec3<f32>) -> bool {
    let y0 = prev.y;
    let y1 = cur.y;
    if (y0 * y1 > 0.0) {
        return false;
    }
    let t = y0 / (y0 - y1);
    let cross = mix(prev, cur, vec3<f32>(t));
    let r = length(vec2<f32>(cross.x, cross.z));
    return r >= uniforms.disk_inner && r <= uniforms.disk_outer;
}

fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let r = length(vec2<f32>(pos.x, pos.z));
    let phi = atan2(pos.z, pos.x);

    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    let n = sin(phi * 8.0 + rot) * 0.5 + 0.5;
    let n2 = sin(phi * 23.0 - rot * 1.7 + r * 2.0) * 0.5 + 0.5;
    let noise = mix(n, n2, 0.4);

    let t = (r - uniforms.disk_inner) / (uniforms.disk_outer - uniforms.disk_inner);
    let tcol = mix(vec3<f32>(1.0, 0.95, 0.85), vec3<f32>(1.0, 0.45, 0.12), clamp(t, 0.0, 1.0));

    let falloff = 1.0 / pow(r / uniforms.disk_inner, 2.0);

    var col = tcol * (0.6 + 0.4 * noise) * falloff;

    let v_orbital = sqrt(uniforms.rs / (2.0 * r));
    let tangent = normalize(vec3<f32>(-sin(phi), 0.0, cos(phi)));
    let vdotn = dot(tangent * v_orbital, -dir);
    let gamma = 1.0 / sqrt(max(1.0 - v_orbital * v_orbital, 1e-4));
    var doppler = 1.0;
    if (uniforms.doppler_enabled != 0u) {
        let delta = 1.0 / (gamma * (1.0 - vdotn));
        doppler = pow(delta, 3.0) * uniforms.doppler_strength;
    }
    col *= doppler;

    return col * uniforms.disk_brightness;
}

// --- planets ---
// `prev`/`cur` are in DISK-LOCAL space; planet centers are world space, so we
// rotate each center into disk-local space here.
fn planet_hit(prev: vec3<f32>, cur: vec3<f32>, dir: vec3<f32>) -> vec4<f32> {
    var nearest_t = 1e9;
    var nearest_col = vec3<f32>(0.0);
    var found = false;
    for (var i: u32 = 0u; i < uniforms.planet_count; i = i + 1u) {
        let s = planets[i];
        let center = rot_x(s.center.xyz, -uniforms.disk_tilt);
        let radius = s.center.w;
        let seg = cur - prev;
        let oc = prev - center;
        let a = dot(seg, seg);
        let b = 2.0 * dot(oc, seg);
        let c = dot(oc, oc) - radius * radius;
        let disc = b * b - 4.0 * a * c;
        if (disc < 0.0) { continue; }
        let sq = sqrt(disc);
        var t = (-b - sq) / (2.0 * a);
        if (t < 0.0) { t = (-b + sq) / (2.0 * a); }
        if (t >= 0.0 && t <= 1.0 && t < nearest_t) {
            nearest_t = t;
            let hit_pos = prev + seg * t;
            let n = normalize(hit_pos - center);
            let light_dir = normalize(vec3<f32>(0.5, 0.8, 0.3));
            let ndl = max(dot(n, light_dir), 0.0);
            var col = s.color.xyz * (0.2 + 0.8 * ndl);
            if (s.color.w > 0.5) { col = s.color.xyz; }
            nearest_col = col;
            found = true;
        }
    }
    if (found) {
        return vec4<f32>(nearest_col, 0.95);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}

// --- grid (Flamm's paraboloid) ---
fn flamm_depth(r: f32) -> f32 {
    if (r <= uniforms.rs) { return 0.0; }
    return -2.0 * sqrt(uniforms.rs * (r - uniforms.rs));
}

fn grid_hit(prev: vec3<f32>, cur: vec3<f32>) -> vec3<f32> {
    let r0 = length(vec2<f32>(prev.x, prev.z));
    let r1 = length(vec2<f32>(cur.x, cur.z));
    let z0_surf = flamm_depth(r0);
    let z1_surf = flamm_depth(r1);
    if ((prev.y - z0_surf) * (cur.y - z1_surf) > 0.0) {
        return vec3<f32>(0.0);
    }
    var hit = vec3<f32>(0.0);
    var found = false;
    for (var s: i32 = 0; s < 8; s = s + 1) {
        let f = f32(s + 1) / 8.0;
        let p = mix(prev, cur, vec3<f32>(f));
        let r = length(vec2<f32>(p.x, p.z));
        let surf = flamm_depth(r);
        if (abs(p.y - surf) < 0.3) {
            hit = p;
            found = true;
            break;
        }
    }
    if (!found) { return vec3<f32>(0.0); }

    let r = length(vec2<f32>(hit.x, hit.z));
    let phi = atan2(hit.z, hit.x);
    let ring = smoothstep(0.06, 0.0, abs(fract(r * uniforms.grid_density * 0.5) - 0.5));
    let spoke = smoothstep(0.04, 0.0, abs(fract(phi * 6.0 / 6.283185) - 0.5));
    let grid = max(ring, spoke);
    let fade = smoothstep(-15.0, -1.0, hit.y);
    let col = vec3<f32>(0.15, 0.3, 0.6) * grid * fade;
    return col * 0.5;
}

// ====================== main ======================
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

    // Total path length to integrate: enough to go from the camera, past the
    // hole, and far enough beyond to count as escaped.
    let eye_dist = length(uniforms.eye.xyz);
    let escape_r = max(eye_dist * 2.0, 100.0);
    let total_path = eye_dist + escape_r;
    let dt = total_path / f32(uniforms.steps);
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
        if (r > escape_r) {
            // Escaped: add background along the (disk-local) final dir.
            // Rotate back to world for the sky/stars sample.
            let world_dir = normalize(rot_x(d, uniforms.disk_tilt));
            var bg = vec3<f32>(0.0);
            bg += star_color(world_dir, uniforms.star_intensity);
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
            let ty = prev.y / (prev.y - new_pos.y);
            let hit = mix(prev, new_pos, vec3<f32>(ty));
            let dc = disk_color(hit, new_dir);
            let a = 0.85;
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
                accum_color += g;
            }
        }

        prev = new_pos;
        pos = new_pos;
        d = new_dir;
    }

    return vec4<f32>(accum_color, 1.0);
}
