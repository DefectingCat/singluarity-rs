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
    star_aa: u32,
    bloom_threshold: f32,
    bloom_strength: f32,
    exposure: f32,
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
    let p = dir * scale;
    let cell = floor(p);
    let h = hash13(cell);
    let threshold = 0.985;
    if (h > threshold) {
        let b = (h - threshold) / (1.0 - threshold);
        let col = mix(vec3<f32>(0.6, 0.7, 1.0), vec3<f32>(1.0, 0.9, 0.7), b);
        if (uniforms.star_aa != 0u) {
            // Gaussian speck: distance to cell center, soft radial falloff.
            // Produces a round 2-3 pixel anti-aliased disk instead of a square.
            let center = cell + vec3<f32>(0.5);
            let dist = length(p - center);
            let radius = 0.25 + b * 0.4;
            let falloff = exp(-dist * dist / (radius * radius));
            return col * b * falloff * 4.0 * intensity;
        } else {
            // Original fast path: square-cell smoothstep (blocky but cheap).
            let f = abs(p - cell);
            let d = max(f.x, max(f.y, f.z));
            let falloff = smoothstep(0.5, 0.0, d);
            return col * b * falloff * 3.0 * intensity;
        }
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
    // Kerr spin. χ ∈ [0,1]; a = χ·M, M = Rs/2 = 0.5 (Rs=1).
    let chi = uniforms.spin;
    let m = 0.5;
    let a = chi * m;
    // Schwarzschild radial bending (identical to Phase 1 at χ=0).
    let h = cross(pos, dir);
    let h2 = dot(h, h);
    let r5 = max(r * r * r * r * r, 1e-6);
    let radial = -1.5 * rs * h2 / r5 * pos;
    // Frame-dragging (Lense-Thirring leading term). Spin axis = +Y.
    let spin_axis = vec3<f32>(0.0, 1.0, 0.0);
    let r3 = max(r * r * r, 1e-6);
    let drag = 2.0 * m * a / r3 * cross(spin_axis, dir);
    let accel = radial + drag;
    return Deriv(dir, accel);
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

// --- disk noise (domain-warped FBM) ---
fn hash33(p: vec3<f32>) -> vec3<f32> {
    let q = vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    );
    return fract(sin(q) * 43758.5453);
}

fn value_noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash33(i + vec3<f32>(0.0, 0.0, 0.0)).x;
    let n100 = hash33(i + vec3<f32>(1.0, 0.0, 0.0)).x;
    let n010 = hash33(i + vec3<f32>(0.0, 1.0, 0.0)).x;
    let n110 = hash33(i + vec3<f32>(1.0, 1.0, 0.0)).x;
    let n001 = hash33(i + vec3<f32>(0.0, 0.0, 1.0)).x;
    let n101 = hash33(i + vec3<f32>(1.0, 0.0, 1.0)).x;
    let n011 = hash33(i + vec3<f32>(0.0, 1.0, 1.0)).x;
    let n111 = hash33(i + vec3<f32>(1.0, 1.0, 1.0)).x;
    let nx00 = mix(n000, n100, u.x);
    let nx10 = mix(n010, n110, u.x);
    let nx01 = mix(n001, n101, u.x);
    let nx11 = mix(n011, n111, u.x);
    let nxy0 = mix(nx00, nx10, u.y);
    let nxy1 = mix(nx01, nx11, u.y);
    return mix(nxy0, nxy1, u.z);
}

fn fbm3(p: vec3<f32>, octaves: u32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i: u32 = 0u; i < octaves; i = i + 1u) {
        sum = sum + amp * value_noise3(p * freq);
        freq = freq * 2.0;
        amp = amp * 0.5;
    }
    return sum;
}

fn disk_noise(pos: vec3<f32>, t: f32) -> f32 {
    let warp = fbm3(pos * 0.8 + vec3<f32>(0.0, 0.0, t * 0.1), 3u);
    let n = fbm3(pos * 2.0 + warp * 1.5 + vec3<f32>(0.0, 0.0, t * 0.3), 4u);
    return n;
}

fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let r = length(vec2<f32>(pos.x, pos.z));
    let phi = atan2(pos.z, pos.x);

    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    // Domain-warped FBM for feathered/smoky gas texture. The Keplerian shear
    // (rot ∝ 1/r^1.5) is folded into the noise flow term so inner radii flow
    // faster than outer — correct differential rotation.
    let noise = disk_noise(vec3<f32>(pos.x * 0.3, pos.z * 0.3, rot), uniforms.time);

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

// One Dormand-Prince RK45 step. Returns the 5th-order solution and the
// error estimate (y5 - y4) as a vec3 (position error; direction error is
// folded in via normalize so we only need position error for step control).
struct RkStep {
    pos: vec3<f32>,
    dir: vec3<f32>,
    err: f32,
};

