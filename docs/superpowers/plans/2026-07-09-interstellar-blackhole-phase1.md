# Interstellar Black Hole Renderer — Phase 1 (Schwarzschild) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A real-time, cross-platform (desktop + WebGPU) Schwarzschild black-hole renderer in Bevy 0.19 that matches the reference video: black shadow, tilted Doppler accretion disk with lensed halo, lensed starfield, plus optional lensed grid/planets/skybox — all driven by an egui control panel.

**Architecture:** One full-screen quad with a custom `Material2d` whose fragment shader geodesic-ray-traces curved spacetime per pixel and intersects the disk, stars, planets, and grid along the bent path. All tunable state lives in a `BlackHoleParams` `Resource`, mirrored into the material uniform each frame. A `bevy_egui` panel edits the resource live.

**Tech Stack:** Rust 2024, Bevy 0.19, bevy_egui 0.41, WGSL shaders, trunk (web build), WebGPU.

**Spec:** `docs/superpowers/specs/2026-07-09-interstellar-blackhole-design.md`

**Verified API facts (do not deviate):**
- `Material2d` material bind group is **group 2**; use `#{MATERIAL_BIND_GROUP}` token in WGSL.
- Imports from `bevy::sprite_render`: `Material2d`, `Material2dPlugin`, `AlphaMode2d`.
- Component-based spawning: `commands.spawn(Camera2d)` and `(Mesh2d(h), MeshMaterial2d(h))`. No bundles.
- `fragment_shader()` is a static fn returning `ShaderRef::Path("...".into())`; entry point is `@fragment fn fragment(...)`.
- WGSL must `#import bevy_sprite::mesh2d_vertex_output::VertexOutput`; `in.position.xy` is pixel coords; `in.uv` is `[0,1]`.
- Web: enable Bevy `webgpu` feature → `WgpuSettings::default()` auto-selects `BROWSER_WEBGPU`. No `RenderPlugin` backend config. `getrandom` handled transitively.
- `AsBindGroup`: `#[uniform(0)]` (single arg = binding index), `#[texture(1)]`, `#[sampler(2)]`, `#[storage(3, read_only)]`. A struct field bound as uniform must derive `ShaderType`.

---

## File structure (created across tasks)

```
Cargo.toml                              # deps (Task 1)
Trunk.toml                              # web build (Task 3)
web/index.html                          # web entry (Task 3)
assets/shaders/
  common.wgsl                           # structs + constants (Task 7)
  stars.wgsl                            # procedural stars (Task 11)
  disk.wgsl                             # disk + Doppler (Task 12)
  planets.wgsl                          # sphere intersection (Task 14)
  grid.wgsl                             # Flamm grid (Task 15)
  skybox.wgsl                           # cubemap sampling (Task 16)
  geodesic_schwarzschild.wgsl           # RK4 integrator (Task 9)
  black_hole.wgsl                       # entry point (Task 8, grows each task)
src/
  main.rs                               # app + plugin wiring
  params.rs                             # BlackHoleParams resource (Task 6)
  camera.rs                             # orbit controller (Task 5)
  ui.rs                                 # egui panel (Task 17)
  web.rs                                # wasm glue (Task 3)
  physics.rs                            # CPU-mirrored math + unit tests (Task 10)
  scene/mod.rs, disk.rs, planets.rs     # scene helpers
  render/
    mod.rs
    plugin.rs                           # BlackHolePlugin (Task 4)
    material.rs                         # BlackHoleMaterial + BlackHoleUniforms (Task 7)
tests/physics_test.rs                   # (Task 10)
```

---

## Task 1: Project dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Write the dependency block**

Replace the entire contents of `Cargo.toml` with:

```toml
[package]
name = "singularity-rs"
version = "0.1.0"
edition = "2024"

[dependencies]
bevy = "0.19"
bevy_egui = "0.41"

# Web-only deps for WebGPU detection + fallback message.
[target.'cfg(target_arch = "wasm32")'.dependencies]
web-sys = { version = "0.3", features = ["Gpu", "Navigator", "Window", "Document", "HtmlElement", "Element"] }
wasm-bindgen = "0.2"

# Smaller wasm binary in release web builds (per Bevy examples README).
[profile.wasm-release]
inherits = "release"
opt-level = "z"
lto = "fat"
codegen-units = 1
```

- [ ] **Step 2: Verify it resolves**

Run: `cargo check`
Expected: compiles with no errors (downloads bevy + bevy_egui; may take a few minutes the first time).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: add bevy 0.19 and bevy_egui 0.41 dependencies"
```

---

## Task 2: Minimal Bevy window opens (smoke test)

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write a minimal app that opens a window**

Replace `src/main.rs` with:

```rust
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
```

- [ ] **Step 2: Run it and confirm a blank window opens**

Run: `cargo run`
Expected: a Bevy window titled "App" opens with a black background and no panics. Close it with the window close button.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: minimal bevy window opens"
```

---

## Task 3: Web build works (Trunk + WebGPU + fallback)

**Files:**
- Create: `Trunk.toml`
- Create: `web/index.html`
- Create: `src/web.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `Trunk.toml`**

```toml
[build]
target = "web/index.html"
dist = "dist"

[serve]
address = "127.0.0.1"
port = 8080
open = false
```

- [ ] **Step 2: Create `web/index.html`**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>singularity-rs</title>
    <style>
      html, body { margin: 0; padding: 0; height: 100%; overflow: hidden; background: #000; }
      canvas { width: 100vw; height: 100vh; display: block; }
    </style>
  </head>
  <body>
    <link
      data-trunk rel="rust"
      data-bin="singularity-rs"
      data-type="main"
      data-cargo-features="webgpu"
      data-wasm-opt="z"
    />
  </body>
</html>
```

- [ ] **Step 3: Create `src/web.rs` with WebGPU check + fallback**

```rust
#[cfg(target_arch = "wasm32")]
pub fn webgpu_available() -> bool {
    web_sys::window()
        .and_then(|w| w.navigator())
        .and_then(|n| n.gpu())
        .is_some()
}

#[cfg(target_arch = "wasm32")]
pub fn show_fallback_message() {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(body) = document.body() {
                let el = document.create_element("div").unwrap();
                el.set_inner_text(
                    "WebGPU is not available in this browser. \
                     Please use a recent version of Chrome, Edge, or Firefox.",
                );
                el.set_attribute(
                    "style",
                    "position:fixed;inset:0;display:flex;align-items:center;\
                     justify-content:center;font-family:sans-serif;font-size:1.5rem;\
                     text-align:center;padding:2rem;background:#111;color:#eee;",
                )
                .ok();
                body.append_child(&el).ok();
            }
        }
    }
}
```

- [ ] **Step 4: Wire the check into `main` and enable canvas-fit on web**

Replace `src/main.rs` with:

```rust
use bevy::prelude::*;
use bevy::window::WindowPlugin;

#[cfg(target_arch = "wasm32")]
mod web;

fn main() {
    // On web, abort startup if WebGPU isn't available and show a message.
    #[cfg(target_arch = "wasm32")]
    {
        if !web::webgpu_available() {
            web::show_fallback_message();
            return;
        }
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "singularity-rs".into(),
                // On web, make the canvas track the browser window size.
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}
```

- [ ] **Step 5: Verify desktop still builds and runs**

Run: `cargo run`
Expected: window opens, title is "singularity-rs".

- [ ] **Step 6: Install trunk and the wasm target**

Run:
```bash
cargo install --locked trunk
rustup target add wasm32-unknown-unknown
```
Expected: `trunk` installed; wasm target added.

- [ ] **Step 7: Verify the web build serves**

Run: `trunk serve`
Expected: serving on `http://127.0.0.1:8080`. Open it in Chrome/Edge → a black canvas fills the window (the `Camera2d` with no content). Check the browser console for no errors. Stop with Ctrl-C.

- [ ] **Step 8: Commit**

```bash
git add Trunk.toml web/index.html src/web.rs src/main.rs
git commit -m "feat: web build via trunk with WebGPU detection + fallback"
```

---

## Task 4: BlackHolePlugin scaffold + full-screen quad

**Files:**
- Create: `src/render/mod.rs`
- Create: `src/render/plugin.rs`
- Modify: `src/main.rs`

This task creates the plugin and spawns a full-screen quad whose material renders a flat color, proving the full-screen-shader pipeline works before any physics.

- [ ] **Step 1: Create `src/render/mod.rs`**

```rust
pub mod plugin;
pub mod material;

pub use plugin::BlackHolePlugin;
```

- [ ] **Step 2: Create `src/render/material.rs` (placeholder flat-color material)**

```rust
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::sprite_render::Material2d;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct BlackHoleMaterial {
    #[uniform(0)]
    pub time: f32,
}

impl Material2d for BlackHoleMaterial {
    fn fragment_shader() -> bevy::sprite_render::ShaderRef {
        "shaders/black_hole.wgsl".into()
    }
}
```

- [ ] **Step 3: Create `src/render/plugin.rs`**

