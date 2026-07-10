#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> time: f32;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Gradient from time, just to prove the uniform flows.
    let t = (sin(time) + 1.0) * 0.5;
    let col = mix(vec3<f32>(0.02, 0.02, 0.05), vec3<f32>(0.08, 0.04, 0.12), t);
    return vec4<f32>(col, 1.0);
}
