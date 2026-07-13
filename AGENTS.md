# AGENTS.md

A real-time Schwarzschild black-hole renderer in Bevy 0.19. One binary, two targets: desktop and web/WebGPU.

## Build & run

- **Desktop:** `cargo run --release` (debug build is too slow to ray-trace; always use `--release` for visual checks).
- **Web** (WebGPU only): `trunk serve` → http://127.0.0.1:8080. First-time setup: `cargo install --locked trunk` and `rustup target add wasm32-unknown-unknown`. Release web build: `trunk build --release`.
- `.cargo/config.toml` sets `--cfg web_sys_unstable_apis` for the `wasm32` target only — required for `web-sys`'s WebGPU bindings. It is a no-op on desktop; do not remove it.
- `edition = "2024"`. No pinned toolchain file; tested on stable 1.96.
- `target/` and `dist/` are gitignored. The `dist/` folder may contain a ~200 MB wasm build locally — never commit it.

## Test

- `cargo test`. There is exactly one testable surface: `src/physics.rs` (inline `#[cfg(test)]`) + `tests/physics_test.rs` (integration test via the `singularity_rs::physics` lib export). `src/lib.rs` exists solely to expose `physics` for these tests.
- The GPU shader is not unit-tested. The whole point of `physics.rs` is to be a CPU mirror that *is* testable.

## Architecture: the CPU ↔ shader mirror

The real renderer is a single full-screen quad running `assets/shaders/black_hole.wgsl` (RK4 geodesic integration + disk/planets/grid/star compositing). `src/physics.rs` is a hand-maintained **CPU mirror** of that integrator, kept only so the capture-vs-escape boundary is unit-testable.

**Changing physics in one place means updating the other.** `bending_accel` / `is_captured` in `physics.rs` must stay in lockstep with the shader's `deriv`/step loop, or the tests will pass on code that the shader contradicts.

Module wiring (entrypoints):
- `main.rs` — app entry, web fallback gate, plugin wiring (`render::BlackHolePlugin`).
- `render/plugin.rs` — the `BlackHolePlugin`: spawns the fullscreen quad + `Camera2d`, mirrors params to the GPU uniform each frame.
- `render/material.rs` — `BlackHoleMaterial` (`Material2d`) + `BlackHoleUniforms` / `SphereData` structs.
- `camera.rs` — orbit controller (yaw/pitch/zoom) + `WantsPointer` (disables orbit over the UI panel).
- `params.rs` — `BlackHoleParams` resource, edited live by the egui panel, mirrored into the material each frame.
- `scene/planets.rs` — `Planet` component + storage-buffer upload.
- `ui.rs` — egui Controls panel.
- `web.rs` — wasm-only: WebGPU detection + fallback message.

## Bevy 0.19 gotchas (cause of the recurring "grey screen")

Three things that silently produce a grey/frozen canvas if broken — recent commits on this branch exist precisely to fix these:

1. **`nudge_camera` (render/plugin.rs)** works around Bevy 0.19 issue #24448: a static `Camera2d` stops rendering after the first frame. It oscillates the camera by a sub-pixel amount each frame. Do not remove it expecting a cleanup.
2. **bevy_egui 0.41** requires UI systems to run in `EguiPrimaryContextPass`, **not** `Update`. Placing `ui_system` in `Update` panics.
3. **The planets storage buffer must be a real `ShaderBuffer` asset**, not `Handle::default()`. A default handle makes `AsBindGroup` return `RetryNextUpdate` every frame, silently skipping the quad's draw — the screen shows only the camera clear color. The quad is pre-filled with a `MAX_PLANETS`-sized zeroed buffer at startup; `upload_planets` updates it.

When debugging a blank/grey screen, check these three before the shader.

## Conventions

- **Natural units: `Rs = 1`** throughout (Rust + WGSL). `BCRIT = 3√3/2·Rs ≈ 2.598` is a literal in `physics.rs` because `f32::sqrt` isn't `const`; the integration test guards the literal.
- **`render_scale` and `spin` are reserved, not wired.** `render_scale` does not map to a real sub-resolution target (README documents this); `spin` is Phase 2 (Kerr). Both carry `#[allow(dead_code)]` deliberately — don't treat them as missing work.
- **`steps` is the real performance/quality lever**, not `render_scale`. Lower it in the Controls panel for FPS.
- **Web defaults differ** via `cfg!(target_arch = "wasm32")`: `steps` 200 (web) vs 300 (desktop), `render_scale` 0.75 vs 1.0.

## Git workflow

- After finishing a change, **decide for yourself whether it should be committed** — don't stop and ask. Commit when the work forms a coherent, complete unit (a fix builds, tests pass, code compiles); hold off only if it's mid-flight or known-broken.
- **Commits are granular and detailed.** Split by concern: one logical change per commit, not one giant dump. The message explains *what* and *why* (the gotcha it fixes, the invariant it restores), not just a restatement of the diff.
- **Never push.** Local commits only; leave pushing to the human.

## Reference docs

- `README.md` — controls, how-it-works, project layout, status.
- `docs/superpowers/specs/2026-07-09-interstellar-blackhole-design.md` — design spec incl. Phase 2 (Kerr) plan.
- `docs/superpowers/plans/2026-07-09-interstellar-blackhole-phase1.md` — Phase 1 implementation plan.

## Skills

This repo has Matt Pocock's engineering skills vendored under `.agents/skills/` and pinned in `skills-lock.json` (e.g. `tdd`, `code-review`, `diagnosing-bugs`, `codebase-design`). They are workspace-scoped and load automatically.
