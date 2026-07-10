const R_ESCAPE: f32 = 1000.0;

// One RK4 sub-step derivative of (pos, dir) under the Schwarzschild
// bending acceleration. Rs is uniforms.rs.
fn deriv(pos: vec3<f32>, dir: vec3<f32>) -> (vec3<f32>, vec3<f32>) {
    let r = length(pos);
    let rs = uniforms.rs;
    // Angular momentum squared: |cross(pos, dir)|^2
    let h = cross(pos, dir);
    let h2 = dot(h, h);
    // Avoid division by zero.
    let r5 = max(r * r * r * r * r, 1e-6);
    // d(pos)/dt = dir
    let dpos = dir;
    // d(dir)/dt = bending acceleration (re-normalized each step in integrate)
    let accel = -1.5 * rs * h2 / r5 * pos;
    return (dpos, accel);
}

// Integrate a ray from `pos` along `dir`. Returns:
//   .status: 0 = escaped, 1 = captured (shadow)
//   .final_pos, .final_dir: end state (used for sky sampling on escape)
struct RayResult {
    status: u32,
    final_pos: vec3<f32>,
    final_dir: vec3<f32>,
}

// Accumulator callback pattern: the caller passes a function-style body via
// a per-step check. Because WGSL has no first-class closures, we inline the
// per-step intersection tests in black_hole.wgsl's integrate_and_trace().
// This function returns ONLY the escape/capture classification, used as a
// fallback when no scene object is hit.
fn classify_ray(start_pos: vec3<f32>, start_dir: vec3<f32>, steps: u32, dt: f32) -> RayResult {
    var pos = start_pos;
    var dir = start_dir;
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let r = length(pos);
        if (r < uniforms.rs) {
            return RayResult(1u, pos, dir);
        }
        if (r > R_ESCAPE) {
            return RayResult(0u, pos, dir);
        }
        // RK4
        let (k1p, k1d) = deriv(pos, dir);
        let (k2p, k2d) = deriv(pos + k1p * dt * 0.5, normalize(dir + k1d * dt * 0.5));
        let (k3p, k3d) = deriv(pos + k2p * dt * 0.5, normalize(dir + k2d * dt * 0.5));
        let (k4p, k4d) = deriv(pos + k3p * dt,     normalize(dir + k3d * dt));
        pos = pos + (k1p + 2.0 * k2p + 2.0 * k3p + k4p) * dt / 6.0;
        dir = normalize(dir + (k1d + 2.0 * k2d + 2.0 * k3d + k4d) * dt / 6.0);
    }
    // Ran out of steps without a clear verdict: treat as escaped.
    return RayResult(0u, pos, dir);
}