```rust
use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

use super::material::BlackHoleMaterial;

pub struct BlackHolePlugin;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(Update, update_time);
    }
}

fn spawn_fullscreen_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
) {
    // A large quad that covers the camera's orthographic view.
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(BlackHoleMaterial { time: 0.0 })),
        // Camera2d default projection spans [-1,1] in x; scale quad to fill.
        Transform::default(),
    ));
}

fn update_time(time: Res<Time>, mut materials: ResMut<Assets<BlackHoleMaterial>>) {
    for (_, mat) in materials.iter_mut() {
        mat.time = time.elapsed_secs();
    }
}
```

- [ ] **Step 4: Create the placeholder shader `assets/shaders/black_hole.wgsl`**

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> time: f32;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Gradient from time, just to prove the uniform flows.
    let t = (sin(time) + 1.0) * 0.5;
    let col = mix(vec3<f32>(0.02, 0.02, 0.05), vec3<f32>(0.08, 0.04, 0.12), t);
    return vec4<f32>(col, 1.0);
}
```

- [ ] **Step 5: Wire the plugin into `main.rs`**

Replace the body of `fn main()` in `src/main.rs` — add the plugin after `DefaultPlugins`:

```rust
mod render;
mod web;   // (keep the existing #[cfg] gate around the `mod web;` declaration)
```

Add `.add_plugins(render::BlackHolePlugin)` to the app builder, after `DefaultPlugins`. The resulting `main()` (keep the WebGPU guard at top):

```rust
use bevy::prelude::*;
use bevy::window::WindowPlugin;

mod render;
#[cfg(target_arch = "wasm32")]
mod web;

fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        if !web::webgpu_available() {
            web::show_fallback_message();
            return;
        }
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "singularity-rs".into(),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(render::BlackHolePlugin)
        .run();
}
```

Remove the now-unused `setup` system and its `Startup` registration (the quad spawn moved into the plugin).

- [ ] **Step 6: Run and confirm a time-varying gradient fills the window**

Run: `cargo run`
Expected: the whole window shows a slowly pulsing purple-blue gradient. No compile errors. If the quad doesn't fill the window, see the sizing note below.

- [ ] **Step 7: Commit**

```bash
git add src/render/ src/main.rs assets/shaders/black_hole.wgsl
git commit -m "feat: full-screen quad + BlackHoleMaterial pipeline"
```

> **Sizing note (if the quad doesn't fill):** `Camera2d`'s default projection uses `ScalingMode::FixedVertical(1.0)`, so the visible x-range is `[-aspect, aspect]` and y is `[-1,1]`. A `Rectangle::new(2.0, 2.0)` centered at the origin therefore covers y but may not cover the full width on wide screens. If you see letterboxing, set the quad's scale based on aspect, or change the camera projection. For correctness in the ray-generation step we'll compute rays from `in.uv` (always `[0,1]` over the quad), so the simplest fix is to scale the quad to the camera's visible width: add `Transform::default().with_scale(Vec3::new(aspect*2.0, 2.0, 1.0))` where `aspect = window width / height`. We'll handle this precisely in Task 8 when we generate rays.

---

## Task 5: Orbit camera controller

**Files:**
- Create: `src/camera.rs`
- Modify: `src/render/plugin.rs`
- Modify: `src/render/mod.rs` (re-export)
- Modify: `src/main.rs` (declare module)

The orbit controller is a `Resource` holding yaw/pitch/distance. It does NOT move a Bevy camera (we have no 3D camera) — it computes the `CameraParams` (eye position + basis vectors) that the shader uses. It consumes mouse input only when egui is not capturing it (the egui capture check is added in Task 17; here we just read raw input).

- [ ] **Step 1: Create `src/camera.rs`**

```rust
use bevy::input::mouse::MouseMotion;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

/// Orbit camera state. The black hole is at the origin (Rs=1 units).
/// `distance` is the camera radius; `yaw`/`pitch` orient it.
#[derive(Resource, Clone, Copy)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 1.3,       // ~75 deg — slightly above the disk plane
            distance: 30.0,
            fov: 1.0,         // radians
        }
    }
}

impl OrbitCamera {
    /// Compute the camera eye position and an orthonormal basis (forward/right/up)
    /// in Bevy's right-handed Y-up coordinate system. The black hole sits at origin.
    /// Disk plane is the xz-plane (y=0); the disk tilt is applied in the shader
    /// via the params, so the camera basis here is in world space.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        // Eye position on a sphere around the origin.
        let eye = Vec3::new(
            self.distance * cp * sy,
            self.distance * sp,
            self.distance * cp * cy,
        );
        // Forward points from eye toward the origin.
        let forward = (-eye).normalize();
        let world_up = Vec3::Y;
        let right = forward.cross(world_up).normalize();
        let up = right.cross(forward).normalize();
        (eye, forward, right, up)
    }
}

pub fn orbit_controller(
    mut camera: ResMut<OrbitCamera>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
) {
    if mouse.pressed(MouseButton::Left) {
        for ev in motion.read() {
            camera.yaw -= ev.delta.x * 0.005;
            // Clamp pitch to avoid flipping.
            camera.pitch = (camera.pitch + ev.delta.y * 0.005).clamp(0.05, std::f32::consts::PI - 0.05);
        }
    }
    for ev in wheel.read() {
        // Zoom: multiply distance by a factor of the scroll amount.
        camera.distance = (camera.distance * (1.0 + ev.y * 0.1)).clamp(2.6, 500.0);
        // 2.6 ≈ bcrit; don't let the camera pass through the shadow.
    }
}
```

> **Note on egui/pointer:** Task 17 introduces a `WantsPointer` resource that the UI system sets and the camera reads, so orbit input is ignored over the panel. We don't add any pointer-awareness scaffolding here in Task 5.

- [ ] **Step 2: Register the resource + system in `BlackHolePlugin`**

In `src/render/plugin.rs`, add to `build()`:
```rust
app.init_resource::<crate::camera::OrbitCamera>()
   .add_systems(Update, crate::camera::orbit_controller);
```

- [ ] **Step 3: Declare the module in `src/main.rs`**

Add near the other `mod` lines: `mod camera;`

- [ ] **Step 4: Build to confirm it compiles**

Run: `cargo build`
Expected: compiles. (No visible behavior change yet — the camera state isn't read by the shader until Task 8.)

- [ ] **Step 5: Commit**

```bash
git add src/camera.rs src/render/plugin.rs src/main.rs
git commit -m "feat: orbit camera controller (yaw/pitch/zoom)"
```

---

## Task 6: `BlackHoleParams` resource

**Files:**
- Create: `src/params.rs`
- Modify: `src/render/plugin.rs`
- Modify: `src/main.rs`

The single source of truth for all tunable values, edited by the UI and mirrored to the GPU.

- [ ] **Step 1: Create `src/params.rs`**

```rust
use bevy::prelude::*;

/// All tunable black-hole parameters. Edited by the egui panel (Task 17),
/// mirrored into BlackHoleUniforms each frame (Task 7).
#[derive(Resource, Clone)]
pub struct BlackHoleParams {
    // Physics (natural units, Rs = 1)
    pub rs: f32,
    // Accretion disk
    pub disk_inner: f32,
    pub disk_outer: f32,
    pub disk_tilt: f32,        // radians, tilt of disk plane vs. camera
    pub disk_brightness: f32,
    pub disk_rotation_speed: f32,
    pub doppler_enabled: bool,
    pub doppler_strength: f32,
    // Renderer
    pub steps: u32,
    pub render_scale: f32,
    pub star_intensity: f32,
    pub grid_enabled: bool,
    pub grid_density: f32,
    // Background
    pub skybox_intensity: f32,
    // Planets (count matches the storage buffer; Task 14)
    pub planet_count: u32,
    // Kerr (Phase 2; unused in Phase 1)
    pub spin: f32,
}

impl Default for BlackHoleParams {
    fn default() -> Self {
        Self {
            rs: 1.0,
            disk_inner: 3.0,
            disk_outer: 15.0,
            disk_tilt: 1.318,      // ~75.5 deg, matching the reference video
            disk_brightness: 1.0,
            disk_rotation_speed: 0.5,
            doppler_enabled: true,
            doppler_strength: 1.0,
            steps: 300,
            render_scale: 1.0,
            star_intensity: 1.0,
            grid_enabled: false,
            grid_density: 1.0,
            skybox_intensity: 0.0, // procedural stars only by default
            planet_count: 0,
            spin: 0.0,
        }
    }
}
```

- [ ] **Step 2: Register the resource in the plugin**

In `src/render/plugin.rs` `build()`, add:
```rust
app.init_resource::<crate::params::BlackHoleParams>();
```

- [ ] **Step 3: Declare the module in `src/main.rs`**

Add: `mod params;`

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add src/params.rs src/render/plugin.rs src/main.rs
git commit -m "feat: BlackHoleParams tunable resource"
```

---

## Task 7: Material uniforms + storage/texture bindings

**Files:**
- Modify: `src/render/material.rs`
- Modify: `src/render/plugin.rs`

Define the uniform struct, the skybox texture/sampler bindings, and the planets storage buffer. Then a mirror system copies `BlackHoleParams` + `OrbitCamera` into the material each frame.

