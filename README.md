# singularity-rs

A real-time, physically-motivated **black-hole renderer** in [Bevy](https://bevyengine.org) 0.20. It traces a bent ray per pixel to produce gravitational lensing, the Einstein ring, a Doppler-beamed accretion disk, a lensed starfield, and optional lensed planets, a spacetime-curvature (Flamm) grid, and a cubemap skybox. Runs on **desktop** and the **web** (WebGPU).

The opening shot is inspired by Gargantua from _Interstellar_. The camera holds just above the accretion disk, with warm foreground gas and two lensed arcs around the shadow. Bright gas streams shear as they move around the disk, with faster flow near the inner edge.

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

- **Drag** (mouse) — orbit the camera around the hole.
- **Scroll** — zoom (changes distance / impact parameter).
- **Controls** (top right, or **H**) opens the inspector.
  - **Camera** — adjust distance, yaw, pitch, and short-axis field of view. **Play flyby** starts a ten-second approach from distance 44 to 22. Drag or scroll in the scene to interrupt it. **Auto orbit** enables continuous orbit.
  - **Scene** — spin (χ), ISCO and horizon readouts, disk geometry and color. **Flow speed** controls the gas pattern rate, and `0×` freezes it in place.
  - **Render** — ray steps, render scale, exposure, antialiasing, and bloom.
  - **Effects** — gas bands, fine filaments, absorption, Doppler beaming, jets, planets, starfield, skybox and grid. The opening shot keeps the planets and jets off.

Orbit input is automatically disabled while the cursor is over the panel.

## How it works

A single full-screen quad carries a custom `Material2d` whose fragment shader, for every pixel:

1. Generates a primary ray from the camera basis + FOV.
2. Advances the ray with **adaptive Cash–Karp RK45** using the approximate acceleration `accel = -1.5·Rs·h²/r⁵ · pos + 2·M·a/r³ · (spin_axis × dir)`, where `h = |pos × dir|` and `a = χ·M`. The shader normalizes the direction at each stage and uses the distance between the fifth- and fourth-order position estimates to control the step size. [GSL's Cash–Karp coefficient table](https://github.com/ampl/gsl/blob/master/ode-initval2/rkck.c#L33-L59)
3. At each accepted step tests the bent segment against the accretion disk (equatorial plane), lensed planets (storage buffer), and the Flamm paraboloid grid surface — compositing hits front-to-back. Rejected steps (error above tolerance, `dt` still above its floor) retry at a smaller step without consuming the ray's step budget.
4. Terminates on capture (`r < r₊(χ)`, the spin-dependent horizon) or escape (samples the procedural starfield / cubemap along the bent final direction).

The front band and both lensed arcs sample the same disk-space flow. The volume integral treats light emission and absorption separately. The default Cinematic color model uses a warm inner layer, amber edges and restrained beaming. At spin > 0 the disk's inner edge tracks the Kerr ISCO (shrinking from 3 Rs at χ = 0 toward Rs/2 at extremal spin) and frame-dragging shears the halo off the line-of-sight axis.

The CPU-side physics (`src/physics.rs`) mirrors the bending acceleration and the adaptive loop. Tests cover a reference Cash–Karp step, zero-spin equivalence to the Schwarzschild bending term, and representative capture and escape cases.

## Performance

Desktop uses `render_scale=0.75` and concentrates extra rays near the lensed ring. The Web default uses `steps=200`, `render_scale=0.5`, and one ray per pixel. Its Low disk quality keeps the main moving bands, and Low bloom uses a single highlight pass. Lower **Ray steps** or **Resolution scale** for higher frame rates on slower GPUs.

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
