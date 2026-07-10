const R_ESCAPE: f32 = 1000.0;

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

struct RayResult {
    status: u32,
    final_pos: vec3<f32>,
    final_dir: vec3<f32>,
}

fn classify_ray(start_pos: vec3<f32>, start_dir: vec3<f32>, steps: u32, dt: f32) -> RayResult {
    var pos = start_pos;
    var dir = start_dir;
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let r = length(pos);
        if (r < uniforms.rs) { return RayResult(1u, pos, dir); }
        if (r > R_ESCAPE) { return RayResult(0u, pos, dir); }
        let k1 = deriv(pos, dir);
        let k2 = deriv(pos + k1.dpos * dt * 0.5, normalize(dir + k1.ddir * dt * 0.5));
        let k3 = deriv(pos + k2.dpos * dt * 0.5, normalize(dir + k2.ddir * dt * 0.5));
        let k4 = deriv(pos + k3.dpos * dt,     normalize(dir + k3.ddir * dt));
        pos = pos + (k1.dpos + 2.0 * k2.dpos + 2.0 * k3.dpos + k4.dpos) * dt / 6.0;
        dir = normalize(dir + (k1.ddir + 2.0 * k2.ddir + 2.0 * k3.ddir + k4.ddir) * dt / 6.0);
    }
    return RayResult(0u, pos, dir);
}
