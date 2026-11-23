import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;

struct BrightPassUniform {
    threshold: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> u: BrightPassUniform;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var tex: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var samp: sampler;

fn extract_highlight(hdr: vec3f) -> vec3f {
    let lum = dot(hdr, vec3<f32>(0.2126, 0.7152, 0.0722));
    let knee = max(u.threshold * 0.45, 1e-4);
    let above = lum - u.threshold;
    let ramp = clamp(above + knee, 0.0, 2.0 * knee);
    let soft = ramp * ramp / (4.0 * knee);
    return hdr * (max(above, soft) / max(lum, 1e-4));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // A little optical spread at half resolution gives Low a soft highlight
    // halo with only this pass. Higher tiers broaden it with the blur pyramid.
    let step = 1.5 / vec2f(textureDimensions(tex));
    let dx = vec2f(step.x, 0.0);
    let dy = vec2f(0.0, step.y);
    let center = extract_highlight(textureSample(tex, samp, in.uv).rgb);
    let spread = extract_highlight(textureSample(tex, samp, in.uv + dx).rgb)
               + extract_highlight(textureSample(tex, samp, in.uv - dx).rgb)
               + extract_highlight(textureSample(tex, samp, in.uv + dy).rgb)
               + extract_highlight(textureSample(tex, samp, in.uv - dy).rgb);
    return vec4f(0.5 * center + 0.125 * spread, 1.0);
}
