use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::Material2d;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct BlackHoleMaterial {
    #[uniform(0)]
    pub time: f32,
}

impl Material2d for BlackHoleMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/black_hole.wgsl".into()
    }
}
