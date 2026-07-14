#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct CompositeUniform {
    bloom_strength: f32,
    exposure: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: CompositeUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var scene_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var scene_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var bloom_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var bloom_samp: sampler;

// ACES Narkowicz fit (5 ops, clamped to [0,1]).
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(scene_tex, scene_samp, in.uv).rgb;
    let bloom = textureSample(bloom_tex, bloom_samp, in.uv).rgb;
    let combined = hdr + bloom * u.bloom_strength;
    let mapped = aces_tonemap(combined * u.exposure);
    return vec4<f32>(mapped, 1.0);
}
