# Interstellar Black Hole Renderer — Phase 2 (Kerr) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Schwarzschild geodesic integrator with a Kerr (spinning) integrator so the black hole shows frame-dragging / ergosphere asymmetry, while staying bit-identical to Phase 1 at spin=0.

**Architecture:** Kerr lives entirely inside the shader's `deriv()` body (one added spin-orthogonal term) and the integrator shell (fixed-step RK4 → adaptive RK45). Spin axis is +Y, so the disk stays in the y=0 plane and every Phase 1 crossing test (disk/grid/planet) runs unchanged on the bent path. The `spin` parameter enters the GPU uniform by consuming an existing `_pad4` slot (no struct growth). `disk_inner` becomes spin-derived ISCO (Bardeen formula, CPU). `render_scale` is finally wired via offscreen render-to-texture + upscale (the highest-risk task, done first).

**Tech Stack:** Rust 2024, Bevy 0.19, bevy_egui 0.41, WGSL, trunk (web), WebGPU.

**Spec:** `docs/superpowers/specs/2026-07-13-interstellar-blackhole-phase2-kerr-design.md`

> **Status (2026-07-14):** Tasks 1–7 are implemented and committed; `cargo test` is green (17 tests). Task 8 remains — it is the human-in-the-loop visual + performance checklist (spin=0 regression, frame-dragging asymmetry, desktop/web FPS), which the code cannot self-verify. Items below are checked to reflect shipped code; unchecked acceptance items are the remaining human verification.


**Verified API facts (do not deviate):**
- Offscreen render: `Camera2d` + `Camera { order: -1, .. }` + `RenderTarget::Image(handle.clone().into())`. A second `Camera2d` (default order 0) draws the offscreen `Image` upscaled to the window. Template: Bevy 0.19 `examples/2d/pixel_grid_snap.rs`.
- Offscreen `Image`: `Image::new_target_texture(w, h, TextureFormat::Bgra8UnormSrgb, None)` — already sets `RENDER_ATTACHMENT` usage. Recreate on `WindowResized` via `MessageReader<WindowResized>` (NOT `EventReader` in 0.19).
- Upscale pass: a second `Material2d` (`UpscaleMaterial`) whose fragment samples the offscreen `Image` via `#[texture(0)] #[sampler(1)] source: Handle<Image>`.
- `Camera2d` is a unit-struct marker in 0.19; spawn `Camera2d` + `Camera { .. }` separately. No `Camera2dBundle`.
- `Material2d` / `Material2dPlugin` / `AlphaMode2d` import from `bevy::sprite_render`.
- Uniform field swap: `BlackHoleUniforms._pad4: f32` (at `material.rs:40`) → `spin: f32`. A `u32` (`steps`) immediately precedes it, and `f32` after a `u32` is 4-byte-aligned — no WGSL alignment shift. Rust `ShaderType` derive and WGSL struct must both change in lockstep.
- `spin: f32` already exists in `BlackHoleParams` (`src/params.rs:29`, default `0.0`); only the uniform plumbing is new.

---

## File structure (delta from Phase 1)

```
src/
  physics.rs            # +kerr_isco, +kerr_horizon, +kerr_bending_accel (CPU mirror, tested)
  render/material.rs    # BlackHoleUniforms: _pad4 → spin; +UpscaleMaterial
  render/plugin.rs      # spawn offscreen+upscale cameras; render_scale resize; mirror_params +spin/+disk_inner
  params.rs             # no struct change; render_scale default desktop 0.75 / web 0.5 (cfg)
  ui.rs                 # +Spin slider + ISCO/Horizon read-only labels; disk_inner slider removed
assets/shaders/
  black_hole.wgsl       # deriv() Kerr body; loop RK45; struct spin field; capture radius spin-dependent
  upscale.wgsl          # NEW: fullscreen texture-sample blit
tests/
  physics_test.rs       # +Kerr degeneracy/ISCO/horizon/monotonic tests
```

---

## Task 1: Wire `render_scale` (offscreen render-to-texture + upscale)

**Why first:** highest-risk unknown; unblocks all Phase 2 perf tuning. Isolated to the render plugin + a new upscale material. After this task, lowering `render_scale` visibly reduces fragment invocations (blurry upscale) without breaking the view.

**Files:**
- Create: `assets/shaders/upscale.wgsl`
- Modify: `src/render/material.rs` (add `UpscaleMaterial`)
- Modify: `src/render/plugin.rs` (offscreen + upscale cameras, resize system)

- [x] **Step 1: Write the upscale WGSL shader**

Create `assets/shaders/upscale.wgsl`:

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var samp: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // in.uv is [0,1]; sample the offscreen image directly (linear sampler upscales).
    return textureSample(tex, samp, in.uv);
}
```

- [x] **Step 2: Add `UpscaleMaterial` to `material.rs`**

In `src/render/material.rs`, add after `BlackHoleMaterial` (before `impl Default for BlackHoleMaterial`):

```rust
/// Samples the sub-resolution offscreen render and blits it fullscreen.
/// Bound to a second Camera2d that draws after the offscreen camera.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct UpscaleMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub source: Handle<Image>,
}

