# singularity-rs

A real-time, physically-motivated **Schwarzschild black-hole renderer** in [Bevy](https://bevyengine.org) 0.19. Each pixel geodesic-ray-traces curved spacetime, producing gravitational lensing, the Einstein ring, a Doppler-beamed accretion disk, a lensed starfield, and optional lensed planets, a spacetime-curvature (Flamm) grid, and a cubemap skybox. Runs on **desktop** and the **web** (WebGPU).

The visual target is Gargantua from *Interstellar*: a black shadow surrounded by a tilted, glowing accretion disk whose back side is lensed up and over the hole (and down underneath), with one side brighter from relativistic Doppler beaming.

## Run

**Desktop** (Vulkan / Metal / D3D12):
```sh
cargo run --release
```

**Web** (WebGPU — Chrome, Edge, recent Firefox/Safari):
```sh
cargo install --locked trunk
rustup target add wasm32-unknown-unknown
trunk serve
# open http://127.0.0.1:8080
```
Release web build: `trunk build --release`. On a browser without WebGPU, the page shows a fallback message instead of a blank canvas.

## Controls

- **Drag** (mouse) — orbit the camera around the hole.
- **Scroll** — zoom (changes distance / impact parameter).
- **Controls panel** (top-left) — live-tune every parameter:
  - **Camera** — distance, yaw, pitch, FOV.
  - **Accretion Disk** — inner/outer radius, tilt, brightness, rotation speed.
  - **Doppler** — enable + strength of the relativistic beaming asymmetry.
  - **Renderer** — integrator step count (the main quality/perf lever).
  - **Background** — procedural star intensity, optional cubemap skybox intensity.
  - **Grid** — toggle the lensed Flamm-paraboloid curvature grid + density.

Orbit input is automatically disabled while the cursor is over the panel.

## How it works

A single full-screen quad carries a custom `Material2d` whose fragment shader, for every pixel:

1. Generates a primary ray from the camera basis + FOV.
2. Integrates the ray through Schwarzschild spacetime with RK4, applying the discretized bending acceleration `a = -1.5·Rs·h²/r⁵ · pos` (`h` = angular momentum).
3. At each step tests the bent segment against the accretion disk (equatorial plane), lensed planets (storage buffer), and the Flamm paraboloid grid surface — compositing hits front-to-back.
4. Terminates on capture (`r < Rs`, the shadow emerges naturally at the critical impact parameter `b_crit = 3√3/2·Rs ≈ 2.598`) or escape (samples the procedural starfield / cubemap along the bent final direction).

The disk is tilted relative to the camera, so the back of the disk is lensed over the top and under the bottom of the shadow — the characteristic "halo." Doppler beaming brightens the approaching side.

The CPU-side physics (`src/physics.rs`) mirrors the integrator and is unit-tested: `b < b_crit` rays are captured, `b > b_crit` rays escape.

## Performance

Defaults target ~60 FPS at full resolution on a discrete/integrated GPU (developed on Apple M4). On web, defaults drop to `steps=200` for interactivity. If you need more FPS, lower **Steps** in the Controls panel.

Note: the `render_scale` parameter exists but is **not wired to a real sub-resolution render target in Phase 1** (the full-screen quad always renders at window resolution). It is reserved for future work; **Steps** is the real performance lever today.

## Project layout

```
src/
  main.rs            app entry, plugin wiring
  camera.rs          orbit controller (yaw/pitch/zoom) + WantsPointer
  params.rs          BlackHoleParams (tunable, mirrored to GPU each frame)
  physics.rs         CPU mirror of the geodesic integrator (unit-tested)
  ui.rs              egui Controls panel (collapsible sections)
  web.rs             wasm glue: WebGPU detection + fallback message
  scene/planets.rs   Planet component + storage-buffer upload
  render/            BlackHolePlugin, material, fullscreen quad
assets/shaders/      WGSL: ray gen, Schwarzschild RK4, disk, stars, planets, grid, skybox
docs/superpowers/    design spec + implementation plan
```

## Status

**Phase 1 (Schwarzschild, this codebase)** — complete: shadow, tilted Doppler accretion disk with lensed Einstein halo, lensed starfield, lensed planets, lensed Flamm grid, optional cubemap skybox, live egui controls, desktop + web/WebGPU.

**Phase 2 (Kerr / true Gargatua) — future work.** Replace the Schwarzschild integrator with the Kerr metric (Boyer-Lindquist, adaptive RK4, spin parameter) for frame-dragging and ergosphere asymmetry. Same scene elements, camera, params, and UI carry over; only the integrator core swaps. See `docs/superpowers/specs/2026-07-09-interstellar-blackhole-design.md`.
