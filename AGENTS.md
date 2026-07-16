# AGENTS.md

A real-time Kerr (spinning) black-hole renderer in Bevy 0.19. One binary, two targets: desktop and web/WebGPU. Spin χ = 0 degenerates exactly to Schwarzschild.

## Build & run

- **Desktop:** `cargo run --release` (debug is too slow to ray-trace; always use `--release` for visual checks).
- **Web** (WebGPU only): `trunk serve` → http://127.0.0.1:8080. First-time setup: `cargo install --locked trunk` and `rustup target add wasm32-unknown-unknown`. Release web build: `trunk build --release`.
- `.cargo/config.toml` sets `--cfg web_sys_unstable_apis` for the `wasm32` target only — required for `web-sys`'s WebGPU bindings. It is a no-op on desktop; do not remove it.
- `edition = "2024"`. No pinned toolchain file; tested on stable 1.96.
- Release profiles (`Cargo.toml`): desktop `[profile.release]` uses `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`. Web `[profile.wasm-release]` inherits release and overrides `opt-level = "z"` for binary size. `trunk build --release` picks up `wasm-release`.
- `target/` and `dist/` are gitignored. `dist/` may hold a ~200 MB wasm build locally — never commit it.

## Test

- `cargo test`. There is exactly one testable surface: `src/physics.rs` (inline `#[cfg(test)]`) + `tests/physics_test.rs` (integration test via the `singularity_rs::physics` lib export). `src/lib.rs` (`pub mod physics;`) exists solely to expose `physics` for these tests.
- The GPU shader is not unit-tested. The whole point of `physics.rs` is to be a CPU mirror that _is_ testable.
- Run a single test: `cargo test rk45_capture_radius_shrinks_with_spin` (substring match works).

## Architecture: the CPU ↔ shader mirror

The renderer is a **multi-stage HDR pipeline** of full-screen quads, each a `Material2d` writing into an offscreen `Rgba16Float` `Image`:

1. **Black-hole quad** (`assets/shaders/black_hole.wgsl`) — Kerr geodesic integration via an adaptive Dormand-Prince RK45 loop + disk/planets/grid/star/jets compositing. Renders at sub-resolution (`render_scale`) into the offscreen target.
2. **Bright-pass** (`brightpass.wgsl`) → **blur pyramid** (`blur.wgsl`, 2 down + 2 up) → **composite** (`composite.wgsl`): bloom extraction, pyramid blur, then ACES tone-map of scene + bloom to the window's LDR surface.

Seven `Camera2d` entities are spawned at startup (offscreen + brightpass + 4 blur + composite), ordered by `Camera.order` from -20 (offscreen) up to the composite/window camera. Bloom is gated by `BloomQuality` (Off on web default, High on desktop); when Off the composite samples a 1×1 black texture (scene-only ACES). `rebuild_bloom` despawns/respawns the whole bloom sub-graph when the quality tier changes.

`src/physics.rs` is a hand-maintained **CPU mirror** of stage 1's integrator, kept so the capture-vs-escape boundary is unit-testable on the CPU.

**Changing physics in one place means updating the other.** `bending_accel` / `kerr_bending_accel` / `rk45_step` / `is_captured` / `is_captured_rk45` in `physics.rs` must stay in lockstep with the shader's `deriv` / `rk45_step` / integration `loop`, or tests pass on code the shader contradicts. The mirror covers: the single-step Kerr derivative (`kerr_bending_accel` ↔ `deriv`), the adaptive step (`rk45_step` ↔ shader `rk45_step`), and the full loop (`is_captured_rk45` ↔ the capture `loop`). The loop-level invariants that must match: budget = accepted-steps-only, the `dt_min` forced-accept floor, and `r₊(χ)` as the capture radius. (Shader line numbers drift as the file grows — locate these by function name: `deriv`, `rk45_step`, and the main `loop`.)

Module wiring (entrypoints):

- `main.rs` — app entry, web fallback gate, plugin wiring (`render::BlackHolePlugin`). See web gotchas below for the two non-obvious `DefaultPlugins` overrides.
- `render/plugin.rs` — `BlackHolePlugin`: spawns the offscreen quad + bloom pipeline + composite camera, `mirror_params` (params → GPU uniform each frame), `resize_offscreen`, `nudge_camera`, `rebuild_bloom`.
- `render/material.rs` — the four `Material2d`s: `BlackHoleMaterial` (+ `BlackHoleUniforms` / `SphereData`), `BrightPassMaterial`, `BlurMaterial`, `CompositeMaterial`.
- `camera.rs` — orbit controller (yaw/pitch/zoom) + `WantsPointer` (disables orbit over the UI panel).
- `params.rs` — `BlackHoleParams` resource + quality-tier enums (`BloomQuality`, `DiskQuality`, `DiskColorMode`, `AaQuality`), edited live by the egui panel, mirrored into the material each frame.
- `scene/planets.rs` — `Planet` component + storage-buffer upload.
- `ui.rs` — egui Controls panel.
- `web.rs` — wasm-only: WebGPU detection + fallback message.

## Bevy 0.19 gotchas (cause of the recurring "grey screen" / crash)

Silent failure modes — recent commits exist solely to fix these. Check them before the shader when debugging a blank/grey/crashing canvas:

1. **`nudge_camera` (render/plugin.rs)** works around Bevy 0.19 issue #24448: a static `Camera2d` stops rendering after the first frame. It oscillates every `Nudgable` camera by a sub-pixel amount each frame. Do not remove it expecting a cleanup — the offscreen camera freezing makes the composite re-sample a stale texture (frozen view).
2. **bevy_egui 0.41 requires UI systems to run in `EguiPrimaryContextPass`, not `Update`.** Placing `ui_system` in `Update` panics.
3. **`PrimaryEguiContext` must be explicitly pinned to the composite (window) camera**, with `disable_egui_auto_context` turning off bevy_egui's auto-assignment in `PreStartup`. Auto-assignment gives `PrimaryEguiContext` to the _first spawned_ camera — the offscreen one, whose `Rgba16Float` render target mismatches egui's `Rgba8UnormSrgb` pipeline → **format-mismatch crash**. This is load-bearing now that the bloom pipeline spawns 7 cameras.
4. **The planets storage buffer must be a real `ShaderBuffer` asset**, not `Handle::default()`. A default handle makes `AsBindGroup` return `RetryNextUpdate` every frame, silently skipping the quad's draw — the screen shows only the camera clear color. The quad is pre-filled with a `MAX_PLANETS`-sized zeroed buffer at startup; `upload_planets` updates it.
5. **The skybox texture binding must declare `dimension = "cube"`** (`render/material.rs`). The `AsBindGroup` derive defaults to D2; the shader declares `texture_cube<f32>`, so a D2 layout makes the pipeline fail to specialize and the quad silently draws nothing. When no cubemap is set, Bevy binds its 1×1 cube fallback (gated out by `skybox_intensity > 0` anyway).

## Web gotchas

- `main.rs` sets `AssetPlugin { meta_check: AssetMetaCheck::Never }`: shaders ship as raw `.wgsl` with no `.meta` companions. The default `Always` fetches `<path>.meta` per asset; the trunk dev server doesn't 404 cleanly, so bevy tries to RON-deserialize the returned bytes and logs a deserialization error per shader.
- `main.rs` disables `bevy::audio::AudioPlugin`: the app has no audio, and the default plugin opens a WebAudio sink that browsers block until a user gesture, logging a noisy "AudioContext was not allowed to start" error.
- The skybox must be sampled with `textureSampleLevel` (not `textureSample`) in the shader — Tint (the WGSL→-SPIR-V compiler used on web) enforces uniform-control-flow rules that `textureSample` violates, failing the web build. See commit `5c0b1db`.
- `Trunk.toml` copies `assets/` (shaders) into `dist/` via `copy-dir`. If you add a new shader, it is picked up automatically because the whole `assets/` tree is copied.

## Conventions

- **Natural units: `Rs = 1`** throughout (Rust + WGSL). `BCRIT = 3√3/2·Rs ≈ 2.598` is a literal in `physics.rs` because `f32::sqrt` isn't `const`; the integration test guards the literal.
- **`disk_inner` is spin-derived, not read from its param field.** `mirror_params` overwrites `uniforms.disk_inner` with `kerr_isco(params.spin)` every frame; `params.disk_inner` exists only for the default/UI readout and is ignored at runtime. The disk inner edge tracks the Kerr ISCO (6M = 3 Rs at χ = 0, shrinking to M = Rs/2 at extremal spin).
- **`spin` (χ ∈ [0,1])** drives the Kerr frame-dragging term in the shader's `deriv` and a spin-dependent capture radius (`r₊`); at χ = 0 the frame-dragging term vanishes and the integrator is exactly Schwarzschild.
- **`steps` and `render_scale` are performance levers.** `steps` caps _accepted_ RK45 steps per ray; `render_scale` lowers the offscreen resolution. The RK45 integrator is ~an order of magnitude costlier than fixed-step RK4, so the default `render_scale` is 0.75 (desktop) / 0.5 (web).
- **Quality tiers** (`params.rs`) gate per-pixel work and default differently per target via `cfg!(target_arch = "wasm32")`: `steps` 200 (web) vs 300 (desktop); `render_scale` 0.5 vs 0.75; `bloom_quality` Low vs High; `disk_quality` Low vs High; `aa_quality` Off vs Low.
- **Mirroring a new param end-to-end** touches four places in lockstep: `BlackHoleParams` (`params.rs`) → `BlackHoleUniforms` (`render/material.rs`, mind WGSL `vec3`-alignment padding) → `mirror_params` assignment (`render/plugin.rs`) → the shader struct + its use. Quality-tier enums also need a UI entry in `ui.rs`.

## Git workflow

- After finishing a change, **decide for yourself whether it should be committed** — don't stop and ask. Commit when the work forms a coherent, complete unit (builds, tests pass, code compiles); hold off only if it's mid-flight or known-broken.
- **Commits are granular and detailed.** Split by concern: one logical change per commit, not one giant dump. The message explains _what_ and _why_ (the gotcha it fixes, the invariant it restores), not just a restatement of the diff.
- **Never push.** Local commits only; leave pushing to the human.

## Reference docs

- `README.md` — controls, how-it-works, project layout, status.
- `docs/superpowers/specs/` + `docs/superpowers/plans/` — phased design specs and implementation plans:
  - Phase 1 (Schwarzschild): `2026-07-09-interstellar-blackhole-*.md`
  - Phase 2 (Kerr): `2026-07-13-interstellar-blackhole-phase2-kerr-*.md`
  - Phase 3 (cinematic: HDR/bloom/tone-map): `2026-07-14-blackhole-cinematic-rendering-*.md`
  - Phase 3.1 (volumetric disk): `2026-07-15-volumetric-disk-*.md`

## Skills

Matt Pocock's engineering skills are vendored under `.agents/skills/` and pinned in `skills-lock.json` (e.g. `tdd`, `code-review`, `diagnosing-bugs`, `codebase-design`). They are workspace-scoped and load automatically.