- [ ] **Step 1: Rewrite `src/render/material.rs`**

```rust
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::sprite_render::Material2d;

/// GPU uniform: params + camera packed into one struct bound at binding 0.
#[derive(Clone, ShaderType)]
pub struct BlackHoleUniforms {
    // Camera basis + eye (4 vec3s = must align; pad each to vec4 via the shader struct)
    pub eye: Vec3,
    pub _pad0: f32,
    pub forward: Vec3,
    pub _pad1: f32,
    pub right: Vec3,
    pub _pad2: f32,
    pub up: Vec3,
    pub fov: f32,
    // Resolution
    pub resolution: Vec2,
    pub time: f32,
    pub _pad3: f32,
    // Physics + disk
    pub rs: f32,
    pub disk_inner: f32,
    pub disk_outer: f32,
    pub disk_tilt: f32,
    pub disk_brightness: f32,
    pub disk_rotation_speed: f32,
    pub doppler_strength: f32,
    pub star_intensity: f32,
    pub skybox_intensity: f32,
    pub grid_density: f32,
    // Flags packed as f32 (bools aren't valid uniform scalar types in WGSL)
    pub doppler_enabled: u32,
    pub grid_enabled: u32,
    pub planet_count: u32,
    pub steps: u32,
    pub _pad4: f32,
    pub _pad5: f32,
}

impl Default for BlackHoleUniforms {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 30.0),
            _pad0: 0.0,
            forward: Vec3::new(0.0, 0.0, -1.0),
            _pad1: 0.0,
            right: Vec3::new(1.0, 0.0, 0.0),
            _pad2: 0.0,
            up: Vec3::new(0.0, 1.0, 0.0),
            fov: 1.0,
            resolution: Vec2::new(1280.0, 720.0),
            time: 0.0,
            _pad3: 0.0,
            rs: 1.0,
            disk_inner: 3.0,
            disk_outer: 15.0,
            disk_tilt: 1.318,
            disk_brightness: 1.0,
            disk_rotation_speed: 0.5,
            doppler_strength: 1.0,
            star_intensity: 1.0,
            skybox_intensity: 0.0,
            grid_density: 1.0,
            doppler_enabled: 1,
            grid_enabled: 0,
            planet_count: 0,
            steps: 300,
            _pad4: 0.0,
            _pad5: 0.0,
        }
    }
}

/// One planet's data, uploaded in a storage buffer (binding 3).
#[derive(Clone, Copy, ShaderType, Default)]
pub struct SphereData {
    pub center: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub emissive: u32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

pub const MAX_PLANETS: usize = 32;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BlackHoleMaterial {
    #[uniform(0)]
    pub uniforms: BlackHoleUniforms,
    #[texture(1)]
    pub skybox: Option<Handle<Image>>,
    #[sampler(2)]
    pub skybox_sampler: bool, // AsBindGroup needs a sampler field; use a dummy bool presence
    #[storage(3, read_only)]
    pub planets: Vec<SphereData>,
}

impl Material2d for BlackHoleMaterial {
    fn fragment_shader() -> bevy::sprite_render::ShaderRef {
        "shaders/black_hole.wgsl".into()
    }
}

impl Default for BlackHoleMaterial {
    fn default() -> Self {
        Self {
            uniforms: BlackHoleUniforms::default(),
            skybox: None,
            skybox_sampler: true,
            planets: vec![SphereData::default(); MAX_PLANETS],
        }
    }
}
```

- [ ] **Step 2: Update `spawn_fullscreen_quad` and add the mirror system in `src/render/plugin.rs`**

Replace the file contents with:

```rust
use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;
use bevy::window::WindowResized;

use super::material::{BlackHoleMaterial, BlackHoleUniforms};

pub struct BlackHolePlugin;

impl Plugin for BlackHolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::camera::OrbitCamera>()
            .init_resource::<crate::params::BlackHoleParams>()
            .add_plugins(Material2dPlugin::<BlackHoleMaterial>::default())
            .add_systems(Startup, spawn_fullscreen_quad)
            .add_systems(Update, (crate::camera::orbit_controller, mirror_params));
    }
}

fn spawn_fullscreen_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    window: Query<&Window>,
) {
    let win = window.single();
    let aspect = win.width() / win.height();
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0 * aspect, 2.0))),
        MeshMaterial2d(materials.add(BlackHoleMaterial::default())),
        Transform::default(),
    ));
    commands.spawn(Camera2d);
}

fn mirror_params(
    camera: Res<crate::camera::OrbitCamera>,
    params: Res<crate::params::BlackHoleParams>,
    time: Res<Time>,
    window: Query<&Window>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
) {
    let win = match window.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    // Only update when something changed.
    let (eye, forward, right, up) = camera.basis();
    for (_, mat) in materials.iter_mut() {
        let u = &mut mat.uniforms;
        u.eye = eye;
        u.forward = forward;
        u.right = right;
        u.up = up;
        u.fov = camera.fov;
        u.resolution = Vec2::new(win.width(), win.height());
        u.time = time.elapsed_secs();
        u.rs = params.rs;
        u.disk_inner = params.disk_inner;
        u.disk_outer = params.disk_outer;
        u.disk_tilt = params.disk_tilt;
        u.disk_brightness = params.disk_brightness;
        u.disk_rotation_speed = params.disk_rotation_speed;
        u.doppler_strength = params.doppler_strength;
        u.star_intensity = params.star_intensity;
        u.skybox_intensity = params.skybox_intensity;
        u.grid_density = params.grid_density;
        u.doppler_enabled = params.doppler_enabled as u32;
        u.grid_enabled = params.grid_enabled as u32;
        u.planet_count = params.planet_count;
        u.steps = params.steps;
    }
}
```

- [ ] **Step 3: Update the placeholder shader to match the new uniform**

Replace `assets/shaders/black_hole.wgsl` with:

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct BlackHoleUniforms {
    eye: vec4<f32>,
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    resolution: vec2<f32>,
    time: f32,
    _pad3: f32,
    rs: f32,
    disk_inner: f32,
    disk_outer: f32,
    disk_tilt: f32,
    disk_brightness: f32,
    disk_rotation_speed: f32,
    doppler_strength: f32,
    star_intensity: f32,
    skybox_intensity: f32,
    grid_density: f32,
    doppler_enabled: u32,
    grid_enabled: u32,
    planet_count: u32,
    steps: u32,
    _pad4: f32,
    _pad5: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> uniforms: BlackHoleUniforms;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Prove the camera basis + uniforms flow: tint by eye direction.
    let n = normalize(uniforms.eye.xyz);
    return vec4<f32>(abs(n) * 0.5 + 0.3, 1.0);
}
```

> Note: the Rust `Vec3 + pad f32` fields become `vec4<f32>` in WGSL (std140 alignment). The shader struct above mirrors that exactly. `right/up/forward/eye` are each a `vec4` whose `.w` is the padding scalar (we only read `.xyz`).

- [ ] **Step 4: Build and run**

Run: `cargo run`
Expected: window fills with a color that changes as you orbit-drag (eye direction tints the screen). Confirms camera basis + uniforms reach the shader. If dragging does nothing, confirm the `OrbitCamera` resource is initialized (it is, via the plugin).

- [ ] **Step 5: Commit**

```bash
git add src/render/material.rs src/render/plugin.rs assets/shaders/black_hole.wgsl
git commit -m "feat: BlackHoleUniforms + camera/params mirror system"
```

---

## Task 8: Ray generation + camera ray launch

**Files:**
- Create: `assets/shaders/ray_gen.wgsl`
- Modify: `assets/shaders/black_hole.wgsl`

Generate a primary ray per pixel from the camera basis + FOV + aspect.

- [ ] **Step 1: Create `assets/shaders/ray_gen.wgsl`**

```wgsl
// Builds a world-space camera ray direction for the current pixel.
// `uv` is the pixel coordinate normalized to [-1,1] with aspect correction.
fn ray_direction(uv: vec2<f32>) -> vec3<f32> {
    let tan_half_fov = tan(uniforms.fov * 0.5);
    let dir =
        normalize(uniforms.forward.xyz)
        + uniforms.right.xyz * (uv.x * tan_half_fov)
        + uniforms.up.xyz    * (uv.y * tan_half_fov);
    return normalize(dir);
}
```

- [ ] **Step 2: Update `black_hole.wgsl` to use it**

After the uniform struct (keep it), replace the `fragment` body:

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import "shaders/ray_gen.wgsl"

// ... (uniform struct stays here, unchanged from Task 7) ...

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> uniforms: BlackHoleUniforms;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // in.uv is [0,1] across the quad. Center and flip y, apply aspect.
    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    var uv = (in.uv * 2.0 - 1.0);
    uv.x *= aspect;
    let dir = ray_direction(uv);
    // Visualize ray direction as a color (sanity check).
    return vec4<f32>(abs(dir), 1.0);
}
```

- [ ] **Step 3: Run and verify**

Run: `cargo run`
Expected: the screen shows a smoothly varying color field (ray directions). As you orbit the camera, the colors shift. Confirms per-pixel ray generation with correct aspect.

