#define_import_path singularity::skybox

@group(#{MATERIAL_BIND_GROUP}) @binding(1) var skybox: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var skybox_sampler: sampler;

// Sample the cubemap along a world-space direction. Caller gates on skybox_intensity.
fn skybox_color(dir: vec3<f32>) -> vec3<f32> {
    return textureSample(skybox, skybox_sampler, dir).rgb;
}
