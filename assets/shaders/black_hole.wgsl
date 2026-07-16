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
    disk_half_thickness: f32,
    filament_freq: f32,
    filament_sharpness: f32,
    density_freq: f32,
    density_strength: f32,
    arm_count: f32,
    arm_tightness: f32,
    arm_strength: f32,
    disk_quality: u32,
    // Disk color mode + blackbody temp (Phase 3.2), relativistic jets.
    disk_color_mode: u32,   // 0=gradient, 1=blackbody
    disk_temp: f32,         // blackbody base temperature (Kelvin)
    jets_enabled: u32,      // 0=off, 1=on
    jets_strength: f32,     // jet brightness multiplier
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
    // The unit-sphere direction is hashed into a 3D cell grid. A pixel's ray
    // usually sits near a cell boundary, so evaluating only the home cell clips
    // any star whose center lies in a neighbor — that clip is what made stars
    // look square even with the Gaussian AA path: the hash changes at the cell
    // edge, so the falloff could never reach zero and the visible shape was
    // just the cell's bounding box. Scan the 3×3×3 neighborhood so each star's
    // radial falloff extends across the whole cell it lives in.
    let scale = 80.0;
    let p = dir * scale;
    let base = floor(p);
    let threshold = 0.985;
    // Brightest contributor wins: `best` = (brightness, r, g, b).
    var best = vec4<f32>(0.0);
    for (var dz: i32 = -1; dz <= 1; dz = dz + 1) {
        for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
            for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
                let cell = base + vec3<f32>(f32(dx), f32(dy), f32(dz));
                let h = hash13(cell);
                if (h <= threshold) { continue; }
                let b = (h - threshold) / (1.0 - threshold);
                let center = cell + vec3<f32>(0.5);
                let dist = length(p - center); // Euclidean → round, never square
                let col = mix(vec3<f32>(0.6, 0.7, 1.0), vec3<f32>(1.0, 0.9, 0.7), b);
                // Both paths share the same brightness-driven radius and the
                // same gain, so toggling AA only softens the edge — it does not
                // change peak brightness or visible count. Before, the AA path
                // used a larger radius (up to 0.65 vs 0.5), a heavier gain
                // (4.0 vs 3.0), and a slow-decaying Gaussian whose long tail
                // lifted many dim stars above the visibility floor — that's
                // what made AA look like "bigger and more" stars.
                let radius = 0.25 + b * 0.4;
                let falloff = select(
                    smoothstep(radius, 0.0, dist),                 // hard edge
                    exp(-4.6 * dist * dist / (radius * radius)),    // soft edge
                    uniforms.star_aa != 0u);
                let bright = b * falloff;
                if (bright > best.x) {
                    best = vec4<f32>(bright, col.r, col.g, col.b);
                }
            }
        }
    }
    return best.yzw * best.x * 3.0 * intensity;
}

// --- skybox ---
// textureSampleLevel (not textureSample) is REQUIRED here: this is called from
// inside the main RK45 integration loop, whose control flow is non-uniform
// (`if (accum_alpha > 0.99) { break; }`, per-pixel early exit). The WGSL spec
// forbids textureSample outside uniform control flow because it needs
// screen-space derivatives for mip selection; Chrome's Tint enforces this and
// rejects the shader, while naga (desktop) does not — which is why this only
// crashed the web build. textureSampleLevel takes an explicit LOD (0 here: the
// skybox cubemap is sampled at full resolution, no mip minification) and is
// permitted in non-uniform flow. The visual result is identical.
fn skybox_color(dir: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(skybox, skybox_sampler, dir, 0.0).rgb;
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
    // MAX_OCTAVES-with-break: the conservative WebGPU form for a runtime-
    // chosen octave count. Older drivers can miscompile non-constant loop
    // bounds; this branch is the first to pass dynamic octaves here.
    const MAX_OCTAVES = 6u;
    var sum = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i: u32 = 0u; i < MAX_OCTAVES; i = i + 1u) {
        if (i >= octaves) { break; }
        sum = sum + amp * value_noise3(p * freq);
        freq = freq * 2.0;
        amp = amp * 0.5;
    }
    return sum;
}

