#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct BrightPassUniform {
    threshold: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: BrightPassUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var samp: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(tex, samp, in.uv).rgb;
    let lum = dot(hdr, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Soft knee: smooth roll-off instead of a hard threshold cut.
    // Fully-bright passes through; near-threshold tapers to zero.
    let soft = max(lum - u.threshold, 0.0) / (lum + 0.0001);
    let contribution = hdr * soft;
    return vec4<f32>(contribution, 1.0);
}
