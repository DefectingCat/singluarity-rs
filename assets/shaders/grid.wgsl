// Flamm's paraboloid embedding: z(r) = 2*sqrt(Rs*(r - Rs)), opens DOWNWARD
// (negative y in disk-local space). Dips below the disk toward the center —
// the classic gravity-well visualization. Traced through curved spacetime, so
// grid lines near the hole bend dramatically.
#define_import_path singularity::grid

fn flamm_depth(r: f32) -> f32 {
    if (r <= uniforms.rs) { return 0.0; }
    return -2.0 * sqrt(uniforms.rs * (r - uniforms.rs));
}

// Returns additive grid color if the segment prev->cur crosses the Flamm
// paraboloid surface; returns black otherwise. `prev`/`cur` are disk-local.
fn grid_hit(prev: vec3<f32>, cur: vec3<f32>) -> vec3<f32> {
    // Sample the paraboloid at the segment endpoints; if the segment crosses it,
    // find an approximate crossing by sampling.
    let r0 = length(vec2<f32>(prev.x, prev.z));
    let r1 = length(vec2<f32>(cur.x, cur.z));
    let z0_surf = flamm_depth(r0);
    let z1_surf = flamm_depth(r1);
    // Did the ray's y cross the surface y between endpoints?
    if ((prev.y - z0_surf) * (cur.y - z1_surf) > 0.0) {
        return vec3<f32>(0.0);
    }
    // Crossing: linear-search for the crossing point.
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

    // Polar grid pattern from (r, phi).
    let r = length(vec2<f32>(hit.x, hit.z));
    let phi = atan2(hit.z, hit.x);
    let ring = smoothstep(0.06, 0.0, abs(fract(r * uniforms.grid_density * 0.5) - 0.5));
    let spoke = smoothstep(0.04, 0.0, abs(fract(phi * 6.0 / 6.283185) - 0.5));
    let grid = max(ring, spoke);
    // Fade with depth so the grid reads as "below" the hole.
    let fade = smoothstep(-15.0, -1.0, hit.y);
    let col = vec3<f32>(0.15, 0.3, 0.6) * grid * fade;
    return col * 0.5; // additive, low intensity
}
