use bevy::prelude::*;
use bevy::window::WindowPlugin;

// The bin consumes the lib crate's modules; it does NOT redeclare them.
// src/lib.rs owns every file-level module (camera/params/physics/render/
// scene/ui/web), so we import what main() needs from the lib root.
use singularity_rs::render::BlackHolePlugin;
#[cfg(target_arch = "wasm32")]
use singularity_rs::web;
// Embeds the `assets/` tree into the binary in release builds so the shipped
// executable is self-contained. The plugin is added under
// `#[cfg(not(debug_assertions))]` in `main()`; see the Cargo.toml comment.
#[cfg(not(debug_assertions))]
use bevy_embedded_assets::{EmbeddedAssetPlugin, PluginMode};

fn main() {
    // On web, abort startup if WebGPU isn't available and show a message.
    #[cfg(target_arch = "wasm32")]
    {
        if !web::webgpu_available() {
            web::show_fallback_message();
            return;
        }
    }

    // Release builds embed the entire `assets/` tree into the binary via
    // `bevy_embedded_assets` so the shipped executable is fully self-contained
    // — no `assets/` folder needs to sit next to it (the cause of the
    // "Path not found: .../assets/shaders/*.wgsl" errors when running the bare
    // binary). `ReplaceAndFallback` keeps the same default asset paths the
    // materials already use (`"shaders/..."`) and only falls back to disk if an
    // asset isn't embedded, so no load-site changes are needed.
    //
    // Debug builds deliberately skip embedding: editing a `.wgsl` and re-running
    // `cargo run` then reflects immediately without recompiling Rust.
    let default_plugins = DefaultPlugins
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
        });

    // In release, prepend the embedder so it replaces the default file source
    // with the embedded one (falling back to disk for anything not embedded).
    // Both the plugin group (debug) and the `(Plugin, group)` tuple (release)
    // implement `Plugins`, so one `add_plugins` call covers both.
    #[cfg(not(debug_assertions))]
    let default_plugins = (
        EmbeddedAssetPlugin {
            mode: PluginMode::ReplaceAndFallback {
                path: "assets".to_owned(),
            },
        },
        default_plugins,
    );

    App::new()
        .add_plugins(default_plugins)
        .add_plugins(BlackHolePlugin)
        .run();
}
