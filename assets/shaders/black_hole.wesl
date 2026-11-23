// Keep the ray integration and shading logic in this file.
import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

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
    disk_time: f32,
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
    disk_color_mode: u32,   // 0=gradient, 1=blackbody, 2=cinematic
    disk_temp: f32,         // blackbody base temperature (Kelvin)
    jets_enabled: u32,      // 0=off, 1=on
    jets_strength: f32,     // jet brightness multiplier
    aa_samples: u32,        // per-pixel supersample count (1/2/4); >1 antialiases the higher-order lensed-image rings
    _pad6: f32,             // explicit round-up to 16-byte uniform alignment
    _pad7: f32,             //   (matches BlackHoleUniforms in material.rs byte-for-byte)
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> uniforms: BlackHoleUniforms;

// Per-pixel fragment coordinate, set at the top of `fragment` from
// `in.position.xy` (the @builtin(position) frag coord). Used as a deterministic
// seed for the slab sub-sample jitter and the supersample jitter so the noise
// is spatially stable (no per-frame flicker). `var<private>` is the same form
// `blur.wgsl` uses for its runtime-indexed kernel tables.
var<private> pixel_seed: vec2<f32>;
var<private> disk_jitter: f32;

// Vertical size of one offscreen-target pixel in uv units (1/offscreen_height),
// captured at the top of `fragment` via fwidth while control flow is still
// uniform (derivatives are forbidden inside the RK45 loop). `pixel_footprint`
// converts it to an angular size. This is the ONLY way to get the true
// offscreen pixel size: uniforms.resolution is the WINDOW size, which differs
// from the offscreen target by render_scale, and passing render_scale through
// the uniform block would touch four files for a value the GPU already knows.
var<private> pixel_size_uv: f32;

// ---------- planets storage (binding 3) ----------
struct SphereData {
    center: vec4<f32>,   // xyz = center (world space), w = radius
    color: vec3<f32>,
    emissive: u32,
};
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var<storage, read> planets: array<SphereData>;

// ---------- optional cubemap skybox (bindings 1 & 2) ----------
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var skybox: texture_cube<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var skybox_sampler: sampler;

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
    return fract(sin(dot(p, vec3f(127.1, 311.7, 74.7))) * 43758.5453);
}

// Smooth value noise shared by the sky, disk and jets.
fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash13(i);
    let c100 = hash13(i + vec3<f32>(1.0, 0.0, 0.0));
    let c010 = hash13(i + vec3<f32>(0.0, 1.0, 0.0));
    let c110 = hash13(i + vec3<f32>(1.0, 1.0, 0.0));
    let c001 = hash13(i + vec3<f32>(0.0, 0.0, 1.0));
    let c101 = hash13(i + vec3<f32>(1.0, 0.0, 1.0));
    let c011 = hash13(i + vec3<f32>(0.0, 1.0, 1.0));
    let c111 = hash13(i + vec3<f32>(1.0, 1.0, 1.0));
    return mix(
        mix(mix(c000, c100, u.x), mix(c010, c110, u.x), u.y),
        mix(mix(c001, c101, u.x), mix(c011, c111, u.x), u.y),
        u.z);
}

fn fbm(p_in: vec3<f32>, octaves: i32) -> f32 {
    var p = p_in;
    var v = 0.0;
    var amp = 0.5;
    for (var i = 0; i < octaves; i = i + 1) {
        v += amp * vnoise(p);
        p = p * 2.03 + vec3<f32>(11.3, 7.1, 5.7);
        amp *= 0.5;
    }
    return v;
}

// Stellar-population color: cool-heavy like the real luminosity function —
// mostly red/orange dwarfs, some sun-like whites, few hot blue giants.
fn star_temp_color(t_in: f32) -> vec3<f32> {
    let t = pow(t_in, 1.6);
    var col = mix(vec3<f32>(1.0, 0.62, 0.42), vec3<f32>(1.0, 0.88, 0.72), smoothstep(0.0, 0.45, t));
    col = mix(col, vec3<f32>(0.92, 0.94, 1.0), smoothstep(0.45, 0.75, t));
    col = mix(col, vec3<f32>(0.68, 0.78, 1.0), smoothstep(0.75, 1.0, t));
    return col;
}