- [ ] **Step 4: Commit**

```bash
git add assets/shaders/ray_gen.wgsl assets/shaders/black_hole.wgsl
git commit -m "feat: per-pixel camera ray generation"
```

---

## Task 9: Schwarzschild geodesic integrator (RK4)

**Files:**
- Create: `assets/shaders/geodesic_schwarzschild.wgsl`
- Modify: `assets/shaders/black_hole.wgsl`

The core: integrate each ray through curved spacetime, terminating on capture or escape. This produces the black shadow (rays with impact parameter < bcrit fall below Rs).

- [ ] **Step 1: Create `assets/shaders/geodesic_schwarzschild.wgsl`**

```wgsl
const R_ESCAPE: f32 = 1000.0;

// One RK4 sub-step derivative of (pos, dir) under the Schwarzschild
// bending acceleration. Rs is uniforms.rs.
fn deriv(pos: vec3<f32>, dir: vec3<f32>) -> (vec3<f32>, vec3<f32>) {
    let r = length(pos);
    let rs = uniforms.rs;
    // Angular momentum squared: |cross(pos, dir)|^2
    let h = cross(pos, dir);
    let h2 = dot(h, h);
    // Avoid division by zero.
    let r5 = max(r * r * r * r * r, 1e-6);
    // d(pos)/dt = dir
    let dpos = dir;
    // d(dir)/dt = bending acceleration (re-normalized each step in integrate)
    let accel = -1.5 * rs * h2 / r5 * pos;
    return (dpos, accel);
}

// Integrate a ray from `pos` along `dir`. Returns:
//   .status: 0 = escaped, 1 = captured (shadow)
//   .final_pos, .final_dir: end state (used for sky sampling on escape)
struct RayResult {
    status: u32,
    final_pos: vec3<f32>,
    final_dir: vec3<f32>,
}

// Accumulator callback pattern: the caller passes a function-style body via
// a per-step check. Because WGSL has no first-class closures, we inline the
// per-step intersection tests in black_hole.wgsl's integrate_and_trace().
// This function returns ONLY the escape/capture classification, used as a
// fallback when no scene object is hit.
fn classify_ray(start_pos: vec3<f32>, start_dir: vec3<f32>, steps: u32, dt: f32) -> RayResult {
    var pos = start_pos;
    var dir = start_dir;
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let r = length(pos);
        if (r < uniforms.rs) {
            return RayResult(1u, pos, dir);
        }
        if (r > R_ESCAPE) {
            return RayResult(0u, pos, dir);
        }
        // RK4
        let (k1p, k1d) = deriv(pos, dir);
        let (k2p, k2d) = deriv(pos + k1p * dt * 0.5, normalize(dir + k1d * dt * 0.5));
        let (k3p, k3d) = deriv(pos + k2p * dt * 0.5, normalize(dir + k2d * dt * 0.5));
        let (k4p, k4d) = deriv(pos + k3p * dt,     normalize(dir + k3d * dt));
        pos = pos + (k1p + 2.0 * k2p + 2.0 * k3p + k4p) * dt / 6.0;
        dir = normalize(dir + (k1d + 2.0 * k2d + 2.0 * k3d + k4d) * dt / 6.0);
    }
    // Ran out of steps without a clear verdict: treat as escaped.
    return RayResult(0u, pos, dir);
}
```

- [ ] **Step 2: Verify shadow appearance in `black_hole.wgsl`**

Replace the `fragment` body in `black_hole.wgsl` with (keep the uniform struct + imports):

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import "shaders/ray_gen.wgsl"
#import "shaders/geodesic_schwarzschild.wgsl"

// ... uniform struct + binding unchanged ...

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    var uv = (in.uv * 2.0 - 1.0);
    uv.x *= aspect;
    let dir = ray_direction(uv);
    // Step size scales with how far we need to travel (~ distance/ steps).
    let dt = max(length(uniforms.eye.xyz), 20.0) / f32(uniforms.steps);
    let res = classify_ray(uniforms.eye.xyz, dir, uniforms.steps, dt);
    if (res.status == 1u) {
        // Captured = shadow.
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    // Escaped: dim grey for now (stars come in Task 11).
    return vec4<f32>(0.02, 0.02, 0.02, 1.0);
}
```

- [ ] **Step 3: Run and confirm a black disk appears**

Run: `cargo run`
Expected: a roughly circular black region (the shadow) appears centered, surrounded by dark grey. As you zoom in (`wheel`), the black disk grows. This is the first money shot — the shadow emerges from the geodesic integration, no hard-coded sphere.

- [ ] **Step 4: Commit**

```bash
git add assets/shaders/geodesic_schwarzschild.wgsl assets/shaders/black_hole.wgsl
git commit -m "feat: Schwarzschild RK4 geodesic integrator + black shadow"
```

---

## Task 10: CPU physics mirror + unit tests

**Files:**
- Create: `src/physics.rs`
- Create: `tests/physics_test.rs`
- Modify: `src/main.rs`

Mirror the shader math in Rust so we can unit-test the constants and the capture criterion (bcrit ≈ 2.598). This guards against silent regressions in the shader.

- [ ] **Step 1: Create `src/physics.rs`**

```rust
//! CPU mirror of the shader physics, for unit-testing.
//! Natural units: Rs = 1.

use glam::{Vec3, Vec4};

pub const RS: f32 = 1.0;
/// Critical impact parameter for a Schwarzschild hole: bcrit = (3*sqrt(3)/2) * Rs.
pub const BCRIT: f32 = 3.0 * 3.0_f32.sqrt() / 2.0;

/// One Euler step of the discretized geodesic (mirrors the shader's `deriv`).
pub fn bending_accel(pos: Vec3, dir: Vec3) -> Vec3 {
    let r = pos.length();
    let h = pos.cross(dir);
    let h2 = h.dot(h);
    let r5 = (r * r * r * r * r).max(1e-6);
    -1.5 * RS * h2 / r5 * pos
}

/// Classify a ray by integrating it. Returns true if captured (r < Rs).
/// Uses RK4 like the shader. `dt` is the step size, `steps` the count.
pub fn is_captured(start_pos: Vec3, start_dir: Vec3, steps: u32, dt: f32) -> bool {
    let mut pos = start_pos;
    let mut dir = start_dir;
    const R_ESCAPE: f32 = 1000.0;
    for _ in 0..steps {
        let r = pos.length();
        if r < RS {
            return true;
        }
        if r > R_ESCAPE {
            return false;
        }
        let k1p = dir;
        let k1d = bending_accel(pos, dir);
        let k2p = dir + k1d * dt * 0.5;
        let k2d = bending_accel(pos + k1p * dt * 0.5, (dir + k1d * dt * 0.5).normalize());
        let k3p = dir + k2d * dt * 0.5;
        let k3d = bending_accel(pos + k2p * dt * 0.5, (dir + k2d * dt * 0.5).normalize());
        let k4p = dir + k3d * dt;
        let k4d = bending_accel(pos + k3p * dt, (dir + k3d * dt).normalize());
        pos += (k1p + 2.0 * k2p + 2.0 * k3p + k4p) * dt / 6.0;
        dir = (dir + (k1d + 2.0 * k2d + 2.0 * k3d + k4d) * dt / 6.0).normalize();
    }
    false
}

/// Compute the impact parameter of a ray given eye position and direction:
/// b = |eye x dir| / |dir|  (since dir is unit, b = |eye x dir|).
pub fn impact_parameter(eye: Vec3, dir: Vec3) -> f32 {
    eye.cross(dir).length()
}

// Silence unused-import warning for Vec4 if not used; kept for future expansion.
#[allow(dead_code)]
fn _phantom(_v: Vec4) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcrit_value() {
        assert!((BCRIT - 2.598).abs() < 0.01, "bcrit should be ~2.598, got {}", BCRIT);
    }

    #[test]
    fn ray_below_bcrit_is_captured() {
        // Eye far on the z-axis; aim slightly off-center with b < bcrit.
        let eye = Vec3::new(0.0, 0.0, 50.0);
        let dir = Vec3::new(0.0, 2.0, -50.0).normalize(); // b ~ 2.0 < 2.598
        let b = impact_parameter(eye, dir);
        assert!(b < BCRIT, "b {} should be < bcrit {}", b, BCRIT);
        assert!(is_captured(eye, dir, 2000, 0.1), "ray below bcrit should be captured");
    }

    #[test]
    fn ray_above_bcrit_escapes() {
        let eye = Vec3::new(0.0, 0.0, 50.0);
        let dir = Vec3::new(0.0, 10.0, -50.0).normalize(); // b ~ 9.8 >> bcrit
        let b = impact_parameter(eye, dir);
        assert!(b > BCRIT);
        assert!(!is_captured(eye, dir, 2000, 0.1), "ray above bcrit should escape");
    }
}
```

- [ ] **Step 2: Declare the module in `src/main.rs`**

Add: `mod physics;`

- [ ] **Step 3: Create an integration test file `tests/physics_test.rs`**

```rust
use singularity_rs::physics;

