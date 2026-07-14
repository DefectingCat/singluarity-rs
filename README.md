# singularity-rs

A real-time, physically-motivated **black-hole renderer** in [Bevy](https://bevyengine.org) 0.19. Each pixel geodesic-ray-traces curved spacetime, producing gravitational lensing, the Einstein ring, a Doppler-beamed accretion disk, a lensed starfield, and optional lensed planets, a spacetime-curvature (Flamm) grid, and a cubemap skybox. Runs on **desktop** and the **web** (WebGPU).

The visual target is Gargantua from *Interstellar*: a black shadow surrounded by a tilted, glowing accretion disk whose back side is lensed up and over the hole (and down underneath), with one side brighter from relativistic Doppler beaming.

The integrator is a spinning (Kerr) geodesic: a dimensionless spin parameter χ ∈ [0,1] drives frame-dragging (Lense-Thirring) asymmetry and pulls the disk's inner edge inward along the Kerr ISCO. At χ = 0 it degenerates exactly to the Schwarzschild (non-spinning) case.

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
  - **Black Hole** — spin (χ), with live ISCO (disk inner edge) and horizon (r+) readouts.
  - **Accretion Disk** — outer radius, tilt, brightness, rotation speed (inner radius is spin-derived ISCO).
  - **Doppler** — enable + strength of the relativistic beaming asymmetry.
  - **Renderer** — integrator step count (quality/perf lever) and render scale (sub-resolution offscreen target).
  - **Background** — procedural star intensity, optional cubemap skybox intensity.
  - **Grid** — toggle the lensed Flamm-paraboloid curvature grid + density.

Orbit input is automatically disabled while the cursor is over the panel.

## How it works

A single full-screen quad carries a custom `Material2d` whose fragment shader, for every pixel:

1. Generates a primary ray from the camera basis + FOV.
2. Integrates the ray through Kerr spacetime with an **adaptive Dormand-Prince RK45** loop, applying the discretized bending acceleration `a = -1.5·Rs·h²/r⁵ · pos + 2·M·a/r³ · (spin_axis × dir)` (`h` = angular momentum, `a = χ·M` the Kerr spin length). At χ = 0 the frame-dragging term vanishes and this is exactly the Schwarzschild bending.
3. At each accepted step tests the bent segment against the accretion disk (equatorial plane), lensed planets (storage buffer), and the Flamm paraboloid grid surface — compositing hits front-to-back. Rejected steps (error above tolerance, `dt` still above its floor) retry at a smaller step without consuming the ray's step budget.
4. Terminates on capture (`r < r₊(χ)`, the spin-dependent horizon; at χ = 0 this is `Rs`, and the shadow emerges naturally at the critical impact parameter `b_crit = 3√3/2·Rs ≈ 2.598`) or escape (samples the procedural starfield / cubemap along the bent final direction).

The disk is tilted relative to the camera, so the back of the disk is lensed over the top and under the bottom of the shadow — the characteristic "halo." Doppler beaming brightens the approaching side. At spin > 0 the disk's inner edge tracks the Kerr ISCO (shrinking from 3 Rs at χ = 0 toward Rs/2 at extremal spin) and frame-dragging shears the halo off the line-of-sight axis.

The CPU-side physics (`src/physics.rs`) mirrors the integrator — both the single-step Kerr derivative and the adaptive RK45 loop with its spin-dependent capture radius — and is unit-tested: `b < b_crit` rays are captured, `b > b_crit` rays escape, spin = 0 degenerates to Schwarzschild, and higher spin does not enlarge the capture set.

## Performance

Defaults target ~60 FPS on a discrete/integrated GPU (developed on Apple M4). The Kerr + adaptive RK45 integrator costs roughly an order of magnitude more per pixel than fixed-step RK4, so the default `render_scale` is 0.75 on desktop (the quad renders into a sub-resolution offscreen target that is then upscaled to the window). On web, defaults drop to `steps=200` and `render_scale=0.5` for interactivity. If you need more FPS, lower **Steps** or **Render scale** in the Controls panel.

## Project layout

```
src/
  main.rs            app entry, plugin wiring
  camera.rs          orbit controller (yaw/pitch/zoom) + WantsPointer
  params.rs          BlackHoleParams (tunable, mirrored to GPU each frame)
  physics.rs         CPU mirror of the geodesic integrator: Kerr deriv, adaptive RK45 loop, ISCO/horizon (unit-tested)
  ui.rs              egui Controls panel (collapsible sections)
  web.rs             wasm glue: WebGPU detection + fallback message
  scene/planets.rs   Planet component + storage-buffer upload
  render/            BlackHolePlugin, material, offscreen + upscale cameras (render_scale)
assets/shaders/      WGSL: ray gen, Kerr RK45, disk, stars, planets, grid, skybox, upscale blit
docs/superpowers/    design spec + implementation plan
```

## Status

**Phase 1 (Schwarzschild)** — shipped: shadow, tilted Doppler accretion disk with lensed Einstein halo, lensed starfield, lensed planets, lensed Flamm grid, optional cubemap skybox, live egui controls, desktop + web/WebGPU.

**Phase 2 (Kerr)** — **implemented, pending visual/performance validation.** The Schwarzschild fixed-step RK4 integrator was replaced by a Kerr geodesic with an adaptive Dormand-Prince RK45 loop: a spin parameter χ ∈ [0,1] drives frame-dragging and a spin-dependent capture radius (Kerr horizon), the disk inner edge tracks the Kerr ISCO, and `render_scale` is now wired through an offscreen render-to-texture + upscale pass. The CPU mirror (`src/physics.rs`) covers the Kerr derivative, the RK45 step, and the adaptive loop, and the unit tests assert the spin = 0 degeneracy plus capture/escape behavior at spin > 0. What remains is the human-in-the-loop checklist: spin = 0 visual regression against Phase 1, frame-dragging asymmetry at spin > 0, and desktop/web FPS at the new defaults — see `docs/superpowers/plans/2026-07-13-interstellar-blackhole-phase2-kerr.md` (Task 8).

**Phase 3 (future work).** Full exact Kerr Cartesian pseudo-Hamiltonian (Σ/Δ/Carter-separable form) for sub-percent photon-orbit accuracy at high spin, retrograde spin, tilted spin axis, and adaptive integrator *order*. See `docs/superpowers/specs/2026-07-13-interstellar-blackhole-phase2-kerr-design.md` §9.