// Galactic-plane window: 1 on the plane, Gaussian falloff off it. The normal
// is fixed (tilted against the disk plane so the band never aligns with it).
fn milky_way_band(dir: vec3<f32>) -> f32 {
    let d = dot(dir, vec3<f32>(0.2857, 0.8571, 0.4286));
    return exp(-d * d * 14.0);
}

// Screen-pixel footprint in cell units at a given grid scale. Stars smaller
// than a pixel must not pop in and out as the camera orbits, so their flux is
// spread over the pixel instead of being clipped to zero by the falloff. Uses
// the analytic footprint from the fov and the offscreen pixel size captured at
// the fragment entry (see pixel_size_uv).
fn pixel_footprint(scale: f32) -> f32 {
    let ang_per_pixel = 2.0 * tan(uniforms.up.w * 0.5) * pixel_size_uv;
    return max(ang_per_pixel * scale, 1e-4);
}

// Milky Way backdrop: domain-warped fbm clouds carved by dark dust lanes,
// windowed to the galactic plane. Deliberately dim — it must sit below the
// bloom threshold and read as haze, not as a bright object.
fn nebula_color(dir: vec3<f32>) -> vec3<f32> {
    let band = milky_way_band(dir);
    if (band < 0.02) { return vec3<f32>(0.0); }
    let p = dir * 6.5;
    let warp = vec3<f32>(
        fbm(p + vec3<f32>(0.0, 5.2, 1.3), 2),
        fbm(p + vec3<f32>(5.2, 1.3, 2.8), 2),
        fbm(p + vec3<f32>(1.3, 2.8, 0.0), 2));
    let q = p + 2.0 * (warp - 0.5);
    let cloud = fbm(q, 3);
    let dust = fbm(q * 2.7 + vec3<f32>(3.7, 8.1, 1.9), 3);
    var dens = max(cloud - 0.48, 0.0) * 2.4;
    dens *= 1.0 - 0.85 * smoothstep(0.45, 0.68, dust);
    // Warm gold toward the denser core, cool blue-purple in the thin wisps.
    let col = mix(vec3<f32>(0.45, 0.28, 0.15), vec3<f32>(0.10, 0.14, 0.38),
                  smoothstep(0.0, 0.28, cloud - 0.48));
    return col * dens * band * 0.38;
}

