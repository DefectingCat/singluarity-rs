# singularity-rs — Interstellar-style Black Hole Renderer (Design Spec)

**Date:** 2026-07-09
**Status:** Draft (awaiting user review)
**Project:** `singularity-rs` (Rust, edition 2024, Bevy 0.19)

## 0. Dependencies

```toml
[dependencies]
bevy = "0.19"        # engine (first release on Rust edition 2024)
bevy_egui = "0.41"   # UI panel; depends on bevy ^0.19 + egui ^0.35
# (rand/noise for procedural stars & disk texture — chosen in plan phase)
```

Bevy feature flags: default. No heavy features needed (no 3D scene, no audio). The full-screen `Material2d` path uses Bevy's core pipeline only.

## 1. Goal

A real-time, physically-motivated renderer of a Gargantua-style black hole in Bevy, matching the visual reference video (`5u1ymibkiixg1.mp4`): a black shadow, a tilted glowing accretion disk with the lensed halo wrapping over and under the hole, a Doppler brightness asymmetry, and a lensed background starfield — all produced by geodesic ray-tracing curved spacetime per pixel.

The scope is deliberately larger than the reference video: it additionally includes a lensed spacetime-curvature (gravity-well) grid, lensed scene-planet spheres, and a cubemap skybox layer. The renderer is delivered in two phases: a fully-working Schwarzschild (non-spinning) renderer first, then an upgrade to the Kerr (spinning) metric for true Gargantua accuracy.

## 2. Reference video analysis

30 frames were extracted (1/sec) from `5u1ymibkiixg1.mp4` (800×432, 10fps, 30s) and analyzed. Findings that constrain this design:

| Feature | Present in video | Notes |
|---|---|---|
| Black hole shadow | Yes | Dark circular region, stays centered |
| Accretion disk | Yes | Orange/red-hot; tilted relative to camera orbit |
| Lensed halo (over + under hole) | Yes | The classic wrap-over/wrap-under Einstein ring |
| Doppler asymmetry | Yes | One side clearly brighter/hotter; bright side shifts as camera orbits |
| Background stars | Yes | Visible in dark regions, distorted near the hole |
| Gravity-well grid | No | Not present (but in scope per user decision) |
| Planets / scene objects | No | Not present (but in scope per user decision) |
| Cubemap skybox | No | Background is stars only (procedural) |
| UI / HUD | No | Clean frame |

**Camera motion in video:** smooth continuous orbit around the hole while slowly zooming; disk is tilted relative to the orbit plane, so both the disk face and the lensed halo are visible throughout.

**Visual style target:** photorealistic, Gargantua-like — not stylized/cartoonish.

## 3. Non-goals (Phase 1)

- Kerr metric / frame dragging (Phase 2).
- Volumetric rendering of the disk (it is a thin emitting surface).
- Audio, gameplay, networking.
- Loading external mesh assets (everything is procedural or simple spheres).
- A real-time editable GUI editor (parameters are tuned via a debug params resource + keyboard; an egui panel is optional later).

## 4. Physics & units

Natural units with the Schwarzschild radius **Rs = 1** everywhere in the shader. This makes the characteristic radii clean and numerically stable:

| Feature | Radius (Rs units) |
|---|---|
| Event horizon | 1.0 |
| Photon sphere | 1.5 |
| Shadow (critical impact parameter bcrit = 3√3/2 · Rs) | ≈ 2.598 |
| ISCO (accretion disk inner edge) | 3.0 |
| Accretion disk outer edge (tunable) | ~12–20 |

### Geodesic integration (Schwarzschild)

For each pixel, generate a photon ray (position `pos`, direction `dir`) and integrate the geodesic. The standard discretized form used across well-known real-time black-hole demos (mholub "Basic Black Hole Rendering"; rantonels *starless*):

```
per step:
  r  = length(pos)
  h2 = |cross(pos, dir)|^2          // squared angular momentum
  a  = -1.5 * Rs * h2 / r^5 * pos   // geodesic bending acceleration (Rs=1)
  dir = normalize(dir + a*dt)
  pos = pos + dir*dt
```

Integrator: **RK4** (4th-order Runge-Kutta) with a tunable step count (default ~300, range 150–600). RK4 is chosen over Euler because Euler requires far more steps for the same accuracy near the photon sphere and smears the Einstein ring.