#[test]
fn public_bcrt_constant_is_correct() {
    // 3*sqrt(3)/2 ≈ 2.598076
    let expected = 1.5 * 3.0_f32.sqrt();
    assert!((physics::BCRIT - expected).abs() < 1e-5);
}
```

For this to compile, `physics` must be reachable from the integration test. Add to `src/main.rs` top, and expose the module publicly on the crate. Since this is a binary crate, create `src/lib.rs`:

```rust
pub mod physics;
```

And add to the `[package]` section of `Cargo.toml` nothing extra (Cargo auto-detects `src/lib.rs` + `src/main.rs`). Add the lib target explicitly to be safe — append to `Cargo.toml` under `[package]`:

```toml
[lib]
name = "singularity_rs"
path = "src/lib.rs"
```

Then have `src/main.rs` use the library: change its module declarations to reference the lib crate. Simplest: in `src/main.rs`, replace `mod physics;` with `use singularity_rs::physics;` and remove the local `mod physics;`. Keep the other `mod` (camera, params, render, web) local to the binary as before. (Or move them all to the lib; but minimal change = just physics.)

- [ ] **Step 4: Run the tests**

Run: `cargo test`
Expected: all tests pass (3 unit tests in `physics.rs` + 1 integration test). If `ray_below_bcrit_is_captured` fails, increase `steps` to 5000 — RK4 with `dt=0.1` near the photon sphere needs fine resolution.

- [ ] **Step 5: Commit**

```bash
git add src/physics.rs src/lib.rs tests/physics_test.rs Cargo.toml src/main.rs
git commit -m "test: CPU-mirrored geodesic physics + bcrit/capture unit tests"
```

---

## Task 11: Procedural starfield (lensed background)

**Files:**
- Create: `assets/shaders/stars.wgsl`
- Modify: `assets/shaders/black_hole.wgsl`

When a ray escapes, sample a procedural star field along its final direction. Because rays bent, the stars are naturally lensed near the hole.

- [ ] **Step 1: Create `assets/shaders/stars.wgsl`**

```wgsl
// Hash-based procedural stars on the unit sphere. Returns RGB radiance.
fn hash13(p: vec3<f32>) -> f32 {
    var q = vec3<f32>(dot(p, vec3<f32>(127.1, 311.7, 74.7)),
                      dot(p, vec3<f32>(269.5, 183.3, 246.1)),
                      dot(p, vec3<f32>(113.5, 271.9, 124.6)));
    let h = fract(sin(q) * 43758.5453);
    return h.x;
}

fn star_color(dir: vec3<f32>, intensity: f32) -> vec3<f32> {
    // Divide the sphere into cells; a cell gets a star if its hash passes a threshold.
    let scale = 80.0;
    let cell = floor(dir * scale);
    let h = hash13(cell);
    let threshold = 0.985; // ~1.5% of cells hold a star
    if (h > threshold) {
        // Brightness from the hash remainder.
        let b = (h - threshold) / (1.0 - threshold);
        let col = mix(vec3<f32>(0.6, 0.7, 1.0), vec3<f32>(1.0, 0.9, 0.7), b);
        // Soften the star with the fractional position inside the cell.
        let f = abs(dir * scale - cell);
        let d = max(f.x, max(f.y, f.z));
        let falloff = smoothstep(0.5, 0.0, d);
        return col * b * falloff * 3.0 * intensity;
    }
    return vec3<f32>(0.0);
}
```

- [ ] **Step 2: Wire stars into `black_hole.wgsl`**

Add the import and use it on escape. Replace the escape branch:

```wgsl
#import "shaders/stars.wgsl"
// ... after classify ...
if (res.status == 1u) {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
// Escaped: sample procedural stars along the bent final direction.
let star = star_color(normalize(res.final_dir), uniforms.star_intensity);
return vec4<f32>(star, 1.0);
```

- [ ] **Step 3: Run and verify lensed stars**

Run: `cargo run`
Expected: the dark-grey background is replaced by a star field. Stars near the black shadow are visibly distorted/smeared into arcs (lensing). The shadow stays black.

- [ ] **Step 4: Commit**

```bash
git add assets/shaders/stars.wgsl assets/shaders/black_hole.wgsl
git commit -m "feat: procedural lensed starfield background"
```

---

## Task 12: Accretion disk + Doppler beaming + lensed halo

**Files:**
- Create: `assets/shaders/disk.wgsl`
- Modify: `assets/shaders/black_hole.wgsl`

The defining feature. We restructure `black_hole.wgsl` from "classify only" to "step-by-step integration that checks the disk plane at each step and composites hits front-to-back." This is the largest shader task.

- [ ] **Step 1: Create `assets/shaders/disk.wgsl`**

```wgsl
// Disk plane is the xz-plane in world space, tilted by `disk_tilt` around the
// x-axis. We work in "disk-local" coordinates by rotating the ray.

// Rotate a vector around the X axis by angle a.
fn rot_x(v: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(v.x, c * v.y - s * v.z, s * v.y + c * v.z);
}

// Returns true and sets out_t if the segment pos->pos+dir*dt crosses the
// disk plane (y=0) within radius [disk_inner, disk_outer].
fn disk_hit(prev: vec3<f32>, cur: vec3<f32>) -> bool {
    let y0 = prev.y;
    let y1 = cur.y;
    if (y0 * y1 > 0.0) {
        return false; // same side, no crossing
    }
    // Linear interpolate to the crossing point.
    let t = y0 / (y0 - y1);
    let cross = mix(prev, cur, vec3<f32>(t));
    let r = length(vec2<f32>(cross.x, cross.z));
    return r >= uniforms.disk_inner && r <= uniforms.disk_outer;
}

// Shade a disk hit: procedural texture + Doppler beaming + temperature color.
fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let r = length(vec2<f32>(pos.x, pos.z));
    let phi = atan2(pos.z, pos.x);

    // Procedural noise: layered angular + radial, animated by rotation.
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    let n = sin(phi * 8.0 + rot) * 0.5 + 0.5;
    let n2 = sin(phi * 23.0 - rot * 1.7 + r * 2.0) * 0.5 + 0.5;
    let noise = mix(n, n2, 0.4);

    // Temperature gradient: hotter (white-blue) near inner edge, cooler (orange-red) outer.
    let t = (r - uniforms.disk_inner) / (uniforms.disk_outer - uniforms.disk_inner);
    let tcol = mix(vec3<f32>(1.0, 0.95, 0.85), vec3<f32>(1.0, 0.45, 0.12), clamp(t, 0.0, 1.0));

    // Falloff: brighter at inner edge.
    let falloff = 1.0 / pow(r / uniforms.disk_inner, 2.0);

    var col = tcol * (0.6 + 0.4 * noise) * falloff;

    // Doppler beaming. Disk orbits Keplerian-ish: v ~ sqrt(Rs/(2r)).
    let v_orbital = sqrt(uniforms.rs / (2.0 * r));
    // Orbital velocity direction (tangent) in the disk plane.
    let tangent = normalize(vec3<f32>(-sin(phi), 0.0, cos(phi)));
    let beta = vec3<f32>(0.0); // placeholder; we use a scalar approximation below
    let _ = beta;
    // Scalar approximation: projection of orbital velocity onto ray direction.
    let vdotn = dot(tangent * v_orbital, -dir); // toward viewer if positive
    let gamma = 1.0 / sqrt(max(1.0 - v_orbital * v_orbital, 1e-4));
    var doppler = 1.0;
    if (uniforms.doppler_enabled != 0u) {
        let delta = 1.0 / (gamma * (1.0 - vdotn));
        doppler = pow(delta, 3.0) * uniforms.doppler_strength;
    }
    col *= doppler;

    return col * uniforms.disk_brightness;
}
```

- [ ] **Step 2: Restructure `black_hole.wgsl` to integrate step-by-step with disk compositing**

Replace the `fragment` function (keep imports + uniform struct) with:

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import "shaders/ray_gen.wgsl"
#import "shaders/geodesic_schwarzschild.wgsl"
#import "shaders/disk.wgsl"
#import "shaders/stars.wgsl"

// ... uniform struct + binding unchanged ...

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let aspect = uniforms.resolution.x / uniforms.resolution.y;
    var uv = (in.uv * 2.0 - 1.0);
    uv.x *= aspect;
    let dir = ray_direction(uv);

    // Work in disk-local space: rotate eye + dir by -disk_tilt around X so the
    // disk lies on y=0. (disk_hit/disk_color assume disk-local coords.)
    var pos = rot_x(uniforms.eye.xyz, -uniforms.disk_tilt);
    var d   = normalize(rot_x(dir, -uniforms.disk_tilt));

    let dt = max(length(uniforms.eye.xyz), 20.0) / f32(uniforms.steps);
    let steps = uniforms.steps;

    // Front-to-back compositing.
    var accum_color = vec3<f32>(0.0);
    var accum_alpha = 0.0;

    var prev = pos;
    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let r = length(pos);
        if (r < uniforms.rs) {
            // Captured: whatever we've composited so far is the result.
            break;
        }
        if (r > 1000.0) {
            // Escaped: add background stars along the (disk-local) final dir.
            // Rotate back to world for the star sample.
            let world_dir = normalize(rot_x(d, uniforms.disk_tilt));
            let star = star_color(world_dir, uniforms.star_intensity);
            accum_color += (1.0 - accum_alpha) * star;
            accum_alpha = 1.0;
            break;
        }

        // RK4 step (single step), then test disk crossing on the segment.
        let (k1p, k1d) = deriv(pos, d);
        let (k2p, k2d) = deriv(pos + k1p * dt * 0.5, normalize(d + k1d * dt * 0.5));
        let (k3p, k3d) = deriv(pos + k2p * dt * 0.5, normalize(d + k2d * dt * 0.5));
        let (k4p, k4d) = deriv(pos + k3p * dt, normalize(d + k3d * dt));
        let new_pos = pos + (k1p + 2.0*k2p + 2.0*k3p + k4p) * dt / 6.0;
        let new_dir = normalize(d + (k1d + 2.0*k2d + 2.0*k3d + k4d) * dt / 6.0);

        if (disk_hit(prev, new_pos)) {
            // Approximate the crossing point by interpolating to y=0.
            let ty = prev.y / (prev.y - new_pos.y);
            let hit = mix(prev, new_pos, vec3<f32>(ty));
            let dc = disk_color(hit, new_dir);
            let a = 0.85; // disk is nearly opaque
            accum_color += (1.0 - accum_alpha) * dc * a;
            accum_alpha += (1.0 - accum_alpha) * a;
            if (accum_alpha > 0.99) { break; }
        }

        prev = new_pos;
        pos = new_pos;
        d = new_dir;
    }

    return vec4<f32>(accum_color, 1.0);
}
```

