// Disk plane is the xz-plane in world space, tilted by `disk_tilt` around the
// x-axis. We work in "disk-local" coordinates by rotating the ray.
#define_import_path singularity::disk

// Rotate a vector around the X axis by angle a.
fn rot_x(v: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
}

// Returns true if the segment pos->pos+dir*dt crosses the disk plane (y=0)
// within radius [disk_inner, disk_outer]. (prev, cur are the segment endpoints.)
fn disk_hit(prev: vec3<f32>, cur: vec3<f32>) -> bool {
    let y0 = prev.y;
    let y1 = cur.y;
    if (y0 * y1 > 0.0) {
        return false; // same side, no crossing
    }
    // Linear interpolate to the crossing point.
    let t = y0 / (y0 - y1);
    let cross = mix(prev, cur, vec3<f32>(t));
    let r = length(vec2<f32>(cross.x, cross.z));
    return r >= uniforms.disk_inner && r <= uniforms.disk_outer;
}

// Shade a disk hit: procedural texture + Doppler beaming + temperature color.
fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let r = length(vec2<f32>(pos.x, pos.z));
    let phi = atan2(pos.z, pos.x);

    // Procedural noise: layered angular + radial, animated by rotation.
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    let n = sin(phi * 8.0 + rot) * 0.5 + 0.5;
    let n2 = sin(phi * 23.0 - rot * 1.7 + r * 2.0) * 0.5 + 0.5;
    let noise = mix(n, n2, 0.4);

    // Temperature gradient: hotter (white-blue) near inner edge, cooler (orange-red) outer.
    let t = (r - uniforms.disk_inner) / (uniforms.disk_outer - uniforms.disk_inner);
    let tcol = mix(vec3<f32>(1.0, 0.95, 0.85), vec3<f32>(1.0, 0.45, 0.12), clamp(t, 0.0, 1.0));

    // Falloff: brighter at inner edge.
    let falloff = 1.0 / pow(r / uniforms.disk_inner, 2.0);

    var col = tcol * (0.6 + 0.4 * noise) * falloff;

    // Doppler beaming. Disk orbits Keplerian-ish: v ~ sqrt(Rs/(2r)).
    let v_orbital = sqrt(uniforms.rs / (2.0 * r));
    // Orbital velocity direction (tangent) in the disk plane.
    let tangent = normalize(vec3<f32>(-sin(phi), 0.0, cos(phi)));
    // Scalar approximation: projection of orbital velocity onto ray direction.
    let vdotn = dot(tangent * v_orbital, -dir); // toward viewer if positive
    let gamma = 1.0 / sqrt(max(1.0 - v_orbital * v_orbital, 1e-4));
    var doppler = 1.0;
    if (uniforms.doppler_enabled != 0u) {
        let delta = 1.0 / (gamma * (1.0 - vdotn));
        doppler = pow(delta, 3.0) * uniforms.doppler_strength;
    }
    col *= doppler;

    return col * uniforms.disk_brightness;
}