Termination conditions for a ray (checked each step):
1. `r < Rs` → captured by horizon → return pure black (this is the shadow; it emerges naturally at bcrit).
2. `r > R_escape` (e.g. 1000) → escaped → sample skybox + procedural stars along final direction.
3. Disk plane intersection within `[r_in, r_out]` → shade disk, **continue** integrating. A ray typically hits the disk multiple times (front pass, then wrapped over/under the hole) — each hit contributes an emissive color. Composite front-to-back by accumulating `(color·α)` and `(1−α)`; stop accumulating once the running alpha saturates near 1. This is what produces the over/under halo and higher-order images.
4. Planet sphere intersection (within current step segment) → shade nearest planet, composite front-to-back; terminate the ray once alpha saturates (opaque planets occlude everything behind).
5. Gravity-well (Flamm) surface intersection → shade grid (additive, low alpha), continue.

**Critical for secondary images:** the loop does NOT terminate when a ray merely crosses the photon sphere and comes back. It runs the full step budget unless the accumulated alpha saturates (opaque hit) or the ray is captured/escapes. This is what produces secondary/higher-order Einstein images as in Luminet's paper.

**Integrator form note:** the discretized pseudo-acceleration shown above (Cartesian `(pos, dir)` with the effective bending acceleration) is one valid real-time approximation. An alternative is the integrable Binet form `d²u/dφ² = (3/2)·Rs·u² − 1/(2Rs)` in `u=1/r`, which is more accurate but harder to combine with the off-plane disk/grid/planet intersection tests. The exact choice is a Phase-1 implementation decision in the plan; either way the *behavior* (lensing, shadow at bcrit, wrap halo) is identical.

## 5. Scene elements

### 5.1 Accretion disk
- Thin emitting ring in the equatorial plane (rotated by a configurable disk tilt).
- `r ∈ [r_in=3, r_out=15]` (tunable).
- Intersection: detect sign change of the disk-plane coordinate across an integration step; solve for the crossing point; accept only if `r ∈ [r_in, r_out]`.
- Procedural texture: layered radial + angular noise (hash-based, time-animated to simulate rotation). Brightness falls off near `r_in` (inner edge hottest) and `r_out`.
- Color temperature gradient: hotter (white/blue) near `r_in`, cooler (orange/red) near `r_out`.
- **Doppler beaming:** at each disk hit, compute the disk orbital velocity (relativistic Keplerian, `v = sqrt(Rs/(2r)) / sqrt(1 - Rs/r)` capped), the Doppler factor `δ = 1/(γ(1 − β·n̂))` relative to the ray direction, and scale intensity by `δ³` (and shift hue slightly blue for δ>1, red for δ<1). This produces the one-side-bright asymmetry seen in the reference.

### 5.2 Background skybox (cubemap) + procedural stars
- Optional cubemap texture bound to the material. When a ray escapes, sample the cubemap along its final direction. Procedural stars are layered on top as an additive detail (hash-based points on the unit sphere) with an intensity uniform.
- If no cubemap is provided, procedural stars are the sole background (matches the reference video).
- Because rays bend, the background (cubemap or stars) is naturally lensed: stars near the hole smear into arcs and can form secondary images.

### 5.3 Planets (scene entities)
- A `Planet` component: `Transform` (center), `radius`, `color`, `emissive` flag.
- A system collects all `Planet` entities each frame into a `Vec<SphereData>` and uploads to a storage buffer bound to the material (`@group(1) var<storage, read> planets: array<SphereData>`).
- In the shader, each integration step tests all spheres for ray-segment intersection; the nearest hit wins and is shaded (Lambert-ish with the color, optionally emissive). Planets are therefore fully lensed: a planet near the hole bends into an arc and can show secondary images.
- The storage buffer also carries a count, and the loop is bounded by a `MAX_PLANETS` constant (e.g. 32) for unrolled/loop performance.

### 5.4 Spacetime-curvature grid (Flamm paraboloid)
- The classic embedding surface `z(r) = 2·sqrt(Rs·(r − Rs))` (dips down toward the center), oriented in the disk plane.
- Ray-traced in the shader: each step tests intersection with the paraboloid surface. At a hit, apply a polar grid pattern from the (embedded) `(r, φ)`: bright rings at chosen radius intervals, radial spokes at chosen angle intervals. Fade with depth (z) so it reads as "below" the hole.
- The grid is **lensed**: because it is traced through curved spacetime, grid lines bend dramatically near the hole and can wrap. Partially transparent (additive) so it never fully occludes the disk.
- This feature is **off by default** (the reference video does not show it); toggled via a parameter.