- [ ] **Step 3: Run — the money shot**

Run: `cargo run`
Expected: the black shadow is now surrounded by a glowing tilted accretion disk. The disk wraps OVER the top of the shadow and UNDER the bottom (the lensed halo / Einstein ring), exactly like the reference video. One side of the disk is brighter (Doppler). Orbit to see it from different angles.

> **If the halo doesn't appear:** the rays must continue past the disk plane and wrap around — confirm the integration loop does NOT break on the first disk hit (it composites and continues). Confirm `steps` is ≥ 200.

- [ ] **Step 4: Commit**

```bash
git add assets/shaders/disk.wgsl assets/shaders/black_hole.wgsl
git commit -m "feat: accretion disk with Doppler beaming + lensed Einstein halo"
```

---

## Task 13: Wire the UI and confirm live params (UI skeleton)

This task wires `bevy_egui` and proves a live param (e.g. disk_brightness) changes the image. Full panel content lands in Task 17.

**Files:**
- Create: `src/ui.rs`
- Modify: `src/render/plugin.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `src/ui.rs`**

```rust
use bevy::prelude::*;

pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
) {
    egui::Window::new("Controls").show(contexts.ctx_mut(), |ui| {
        ui.add(egui::Slider::new(&mut params.disk_brightness, 0.0..=3.0).text("Disk brightness"));
        ui.add(egui::Slider::new(&mut params.steps, 50..=600).text("Steps"));
    });
}
```

- [ ] **Step 2: Add `EguiPlugin` + the system in the plugin**

In `src/render/plugin.rs`, change `build()` to add the egui plugin and system:

```rust
app.add_plugins(bevy_egui::EguiPlugin)
   .add_systems(Update, crate::ui::ui_system);
```

(Add the `.add_plugins((Material2dPlugin::<BlackHoleMaterial>::default(), bevy_egui::EguiPlugin))` form, or two separate `.add_plugins` calls — either works.)

- [ ] **Step 3: Declare the module in `src/main.rs`**

Add: `mod ui;`

- [ ] **Step 4: Build, run, confirm live control**

Run: `cargo run`
Expected: an egui "Controls" window appears (top-left) with two sliders. Dragging "Disk brightness" visibly brightens/dims the accretion disk in real time. Dragging "Steps" changes render quality.

- [ ] **Step 5: Commit**

```bash
git add src/ui.rs src/render/plugin.rs src/main.rs
git commit -m "feat: egui UI wired with live disk brightness + steps"
```

---

## Task 14: Planets (scene entities via storage buffer)

**Files:**
- Create: `src/scene/mod.rs`
- Create: `src/scene/planets.rs`
- Modify: `src/render/plugin.rs`
- Modify: `src/render/material.rs` (mirror planets)
- Create: `assets/shaders/planets.wgsl`
- Modify: `assets/shaders/black_hole.wgsl`

- [ ] **Step 1: Create `src/scene/mod.rs`**

```rust
pub mod planets;
```

- [ ] **Step 2: Create `src/scene/planets.rs`**

```rust
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

use crate::render::material::{SphereData, MAX_PLANETS};

/// A planet rendered as a lensed sphere inside the geodesic shader.
#[derive(Component, Clone, Copy)]
pub struct Planet {
    pub center: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub emissive: bool,
}

/// Collects all Planet components and uploads them to every BlackHoleMaterial's
/// planets storage buffer each frame. Also updates planet_count in params.
pub fn upload_planets(
    planets: Query<&Planet>,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut materials: ResMut<Assets<crate::render::material::BlackHoleMaterial>>,
) {
    let mut data: Vec<SphereData> = planets
        .iter()
        .take(MAX_PLANETS)
        .map(|p| SphereData {
            center: p.center,
            radius: p.radius,
            color: p.color,
            emissive: p.emissive as u32,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        })
        .collect();
    // Pad to MAX_PLANETS so the buffer size is constant (avoids reallocation).
    data.resize(MAX_PLANETS, SphereData::default());
    params.planet_count = planets.iter().count().min(MAX_PLANETS as u32) as u32;
    for (_, mat) in materials.iter_mut() {
        mat.planets = data.clone();
    }
}

/// Spawns a default test planet behind the hole so lensing is visible.
pub fn spawn_default_planet(mut commands: Commands) {
    commands.spawn(Planet {
        center: Vec3::new(0.0, 2.0, -25.0),
        radius: 2.0,
        color: Vec3::new(0.3, 0.5, 1.0),
        emissive: false,
    });
}
```

- [ ] **Step 3: Register planet systems in the plugin**

In `src/render/plugin.rs` `build()`, add:
```rust
app.add_systems(Startup, crate::scene::planets::spawn_default_planet)
   .add_systems(Update, crate::scene::planets::upload_planets);
```

- [ ] **Step 4: Declare modules in `src/main.rs`**

Add: `mod scene;`

- [ ] **Step 5: Create `assets/shaders/planets.wgsl`**

```wgsl
struct SphereData {
    center: vec4<f32>,   // xyz + radius
    color: vec4<f32>,    // rgb + emissive flag
};

@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<storage, read> planets: array<SphereData>;