impl Material2d for UpscaleMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/upscale.wgsl".into()
    }
}
```

`ShaderRef`, `Asset`, `TypePath`, `AsBindGroup`, `Material2d`, `Image`, `Handle` are already imported at the top of `material.rs`. Verify the imports compile; add `use bevy::image::Image;` if missing.

- [x] **Step 3: Refactor `spawn_fullscreen_quad` to build the offscreen pipeline**

In `src/render/plugin.rs`, replace the body of `spawn_fullscreen_quad` (currently `plugin.rs:39-75`). The black-hole quad now renders into an offscreen `Image`; a second camera + upscale quad draws that image to the window. Add these marker components at the top of the file (after `struct FullscreenQuad;`):

```rust
/// The offscreen Image the black-hole shader renders into (sub-resolution).
#[derive(Component)]
pub struct OffscreenTarget(pub Handle<Image>);

/// The camera that renders the black-hole quad into the offscreen Image.
#[derive(Component)]
pub struct OffscreenCamera;

/// The camera that draws the upscaled offscreen Image to the window.
#[derive(Component)]
pub struct UpscaleCamera;

/// The quad that displays the upscaled image.
#[derive(Component)]
struct UpscaleQuad;
```

Then replace `spawn_fullscreen_quad` with:

```rust
fn spawn_fullscreen_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    mut upscale_materials: ResMut<Assets<crate::render::material::UpscaleMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window>,
    params: Res<crate::params::BlackHoleParams>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    let scale = params.render_scale.clamp(0.25, 1.0);
    let w = ((win.width() * scale) as u32).max(1);
    let h = ((win.height() * scale) as u32).max(1);

    // Offscreen target at sub-resolution. new_target_texture sets RENDER_ATTACHMENT.
    let offscreen = images.add(Image::new_target_texture(
        w,
        h,
        TextureFormat::Bgra8UnormSrgb,
        None,
    ));
    commands.spawn(OffscreenTarget(offscreen.clone()));

    // --- Black-hole quad (renders into the offscreen Image) ---
    let half_w = w as f32 / 2.0;
    let half_h = h as f32 / 2.0;
    let planets_buffer = buffers.add(ShaderBuffer::from(vec![
        super::material::SphereData::default();
        super::material::MAX_PLANETS
    ]));
    let mut material = BlackHoleMaterial::default();
    material.planets = planets_buffer;
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(material)),
        Transform::default().with_scale(Vec3::new(half_w, half_h, 1.0)),
        FullscreenQuad,
    ));
    // Offscreen camera: order -1 so it renders before the upscale camera.
    commands.spawn((
        Camera2d,
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.1, 0.1, 0.1)),
            ..default()
        },
        RenderTarget::Image(offscreen.clone().into()),
        Msaa::Off,
        OffscreenCamera,
    ));

    // --- Upscale quad (draws offscreen Image to the window) ---
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(upscale_materials.add(crate::render::material::UpscaleMaterial {
            source: offscreen.clone(),
        })),
        Transform::default().with_scale(Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0)),
        UpscaleQuad,
    ));
    commands.spawn((Camera2d, Msaa::Off, UpscaleCamera));
}
```

Add imports to `plugin.rs` top: `use bevy::camera::RenderTarget; use bevy::render::render_resource::TextureFormat; use bevy::image::Image;`. (`Clear color` value matches the Phase 1 grey.)

- [x] **Step 4: Replace `fit_quad_to_window` with a resize system that resizes the offscreen Image**

In `src/render/plugin.rs`, delete the existing `fit_quad_to_window` (`plugin.rs:79-93`) and add:

```rust
/// Recreate the offscreen Image and rescale both quads on window resize,
/// honoring the live `render_scale` param.
fn resize_offscreen(
    mut images: ResMut<Assets<Image>>,
    params: Res<crate::params::BlackHoleParams>,
    target: Query<&OffscreenTarget>,
    mut bh_quad: Query<&mut Transform, With<FullscreenQuad>>,
    mut up_quad: Query<&mut Transform, With<UpscaleQuad>>,
    window: Query<&Window>,
    mut resized: MessageReader<bevy::window::WindowResized>,
) {
    if resized.read().next().is_none() {
        return;
    }
    let Ok(win) = window.single() else { return; };
    let scale = params.render_scale.clamp(0.25, 1.0);
    let w = ((win.width() * scale) as u32).max(1);
    let h = ((win.height() * scale) as u32).max(1);
    if let Ok(handle) = target.single() {
        let img = Image::new_target_texture(w, h, TextureFormat::Bgra8UnormSrgb, None);
        images.insert(handle.0.clone(), img);
    }
    for mut t in &mut bh_quad {
        t.scale = Vec3::new(w as f32 / 2.0, h as f32 / 2.0, 1.0);
    }
    for mut t in &mut up_quad {
        t.scale = Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0);
    }
}
```

Then update the `Update` system set in `BlackHolePlugin::build` (`plugin.rs:23-31`): replace `fit_quad_to_window` with `resize_offscreen`, and add `Material2dPlugin::<crate::render::material::UpscaleMaterial>::default()` next to the existing `Material2dPlugin::<BlackHoleMaterial>`:

```rust
.add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
.add_plugins(Material2dPlugin::<crate::render::material::UpscaleMaterial>::default())
```

- [x] **Step 5: Add `render_scale` to the egui Renderer section**

In `src/ui.rs`, inside the `"Renderer"` `CollapsingHeader` (currently `ui.rs:36-41`), replace the comment block with a live slider:

```rust
egui::CollapsingHeader::new("Renderer").show(ui, |ui| {
    ui.add(egui::Slider::new(&mut params.steps, 50..=600).text("Steps"));
    ui.add(egui::Slider::new(&mut params.render_scale, 0.25..=1.0).text("Render scale"));
});
```

- [x] **Step 6: Bump `render_scale` defaults for Phase 2**

In `src/params.rs` (`params.rs:44`), change:

```rust
render_scale: if cfg!(target_arch = "wasm32") { 0.75 } else { 1.0 },
```

to:

```rust
render_scale: if cfg!(target_arch = "wasm32") { 0.5 } else { 0.75 },
```

Also update the `#[allow(dead_code)]` attribute at `params.rs:6` — remove `render_scale` from the "reserved" comment since it's now wired:

```rust
#[allow(dead_code)] // spin is reserved for Phase 2 (Kerr); render_scale now wired in Phase 2
```

- [x] **Step 7: Compile and run**

Run: `cargo build`
Expected: compiles with no errors. (Warnings about unused `UpscaleCamera`/`OffscreenCamera` markers are fine — they're used for querying.)

Run: `cargo run`
Expected: the black hole renders as before, but the image is slightly blurry (0.75 upscale). Moving the `Render scale` slider in the UI changes sharpness live. Resizing the window does not break the view.

- [x] **Step 8: Commit**

```bash
git add assets/shaders/upscale.wgsl src/render/material.rs src/render/plugin.rs src/ui.rs src/params.rs
git commit -m "feat: wire render_scale via offscreen render-to-texture + upscale"
```

---

## Task 2: Plumb `spin` into the GPU uniform

**Why second:** smallest change; proves the uniform path end-to-end before touching math. At spin=0 nothing visible changes (the shader doesn't use `spin` yet — that's Task 5).

**Files:**
- Modify: `src/render/material.rs` (uniform struct + default)
- Modify: `src/render/plugin.rs` (`mirror_params` gains one line)
- Modify: `assets/shaders/black_hole.wgsl` (struct field)

- [x] **Step 1: Swap `_pad4` → `spin` in `BlackHoleUniforms`**

In `src/render/material.rs:40`, change:

```rust
pub steps: u32,
pub _pad4: f32,
pub _pad5: f32,
```

to:

```rust
pub steps: u32,
pub spin: f32,       // Phase 2: dimensionless Kerr spin χ = a/M ∈ [0,1].
pub _pad5: f32,
```

In the `Default` impl (`material.rs:71-73`), change:

```rust
steps: 300,
_pad4: 0.0,
_pad5: 0.0,
```

to:

```rust
steps: 300,
spin: 0.0,
_pad5: 0.0,
```

- [x] **Step 2: Mirror `spin` into the uniform each frame**

In `src/render/plugin.rs`, inside `mirror_params` (after `u.steps = params.steps;` at `plugin.rs:142`), add:

```rust
u.spin = params.spin;
```

- [x] **Step 3: Add `spin` to the WGSL uniform struct**

In `assets/shaders/black_hole.wgsl:35-37`, change:

```wgsl
steps: u32,
_pad4: f32,
_pad5: f32,
```

to:

```wgsl
steps: u32,
spin: f32,
_pad5: f32,
```

- [x] **Step 4: Compile**

Run: `cargo build`
Expected: compiles. No visual change (spin unused in deriv yet).

- [x] **Step 5: Commit**

```bash
git add src/render/material.rs src/render/plugin.rs assets/shaders/black_hole.wgsl
git commit -m "feat: plumb spin parameter into GPU uniform"
```

---

## Task 3: CPU Kerr helpers (`kerr_isco`, `kerr_horizon`) with tests

**Why TDD here:** pure Rust, no GPU. Locks the ISCO/horizon formulas before the shader depends on them. The spin=0 values (3.0 and 1.0) are the Phase 1 compatibility contract.

**Files:**
- Modify: `src/physics.rs` (add two functions)
- Modify: `tests/physics_test.rs` (add tests)

- [x] **Step 1: Write the failing tests**

Append to `tests/physics_test.rs`:

```rust
#[test]
fn kerr_isco_at_zero_is_schwarzschild() {
    // spin=0 → ISCO = 6M = 3 Rs (Rs=1).
    let isco = physics::kerr_isco(0.0);
    assert!((isco - 3.0).abs() < 1e-3, "spin=0 ISCO should be 3.0, got {}", isco);
}

#[test]
fn kerr_isco_at_extremal_is_half_rs() {
    // spin=1 → ISCO = M = Rs/2 = 0.5.
    let isco = physics::kerr_isco(1.0);
    assert!((isco - 0.5).abs() < 1e-3, "spin=1 ISCO should be 0.5, got {}", isco);
}

#[test]
fn kerr_isco_is_monotonically_decreasing() {
    let a = physics::kerr_isco(0.3);
    let b = physics::kerr_isco(0.6);
    let c = physics::kerr_isco(0.9);
    assert!(a > b, "0.3 > 0.6: {} vs {}", a, b);
    assert!(b > c, "0.6 > 0.9: {} vs {}", b, c);
}

#[test]
fn kerr_horizon_at_zero_is_rs() {
    // spin=0 → r+ = Rs = 1.0.
    let r = physics::kerr_horizon(0.0);
    assert!((r - 1.0).abs() < 1e-3, "spin=0 horizon should be 1.0, got {}", r);
}

#[test]
fn kerr_horizon_at_extremal_is_half_rs() {
    // spin=1 → r+ = M = 0.5.
    let r = physics::kerr_horizon(1.0);
    assert!((r - 0.5).abs() < 1e-3, "spin=1 horizon should be 0.5, got {}", r);
}

#[test]
fn kerr_horizon_is_monotonically_decreasing() {
    let a = physics::kerr_horizon(0.3);
    let b = physics::kerr_horizon(0.6);
    let c = physics::kerr_horizon(0.9);
    assert!(a > b, "0.3 > 0.6: {} vs {}", a, b);
    assert!(b > c, "0.6 > 0.9: {} vs {}", b, c);
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test --test physics_test`
Expected: FAIL — `kerr_isco` and `kerr_horizon` do not exist (compile error).

- [x] **Step 3: Implement `kerr_isco` and `kerr_horizon`**

In `src/physics.rs`, add after the existing `impact_parameter` function (before the `#[allow(dead_code)] fn _phantom`):

```rust
/// Prograde Kerr ISCO in Rs units (Rs=1, so M=0.5). `chi = a/M ∈ [0,1]`.
/// Bardeen-Press-Teukolsky (1972) closed form. Returns 6M=3.0 at chi=0,
/// M=0.5 at chi=1.
pub fn kerr_isco(chi: f32) -> f32 {
    let m = 0.5;
    let cbrt_pos = (1.0 + chi).cbrt();
    let cbrt_neg = (1.0 - chi).cbrt();
    let z1 = 1.0 + (1.0 - chi * chi).cbrt() * (cbrt_pos + cbrt_neg);
    let z2 = (3.0 * chi * chi + z1 * z1).sqrt();
    m * (3.0 + z2 - ((3.0 - z1) * (3.0 + z1 + 2.0 * z2)).sqrt())
}

/// Kerr event-horizon radius r+ in Rs units (Rs=1, M=0.5). `chi = a/M ∈ [0,1]`.
/// Returns Rs=1.0 at chi=0, M=0.5 at chi=1.
pub fn kerr_horizon(chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    m + (m * m - a * a).max(0.0).sqrt()
}
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test --test physics_test`
Expected: PASS — all 6 Kerr tests + the existing `public_bcrt_constant_is_correct` test pass.

- [x] **Step 5: Commit**

```bash
git add src/physics.rs tests/physics_test.rs
git commit -m "feat: add kerr_isco and kerr_horizon CPU helpers with tests"
```

---

## Task 4: Make `disk_inner` ISCO-derived in `mirror_params`

**Why before the shader math:** locks the disk-inner-edge behavior to the spin value before the integrator depends on it. At spin=0 disk_inner is still 3.0 (Phase 1 identical).

**Files:**
- Modify: `src/render/plugin.rs` (`mirror_params`)

- [x] **Step 1: Override `disk_inner` with the ISCO value in `mirror_params`**

In `src/render/plugin.rs`, inside `mirror_params`, find the line `u.disk_inner = params.disk_inner;` (currently `plugin.rs:130`) and replace it with:

```rust
// disk_inner is spin-derived (Kerr ISCO); the params.disk_inner field is ignored.
u.disk_inner = crate::physics::kerr_isco(params.spin);
```

- [x] **Step 2: Compile and run**

Run: `cargo build`
Expected: compiles.

Run: `cargo run`
Expected: at spin=0 (default) the disk looks identical to Phase 1 (disk_inner = 3.0). No visible change yet.

- [x] **Step 3: Commit**

```bash
git add src/render/plugin.rs
git commit -m "feat: derive disk_inner from Kerr ISCO"
```

---

## Task 5: Kerr `deriv()` in the shader + spin-dependent capture radius

**The core math change.** After this task, spin>0 visibly bends rays asymmetrically (frame-dragging). spin=0 is bit-identical to Phase 1.

**Files:**
- Modify: `assets/shaders/black_hole.wgsl` (`deriv`, capture test)

- [x] **Step 1: Replace the `deriv` body with the Kerr pseudo-Hamiltonian**

In `assets/shaders/black_hole.wgsl:110-119`, replace the entire `deriv` function:

```wgsl
fn deriv(pos: vec3<f32>, dir: vec3<f32>) -> Deriv {
    let r = length(pos);
    let rs = uniforms.rs;
    // Kerr spin. χ ∈ [0,1]; a = χ·M, M = Rs/2 = 0.5 (Rs=1).
    let chi = uniforms.spin;
    let m = 0.5;
    let a = chi * m;
    // Schwarzschild radial bending (identical to Phase 1 at χ=0).
    let h = cross(pos, dir);
    let h2 = dot(h, h);
    let r5 = max(r * r * r * r * r, 1e-6);
    let radial = -1.5 * rs * h2 / r5 * pos;
    // Frame-dragging (Lense-Thirring leading term). Spin axis = +Y.
    let spin_axis = vec3<f32>(0.0, 1.0, 0.0);
    let r3 = max(r * r * r, 1e-6);
    let drag = 2.0 * m * a / r3 * cross(spin_axis, dir);
    let accel = radial + drag;
    return Deriv(dir, accel);
}
```

- [x] **Step 2: Make the capture radius spin-dependent**

In `assets/shaders/black_hole.wgsl:268-273`, find the capture test:

```wgsl
let r = length(pos);
if (r < uniforms.rs) {
    // Captured: whatever we've composited so far is the result.
    break;
}
```

Replace the condition with the spin-dependent horizon radius:

```wgsl
let r = length(pos);
// Kerr horizon r+ = M + sqrt(M² - a²), M=0.5, a=χ·M. Equals Rs at χ=0.
let chi = uniforms.spin;
let m = 0.5;
let a = chi * m;
let r_plus = m + sqrt(max(m * m - a * a, 0.0));
if (r < r_plus) {
    // Captured: whatever we've composited so far is the result.
    break;
}
```

- [x] **Step 3: Compile**

Run: `cargo build`
Expected: compiles.

- [x] **Step 4: Add a CPU degeneracy test for the Kerr bending accel**

Append to `tests/physics_test.rs`:

```rust
#[test]
fn kerr_bending_accel_degenerates_to_schwarzschild_at_zero_spin() {
    // At χ=0 the Kerr bending accel must equal the Schwarzschild one.
    let pos = bevy::math::Vec3::new(3.0, 1.0, 4.0);
    let dir = bevy::math::Vec3::new(0.2, -0.1, -0.97).normalize();
    let schw = physics::bending_accel(pos, dir);
    let kerr = physics::kerr_bending_accel(pos, dir, 0.0);
    let diff = (schw - kerr).length();
    assert!(diff < 1e-6, "spin=0 Kerr should match Schwarzschild; diff = {}", diff);
}

#[test]
fn kerr_bending_accel_nonzero_off_axis_at_nonzero_spin() {
    // At χ>0 the drag term must produce a different accel (frame-dragging exists).
    let pos = bevy::math::Vec3::new(3.0, 1.0, 4.0);
    let dir = bevy::math::Vec3::new(0.2, -0.1, -0.97).normalize();
    let schw = physics::bending_accel(pos, dir);
    let kerr = physics::kerr_bending_accel(pos, dir, 0.8);
    let diff = (schw - kerr).length();
    assert!(diff > 1e-4, "spin=0.8 Kerr should differ from Schwarzschild; diff = {}", diff);
}
```

- [x] **Step 5: Add `kerr_bending_accel` to `src/physics.rs`**

In `src/physics.rs`, after `bending_accel`, add:

```rust
/// Kerr bending acceleration (CPU mirror of the shader `deriv` accel).
/// `chi = a/M ∈ [0,1]`. At chi=0 this equals `bending_accel`.
pub fn kerr_bending_accel(pos: Vec3, dir: Vec3, chi: f32) -> Vec3 {
    let r = pos.length();
    let m = 0.5;
    let a = chi * m;
    let h = pos.cross(dir);
    let h2 = h.dot(h);
    let r5 = (r * r * r * r * r).max(1e-6);
    let radial = -1.5 * RS * h2 / r5 * pos;
    let spin_axis = Vec3::Y;
    let r3 = (r * r * r).max(1e-6);
    let drag = 2.0 * m * a / r3 * spin_axis.cross(dir);
    radial + drag
}
```

- [x] **Step 6: Run tests**

Run: `cargo test --test physics_test`
Expected: PASS — all tests including the two new degeneracy tests.

- [x] **Step 7: Run the app and verify spin=0 is unchanged, spin>0 shows asymmetry**

Run: `cargo run`
Expected: at default (spin=0) the image is identical to Phase 1. There is no Spin UI yet — to test spin>0, temporarily add `params.spin = 0.5;` in `params.rs` `Default`, run, observe the disk halo is no longer mirror-symmetric, then revert the default back to `0.0`.

- [x] **Step 8: Commit**

```bash
git add assets/shaders/black_hole.wgsl src/physics.rs tests/physics_test.rs
git commit -m "feat: Kerr deriv() with frame-dragging + spin-dependent horizon"
```

---

## Task 6: Adaptive RK45 integrator shell

**Replaces the fixed-step RK4 loop.** This is the second structural change after Task 1. The crossing tests (disk/grid/planet) and compositing stay identical — they run on each *accepted* RK45 segment.

**Files:**
- Modify: `assets/shaders/black_hole.wgsl` (integration loop)

- [x] **Step 1: Add the Dormand-Prince RK45 step function**

In `assets/shaders/black_hole.wgsl`, add immediately before the `@fragment fn fragment` entry point (after the `grid_hit` function, before `// ====================== main ======================`):

```wgsl
// One Dormand-Prince RK45 step. Returns the 5th-order solution and the
// error estimate (y5 - y4) as a vec3 (position error; direction error is
// folded in via normalize so we only need position error for step control).
struct RkStep {
    pos: vec3<f32>,
    dir: vec3<f32>,
    err: f32,
};

fn rk45_step(pos: vec3<f32>, dir: vec3<f32>, dt: f32) -> RkStep {
    // Butcher tableau (Dormand-Prince), 6 stages. Each deriv() returns Deriv{dpos, ddir}.
    let k1 = deriv(pos, dir);
    let p2 = pos + k1.dpos * dt * 0.2;
    let d2 = normalize(dir + k1.ddir * dt * 0.2);
    let k2 = deriv(p2, d2);
    let p3 = pos + (k1.dpos * 0.075 + k2.dpos * 0.225) * dt;
    let d3 = normalize(dir + (k1.ddir * 0.075 + k2.ddir * 0.225) * dt);
    let k3 = deriv(p3, d3);
    let p4 = pos + (k1.dpos * 0.3 + k2.dpos * -0.9 + k3.dpos * 1.2) * dt;
    let d4 = normalize(dir + (k1.ddir * 0.3 + k2.ddir * -0.9 + k3.ddir * 1.2) * dt);
    let k4 = deriv(p4, d4);
    let p5 = pos + (k1.dpos * -11.0/54.0 + k2.dpos * 2.5 + k3.dpos * -70.0/27.0 + k4.dpos * 35.0/27.0) * dt;
    let d5 = normalize(dir + (k1.ddir * -11.0/54.0 + k2.ddir * 2.5 + k3.ddir * -70.0/27.0 + k4.ddir * 35.0/27.0) * dt);
    let k5 = deriv(p5, d5);
    let p6 = pos + (k1.dpos * 1631.0/55296.0 + k2.dpos * 175.0/512.0 + k3.dpos * 575.0/13824.0 + k4.dpos * 44275.0/110592.0 + k5.dpos * 253.0/4096.0) * dt;
    let d6 = normalize(dir + (k1.ddir * 1631.0/55296.0 + k2.ddir * 175.0/512.0 + k3.ddir * 575.0/13824.0 + k4.ddir * 44275.0/110592.0 + k5.ddir * 253.0/4096.0) * dt);
    let k6 = deriv(p6, d6);
    // 5th-order solution (used to advance).
    let new_pos = pos + (k1.dpos * 37.0/378.0 + k3.dpos * 250.0/621.0 + k4.dpos * 125.0/594.0 + k5.dpos * 512.0/1771.0 + k6.dpos * 0.0) * dt;
    let new_dir = normalize(dir + (k1.ddir * 37.0/378.0 + k3.ddir * 250.0/621.0 + k4.ddir * 125.0/594.0 + k5.ddir * 512.0/1771.0 + k6.ddir * 0.0) * dt);
    // 4th-order solution (for error estimate).
    let pos4 = pos + (k1.dpos * 2825.0/27648.0 + k3.dpos * 18575.0/48384.0 + k4.dpos * 13525.0/55296.0 + k5.dpos * 277.0/14336.0 + k6.dpos * 0.25) * dt;
    let err = length(new_pos - pos4);
    return RkStep(new_pos, new_dir, err);
}
```

- [x] **Step 2: Replace the integration loop with the adaptive RK45 loop**

In `assets/shaders/black_hole.wgsl`, find the integration loop (the section starting around `// Total path length to integrate:` at line ~256 through the end of the `for` loop at ~323). Replace from the line `let dt = total_path / f32(uniforms.steps);` through the closing brace of the `for` loop with:

```wgsl
    let total_path = eye_dist + escape_r;
    // Adaptive RK45 constants.
    let steps_max = uniforms.steps;
    let dt_init = total_path / f32(steps_max);
    let dt_min = dt_init * 0.25;
    let dt_max = dt_init * 4.0;
    let tol = 1e-3;
    let r_plus = 0.5 + sqrt(max(0.25 - (uniforms.spin * 0.5) * (uniforms.spin * 0.5), 0.0));

    var pos = rot_x(uniforms.eye.xyz, -uniforms.disk_tilt);
    var d   = normalize(rot_x(dir, -uniforms.disk_tilt));
    var dt  = dt_init;
    var prev = pos;
    var budget = steps_max;

    var accum_color = vec3<f32>(0.0);
    var accum_alpha = 0.0;

    loop {
        if (budget == 0u) { break; }

        let step = rk45_step(pos, d, dt);
        let err = step.err;

        if (err > tol * 10.0) {
            // Reject: shrink dt, retry (does not consume budget).
            dt = clamp(dt * 0.2, dt_min, dt_max);
            continue;
        }
        // Accept: consume one budget unit, refine dt.
        budget = budget - 1u;
        dt = clamp(dt * pow(tol / max(err, 1e-12), 0.2), dt_min, dt_max);

        let new_pos = step.pos;
        let new_dir = step.dir;

        let r = length(new_pos);
        if (r < r_plus) {
            break;
        }
        if (r > escape_r) {
            let world_dir = normalize(rot_x(new_dir, uniforms.disk_tilt));
            var bg = vec3<f32>(0.0);
            bg += star_color(world_dir, uniforms.star_intensity);
            if (uniforms.skybox_intensity > 0.0) {
                bg += skybox_color(world_dir) * uniforms.skybox_intensity;
            }
            accum_color += (1.0 - accum_alpha) * bg;
            accum_alpha = 1.0;
            break;
        }

        if (disk_hit(prev, new_pos)) {
            let ty = prev.y / (prev.y - new_pos.y);
            let hit = mix(prev, new_pos, vec3<f32>(ty));
            let dc = disk_color(hit, new_dir);
            let a = 0.85;
            accum_color += (1.0 - accum_alpha) * dc * a;
            accum_alpha += (1.0 - accum_alpha) * a;
            if (accum_alpha > 0.99) { break; }
        }

        let ph = planet_hit(prev, new_pos, new_dir);
        if (ph.w > 0.0) {
            accum_color += (1.0 - accum_alpha) * ph.xyz * ph.w;
            accum_alpha += (1.0 - accum_alpha) * ph.w;
            if (accum_alpha > 0.99) { break; }
        }

        if (uniforms.grid_enabled != 0u) {
            let g = grid_hit(prev, new_pos);
            if (g.x + g.y + g.z > 0.0) {
                accum_color += g;
            }
        }

        prev = new_pos;
        pos = new_pos;
        d = new_dir;
    }

    return vec4<f32>(accum_color, 1.0);
```

This replaces everything from the old `var pos = rot_x(...)` through the old `return vec4<f32>(accum_color, 1.0);`. Delete the old `for` loop and the old fixed-step RK4 body entirely. The `escape_r`, `eye_dist`, `disk_tilt`, compositing, and crossing-test calls are all preserved — only the loop machinery changes.

- [x] **Step 3: Compile**

Run: `cargo build`
Expected: compiles.

- [x] **Step 4: Run and verify spin=0 still looks right, then tune**

Run: `cargo run`
Expected: at spin=0 the image matches Phase 1 closely (adaptive stepping may produce very slightly different secondary-image detail, but the shadow, halo, and disk are visually equivalent). Rays near the photon sphere take small steps; far-field rays take large steps.

If the Einstein ring looks noisy/jagged, raise `tol` toward `5e-4` (tighter) is wrong direction — instead increase `steps` in the UI, or loosen `tol` toward `2e-3` if too many rays terminate early. Default `tol = 1e-3` is the spec value; only deviate if a real artifact appears.

- [x] **Step 5: Commit**

```bash
git add assets/shaders/black_hole.wgsl
git commit -m "feat: adaptive RK45 integrator replacing fixed-step RK4"
```

---

## Task 7: UI — Spin slider + ISCO/Horizon read-only labels

**Files:**
- Modify: `src/ui.rs`

- [x] **Step 1: Add the Black Hole section and remove the disk_inner slider**

In `src/ui.rs`, inside the `egui::Window::new("Controls")` closure, add a new collapsing header *before* the "Accretion Disk" header, and modify the "Accretion Disk" header to remove the `disk_inner` slider (it is now spin-derived).

Replace the block starting at `egui::CollapsingHeader::new("Accretion Disk")` (currently `ui.rs:23-31`) — insert the new "Black Hole" header before it and edit the disk section:

```rust
                egui::CollapsingHeader::new("Black Hole")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add(egui::Slider::new(&mut params.spin, 0.0..=1.0).text("Spin (χ)"));
                        ui.label(format!("ISCO (disk inner): {:.3}", crate::physics::kerr_isco(params.spin)));
                        ui.label(format!("Horizon r+: {:.3}", crate::physics::kerr_horizon(params.spin)));
                    });
                egui::CollapsingHeader::new("Accretion Disk")
                    .default_open(true)
                    .show(ui, |ui| {
                        // disk_inner removed — now spin-derived (see Black Hole section).
                        ui.add(egui::Slider::new(&mut params.disk_outer, 6.0..=40.0).text("Outer radius"));
                        ui.add(egui::Slider::new(&mut params.disk_tilt, 0.0..=3.14).text("Tilt"));
                        ui.add(egui::Slider::new(&mut params.disk_brightness, 0.0..=3.0).text("Brightness"));
                        ui.add(egui::Slider::new(&mut params.disk_rotation_speed, 0.0..=3.0).text("Rotation speed"));
                    });
```

- [x] **Step 2: Compile and run**

Run: `cargo build`
Expected: compiles.

Run: `cargo run`
Expected: the Controls panel has a new "Black Hole" section with a Spin slider (0–1) and two read-only ISCO/Horizon labels that update live as the slider moves. The "Accretion Disk" section no longer has an "Inner radius" slider. Sweeping spin from 0 to 0.9 visibly shrinks the disk inner edge and introduces frame-dragging asymmetry.

- [x] **Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add Spin slider + ISCO/Horizon read-only labels"
```

---

## Task 8: Validation — visual milestones + performance + web build

**Files:** none (verification only).

- [ ] **Step 1: spin=0 regression check**

Run: `cargo run`. Set spin=0 in the UI.
Expected: image is visually indistinguishable from Phase 1 (same shadow size ≈ bcrit, same halo, same Doppler side).

- [ ] **Step 2: Frame-dragging asymmetry at spin=0.5**

Set spin=0.5 in the UI.
Expected: the disk's bright Doppler side shears off the pure line-of-sight axis; the halo is no longer mirror-smetric across the spin axis (the +Y axis).

- [ ] **Step 3: ISCO shrink sweep**

Slowly sweep spin from 0 → 0.9.
Expected: disk inner edge visibly pulls inward (the read-only ISCO label confirms the radius dropping from 3.0 toward ~0.7).

- [ ] **Step 4: Higher-order Einstein ring still forms**

At spin=0.5, steps=300, look near the photon sphere edge.
Expected: secondary Einstein images still form (budget does not starve them at default steps).

- [ ] **Step 5: Desktop performance**

Run: `cargo run --release`. Default settings (render_scale=0.75, steps=300, spin=0.5).
Expected: ≥60 fps on a typical discrete GPU. If below, lower render_scale to 0.5 or steps to 200 in the UI and re-check.

- [ ] **Step 6: Grid/planets/skybox no regression**

Toggle Grid on; add a planet near the hole (if the planet feature is wired); verify they still render correctly at spin>0 (lensed through the Kerr geodesic).

- [ ] **Step 7: Full test suite**

Run: `cargo test`
Expected: all tests pass (Phase 1 bcrit tests + all Kerr tests).

- [ ] **Step 8: Web build**

Run: `trunk build --release`
Expected: wasm artifact builds. Open in Chrome/Edge; verify it renders at render_scale=0.5, steps=200, ≥30 fps, and the egui panel works. A non-WebGPU browser shows the fallback message.

- [ ] **Step 9: Final commit (if any tuning changes were made)**

If any defaults were tuned during validation, commit them:

```bash
git add -A
git commit -m "chore: Phase 2 tuning from validation"
```

If no changes, skip this step.

---

## Phase 2 complete — acceptance checklist

- [ ] spin=0 is visually identical to Phase 1 (no regression).
- [ ] spin>0 shows frame-dragging asymmetry (disk halo no longer mirror-symmetric across the spin axis).
- [ ] Disk inner edge tracks Kerr ISCO (Bardeen formula), shrinking from 3 Rs at spin=0 toward 0.5 Rs at extremal.
- [ ] Horizon radius shrinks with spin (r+ = M + sqrt(M²−a²)).
- [x] Adaptive RK45 integrator runs; render_scale=0.75 desktop / 0.5 web.
- [ ] All Phase 1 features (disk, Doppler, stars, grid, planets, skybox) work at spin>0.
- [x] `cargo test` passes (Phase 1 + Kerr degeneracy/ISCO/horizon tests).
- [ ] Desktop ≥60 fps at default Phase 2 settings; web ≥30 fps on WebGPU.
- [x] egui Spin slider + ISCO/Horizon read-only labels work; disk_inner slider removed.

---

## Phase 3 follow-up (out of scope; documented)

- Full exact Kerr Cartesian pseudo-Hamiltonian (Σ/Δ/Carter-separable form) for <1% photon-orbit accuracy at high spin.
- Negative spin / retrograde disk.
- Adaptive integrator *order* (currently fixed RK45 order, adaptive step only).
- Tilted spin axis (would break the y=0 disk assumption).
