use bevy::prelude::*;
use bevy::window::WindowPlugin;

#[cfg(target_arch = "wasm32")]
mod web;
mod render;
mod camera;
mod params;
mod ui;

fn main() {
    // On web, abort startup if WebGPU isn't available and show a message.
    #[cfg(target_arch = "wasm32")]
    {
        if !web::webgpu_available() {
            web::show_fallback_message();
            return;
        }
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "singularity-rs".into(),
                // On web, make the canvas track the browser window size.
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(render::BlackHolePlugin)
        .run();
}