// Ridged multifractal noise: 1 - |2n-1| turns value-noise gradients into
// sharp ridges (peak where n=0.5, zero at n=0 and n=1). Raising to
// `sharpness` thins the ridges into filaments. MAX_OCTAVES-with-break is
// the conservative WebGPU form for a runtime-chosen octave count.
fn ridged_fbm(p: vec3<f32>, octaves: u32, sharpness: f32) -> f32 {
    const MAX_OCTAVES = 6u;
    var sum = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i: u32 = 0u; i < MAX_OCTAVES; i = i + 1u) {
        if (i >= octaves) { break; }
        let n = value_noise3(p * freq);
        let ridge = 1.0 - abs(2.0 * n - 1.0);
        sum = sum + amp * pow(ridge, sharpness);
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

// Result of a disk color query: emitted radiance + opacity contribution.
// Both the volumetric and flat paths return this struct so the main loop
// can treat them uniformly.
struct DiskSample {
    color: vec3<f32>,
    density: f32,
}

// Radial temperature gradient: white-hot inner → deep-orange outer.
fn temperature_color(t: f32) -> vec3<f32> {
    return mix(vec3<f32>(1.0, 0.95, 0.85), vec3<f32>(1.0, 0.45, 0.12), clamp(t, 0.0, 1.0));
}

// Analytic blackbody color (Tanner-Helland / Mitchell Charity approximation).
// Input temp in Kelvin; output linear RGB. Ported from the others/ GLSL shader.
fn blackbody(temp: f32) -> vec3f {
    // Clamp to prevent log(0) at the event horizon (infinite redshift).
    let t = max(temp, 1.0) / 100.0;
    var r: f32;
    var g: f32;
    var b: f32;
    if (t <= 66.0) {
        r = 255.0;
        g = 99.4708025861 * log(t) - 161.1195681661;
        if (t <= 19.0) {
            b = 0.0;
        } else {
            b = 138.5177312231 * log(t - 10.0) - 305.0447927307;
        }
    } else {
        r = 329.698727446 * pow(t - 60.0, -0.1332047592);
        g = 288.1221695283 * pow(t - 60.0, -0.0755148492);
        b = 255.0;
    }
    // Formula produces sRGB; convert to linear for the HDR pipeline.
    let srgb = vec3f(r, g, b) / 255.0;
    return pow(max(srgb, vec3f(0.0)), vec3f(2.2));
}

// Radial brightness falloff (∝ 1/r² from the inner edge).
fn radial_falloff(r: f32, inner: f32) -> f32 {
    return 1.0 / pow(r / inner, 2.0);
}

// Cylindrical radius in the disk plane.
fn r_of(pos: vec3<f32>) -> f32 {
    return length(vec2<f32>(pos.x, pos.z));
}

// Relativistic Doppler beaming. `dir` is the ray direction (disk-local).
fn apply_doppler(col: vec3<f32>, pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let phi = atan2(pos.z, pos.x);
    let v_orbital = sqrt(uniforms.rs / (2.0 * r_of(pos)));
    let tangent = normalize(vec3<f32>(-sin(phi), 0.0, cos(phi)));
    let vdotn = dot(tangent * v_orbital, -dir);
    let gamma = 1.0 / sqrt(max(1.0 - v_orbital * v_orbital, 1e-4));
    if (uniforms.doppler_enabled == 0u) {
        return col;
    }
    let delta = 1.0 / (gamma * (1.0 - vdotn));
    let doppler = pow(delta, 3.0) * uniforms.doppler_strength;
    return col * doppler;
}

// Kerr equatorial 4-velocity Doppler factor δ = E_obs/E_em (Page & Thorne 1974).
// Exact: solves g_μν u^μ u^ν = -1 for circular equatorial orbits, then folds in
// the conserved photon angular momentum. Returns vec2(delta, beaming=δ^3.5).
// Used by the blackbody disk color mode; the floor guards against NaN from
// pow of a negative base.
fn kerr_doppler(pos: vec3f, dir: vec3f) -> vec2f {
    let r = r_of(pos);
    let m = 0.5;
    let a = uniforms.spin * m;
    let sqrt_m = sqrt(m);
    let sign_spin = sign(uniforms.spin + 1.0e-8);
    // 1. Keplerian angular velocity Ω = dφ/dt.
    let omega = (sign_spin * sqrt_m) / (r * sqrt(r) + a * sqrt_m);
    // 2. Equatorial metric components (θ = π/2).
    let g_tt = -(1.0 - 2.0 * m / r);
    let g_tphi = -2.0 * m * a / r;
    let g_phiphi = r * r + a * a + 2.0 * m * a * a / r;
    // 3. Time component of the circular-orbit 4-velocity.
    let u_t_sq = -(g_tt + 2.0 * omega * g_tphi + omega * omega * g_phiphi);
    let u_t = 1.0 / sqrt(max(1.0e-6, u_t_sq));
    // 4. Conserved photon angular momentum (impact-parameter mapping).
    let l_photon = pos.z * dir.x - pos.x * dir.z;
    // 5. δ = E_obs/E_em = 1 / (u_t · (1 − Ω · L_photon)).
    let delta = 1.0 / max(0.01, u_t * (1.0 - omega * l_photon));
    let beaming = max(0.01, pow(delta, 3.5));
    return vec2f(delta, beaming);
}

// Unifies the color assembly for both disk paths. Mode 0 = existing gradient +
// Newtonian apply_doppler (preserves the pre-blackbody appearance). Mode 1 =
// Novikov-Thorne radial temperature gradient × Kerr δ → blackbody color × δ^3.5
// beaming, so the approaching side shifts hotter/bluer and the receding side
// cooler/redder — the physical signature absent from the gradient mode.
fn disk_emission(r: f32, pos: vec3f, dir: vec3f, brightness: f32) -> vec3f {
    let inner = uniforms.disk_inner;
    let t = (r - inner) / (uniforms.disk_outer - inner);
    let falloff = radial_falloff(r, inner);
    if (uniforms.disk_color_mode == 0u) {
        var col = temperature_color(t) * brightness * falloff * uniforms.disk_brightness;
        return apply_doppler(col, pos, dir);
    }
    // Blackbody mode: NT radial temperature profile × Kerr Doppler shift.
    let isco_r = clamp(inner / r, 0.0, 1.0);
    let nt_factor = max(0.0, 1.0 - sqrt(isco_r));
    let radial_temp = pow(isco_r, 0.75) * pow(nt_factor, 0.25);
    let d = kerr_doppler(pos, dir);
    let temperature = uniforms.disk_temp * radial_temp * d.x;
    return blackbody(temperature) * d.y * brightness * falloff * uniforms.disk_brightness;
}

// Off-tier fallback: zero-thickness disk, single midplane sample. Density is
// driven by the same radial smoothstep falloff as the volumetric path (no flat
// 0.85 floor) so the edges feather out naturally. Returns DiskSample so the
// main loop dispatches both paths uniformly.
fn disk_color_flat(pos: vec3<f32>, dir: vec3<f32>) -> DiskSample {
    let r = r_of(pos);
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    // Domain-warped FBM for a feathered gas texture. The Keplerian shear
    // (rot ∝ 1/r^1.5) is folded into the noise flow term so inner radii flow
    // faster than outer — correct differential rotation. Amplitude is kept mild
    // so the radial temperature gradient (inside disk_emission) dominates.
    let noise = disk_noise(vec3<f32>(pos.x * 0.3, pos.z * 0.3, rot), uniforms.time);

    let col = disk_emission(r, pos, dir, 0.8 + 0.4 * noise * noise);

    // Radial smoothstep: solid through the bulk, feathered at both edges.
    let radial = smoothstep(uniforms.disk_outer, uniforms.disk_inner, r);
    return DiskSample(vec3<f32>(col), radial * 0.9);
}

// Volumetric disk color. Models the accretion disk as a physically-structured
// density field (no flat floor): radial smoothstep falloff × Gaussian vertical
// decay × a WEAK low-frequency turbulence modulation. The radial temperature
// gradient inside disk_emission drives the dominant appearance (Gargantua's
// smooth white-hot inner → deep-orange outer look); turbulence is kept to a
// subtle ±25% texture accent so the disk reads as solid gas rather than the
// jelly-like translucent slab the old 0.55-floor + ridged-filament model gave.
//
// Noise is sampled in POLAR coordinates (r_norm, phi·freq + Keplerian flow,
// height) so what little texture there is flows tangentially — correct for a
// rotating fluid. The Keplerian flow term (`+ rot`) advects the turbulence.
fn disk_color_volumetric(pos: vec3<f32>, dir: vec3<f32>) -> DiskSample {
    let r = r_of(pos);
    let phi = atan2(pos.z, pos.x);
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);

    // Polar sample coordinate: (r normalized, angle × freq + Keplerian flow,
    // height within slab). The flow term advects the noise so inner radii
    // (faster rotation) drift ahead of outer radii — differential rotation.
    // NOTE: h here is normalized by disk_half_thickness, but disk_half_thickness
    // is now an H/R RATIO (semantics changed from absolute world units), so the
    // caller scales H = r * disk_half_thickness before sampling — see the
    // per-step accumulation site in the main loop.
    let r_norm = r / uniforms.disk_inner;
    let h = pos.y / max(r * uniforms.disk_half_thickness, 1e-3);
    let sp = vec3<f32>(r_norm, phi * 2.5 + rot, h);

    // Weak low-frequency turbulence. Two octaves only (was 4–5): the goal is a
    // Gargantua-style smooth disk, so turbulence is a texture accent (±25%
    // density modulation), NOT the dominant carrier. ridged filaments and
    // logarithmic-spiral arms were removed — they produced the noisy,
    // jelly-like, striated look this model replaces.
    let turbulence = fbm3(sp * uniforms.density_freq, 2u);
    let turb_mod = mix(1.0, 0.75 + 0.5 * turbulence, 0.5);  // 0.75–1.25

    // Gaussian vertical decay: densest at the midplane, ~0 at the slab edge.
    // h is normalized to the slab half-height, so exp(-(h·h)·4) → 0.018 at the
    // rim. This replaces the old uniform-in-slab density (the cause of the
    // jelly: the whole thickness glowed at a constant floor alpha).
    let h_falloff = exp(-(h * h) * 4.0);

    // Radial smoothstep falloff (outer → 0, inner → 1): smooth inner+outer
    // edges instead of a hard annulus, matching the others/ reference disk.ts.
    let radial = smoothstep(uniforms.disk_outer, uniforms.disk_inner, r);

    let total_density = radial * h_falloff * turb_mod * uniforms.density_strength;

    // Brightness: turbulence is a mild accent so the radial temperature gradient
    // (inside disk_emission) dominates the visible luminosity. This keeps the
    // disk smooth and gradient-driven rather than streaked by ridged filaments.
    let brightness = mix(0.8, 1.2, turbulence);

    let col = disk_emission(r, pos, dir, brightness);

    return DiskSample(vec3<f32>(col), total_density);
}