## 6. Bevy rendering architecture

- The scene is 100% procedural (no 3D meshes to composite over). The black hole is drawn by a **single full-screen quad** with a custom `Material2d`, whose fragment shader performs all ray-tracing. No scene-color texture is read, so a full-screen `Material2d` is simpler than a render-graph post-processing node.
- The "camera" is just a set of material uniforms (eye position, forward/right/up vectors, FOV, aspect) updated each frame from the orbit controller.
- `Material2d` puts our bind group at **group 1** (group 0 is Bevy's view uniforms). WGSL bindings:
  - `@group(1) @binding(0) var<uniform> params: BlackHoleParams;`
  - `@group(1) @binding(1) var<uniform> camera: CameraParams;`
  - `@group(1) @binding(2) var skybox: texture_cube<f32>;` + `@group(1) @binding(3) var skybox_sampler;`
  - `@group(1) @binding(4) var<storage, read> planets: array<SphereData>;` (+ count in params)
- Render-scale support: the quad is drawn at `render_scale` (0.5–1.0) of the window and upscaled, needed for the Phase 2 Kerr integrator. Phase 1 targets 1.0 at 60fps.
- **UI compositing:** `bevy_egui` (v0.41.0, depends on Bevy `^0.19` + egui `^0.35`) renders the control panel on top of the black-hole quad via its own render pass. Mouse input is routed to the egui panel when the cursor is over it, otherwise to the orbit controller (see §7).

## 7. Camera & interaction

Orbit controller around the origin:
- **Drag** (mouse) → yaw/pitch the camera around the hole (ignored while the cursor is over the egui panel).
- **Wheel** → change camera distance (= impact parameter / how close to the hole); consumed by egui when over the panel.
- Parameters are **live-tuned via an egui panel** (no keyboard hotkeys required). The panel reads/writes the `BlackHoleParams` `Resource`, and a system mirrors that resource into the material's uniform each frame.

Default scene (matches the reference): camera at moderate distance, disk tilted ~70–80° from face-on (so the lensed halo is prominent), Doppler enabled, grid off, procedural stars on.

## 7.5. Parameter UI (egui)

A single **collapsible** egui window docked to a screen corner (top-left), titled "Controls", with grouped `egui::CollapsingHeader` sections so the user can fold sections away or hide the whole window to admire the view:

- **Camera** — orbit distance (slider), yaw/pitch (sliders; also driven by drag), FOV, reset button.
- **Accretion Disk** — inner radius, outer radius, tilt, brightness, rotation speed.
- **Doppler** — enable checkbox, intensity slider.
- **Renderer** — integrator step count (slider 100–600), render scale (0.5–1.0), show FPS.
- **Background** — procedural star intensity, skybox load/clear, skybox intensity.
- **Grid** — enable checkbox (off by default), line density, depth fade, color.
- **Planets** — add/remove a test planet, edit the active planet's radius/color/position.

Every control mutates `BlackHoleParams` (or the relevant component) in place; the mirror system propagates it to the GPU uniform, so changes appear within one frame. Sliders show live numeric values. `bevy_egui` `EguiPlugin` is added to the app; a `ui_system` in `Update` builds the window each frame from the `Resource`.

## 8. File structure

## 8. File structure

```
src/
  main.rs              # App setup, plugin registration, default scene
  params.rs            # BlackHoleParams resource (CPU mirror of uniforms)
  camera.rs            # Orbit camera input controller
  ui.rs                # egui "Controls" panel (collapsible sections)
  scene/
    mod.rs
    disk.rs            # Disk parameters & defaults
    planets.rs         # Planet component + storage-buffer upload system
  render/
    plugin.rs          # Full-screen quad + camera + material setup
    material.rs        # BlackHoleMaterial: AsBindGroup (uniforms, cubemap, storage buffer)
assets/shaders/
  black_hole.wgsl      # Entry point: ray gen → integrate → composite
  ray_gen.wgsl         # Per-pixel ray direction from camera params
  geodesic_schwarzschild.wgsl   # RK4 integrator (Phase 1)
  geodesic_kerr.wgsl            # Kerr integrator (Phase 2)
  disk.wgsl            # Plane intersection + procedural texture + Doppler
  planets.wgsl         # Sphere intersection + shading
  grid.wgsl            # Flamm paraboloid intersection + grid pattern
  stars.wgsl           # Procedural starfield
  skybox.wgsl          # Cubemap sampling
  common.wgsl          # Shared structs (SphereData, params, camera), constants
```

## 9. Parameters (`BlackHoleParams`)

Tunable, mirrored to the GPU uniform each frame:

| Field | Default | Meaning |
|---|---|---|
| `rs` | 1.0 | Schwarzschild radius (natural units) |
| `disk_inner` | 3.0 | Disk inner radius (ISCO) |
| `disk_outer` | 15.0 | Disk outer radius |
| `disk_tilt` | ~75° | Disk plane tilt vs. camera |
| `disk_brightness` | 1.0 | Global disk intensity |
| `doppler_strength` | 1.0 | Multiplier on beaming asymmetry |
| `steps` | 300 | Integrator step count |
| `dt` | derived | Step size (auto from escape radius / steps) |
| `render_scale` | 1.0 | Render resolution scale |
| `grid_enabled` | false | Toggle Flamm grid |
| `star_intensity` | 1.0 | Procedural star brightness |
| `spin` | 0.0 | Kerr spin parameter (Phase 2) |
| `planet_count` | 0 | Number of valid entries in planets buffer |

## 10. Phasing

### Phase 1 — Schwarzschild (this project's main deliverable)
All scene elements (disk + Doppler, skybox/stars, planets, grid) against the Schwarzschild geodesic integrator. Deliverables:
- Working Bevy app matching the reference video look (shadow, tilted Doppler disk, lensed halo over/under, lensed stars) by default.
- Grid + planets + cubemap as additional enabled features.
- Orbit camera + live params.
- Unit tests for the CPU-mirrored math constants (bcrit ≈ 2.598; a ray at large impact parameter has known small deflection).
- Target 60fps at full render scale on a typical discrete GPU.

### Phase 2 — Kerr (true Gargantua; documented follow-up, separate plan)
Replace `geodesic_schwarzschild.wgsl` with `geodesic_kerr.wgsl` integrating the Kerr geodesic equations (Boyer-Lindquist) with adaptive RK4. Adds frame-dragging/ergosphere asymmetry. Expected to require `render_scale ≈ 0.5` + upscaling. The same scene elements, camera, and params carry over unchanged; only the integrator core swaps. This is a separate spec/plan once Phase 1 ships.

## 11. Testing & verification

This is primarily a visual artifact; verification strategy:
- **Unit tests (Rust):** mirror the shader's key constants/relations in CPU code and assert: bcrit = 3√3/2 ≈ 2.598; a ray launched with impact parameter `b < bcrit` falls below `Rs`; a ray with `b >> bcrit` is deflected by an amount within tolerance of the weak-field formula `δθ ≈ 2Rs/b`. These guard the math against silent regressions.
- **Visual milestones (manual):**
  1. Plain starfield renders with the full-screen shader.
  2. Black circular shadow appears, sized ~bcrit.
  3. First Einstein ring appears when a disk is added edge-on.
  4. Tilted disk shows the over/under halo (the money shot matching the reference).
  5. Doppler asymmetry: one side brighter, bright side shifts as camera orbits.
  6. Lensing of stars visible near the hole edge.
  7. (Feature) grid bends near the hole; (feature) planet arcs near the hole.
- Performance budget check: confirm ≥60fps at `render_scale=1.0`, Steps=300, in Phase 1.

## 12. Risks & mitigations

| Risk | Mitigation |
|---|---|
| RK4 too slow at 300 steps/full-res | Make steps + render_scale live params; profile early. Phase 1 default can drop to 200 steps. |
| WGSL storage-buffer support edge cases for planets | Bound loop with `MAX_PLANETS` constant; fall back to uniform array if needed. |
| Flamm grid adds noise/visual clutter | Off by default; additive + faded; separate toggle. |
| Kerr math instability (Phase 2) | Isolated to Phase 2; adaptive step; does not block Phase 1. |
| Secondary images absent (loop terminates early) | Explicit non-termination policy across photon sphere (Section 4). |
