use bevy::prelude::*;
use bevy::window::WindowPlugin;

#[cfg(target_arch = "wasm32")]
mod web;
mod render;
mod camera;
mod params;
mod ui;
mod scene;
mod physics;

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
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "singularity-rs".into(),
                        // On web, make the canvas track the browser window size.
                        fit_canvas_to_parent: true,
                        ..default()
                    }),
                    ..default()
                })
                // The shaders ship as raw `.wgsl` files with no companion
                // `.meta` files. The default `AssetMetaCheck::Always` makes bevy
                // fetch `<path>.meta` for every asset; on the web dev server
                // those requests don't return a clean 404, so bevy receives
                // bytes it tries to RON-deserialize as `AssetMetaMinimal` and
                // logs a deserialization error per shader. `Never` skips the
                // meta lookup entirely and uses the loader's default meta —
                // exactly right for processor-free assets.
                .set(AssetPlugin {
                    meta_check: bevy::asset::AssetMetaCheck::Never,
                    ..default()
                })
                // The app has no audio. The default AudioPlugin opens a WebAudio
                // sink at startup, which browsers block until a user gesture and
                // log as a noisy "AudioContext was not allowed to start" error.
                // Dropping it removes that noise and shrinks the wasm binary.
                .disable::<bevy::audio::AudioPlugin>(),
        )
        .add_plugins(render::BlackHolePlugin)
        .run();
}