// Per-step disk integration that is DECOUPLED from the RK45 step size.
//
// Why this exists: the previous "sample at new_pos, weight by step_len" form
// produced radial spokes. RK45's step size is spatially adaptive (small near
// the hole where curvature is high, large in flat regions) AND varies between
// neighbouring pixels, so weighting density by `step_len` injected a per-pixel
// brightness modulation along the radial direction → spokes, densest at the
// inner disk where steps are smallest.
//
// Fix: clip THIS step's segment to the disk slab analytically and weight each
// sample by `seg_len` — the world-space length of the ray-slab intersection,
// which depends only on the ray's incidence angle and the local slab height,
// never on the RK45 step size. Then sample N points uniformly along the
// clipped segment so the integral is well-resolved (a single midpoint sample
// under-integrates the Gaussian vertical decay and leaves the disk translucent).
//
// `prev`/`new_pos` are disk-local (caller rotates by -disk_tilt).
fn integrate_disk_segment(prev: vec3f, new_pos: vec3f, dir: vec3f,
                          accum_color: ptr<function, vec3f>,
                          accum_alpha: ptr<function, f32>) {
    // Slab height H is radius-dependent (H/R ratio). Use the step's midpoint r
    // to define H for this segment — the change in r across one RK45 step is
    // small, so this local-constant approximation is accurate enough.
    let mid = (prev + new_pos) * 0.5;
    let r_mid = r_of(mid);
    let H = r_mid * uniforms.disk_half_thickness;

    // Clip the parametric segment prev + t·(new_pos − prev), t∈[0,1], to |y|≤H.
    let dy = new_pos.y - prev.y;
    var t0 = 0.0;
    var t1 = 1.0;
    if (abs(dy) < 1e-6) {
        // Step is parallel to the disk plane: in-slab iff prev is in-slab.
        if (abs(prev.y) > H) { return; }
    } else {
        let ta = ( H - prev.y) / dy;
        let tb = (-H - prev.y) / dy;
        t0 = clamp(min(ta, tb), 0.0, 1.0);
        t1 = clamp(max(ta, tb), 0.0, 1.0);
        if (t1 <= t0) { return; }  // segment does not cross the slab
    }

    // Analytic in-slab length — the key: depends on geometry, NOT on RK45 dt.
    let seg_len = (t1 - t0) * length(new_pos - prev);

    // N uniform samples along the clipped segment, front-to-back composite.
    // Each sample carries density × (seg_len / N) so the N samples sum to the
    // full segment integral when density is roughly constant; the Gaussian
    // vertical decay is captured by sampling at differing heights within the
    // slab. N is a compile-time constant (WGSL requires static loop bounds).
    const N = 4u;
    for (var i: u32 = 0u; i < N; i = i + 1u) {
        let t = t0 + (t1 - t0) * (f32(i) + 0.5) / f32(N);
        let p = mix(prev, new_pos, vec3f(t));
        let rp = r_of(p);
        if (rp < uniforms.disk_inner || rp > uniforms.disk_outer) {
            continue;  // radial clipping: outside the annulus
        }
        let s = disk_color_volumetric(p, dir);
        let ds = s.density * seg_len / f32(N);
        *accum_color += (1.0 - *accum_alpha) * s.color * ds;
        *accum_alpha += (1.0 - *accum_alpha) * ds;
    }
}

