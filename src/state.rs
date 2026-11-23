use bevy::prelude::*;
use bevy_asset_loader::prelude::*;

/// Global application state machine.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// Initial asset loading, WESL shader parsing, and pipeline warmup.
    #[default]
    Loading,
    /// Main interactive black-hole rendering loop.
    Running,
}

/// Asset collection managed by `bevy_asset_loader`.
/// Ensures all 4 WESL shaders are loaded before transitioning states.
#[derive(AssetCollection, Resource)]
pub struct ShaderAssets {
    #[asset(path = "shaders/black_hole.wesl")]
    pub black_hole: Handle<Shader>,
    #[asset(path = "shaders/brightpass.wesl")]
    pub brightpass: Handle<Shader>,
    #[asset(path = "shaders/blur.wesl")]
    pub blur: Handle<Shader>,
    #[asset(path = "shaders/composite.wesl")]
    pub composite: Handle<Shader>,
}

/// Resource for tracking loading progress and status message for UI rendering.
#[derive(Resource)]
pub struct LoadingProgress {
    /// Raw progress value [0.0, 1.0].
    pub progress: f32,
    /// Smoothly interpolated progress value for rendering.
    pub display_progress: f32,
    /// Current status label displayed on the HUD card.
    pub status: String,
    /// Number of GPU warmup frames rendered.
    pub warmup_frames: u32,
}

impl Default for LoadingProgress {
    fn default() -> Self {
        Self {
            progress: 0.0,
            display_progress: 0.0,
            status: "Loading Shaders...".to_string(),
            warmup_frames: 0,
        }
    }
}
