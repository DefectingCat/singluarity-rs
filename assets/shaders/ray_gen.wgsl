// Builds a world-space camera ray direction for the current pixel.
// `uv` is the pixel coordinate normalized to [-1,1] with aspect correction.
fn ray_direction(uv: vec2<f32>) -> vec3<f32> {
    // NOTE: `fov` is packed into the `.w` of `up` in BlackHoleUniforms
    // (the Rust struct lays out `up: Vec3` + `fov: f32` as one vec4 block).
    // The WGSL struct must stay exactly as-is per the task spec, so read fov
    // from `uniforms.up.w` rather than a separate `uniforms.fov` field.
    let tan_half_fov = tan(uniforms.up.w * 0.5);
    let dir =
        normalize(uniforms.forward.xyz)
        + uniforms.right.xyz * (uv.x * tan_half_fov)
        + uniforms.up.xyz    * (uv.y * tan_half_fov);
    return normalize(dir);
}
