# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] - 2026-08-21

### Added

- Idle auto-orbit camera behavior: rotates yaw on a fixed 45-second period, pauses on camera or UI input, and resumes after 1.5 seconds idle; toggle exposed in the Camera panel alongside the existing manual orbit/zoom controls.
- WebGPU Docker dev mode (`docker compose -f docker-compose.dev.yml watch`) with Compose Watch hot-reload for `src`, `assets`, `web`, and `Trunk.toml`, plus a Dev Containers config (`.devcontainer/devcontainer.json`).
- `Makefile` with a `docker-dev` target and a production web Docker image build target.

### Changed

- Rewrote the procedural starfield shader for a deep-space look: layered faint/bright star populations with power-law magnitude distributions, blackbody-ish stellar colors, sub-pixel flux-preserving falloff, and a domain-warped Milky Way dust-lane backdrop.

### Fixed

- Fixed Camera-panel sliders fighting the auto-orbit/zoom controllers ("stars shaking, no rotation"): egui `Slider`s bound directly to the live `OrbitCamera` resource snapped values back to their rounding grid every idle frame, eating sub-grid per-frame orbit/zoom/drag deltas; switched to `SliderClamping::Edits` so sliders clamp only on real user edits.
- Fixed starfield strobing during camera motion: sub-pixel stars (undersized further because the pixel footprint used window resolution instead of the offscreen render-scale resolution) now enforce a 3px minimum on-screen size with flux preservation.
- Fixed mouse-wheel zoom exploding to the distance clamp or jittering back and forth on macOS trackpads: `MouseScrollUnit::Pixel` deltas were treated as line deltas, and the `distance/(1 + y*0.1)` formula had a singularity at `y <= -10`; replaced with a unit-aware, singularity-free multiplicative zoom (`1.1^(-lines)`).
- Migrated `Trunk.toml`'s deprecated `address` field to the `addresses` array.

### CI/CD

- Fixed Clippy failures from the egui 0.35 upgrade (`Context::run` renamed to `run_ui`) and allowed `too_many_arguments` on `orbit_controller`, consistent with other multi-param systems in `render/plugin.rs` and `ui/mod.rs`.
- Configured domestic (Tsinghua/rustup/cargo) mirrors and pre-installed `wasm-bindgen-cli`/`binaryen` in the Docker dev image, and eliminated Compose Watch bind-mount warnings.

## [0.2.1] - 2026-08-03

### Performance

- Pruned unused default Bevy engine features (`bevy_gilrs`, `bevy_gltf`, `bevy_scene`, `bevy_pbr`, `bevy_audio`, etc.) in `Cargo.toml`, significantly shrinking the WebAssembly binary size.

### Fixed

- Fixed desktop release workflow packaging paths by ensuring `--target` triple is specified for `cargo build` and `cargo packager` (`release.yml`).
- Fixed WebAssembly build pipeline and `wasm-opt` configuration for compatibility with Binaryen 131 and bulk-memory features.
- Added missing `DomTokenList` feature flag to `web-sys` dependency for WebGPU canvas handling.

### CI/CD

- Added automated container deployment for WebGPU builds to production server on tag push.
- Updated CI workflows to use `taiki-e/install-action` for `cargo-packager` installation.

## [0.2.0] - 2026-07-31

### Added

- Complete UI panel redesign featuring a collapsible right side-panel chassis, sci-fi deep-space cyan/orange styling, global quality presets (Default, High Quality, Performance, Cinematic, Custom), and toggle shortcut (`Ctrl/Cmd+H`).
- Asset loading state management via `bevy_asset_loader` and `AppState`, with an in-engine egui HUD progress card during initial loading.
- Web HTML/CSS preloader HUD with smooth DOM fade-out once WebGPU rendering is initialized.
- Embedded release assets using `bevy_embedded_assets` for single self-contained executable desktop distribution.
- Application packaging configuration for `cargo-packager` with platform icons and binary metadata.
- Automated GitHub Actions CI workflow (formatting, clippy, tests, desktop & WebGPU builds) and release deployment pipeline (`release.yml`).

### Fixed

- Fixed mouse scroll-wheel passthrough to orbit camera when interacting with egui UI panels.
- Resolved code style and clippy lints for Rust 2024 edition compliance.

## [0.1.0] - 2026-07-31

### Added

- Real-time Kerr (spinning) black-hole renderer in Bevy 0.19 with Schwarzschild degeneration at spin $\chi = 0$.
- Multi-stage HDR pipeline: sub-resolution offscreen black hole quad, bright-pass filter, 4-stage blur pyramid, and ACES tone mapping.
- CPU ↔ WGSL shader mirror for RK45 geodesic integration with automated unit and integration tests.
- Volumetric accretion disk with blackbody radiation color spectrum and Doppler / gravitational redshift.
- Keplerian planet orbits with Lense-Thirring nodal precession and relativistic light-bending shadow/lensing.
- Interactive egui control panel with instant parameter adjustments, quality presets, and orbit camera navigation.
- Desktop (LTO fat profile) and Web (WebGPU / WebAssembly via Trunk) dual-target support.