// Relativistic jets along the spin axis (Y). Bipolar cones above/below the
// disk with 0.92c outflow beaming. Ported from others/' sample_relativistic_jets:
// Gaussian radial falloff, exponential length decay, outward-flowing noise,
// δ^3.5 beaming (no floor needed — β=0.92 keeps the denominator positive).
// Front-to-back composited into the same accumulators as the disk.
fn sample_jets(pos: vec3f, dir: vec3f, r_plus: f32,
               accum_color: ptr<function, vec3f>, accum_alpha: ptr<function, f32>) {
    // Relativistic jets are spin-powered (Blandford-Znajek): the mechanism taps
    // the ergosphere, which only exists for a rotating hole. At χ ≈ 0 there is
    // nothing to drive an outflow, so suppress the jets regardless of the
    // jets_enabled toggle — the toggle expresses user intent, spin expresses
    // physics. Without this gate the default scene (spin = 0, jets_enabled =
    // true) shows blue/white columns over the poles that have no physical cause.
    if (uniforms.spin < 0.05) { return; }
    let jet_v = abs(pos.y);
    let jet_max_h = 80.0;
    if (jet_v <= r_plus * 1.8 || jet_v >= jet_max_h) {
        return;
    }
    let jet_r = length(vec2f(pos.x, pos.z));
    let jet_width = 1.0 + jet_v * 0.15;
    if (jet_r >= jet_width * 2.0) {
        return;
    }
    let radial_falloff = exp(-(jet_r * jet_r) / (jet_width * 0.5));
    let length_falloff = exp(-jet_v * 0.05);

    // Outward-flowing turbulence: the y term reverses sign of the time flow so
    // the lower jet streams downward and the upper jet upward.
    let flow = pos.y * 2.0 - uniforms.time * 8.0;
    let uv_jet = vec3f(pos.x, flow, pos.z);
    let noise_val = value_noise3(uv_jet * 0.5) * 0.6 + value_noise3(uv_jet * 1.5) * 0.4;
    let jet_density = radial_falloff * length_falloff * max(0.0, noise_val - 0.2);

    if (jet_density <= 0.001) {
        return;
    }
    // 0.92c outflow beaming.
    let jet_vel = 0.92 * sign(pos.y);
    let jet_vel_vec = vec3f(0.0, jet_vel, 0.0);
    let cos_theta = dot(normalize(jet_vel_vec), -dir);
    let beta = abs(jet_vel);
    let gamma = 1.0 / sqrt(1.0 - beta * beta);
    let delta = 1.0 / (gamma * (1.0 - beta * cos_theta));
    // Cap the beaming: with β=0.92 the approaching jet's δ^3.5 reaches ~258×,
    // which blows the blue jet past the bloom threshold and saturates it to
    // white. Clamp to 8× — still visibly brighter on the approaching side, but
    // the base color (0.4, 0.7, 1.0) survives the HDR + bloom pipeline.
    let beaming = min(pow(delta, 3.5), 8.0);

    let base_color = vec3f(0.4, 0.7, 1.0);
    // Decouple accumulation from the RK45 step size. Multiplying by `dt` (the
    // adaptive step the caller passes in) injects a per-pixel brightness
    // modulation along the jet axis: dt varies across a 16× range (dt_min..
    // dt_max) and between neighbouring pixels, so `* dt` paints bright/dim
    // bands and ring structures whose only cause is the integrator's step
    // choice, not the physics. The disk path hit the identical bug (radial
    // spokes) and fixed it by weighting on the analytic in-slab length instead
    // of dt (see integrate_disk_segment). The jet has no comparable analytic
    // segment to clip to, so use a fixed normalized weight (0.5) — brightness
    // then depends only on geometry and beaming, never on dt.
    let emission = base_color * jet_density * 0.05 * beaming * uniforms.jets_strength * 0.5;
    *accum_color += emission * (1.0 - *accum_alpha);
    *accum_alpha += jet_density * 0.05 * uniforms.jets_strength * 0.5;
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

        // --- volumetric disk ---
        if (uniforms.disk_quality == 0u) {
            // Off tier: zero-thickness single midplane sample, fixed alpha.
            if (disk_hit(prev, new_pos)) {
                let ty = prev.y / (prev.y - new_pos.y);
                let hit = mix(prev, new_pos, vec3<f32>(ty));
                let s = disk_color_flat(hit, new_dir);
                accum_color += (1.0 - accum_alpha) * s.color * s.density;
                accum_alpha += (1.0 - accum_alpha) * s.density;
                if (accum_alpha > 0.99) { break; }
            }
        } else {
            // Volumetric tier: clip this step to the disk slab and integrate N
            // samples along the clipped segment, weighted by the analytic
            // in-slab length (NOT the RK45 step size). See integrate_disk_segment
            // for why the step-size-decoupled weight is what kills the radial
            // spokes that the previous `* step_len` form produced.
            integrate_disk_segment(prev, new_pos, new_dir, &accum_color, &accum_alpha);
            if (accum_alpha > 0.99) { break; }
        }

        // --- relativistic jets (along the spin axis) ---
        if (uniforms.jets_enabled != 0u) {
            sample_jets(new_pos, new_dir, r_plus, &accum_color, &accum_alpha);
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
