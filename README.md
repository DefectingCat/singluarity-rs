# singularity-rs

A real-time, physically-motivated **black-hole renderer** in [Bevy](https://bevyengine.org) 0.20. It traces a bent ray per pixel to produce gravitational lensing, the Einstein ring, a Doppler-beamed accretion disk, a lensed starfield, and optional lensed planets, a spacetime-curvature (Flamm) grid, and a cubemap skybox. Runs on **desktop** and the **web** (WebGPU).

The opening shot is inspired by Gargantua from _Interstellar_. Over 1.8 seconds the camera moves from 60 to 22 Schwarzschild radii and lowers from 21° to 7° above the accretion disk. Its 48° field of view stays fixed along the shorter viewport axis. Drag or scroll to take over at any point. Warm foreground gas and two lensed arcs frame the shadow. Bright clumps move and shear around the disk, with faster flow near the inner edge.

The ray model combines Schwarzschild bending with a leading-order Lense–Thirring frame-dragging term. The dimensionless spin χ ∈ [0,1] controls this term and sets the disk's inner edge via the prograde Kerr ISCO. At χ = 0 the frame-dragging term vanishes.

The model is intended for visual demonstrations. Quantitative estimates of shadow size or spin effects require independent benchmarks against solutions of the full Kerr null-geodesic equations. [Gralla and Lupsasca, *The Null Geodesics of the Kerr Exterior*](https://arxiv.org/abs/1910.12881)

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

- **Drag** (mouse) — orbit the camera around the hole. Release while moving to coast briefly.
- **Scroll** — zoom (changes distance / impact parameter).
- **Controls** (top right, or **H**) opens the inspector.
  - **Camera** — adjust distance, yaw, pitch, and short-axis field of view. **Play flyby** starts a ten-second approach from distance 44 to 22. Drag or scroll in the scene to interrupt it. **Auto orbit** is on by default and rotates the camera while idle.
  - **Scene** — spin (χ), ISCO and horizon readouts, disk geometry and color. **Flow speed** controls the gas pattern rate, and `0×` freezes it in place.
  - **Render** — ray steps, render scale, exposure, antialiasing, and bloom.
  - **Effects** — bright clumps, fine filaments, absorption, Doppler beaming, jets, planets, starfield, skybox and grid. The opening shot keeps the planets and jets off.

Orbit input is automatically disabled while the cursor is over the panel.

## How it works

A single full-screen quad carries a custom `Material2d` whose fragment shader, for every pixel:

1. Generates a primary ray from the camera basis + FOV.
2. Advances the ray with **adaptive Cash–Karp RK45** using the approximate acceleration `accel = -1.5·Rs·h²/r⁵ · pos + 2·M·a/r³ · (spin_axis × dir)`, where `h = |pos × dir|` and `a = χ·M`. The shader normalizes the direction at each stage and uses the distance between the fifth- and fourth-order position estimates to control the step size. [GSL's Cash–Karp coefficient table](https://github.com/ampl/gsl/blob/master/ode-initval2/rkck.c#L33-L59)
3. Shortens steps near the inner disk as spin increases so the straight segments used for shading follow the bent ray. Each accepted segment composites the accretion disk, lensed planets and grid front-to-back. Rejected steps retry at a smaller step size without consuming the ray's budget.
4. On capture, clips the last segment to its first intersection with the spin-dependent horizon `r₊(χ)` and keeps the foreground light up to that point. Escaping rays sample the procedural starfield and cubemap along their final direction.

The foreground disk and both lensed arcs sample the same orbiting clumps, each with its own lifetime and radial position. The volume integral samples the disk vertically at two points per subsegment and uses separate light emission and absorption profiles. The default Cinematic model gives the disk a dense core with irregular golden streaks and a faint, broader atmosphere. The core attenuates background starlight, with lower opacity in the dark lanes and toward the outer edge. The Blackbody mode exposes the temperature and Doppler shift of the disk. At spin > 0 the disk's inner edge tracks the Kerr ISCO (shrinking from 3 Rs at χ = 0 toward Rs/2 at extremal spin) and frame-dragging shears the halo off the line-of-sight axis.

The CPU-side physics (`src/physics.rs`) mirrors the bending acceleration and the adaptive loop. Tests cover a reference Cash–Karp step, zero-spin equivalence to the Schwarzschild bending term, and representative capture and escape cases.

## Performance

Desktop uses `render_scale=0.75` and Low ring antialiasing, with two rays per pixel around the lensed rings and four closest to the shadow. The Web default uses `steps=200`, `render_scale=0.5`, and one ray per pixel. Its Low disk quality keeps the moving clumps, and Low bloom uses a single highlight pass. Lower **Ray steps** or **Resolution scale** for higher frame rates on slower GPUs.

## Project layout

```
src/
  main.rs            app entry, plugin wiring
  camera.rs          orbit controller (yaw/pitch/zoom) + WantsPointer
  params.rs          BlackHoleParams (tunable, mirrored to GPU each frame)
  physics.rs         CPU mirror of ray bending and adaptive Cash–Karp RK45, plus Kerr ISCO/horizon
  ui.rs              egui Controls panel (collapsible sections)
  web.rs             wasm glue: WebGPU detection + fallback message
  scene/planets.rs   Planet component + storage-buffer upload
  render/            BlackHolePlugin, material, offscreen + upscale cameras (render_scale)
assets/shaders/      WESL for ray generation, integration, shading, and upscale blit
docs/superpowers/    design spec + implementation plan
```