fn rk45_step(pos: vec3<f32>, dir: vec3<f32>, dt: f32) -> RkStep {
    // Butcher tableau (Dormand-Prince), 6 stages. Each deriv() returns Deriv{dpos, ddir}.
    let k1 = deriv(pos, dir);
    let p2 = pos + k1.dpos * dt * 0.2;
    let d2 = normalize(dir + k1.ddir * dt * 0.2);
    let k2 = deriv(p2, d2);
    let p3 = pos + (k1.dpos * 0.075 + k2.dpos * 0.225) * dt;
    let d3 = normalize(dir + (k1.ddir * 0.075 + k2.ddir * 0.225) * dt);
    let k3 = deriv(p3, d3);
    let p4 = pos + (k1.dpos * 0.3 + k2.dpos * -0.9 + k3.dpos * 1.2) * dt;
    let d4 = normalize(dir + (k1.ddir * 0.3 + k2.ddir * -0.9 + k3.ddir * 1.2) * dt);
    let k4 = deriv(p4, d4);
    let p5 = pos + (k1.dpos * -11.0/54.0 + k2.dpos * 2.5 + k3.dpos * -70.0/27.0 + k4.dpos * 35.0/27.0) * dt;
    let d5 = normalize(dir + (k1.ddir * -11.0/54.0 + k2.ddir * 2.5 + k3.ddir * -70.0/27.0 + k4.ddir * 35.0/27.0) * dt);
    let k5 = deriv(p5, d5);
    let p6 = pos + (k1.dpos * 1631.0/55296.0 + k2.dpos * 175.0/512.0 + k3.dpos * 575.0/13824.0 + k4.dpos * 44275.0/110592.0 + k5.dpos * 253.0/4096.0) * dt;
    let d6 = normalize(dir + (k1.ddir * 1631.0/55296.0 + k2.ddir * 175.0/512.0 + k3.ddir * 575.0/13824.0 + k4.ddir * 44275.0/110592.0 + k5.ddir * 253.0/4096.0) * dt);
    let k6 = deriv(p6, d6);
    // 5th-order solution (used to advance).
    let new_pos = pos + (k1.dpos * 37.0/378.0 + k3.dpos * 250.0/621.0 + k4.dpos * 125.0/594.0 + k5.dpos * 512.0/1771.0 + k6.dpos * 0.0) * dt;
    let new_dir = normalize(dir + (k1.ddir * 37.0/378.0 + k3.ddir * 250.0/621.0 + k4.ddir * 125.0/594.0 + k5.ddir * 512.0/1771.0 + k6.ddir * 0.0) * dt);
    // 4th-order solution (for error estimate).
    let pos4 = pos + (k1.dpos * 2825.0/27648.0 + k3.dpos * 18575.0/48384.0 + k4.dpos * 13525.0/55296.0 + k5.dpos * 277.0/14336.0 + k6.dpos * 0.25) * dt;
    let err = length(new_pos - pos4);
    return RkStep(new_pos, new_dir, err);
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

    // Total path length to integrate: enough to go from the camera, past the
    // hole, and far enough beyond to count as escaped.
    let eye_dist = length(uniforms.eye.xyz);
    let escape_r = max(eye_dist * 2.0, 100.0);
    let total_path = eye_dist + escape_r;
    // Adaptive RK45 constants.
    let steps_max = uniforms.steps;
    let dt_init = total_path / f32(steps_max);
    let dt_min = dt_init * 0.25;
    let dt_max = dt_init * 4.0;
    let tol = 1e-3;
    let r_plus = 0.5 + sqrt(max(0.25 - (uniforms.spin * 0.5) * (uniforms.spin * 0.5), 0.0));

    var pos = rot_x(uniforms.eye.xyz, -uniforms.disk_tilt);
    var d   = normalize(rot_x(dir, -uniforms.disk_tilt));
    var dt  = dt_init;
    var prev = pos;
    var budget = steps_max;

    var accum_color = vec3<f32>(0.0);
    var accum_alpha = 0.0;

    loop {
        if (budget == 0u) { break; }

        let step = rk45_step(pos, d, dt);
        let err = step.err;

        if (err > tol * 10.0 && dt > dt_min) {
            // Reject: shrink dt, retry (does not consume budget).
            // The `dt > dt_min` guard makes dt_min a forced-accept floor: once
            // dt is already at its minimum, accept the step regardless of error
            // rather than spinning forever on `continue`. This prevents a GPU
            // hang when a ray hits a region whose error is intrinsically above
            // tolerance even at the smallest step (extreme spin, near-horizon).
            dt = dt_min;
            continue;
        }
        // Accept: consume one budget unit, refine dt.
        budget = budget - 1u;
        if (err > tol * 10.0) {
            // Forced accept at dt_min (err still high): don't grow dt back up.
        } else {
            dt = clamp(dt * pow(tol / max(err, 1e-12), 0.2), dt_min, dt_max);
        }

        let new_pos = step.pos;
        let new_dir = step.dir;

        let r = length(new_pos);
        if (r < r_plus) {
            break;
        }
        if (r > escape_r) {
            let world_dir = normalize(rot_x(new_dir, uniforms.disk_tilt));
            var bg = vec3<f32>(0.0);
            bg += star_color(world_dir, uniforms.star_intensity);
            if (uniforms.skybox_intensity > 0.0) {
                bg += skybox_color(world_dir) * uniforms.skybox_intensity;
            }
            accum_color += (1.0 - accum_alpha) * bg;
            accum_alpha = 1.0;
            break;
        }

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
