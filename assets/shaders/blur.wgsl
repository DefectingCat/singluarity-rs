#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct BlurUniform {
    mode: u32,           // 0 = downsample, 1 = upsample
    texel_size: vec2<f32>,
    blend: f32,          // upsample blend factor (ignored for down)
    _pad0: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: BlurUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var samp: sampler;

// 13-tap weighted kernel (Gaussian approximation for HDR bloom).
// NOTE: naga rejects dynamic indexing of const arrays ("may only be indexed
// by a constant"), so we use var<private> here — it allows runtime indexing.
var<private> KERNEL_OFFSETS: array<vec2<f32>, 13> = array<vec2<f32>, 13>(
    vec2<f32>( 0.0,  0.0),
    vec2<f32>( 1.0,  0.0), vec2<f32>(-1.0,  0.0),
    vec2<f32>( 0.0,  1.0), vec2<f32>( 0.0, -1.0),
    vec2<f32>( 1.0,  1.0), vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0, -1.0), vec2<f32>(-1.0, -1.0),
    vec2<f32>( 2.0,  0.0), vec2<f32>(-2.0,  0.0),
    vec2<f32>( 0.0,  2.0), vec2<f32>( 0.0, -2.0),
);
var<private> KERNEL_WEIGHTS: array<f32, 13> = array<f32, 13>(
    0.5,
    0.25, 0.25, 0.25, 0.25,
    0.125, 0.125, 0.125, 0.125,
    0.0625, 0.0625, 0.0625, 0.0625,
);

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    // Downsample and upsample share the same 13-tap weighted kernel.
    // mode 0 = down (plain weighted average), mode 1 = up (scaled by blend).
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i: i32 = 0; i < 13; i = i + 1) {
        let off = KERNEL_OFFSETS[i] * u.texel_size;
        let c = textureSample(tex, samp, uv + off).rgb;
        sum = sum + c * KERNEL_WEIGHTS[i];
        wsum = wsum + KERNEL_WEIGHTS[i];
    }
    let blurred = sum / wsum;
    if (u.mode == 0u) {
        return vec4<f32>(blurred, 1.0);
    } else {
        return vec4<f32>(blurred * u.blend, 1.0);
    }
}