fn star_color(dir: vec3<f32>, intensity: f32) -> vec3<f32> {
    let band = milky_way_band(dir);
    var acc = vec3<f32>(0.0);

    // --- faint deep layer: the "deep space" dust of unresolved stars ---
    // Each star sits exactly at its cell center, so evaluating only the single
    // nearest cell can never clip it (unlike the bright layer below, which
    // offsets stars within cells and must scan a neighborhood). Brightness
    // follows a steep power law — almost every star is dim, matching the real
    // magnitude distribution.
    //
    // TEMPORAL STABILITY: this layer first shipped with stars as small as
    // r0 ≈ 0.1 cells ≈ 0.4 px, and the whole background shimmered ("shaking
    // stars") as soon as the camera moved — each sub-pixel point source
    // strobes as it crosses pixel centers, and thousands of them twinkling
    // out of phase reads as noise, masking the coherent rotation. Every faint
    // star is now spread over at least ~3 px (with flux preservation, so the
    // smearing dims rather than brightens), which keeps frame-to-frame change
    // per star small. Measured with a yaw-sweep flicker harness (mean abs
    // frame-to-frame LDR pixel delta): sub-pixel version 2.9, this version
    // 0.45, old bright-layer-only background 0.31.
    {
        let scale = 200.0;
        let p = dir * scale;
        let cell = floor(p + vec3<f32>(0.5)); // nearest cell
        let h = hash13(cell + vec3<f32>(17.0));
        if (h > 0.90 - 0.12 * band) { // denser toward the galactic plane
            let d = length(p - cell);
            let b = pow(hash13(cell + vec3<f32>(31.0)), 7.0);
            let pix = pixel_footprint(scale);
            let r0 = 0.10 + 0.10 * hash13(cell + vec3<f32>(53.0));
            let r = max(r0, 3.0 * pix);
            // Preserve total flux when the star is smeared over its ~3px
            // footprint: without the r0²/r² term the layer would brighten as
            // the footprint grows (e.g. at low render_scale on web).
            let flux = (r0 * r0) / (r * r);
            let falloff = smoothstep(r, 0.0, d);
            acc += star_temp_color(hash13(cell + vec3<f32>(47.0))) * (b * falloff * flux * 8.0);
        }
    }

    // --- bright layer: resolvable stars, 3×3×3 neighborhood scan ---
    // The unit-sphere direction is hashed into a 3D cell grid. A pixel's ray
    // usually sits near a cell boundary, so evaluating only the home cell clips
    // any star whose center lies in a neighbor — that clip is what made stars
    // look square even with the Gaussian AA path: the hash changes at the cell
    // edge, so the falloff could never reach zero and the visible shape was
    // just the cell's bounding box. Scan the 3×3×3 neighborhood so each star's
    // radial falloff extends across the whole cell it lives in.
    {
        let scale = 80.0;
        let p = dir * scale;
        let base = floor(p);
        let threshold = 0.985 - 0.012 * band; // slightly denser in the band
        // Brightest contributor wins: `best` = (brightness, r, g, b).
        var best = vec4<f32>(0.0);
        for (var dz: i32 = -1; dz <= 1; dz = dz + 1) {
            for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
                for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
                    let cell = base + vec3<f32>(f32(dx), f32(dy), f32(dz));
                    let h = hash13(cell);
                    if (h <= threshold) { continue; }
                    let b_raw = (h - threshold) / (1.0 - threshold);
                    // Power-law magnitudes: many dim stars, few heroes.
                    let b = pow(b_raw, 2.5);
                    let center = cell + vec3<f32>(0.5);
                    let dist = length(p - center); // Euclidean → round, never square
                    let col = star_temp_color(hash13(cell + vec3<f32>(7.7)));
                    // Both paths share the same brightness-driven radius and the
                    // same gain, so toggling AA only softens the edge — it does not
                    // change peak brightness or visible count. Before, the AA path
                    // used a larger radius (up to 0.65 vs 0.5), a heavier gain
                    // (4.0 vs 3.0), and a slow-decaying Gaussian whose long tail
                    // lifted many dim stars above the visibility floor — that's
                    // what made AA look like "bigger and more" stars.
                    let radius = 0.16 + b_raw * 0.42;
                    let falloff = select(
                        smoothstep(radius, 0.0, dist),                 // hard edge
                        exp(-4.6 * dist * dist / (radius * radius)),    // soft edge
                        uniforms.star_aa != 0u);
                    // Bright stars get a soft halo; the bloom pass then lifts
                    // the few heroes into a natural glare.
                    let halo = exp(-9.0 * dist * dist) * b * b * 0.5;
                    let bright = b * falloff + halo;
                    if (bright > best.x) {
                        best = vec4<f32>(bright, col.r, col.g, col.b);
                    }
                }
            }
        }
        acc += best.yzw * best.x * 3.0;
    }

    acc += nebula_color(dir);
    return acc * intensity;
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

// --- approximate ray bending ---
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
    if (y0 * y1 > 0.0 || abs(y0 - y1) < 1e-6) {
        return false;
    }
    let t = y0 / (y0 - y1);
    let cross = mix(prev, cur, vec3<f32>(t));
    let r = length(vec2<f32>(cross.x, cross.z));
    return r >= uniforms.disk_inner && r <= uniforms.disk_outer;
}

fn hash33(p: vec3<f32>) -> vec3<f32> {
    let q = vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    );
    return fract(sin(q) * 43758.5453);
}

