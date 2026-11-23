# Repository Guidelines

## Project Overview

`singularity-rs` is a Rust 2024 / Bevy 0.20 real-time Kerr black-hole visualizer for native GPUs and browser WebGPU. The GPU shaders perform rendering and ray integration; `src/physics.rs` mirrors critical shader physics on the CPU for UI calculations and deterministic tests.

## Architecture & Data Flow

- `src/main.rs` is deliberately thin: configure platform/window/assets, add `render::BlackHolePlugin`, then run Bevy. Keep module ownership and exports in `src/lib.rs`; do not redeclare file modules in the binary.
- `src/render/plugin.rs` is the composition root. It registers ECS resources/plugins/systems, drives `AppState::Loading -> Running`, creates render targets, and orders the black-hole, bloom, and composite passes.
- Bevy ECS is both state management and dependency injection. Use `Resource` for shared state, `Component` for entity state, system parameters (`Res`, `ResMut`, `Query`, `Commands`, `MessageReader`) for dependencies, and `Local<T>` for system-private UI state. There is no service container or application async runtime.
- Frame flow: egui/input mutate `BlackHoleParams` and `OrbitCamera`; planet systems update entities and a stable GPU storage buffer; `mirror_params` copies CPU state into material uniforms; ordered cameras render black hole -> bloom -> final composite.
- The CPU/GPU boundary is load-bearing. Keep `src/render/material.rs` structs, padding, field order, bindings, and flag types byte-compatible with `assets/shaders/*.wesl`. Preserve CPU/WGSL physics behavior when changing the Kerr integrator.
- Preserve GPU asset handles and mutate assets in place. Replacing bound handles each frame can leave assets perpetually unready.

## Key Directories

- `src/render/`: render plugin, material/uniform definitions, render targets, bloom pipeline.
- `src/scene/`: planet components, deterministic generation/orbits, GPU buffer upload.
- `src/ui/`: egui orchestration, sections, presets, loading UI, shared style.
- `assets/shaders/`: primary ray-marching and post-processing WESL shaders.
- `tests/`: public-library integration tests; private behavior tests stay beside source modules.
- `web/`: Trunk HTML entry and web asset/wasm optimization directives.
- `.github/workflows/`: authoritative CI and tagged release automation.

## Development Commands

```sh
make run          # cargo run --release (native)
make web          # trunk serve at http://127.0.0.1:8080
make test         # cargo test
trunk build --release
```

Match CI before submission:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

## Commit Workflow

Create a separate Git commit for each feature as soon as it is complete.

## Code Conventions & Common Patterns

- Follow standard `rustfmt`; CI treats every Clippy warning as an error. Types/components/resources are nouns; ECS systems use verb names such as `spawn_*`, `resize_*`, `rebuild_*`, `mirror_*`, and `upload_*`; UI groups use `section_*`.
- Prefer pure calculation helpers beside thin ECS systems, as in `OrbitCamera::basis`, `physics.rs`, and planet orbit helpers.
- Model domain choices as small `Copy` enums with `Default` and explicit shader conversions (`as_u32`, `levels`, `samples`). WGSL-facing flags are `u32`, not booleans.
- Express ordering through Bevy schedules, state run conditions, and `.before(...)`. Structural changes use deferred `Commands`; do not call systems manually in runtime code.
- Handle temporarily missing entities/assets with early-return `let Ok(...) else { return; }` or `if let`. No project-wide custom error abstraction exists.
- Guard expensive structural work with state/dirty comparisons. Planet generation is deterministic and rebuilds only when dirty; bloom changes rebuild the pipeline.
- Local `#[allow(clippy::too_many_arguments, clippy::type_complexity)]` is acceptable for inherently broad ECS system parameters; do not hide them behind bespoke context objects.
- Treat comments about alignment/bindings, camera order/render layers, stable handles, and Bevy workarounds as invariants. Keep the main shader’s ray integration and shading logic in one WESL file.
- Camera sliders bound to auto-mutated `OrbitCamera` fields must use `SliderClamping::Edits`; default writeback can round values and stall auto-orbit (`CONTEXT.md`).

## Important Files

- `src/main.rs`: native/wasm entry point and platform setup.
- `src/lib.rs`: canonical module root and public API.
- `src/render/plugin.rs`: application/render lifecycle and system scheduling.
- `src/render/material.rs`: Rust side of shader uniform/storage contracts.
- `src/params.rs`: central runtime parameters and quality defaults.
- `src/state.rs`: loading/running state and shader asset collection.
- `src/camera.rs`: orbit camera state, input, and auto-orbit.
- `src/physics.rs`: CPU mirror of shader physics.
- `Cargo.toml` / `Cargo.lock`: single-package manifest and committed resolution.
- `.cargo/config.toml`: required wasm-only `web_sys_unstable_apis` cfg.
- `Trunk.toml` / `web/index.html`: web entry, Binaryen compatibility, wasm-opt flags, and asset copying.
- `Makefile`: local development commands and production Web image build target.
- `Dockerfile`: production Web image used by the release workflow.

## Runtime/Tooling Preferences

- This is one Cargo package, not a workspace: explicit library `singularity_rs` plus implicit binary `singularity-rs`.
- Use stable Rust with edition 2024. No exact toolchain is pinned. Keep `Cargo.lock` generated, committed, and unedited by hand.
- Web development requires `wasm32-unknown-unknown`, Trunk, WebGPU, Binaryen/`wasm-opt`, and the target rustflags in `.cargo/config.toml`. `web/index.html` must retain both `data-wasm-opt-params` and the `copy-dir` for `assets/`; removing either can produce validation errors or shader 404s.
- Native release builds embed assets; debug builds load WESL from disk for rapid shader iteration.
- Linux native builds need `pkg-config`, `libasound2-dev`, and `libudev-dev`.
- No `scripts/`, `build.rs`, custom rustfmt/clippy configuration, or Node/Bun package manager is used.

## Testing & QA

- Use Rust's built-in `#[test]` harness. Put private helper/system tests in `#[cfg(test)] mod tests` beside their module; put public contracts in `tests/*_test.rs` through the `singularity_rs` API.
- Existing integration suites: `tests/physics_test.rs` and `tests/preset_test.rs`. Inline examples exist in `src/physics.rs`, `src/camera.rs`, `src/scene/planets.rs`, and `src/ui/sections.rs`.
- Keep tests deterministic and headless. For Bevy systems, build a minimal real `World` and run the system; for egui, use a real `Context`; avoid mocks when existing in-memory patterns suffice.
- Physics tests should assert boundaries, monotonicity, degeneration/equivalence, and justified floating-point tolerances rather than exact floats.
- There is no coverage threshold, benchmark suite, snapshot framework, fixture hierarchy, or GPU shader execution harness. Do not invent one merely for consistency.