// Test the segment prev->cur against all planets. Returns hit color & alpha,
// or black/0 if no hit. `dir` is the ray direction (for shading).
fn planet_hit(prev: vec3<f32>, cur: vec3<f32>, dir: vec3<f32>) -> vec4<f32> {
    var nearest_t = 1e9;
    var nearest_col = vec3<f32>(0.0);
    var found = false;
    for (var i: u32 = 0u; i < uniforms.planet_count; i = i + 1u) {
        let s = planets[i];
        let center = s.center.xyz;
        let radius = s.center.w;
        // Ray-sphere intersection for the segment.
        let seg = cur - prev;
        let oc = prev - center;
        let a = dot(seg, seg);
        let b = 2.0 * dot(oc, seg);
        let c = dot(oc, oc) - radius * radius;
        let disc = b * b - 4.0 * a * c;
        if (disc < 0.0) { continue; }
        let sq = sqrt(disc);
        var t = (-b - sq) / (2.0 * a);
        if (t < 0.0) { t = (-b + sq) / (2.0 * a); }
        if (t >= 0.0 && t <= 1.0 && t < nearest_t) {
            nearest_t = t;
            let hit_pos = prev + seg * t;
            let n = normalize(hit_pos - center);
            // Lambert shading from a fixed light direction.
            let light_dir = normalize(vec3<f32>(0.5, 0.8, 0.3));
            let ndl = max(dot(n, light_dir), 0.0);
            var col = s.color.xyz * (0.2 + 0.8 * ndl);
            if (s.color.w > 0.5) { col = s.color.xyz; } // emissive
            nearest_col = col;
            found = true;
        }
    }
    if (found) {
        return vec4<f32>(nearest_col, 0.95);
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
```

- [ ] **Step 6: Add planet compositing into `black_hole.wgsl`**

Add `#import "shaders/planets.wgsl"` to the imports. Inside the integration loop, after the disk-hit test (and before `prev = new_pos;`), add:

```wgsl
        let ph = planet_hit(prev, new_pos, new_dir);
        if (ph.w > 0.0) {
            accum_color += (1.0 - accum_alpha) * ph.xyz * ph.w;
            accum_alpha += (1.0 - accum_alpha) * ph.w;
            if (accum_alpha > 0.99) { break; }
        }
```

- [ ] **Step 7: Run and verify a lensed planet**

Run: `cargo run`
Expected: a blue planet is visible behind/above the hole. Move the camera so the planet passes near the shadow — it bends into an arc and may show a secondary image on the opposite side of the hole.

- [ ] **Step 8: Commit**

```bash
git add src/scene/ assets/shaders/planets.wgsl assets/shaders/black_hole.wgsl src/render/plugin.rs src/main.rs
git commit -m "feat: lensed planets via storage buffer"
```

---

## Task 15: Spacetime-curvature grid (Flamm paraboloid)

**Files:**
- Create: `assets/shaders/grid.wgsl`
- Modify: `assets/shaders/black_hole.wgsl`

- [ ] **Step 1: Create `assets/shaders/grid.wgsl`**

```wgsl
// Flamm's paraboloid embedding: z(r) = 2*sqrt(Rs*(r - Rs)), dips below the disk.
// We trace against this surface in disk-local space (disk on y=0); the paraboloid
// opens downward (negative y). Returns additive grid color + alpha.
fn flamm_depth(r: f32) -> f32 {
    if (r <= uniforms.rs) { return 0.0; }
    return -2.0 * sqrt(uniforms.rs * (r - uniforms.rs));
}

fn grid_hit(prev: vec3<f32>, cur: vec3<f32>) -> vec3<f32> {
    // Sample the paraboloid at the segment endpoints; if the segment crosses it,
    // find an approximate crossing by sampling.
    let r0 = length(vec2<f32>(prev.x, prev.z));
    let r1 = length(vec2<f32>(cur.x, cur.z));
    let z0_surf = flamm_depth(r0);
    let z1_surf = flamm_depth(r1);
    // Did the ray's y cross the surface y between endpoints?
    if ((prev.y - z0_surf) * (cur.y - z1_surf) > 0.0) {
        return vec3<f32>(0.0);
    }
    // Crossing: linear-search for the crossing point.
    var hit = vec3<f32>(0.0);
    var found = false;
    for (var s: i32 = 0; s < 8; s = s + 1) {
        let f = f32(s + 1) / 8.0;
        let p = mix(prev, cur, vec3<f32>(f));
        let r = length(vec2<f32>(p.x, p.z));
        let surf = flamm_depth(r);
        if (abs(p.y - surf) < 0.3) {
            hit = p;
            found = true;
            break;
        }
    }
    if (!found) { return vec3<f32>(0.0); }

    // Polar grid pattern from (r, phi).
    let r = length(vec2<f32>(hit.x, hit.z));
    let phi = atan2(hit.z, hit.x);
    let ring = smoothstep(0.06, 0.0, abs(fract(r * uniforms.grid_density * 0.5) - 0.5));
    let spoke = smoothstep(0.04, 0.0, abs(fract(phi * 6.0 / 6.283185) - 0.5));
    let grid = max(ring, spoke);
    // Fade with depth so the grid reads as "below" the hole.
    let fade = smoothstep(-15.0, -1.0, hit.y);
    let col = vec3<f32>(0.15, 0.3, 0.6) * grid * fade;
    return col * 0.5; // additive, low intensity
}
```

> **Note:** WGSL allows calling `flamm_depth` before its definition (forward references are legal), so no alias is needed. `flamm_depth` is defined at the top of this file; `grid_hit` calls it.

- [ ] **Step 2: Add grid into `black_hole.wgsl`**

Add `#import "shaders/grid.wgsl"`. Inside the integration loop, after the planet test, add (only when grid is enabled):

```wgsl
        if (uniforms.grid_enabled != 0u) {
            let g = grid_hit(prev, new_pos);
            if (g.x + g.y + g.z > 0.0) {
                accum_color += g; // additive
            }
        }
```

- [ ] **Step 3: Run and verify**

Run: `cargo run` then enable the grid (add `grid_enabled: true` temporarily to `BlackHoleParams::default()`, or wait for the UI in Task 17).
Expected: a polar grid appears below the hole, dipping toward the center (the gravity well), and grid lines near the hole are visibly bent by lensing.

- [ ] **Step 4: Commit**

```bash
git add assets/shaders/grid.wgsl assets/shaders/black_hole.wgsl
git commit -m "feat: lensed Flamm paraboloid curvature grid"
```

---

## Task 16: Cubemap skybox

**Files:**
- Modify: `src/render/material.rs` (load a skybox)
- Create: `assets/shaders/skybox.wgsl`
- Modify: `assets/shaders/black_hole.wgsl`

- [ ] **Step 1: Create `assets/shaders/skybox.wgsl`**

```wgsl
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var skybox: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var skybox_sampler: sampler;

fn skybox_color(dir: vec3<f32>) -> vec3<f32> {
    return textureSample(skybox, skybox_sampler, dir).rgb * uniforms.skybox_intensity;
}
```

- [ ] **Step 2: Load a skybox (optional asset) in the plugin**

Add to `spawn_fullscreen_quad` signature an `assets: Res<AssetServer>` param, and load a cubemap if present. For robustness, only set the skybox if the asset exists — but Bevy's asset server will just log a warning on a missing path. Simplest: bundle a placeholder cubemap at `assets/skybox/skybox.png` (a panorama) OR leave it `None` and rely on procedural stars. To enable it, the user places a cubemap and sets `skybox_intensity > 0`.

For this task, just ensure the binding compiles with `None` (procedural stars remain the default). No Rust change strictly required beyond what Task 7 already has (`skybox: Option<Handle<Image>>`). Document in the panel (Task 17) that intensity > 0 needs an asset.

- [ ] **Step 3: Use skybox in the escape branch of `black_hole.wgsl`**

In the escape branch (where stars are added), prepend the skybox sample:

```wgsl
        // Escaped:
        let world_dir = normalize(rot_x(d, uniforms.disk_tilt));
        var bg = skybox_color(world_dir);
        bg += star_color(world_dir, uniforms.star_intensity);
        accum_color += (1.0 - accum_alpha) * bg;
        accum_alpha = 1.0;
        break;
```

- [ ] **Step 4: Build (skybox texture may be None — confirm no GPU error)**

Run: `cargo run`
Expected: with no cubemap asset and `skybox_intensity = 0`, output is identical to before (procedural stars). Confirms the binding path doesn't crash when `skybox` is `None`.

> **If binding a None texture crashes:** WGSL `textureSample` of an unbound texture is invalid. In that case gate the skybox sample: `if (uniforms.skybox_intensity > 0.0) { bg = skybox_color(...) }` — but the binding still must exist. `AsBindGroup` with `Option<Handle<Image>>` binds a 1x1 fallback when `None`, which is safe to sample. If your build complains, provide a tiny 1x1 cubemap fallback asset.

- [ ] **Step 5: Commit**

```bash
git add assets/shaders/skybox.wgsl assets/shaders/black_hole.wgsl
git commit -m "feat: cubemap skybox layer (optional, procedural stars default)"
```

---

## Task 17: Full egui control panel + pointer routing

**Files:**
- Modify: `src/ui.rs`
- Modify: `src/camera.rs` (real egui-wants-pointer check)
- Modify: `src/render/plugin.rs`

- [ ] **Step 1: Expand `src/ui.rs` with all sections**

```rust
use bevy::prelude::*;

pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
) {
    let mut wants = false;
    egui::Window::new("Controls")
        .collapsible(true)
        .default_pos([16.0, 16.0])
        .show(contexts.ctx_mut(), |ui| {
            egui::CollapsingHeader::new("Camera")
                .default_open(true)
                .show(ui, |ui| {
                    ui.add(egui::Slider::new(&mut camera.distance, 3.0..=200.0).text("Distance"));
                    ui.add(egui::Slider::new(&mut camera.yaw, -3.14..=3.14).text("Yaw"));
                    ui.add(egui::Slider::new(&mut camera.pitch, 0.05..=3.09).text("Pitch"));
                    ui.add(egui::Slider::new(&mut camera.fov, 0.3..=2.0).text("FOV"));
                });
            egui::CollapsingHeader::new("Accretion Disk")
                .default_open(true)
                .show(ui, |ui| {
                    ui.add(egui::Slider::new(&mut params.disk_inner, 1.5..=6.0).text("Inner radius"));
                    ui.add(egui::Slider::new(&mut params.disk_outer, 6.0..=40.0).text("Outer radius"));
                    ui.add(egui::Slider::new(&mut params.disk_tilt, 0.0..=3.14).text("Tilt"));
                    ui.add(egui::Slider::new(&mut params.disk_brightness, 0.0..=3.0).text("Brightness"));
                    ui.add(egui::Slider::new(&mut params.disk_rotation_speed, 0.0..=3.0).text("Rotation speed"));
                });
            egui::CollapsingHeader::new("Doppler").show(ui, |ui| {
                ui.checkbox(&mut params.doppler_enabled, "Enabled");
                ui.add_enabled(params.doppler_enabled, egui::Slider::new(&mut params.doppler_strength, 0.0..=3.0).text("Strength"));
            });
            egui::CollapsingHeader::new("Renderer").show(ui, |ui| {
                ui.add(egui::Slider::new(&mut params.steps, 50..=600).text("Steps"));
                ui.add(egui::Slider::new(&mut params.render_scale, 0.5..=1.0).text("Render scale"));
            });
            egui::CollapsingHeader::new("Background").show(ui, |ui| {
                ui.add(egui::Slider::new(&mut params.star_intensity, 0.0..=3.0).text("Star intensity"));
                ui.add(egui::Slider::new(&mut params.skybox_intensity, 0.0..=3.0).text("Skybox intensity"));
            });
            egui::CollapsingHeader::new("Grid").show(ui, |ui| {
                ui.checkbox(&mut params.grid_enabled, "Enabled");
                ui.add_enabled(params.grid_enabled, egui::Slider::new(&mut params.grid_density, 0.1..=4.0).text("Density"));
            });
        });
    // bevy_egui sets ctx wants pointer; we read it in the camera system via the resource.
    wants = contexts.ctx_mut().wants_pointer_input();
    // Store via a dedicated resource updated here; camera reads it.
    // (Simplest: use bevy_egui's EguiInputSet; here we use a lightweight flag resource.)
    let _ = wants;
}
```

- [ ] **Step 2: Add a `WantsPointer` resource and wire the camera to respect it**

In `src/camera.rs`, add:
```rust
#[derive(Resource, Default)]
pub struct WantsPointer(pub bool);
```
Replace `orbit_controller` to consume the resource and bail when egui wants input:
```rust
pub fn orbit_controller(
    wants: Res<WantsPointer>,
    mut camera: ResMut<OrbitCamera>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
) {
    if wants.0 { motion.clear(); wheel.clear(); return; }
    // ... (existing body unchanged) ...
}
```
(The Task 5 `orbit_controller` signature changes to add the `wants` parameter; keep its body identical otherwise.)

In `src/ui.rs`, set the resource:
```rust
pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
    mut wants: ResMut<crate::camera::WantsPointer>,
) {
    // ... panel ...
    wants.0 = contexts.ctx_mut().wants_pointer_input();
}
```

Register `WantsPointer` in the plugin: `app.init_resource::<crate::camera::WantsPointer>();`

- [ ] **Step 3: Build and run**

Run: `cargo run`
Expected: full collapsible control panel. Dragging sliders live-updates the scene. Orbit-drag/zoom only works when the cursor is NOT over the panel.

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs src/camera.rs src/render/plugin.rs
git commit -m "feat: full egui control panel + pointer routing"
```

---

## Task 18: Render-scale implementation + resize handling

**Files:**
- Modify: `src/render/plugin.rs`

`render_scale` lowers the effective resolution for performance. The cleanest approach in our full-screen-quad setup is to scale the quad smaller and let it stretch — but that distorts. Better: adjust the `Camera2d` to render at lower res. Simplest correct approach for a procedural shader: scale the window's logical resolution used in the shader (multiply `resolution` by `render_scale`) and let the GPU upscale the quad output. Since our shader is fully procedural (no texture detail to lose except stars), the visual cost is minimal.

- [ ] **Step 1: Apply render_scale to the effective resolution**

In `mirror_params` in `src/render/plugin.rs`, change:
```rust
u.resolution = Vec2::new(win.width(), win.height());
```
to:
```rust
u.resolution = Vec2::new(win.width(), win.height()) * params.render_scale;
```
This makes the shader compute rays at fewer "logical" pixels' worth of UV granularity; combined with the quad filling the screen, lower `render_scale` gives a blockier but faster result. (True sub-resolution rendering would need an offscreen render target — out of scope; this gives the perf lever.)

- [ ] **Step 2: Handle window resize (recompute quad aspect)**

Add a system that rebuilds the quad on resize:
```rust
pub fn on_resize(
    mut resize_reader: EventReader<bevy::window::WindowResized>,
    window: Query<&Window>,
    meshes: Res<Assets<Mesh>>,
    mut commands: Commands,
    mut mesh_handles: Query<&Mesh2d>,
) {
    if resize_reader.read().next().is_none() { return; }
    let win = match window.single() { Ok(w) => w, Err(_) => return };
    let aspect = win.width() / win.height();
    // Rebuild the rectangle mesh in place. (Simpler: despawn + respawn; see note.)
    // Minimal: just update aspect in the shader via resolution (already done).
    // Quad is already sized via the Camera2d projection scaling, so no change needed.
    let _ = (meshes, commands, mesh_handles, aspect);
}
```
Add it to `Update` in the plugin. In practice the quad is sized by the `Camera2d` projection and `resolution` updates live, so resize works automatically. This system is a no-op placeholder confirming the resize path; remove if unused after verification.

- [ ] **Step 3: Build, run, resize the window, confirm no distortion**

Run: `cargo run`
Expected: resizing the window keeps the black hole circular and centered (no stretching). Lowering "Render scale" in the panel reduces detail but keeps frame rate up.

- [ ] **Step 4: Commit**

```bash
git add src/render/plugin.rs
git commit -m "feat: render-scale lever + resize handling"
```

---

## Task 19: Web verification + polish

**Files:**
- Modify: `Trunk.toml` (if needed for release profile)
- Modify: `Cargo.toml` (ensure wasm-release profile is used)

- [ ] **Step 1: Run the web build in release**

Run: `trunk build --release`
Expected: produces `dist/` with the wasm artifact. No compile errors.

- [ ] **Step 2: Serve and verify in Chrome/Edge**

Run: `trunk serve`
Open in Chrome/Edge. Expected:
- The black hole renders identically to desktop.
- The egui panel is usable.
- Orbit-drag and zoom work (mouse + touch).
- Performance is interactive (≥30fps); if not, lower default `steps` to 200 and `render_scale` to 0.75 in `params.rs` for web via a `#[cfg(target_arch = "wasm32")]` default.

- [ ] **Step 3: Add web-specific defaults to `params.rs`**

In `BlackHoleParams::default()`, branch on target:
```rust
steps: if cfg!(target_arch = "wasm32") { 200 } else { 300 },
render_scale: if cfg!(target_arch = "wasm32") { 0.75 } else { 1.0 },
```

- [ ] **Step 4: Test the fallback in a non-WebGPU browser**

Open in Safari with WebGPU disabled (or an old Firefox). Expected: the fallback message div appears instead of a blank canvas.

- [ ] **Step 5: Final commit**

```bash
git add Cargo.toml Trunk.toml src/params.rs
git commit -m "feat: web release build + web-tuned defaults + fallback verified"
```

---

## Task 20: Performance check + final documentation

**Files:**
- Create: `README.md`
- Modify: nothing else

- [ ] **Step 1: Measure desktop FPS**

Run: `cargo run --release`, orbit and zoom. Confirm ≥60fps at default settings (steps=300, render_scale=1.0) on a discrete GPU. If below, document recommended `steps=200`.

- [ ] **Step 2: Write `README.md`**

```markdown
# singularity-rs

A real-time, physically-motivated Schwarzschild black-hole renderer in Bevy 0.19.
Geodesic ray-tracing per pixel produces gravitational lensing, the Einstein ring,
a Doppler-beamed accretion disk, lensed stars, and optional lensed planets,
a curvature grid, and a cubemap skybox. Runs on desktop and WebGPU.

## Run

Desktop:
```sh
cargo run --release
```

Web (WebGPU; Chrome/Edge/recent Firefox):
```sh
cargo install --locked trunk
rustup target add wasm32-unknown-unknown
trunk serve
# open http://127.0.0.1:8080
```

## Controls

- **Drag** (mouse) — orbit the camera.
- **Scroll** — zoom.
- **Controls panel** (top-left) — live-tune all parameters.

## Tuning performance

Lower `Steps` and `Render scale` in the Controls panel for higher FPS.
Web defaults to `steps=200`, `render_scale=0.75`.
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README with run + control instructions"
```

---

## Phase 1 complete — acceptance checklist

- [ ] Black shadow appears, sized ~bcrit (≈2.6 Rs), centered.
- [ ] Tilted accretion disk wraps OVER and UNDER the hole (Einstein halo), matching the reference video.
- [ ] Doppler asymmetry: one side brighter; bright side shifts as the camera orbits.
- [ ] Lensed procedural starfield; stars near the hole smear into arcs.
- [ ] (Optional, toggled) Flamm curvature grid dips toward the center and bends near the hole.
- [ ] (Optional) Lensed planets, including secondary images near the hole.
- [ ] (Optional) Cubemap skybox when an asset + intensity > 0.
- [ ] egui panel live-tunes all params; orbit-drag/zoom disabled over the panel.
- [ ] `cargo test` passes (bcrit + capture/escape unit tests).
- [ ] Desktop ≥60fps at default settings; web interactive (≥30fps) on WebGPU.
- [ ] Non-WebGPU browser shows the fallback message.

## Phase 2 follow-up (Kerr — separate spec/plan)

Replace `geodesic_schwarzschild.wgsl` with `geodesic_kerr.wgsl` (Boyer-Lindquist, adaptive RK4, spin parameter). Adds frame-dragging/ergosphere asymmetry. Likely needs `render_scale≈0.5`. Same scene elements, camera, params, UI carry over.