// Emissivity and extinction per unit ray length, in disk-local coordinates.
struct DiskSample {
    emission: vec3f,
    extinction: f32,
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

// Cylindrical radius in the disk plane.
fn r_of(pos: vec3<f32>) -> f32 {
    return length(vec2<f32>(pos.x, pos.z));
}

// Kerr equatorial 4-velocity Doppler factor δ = E_obs/E_em (Page & Thorne 1974).
// Exact: solves g_μν u^μ u^ν = -1 for circular equatorial orbits, then folds in
// the conserved photon angular momentum. Returns vec2(delta, beaming=δ^3.5).
// Used by the blackbody disk color mode; the floor guards against NaN from
// pow of a negative base.
fn kerr_doppler(r: f32, pos: vec3f, dir: vec3f) -> vec2f {
    let m = uniforms.rs * 0.5;
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
    let delta = 1.0 / max(0.25, u_t * (1.0 - omega * l_photon));
    let beaming = max(0.01, pow(delta, 3.5));
    return vec2f(delta, beaming);
}

fn disk_color(r: f32, pos: vec3f, dir: vec3f, r_ratio: f32) -> vec3f {
    let span = uniforms.disk_outer - uniforms.disk_inner;
    let radial_t = (r - uniforms.disk_inner) / span;
    let strength = select(0.0, uniforms.doppler_strength, uniforms.doppler_enabled != 0u);
    var shift = vec2f(1.0);
    if (strength > 0.0) {
        shift = kerr_doppler(r, pos, dir);
    }

    if (uniforms.disk_color_mode == 1u) {
        let nt_factor = max(0.0, 1.0 - sqrt(r_ratio));
        let radial_temp = pow(r_ratio, 0.75) * pow(nt_factor, 0.25);
        let temperature = uniforms.disk_temp * radial_temp * mix(1.0, shift.x, strength);
        return blackbody(temperature) * mix(1.0, shift.y, strength);
    }
    if (uniforms.disk_color_mode == 0u) {
        return temperature_color(radial_t) * mix(1.0, shift.y, strength);
    }

    // Display palette with a soft warm-white core. Limit beaming so the
    // receding side remains readable next to the shadow.
    let warm = mix(vec3f(1.0, 0.88, 0.68), vec3f(1.0, 0.31, 0.075),
                   smoothstep(0.0, 0.65, radial_t));
    return warm * mix(1.0, clamp(shift.y, 0.30, 2.5), strength);
}

const FLOW_LIFETIME: f32 = 5.0;
var<private> flow_age: f32;
var<private> flow_epoch: f32;
var<private> flow_blend: f32;

// Periodic angular coordinates keep the texture continuous across atan2's seam.
// The radial coordinate varies faster than azimuth, producing long gas streams.
fn disk_pattern(angle: f32, radial: f32, epoch: f32) -> f32 {
    let along = vec2f(cos(angle), sin(angle));
    let cluster_scale = 0.85 + uniforms.arm_count * 0.62;
    let shape = vec3f(along * cluster_scale, radial + along.x * 0.45 + along.y * 0.25);
    let offset = vec3f(4.2, 1.6, 3.7) * (epoch % 127.0);
    let cloud = vnoise(shape + offset);
    // A drifting, asymmetric bright sector gives the eye something to track
    // even where edge-on rays integrate several separate bands.
    let sector = smoothstep(-0.40, 0.84, along.x * 0.82 + along.y * 0.57);
    var flow = mix(smoothstep(0.24, 0.77, cloud), sector, 0.32);
    if (uniforms.disk_quality >= 2u) {
        let frequency = uniforms.filament_freq;
        let detail = vec3f(shape.xy * 1.78, radial * 2.7) * frequency;
        let ridge = 1.0 - abs(vnoise(detail + offset + vec3f(10.1)) * 2.0 - 1.0);
        flow = mix(flow, flow * pow(ridge, uniforms.filament_sharpness), 0.33);
    }
    return flow;
}

fn disk_flow(pos: vec3f, r: f32) -> f32 {
    // Scale the orbital shear for display, with a soft inner limit to keep
    // the brightest streams visible even when the ISCO shrinks at high spin.
    // One turn takes about 9 s at r=6 and 22 s at r=12.
    let orbit_ratio = 7.8 / (r + 1.8);
    let omega = 0.71 * orbit_ratio * sqrt(orbit_ratio);
    let radial = (r - uniforms.disk_inner) * (0.43 * uniforms.density_freq);
    let angle = atan2(pos.z, pos.x) + uniforms.arm_tightness * 0.45 * log2(r / uniforms.disk_inner);
    let phase = angle - flow_age * omega;
    let current = disk_pattern(phase, radial, flow_epoch);
    if (flow_blend >= 1.0) {
        return current;
    }
    // Let adjacent generations crossfade in disk space so shear does not
    // wind a single texture up into subpixel bands over a long session.
    let previous = disk_pattern(phase - FLOW_LIFETIME * omega, radial, flow_epoch - 1.0);
    return mix(previous, current, flow_blend);
}

fn disk_sample(r: f32, pos: vec3f, dir: vec3f, height_samples: vec2f) -> DiskSample {
    let inner = uniforms.disk_inner;
    let t = (r - inner) / (uniforms.disk_outer - inner);
    let edge = smoothstep(0.0, 0.08 * inner, r - inner) * (1.0 - smoothstep(0.52, 1.0, t));
    let r_ratio = inner / r;
    let radial = r_ratio * r_ratio;
    let height_squared = height_samples * height_samples;
    let absorption_profile = dot(exp(-5.0 * height_squared), vec2f(0.5));
    let flow = disk_flow(pos, r);
    let strands = mix(1.0, mix(0.18, 1.95, flow), uniforms.arm_strength);
    // Emission is concentrated near the midplane and fades before the
    // absorbing layer ends, leaving contrast around the lower lensed arc.
    let emission_profile = dot(exp(-13.0 * height_squared), vec2f(0.65));
    let outer_emission = 1.0 - 0.88 * smoothstep(0.18, 0.72, t);
    let warmth = disk_color(r, pos, dir, r_ratio);
    let emission = warmth * emission_profile * edge * radial * outer_emission
                 * strands * uniforms.disk_brightness;
    let extinction = edge * absorption_profile * mix(0.035, 0.52, radial) * uniforms.density_strength;
    return DiskSample(emission, extinction);
}

// Integrate constant emission and extinction over this sample's path length
// using Beer–Lambert transmittance. At low optical depth the integral tends
// to the full path length, including the transparent (zero extinction) case.
fn composite_disk(sample: DiskSample, path: f32,
                  accum_color: ptr<function, vec3f>, accum_alpha: ptr<function, f32>) {
    let depth = sample.extinction * path;
    let opacity = 1.0 - exp(-depth);
    let integral = select(path, opacity / max(sample.extinction, 1e-6), depth > 1e-3);
    let remaining = 1.0 - *accum_alpha;
    *accum_color += remaining * sample.emission * integral;
    *accum_alpha += remaining * opacity;
}

// Clip each accepted RK45 segment to the local slab before quadrature.
// The material integral then depends on the in-slab length, even as RK45
// changes the size of neighboring steps.
fn integrate_disk_segment(prev: vec3f, new_pos: vec3f, dir: vec3f,
                          accum_color: ptr<function, vec3f>, accum_alpha: ptr<function, f32>) {
    let mid = (prev + new_pos) * 0.5;
    let height_ratio = uniforms.disk_half_thickness;
    let height = r_of(mid) * height_ratio;
    let dy = new_pos.y - prev.y;
    var t0 = 0.0;
    var t1 = 1.0;
    if (abs(dy) < 1e-6) {
        if (abs(prev.y) > height) { return; }
    } else {
        let ta = (height - prev.y) / dy;
        let tb = (-height - prev.y) / dy;
        t0 = clamp(min(ta, tb), 0.0, 1.0);
        t1 = clamp(max(ta, tb), 0.0, 1.0);
        if (t1 <= t0) { return; }
    }

    // Low keeps the broad bands, while High integrates sharper filaments.
    const MAX_SAMPLES = 5u;
    let samples = min(2u + uniforms.disk_quality, MAX_SAMPLES);
    let dt = (t1 - t0) / f32(samples);
    let path = dt * length(new_pos - prev);
    for (var i = 0u; i < MAX_SAMPLES; i += 1u) {
        if (i >= samples || *accum_alpha > 0.99) { break; }
        // Stable stratified offsets suppress coherent spokes without a
        // separate hash at every volume sample.
        let jitter = fract(disk_jitter + f32(i) * 0.618034);
        let t = t0 + dt * (f32(i) + mix(0.13, 0.87, jitter));
        let p = mix(prev, new_pos, t);
        let r = r_of(p);
        if (r <= uniforms.disk_inner || r >= uniforms.disk_outer) { continue; }
        // Integrate the steep vertical profiles at two Gauss–Legendre
        // points. The jittered position still samples the in-plane texture.
        let center = t0 + dt * (f32(i) + 0.5);
        let nodes = vec2f(center) + dt * vec2f(-0.28867513, 0.28867513);
        let heights = (vec2f(prev.y) + dy * nodes) / max(r * height_ratio, 1e-3);
        composite_disk(disk_sample(r, p, dir, heights), path, accum_color, accum_alpha);
    }
}

// Relativistic jets along the spin axis (Y). Bipolar cones above/below the
// disk with 0.92c outflow beaming. Ported from others/' sample_relativistic_jets:
// Gaussian radial falloff, exponential length decay, outward-flowing noise,
// δ^3.5 beaming (no floor needed — β=0.92 keeps the denominator positive).
// Front-to-back composited into the same accumulators as the disk.
//
// `step_len` is the world-space length of THIS RK45 step (|new_pos − prev|),
// passed in by the caller. The per-step emission is weighted by it so the
// integrated jet brightness is step-size-independent — the same technique the
// disk path uses (integrate_disk_segment weights on the analytic in-slab
// length). Two prior attempts both produced banding: `* dt` varied with the
// adaptive step size (radial bands), and a fixed `0.5` made the per-UNIT-length
// contribution ∝ 1/step_len, so it brightened wherever steps packed together
// (near the hole) → the concentric base-of-jet bands. Weighting on the actual
// geometric step length makes the contribution per unit length constant.
fn sample_jets(pos: vec3f, dir: vec3f, r_plus: f32, step_len: f32,
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
    let noise_val = vnoise(uv_jet * 0.5) * 0.6 + vnoise(uv_jet * 1.5) * 0.4;
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
    // Weight the per-step emission by the geometric step length so the
    // integrated brightness is proportional to the path length through the jet,
    // not to how many RK45 steps happened to fall there. The 0.1 coefficient is
    // a per-unit-length strength, calibrated against the default scene to match
    // the old fixed-0.5 brightness at the typical accepted-step length (~0.5).
    let emission = base_color * jet_density * 0.05 * beaming * uniforms.jets_strength * step_len * 0.1;
    *accum_color += emission * (1.0 - *accum_alpha);
    *accum_alpha += jet_density * 0.05 * uniforms.jets_strength * step_len * 0.1;
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
            var col = s.color * (0.2 + 0.8 * ndl);
            if (s.emissive != 0u) { col = s.color; }
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

// One Cash–Karp RK45 step with direction normalization at each stage.
// Advance with the fifth-order weights and estimate the error from the
// distance between the fifth- and fourth-order position estimates.
struct RkStep {
    pos: vec3<f32>,
    dir: vec3<f32>,
    err: f32,
};

fn rk45_step(pos: vec3<f32>, dir: vec3<f32>, dt: f32) -> RkStep {
    // Six-stage Cash–Karp tableau. Each deriv() returns Deriv{dpos, ddir}.
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
    // Fifth-order weights (k2 and k5 have zero weight).
    let new_pos = pos + (k1.dpos * 37.0/378.0 + k3.dpos * 250.0/621.0 + k4.dpos * 125.0/594.0 + k6.dpos * 512.0/1771.0) * dt;
    let new_dir = normalize(dir + (k1.ddir * 37.0/378.0 + k3.ddir * 250.0/621.0 + k4.ddir * 125.0/594.0 + k6.ddir * 512.0/1771.0) * dt);
    // 4th-order solution (for error estimate).
    let pos4 = pos + (k1.dpos * 2825.0/27648.0 + k3.dpos * 18575.0/48384.0 + k4.dpos * 13525.0/55296.0 + k5.dpos * 277.0/14336.0 + k6.dpos * 0.25) * dt;
    let err = length(new_pos - pos4);
    return RkStep(new_pos, new_dir, err);
}

// ====================== main ======================

// One full ray march (camera → capture/escape/budget) for a single ray
// direction, returning the accumulated HDR color. Factored out of `fragment`
// so the supersampling loop can fire several jittered rays per pixel and
// average them. `dir` is the camera-space ray direction; the disk-local
// rotation and all per-ray state live in here so each sample is independent.
fn march(dir: vec3<f32>) -> vec3<f32> {
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

    // Work in disk-local space: rotate eye + dir by -disk_tilt around X so the
    // disk lies on y=0. (disk_hit/disk_color assume disk-local coords.)
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

        if (uniforms.disk_quality == 0u) {
            // Flat tier: a single crossing uses the same moving emissivity.
            if (disk_hit(prev, new_pos)) {
                let ty = prev.y / (prev.y - new_pos.y);
                let hit = mix(prev, new_pos, ty);
                composite_disk(disk_sample(r_of(hit), hit, new_dir, vec2f(0.0)),
                               0.85, &accum_color, &accum_alpha);
            }
        } else {
            integrate_disk_segment(prev, new_pos, new_dir, &accum_color, &accum_alpha);
        }
        if (accum_alpha > 0.99) { break; }

        // --- relativistic jets (along the spin axis) ---
        if (uniforms.jets_enabled != 0u) {
            sample_jets(new_pos, new_dir, r_plus, length(new_pos - prev), &accum_color, &accum_alpha);
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

    return accum_color;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Seed the per-pixel hash used by the slab sub-sample jitter (Fix B) and
    // the supersample jitter (Fix A). @builtin(position) is the fragment's
    // framebuffer coordinate; using it makes the noise spatially stable.
    pixel_seed = in.position.xy;
    disk_jitter = hash13(vec3f(pixel_seed, 0.0));
    let flow_time = uniforms.disk_time + FLOW_LIFETIME * 0.5;
    flow_epoch = floor(flow_time / FLOW_LIFETIME);
    flow_age = flow_time - flow_epoch * FLOW_LIFETIME;
    flow_blend = smoothstep(0.0, FLOW_LIFETIME * 0.44, flow_age);
    // fwidth is legal here (uniform control flow, before the RK45 loop) and
    // gives the true offscreen pixel size regardless of render_scale.
    pixel_size_uv = fwidth(in.uv.y);

    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    // The quad uses top-left UVs, while the ray basis points up in world space.
    let base_uv = vec2f((2.0 * in.uv.x - 1.0) * aspect, 1.0 - 2.0 * in.uv.y);
    let center_ray = ray_direction(base_uv);

    // Supersample around the critical impact parameter. A wide margin covers
    // the displacement and asymmetry at high spin. Rays outside this region
    // contribute the smooth outer disk or sky and need one march per pixel.
    let impact = length(cross(uniforms.eye.xyz, center_ray));
    let near_ring = impact >= 1.1 * uniforms.rs && impact <= 4.5 * uniforms.rs;
    let samples = select(1u, uniforms.aa_samples, near_ring);
    //
    // MAX_AA_SAMPLES is a compile-time cap (WGSL needs static loop bounds);
    // the runtime count comes from uniforms.aa_samples with an early break —
    // the same MAX-with-break form as the disk integration.
    const MAX_AA_SAMPLES = 4u;
    var color_sum = vec3<f32>(0.0);
    for (var s: u32 = 0u; s < MAX_AA_SAMPLES; s = s + 1u) {
        if (s >= samples) { break; }
        var ray = center_ray;
        if (samples > 1u) {
            // Fit the sub-rays to the actual offscreen pixel at any render scale.
            let jitter = hash33(vec3f(pixel_seed, f32(s))).xy - 0.5;
            ray = ray_direction(base_uv + jitter * (2.0 * pixel_size_uv));
        }
        color_sum += march(ray);
    }

    return vec4f(color_sum / f32(samples), 1.0);
}
