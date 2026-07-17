// Lib crate root. The bin (main.rs) consumes these via `singularity_rs::...`
// rather than redeclaring the modules itself; otherwise both crate roots
// would try to own the same file-level modules (compile error: E0428/
// E0152). This is why every module the binary needs lives here.
pub mod camera;
pub mod params;
pub mod physics;
pub mod render;
pub mod scene;
pub mod ui;
#[cfg(target_arch = "wasm32")]
pub mod web;
