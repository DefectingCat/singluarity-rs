# Cinematic Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Raise the renderer from "geometrically correct but flat" to Gargantua-reference fidelity: HDR color + ACES tone mapping, multi-pass HDR bloom, domain-warped FBM disk texture, and anti-aliased round stars — all configurable via a dedicated Quality panel with tiered web defaults.

**Architecture:** A 5-stage HDR pipeline replaces the 2-pass offscreen→upscale path: `Offscreen (Rgba16Float) → BrightPass → Down-pyramid → Up-pyramid → Composite+ACES`. The geodesic integrator is untouched; all changes are in compositing/shading.

**Tech Stack:** Bevy 0.19, WGSL, egui 0.41. No new crate dependencies.

**Spec:** `docs/superpowers/specs/2026-07-14-blackhole-cinematic-rendering-design.md`

---

## Conventions used throughout this plan

**Camera `order` is `i32`; lower renders first.** The full pipeline order, set once and referenced by all tasks:

| Camera        | order | Renders into      |
|---------------|-------|-------------------|
| offscreen     | -20   | offscreen_hdr     |
| brightpass    | -19   | bloom_0           |
| blur down01   | -18   | bloom_1           |
| blur down12   | -17   | bloom_2           |
| blur up21     | -16   | bloom_1           |
| blur up10     | -15   | bloom_final       |
| composite     | 0     | window            |

**RenderLayers** isolate each camera's draw to its own quad: layer 0 = offscreen, 1 = composite, 2 = brightpass, 3 = down01, 4 = down12, 5 = up21, 6 = up10.

**`QuadScaleFactor(f32, f32)`** — a component on every quad storing the fraction of the offscreen resolution that the quad's target fills. Used by `resize_offscreen` to rescale each quad independently:
- offscreen quad: `(1.0, 1.0)`
- brightpass (writes bloom_0 at half-res): `(0.5, 0.5)`
- blur down01 (writes bloom_1 at quarter): `(0.25, 0.25)`
- blur down12 (writes bloom_2 at eighth): `(0.125, 0.125)`
- blur up21 (writes bloom_1 at quarter): `(0.25, 0.25)`
- blur up10 (writes bloom_final at half): `(0.5, 0.5)`
- composite quad: rescaled against the WINDOW (not offscreen), so it carries a `CompositeQuad` marker instead and is handled separately in resize.

---

## File Structure

**Created:**
- `assets/shaders/brightpass.wgsl` — stage [2]: extracts luminance > threshold with soft knee
- `assets/shaders/blur.wgsl` — stages [3]/[4]: one shader, down/up modes via uniform
- `assets/shaders/composite.wgsl` — stage [5]: ACES tone-map + bloom blend, replaces upscale.wgsl

**Modified:**
- `assets/shaders/black_hole.wgsl` — FBM disk noise, gaussian-speck stars, `star_aa` branch, new uniform fields read
- `src/render/material.rs` — add `BrightPassMaterial`, `BlurMaterial`, `CompositeMaterial`; add new uniform fields to `BlackHoleUniforms`; remove `UpscaleMaterial`
- `src/render/plugin.rs` — spawn bloom targets/materials/cameras; `Nudgable` + `QuadScaleFactor` + `CompositeQuad` markers; generalize `nudge_camera`; bloom quality rebuild on param change
- `src/params.rs` — add `BloomQuality` enum + quality fields to `BlackHoleParams`; tiered defaults
- `src/ui.rs` — new `Quality` panel section

**Deleted:**
- `assets/shaders/upscale.wgsl`

---

## Task 0: Establish green baseline

**Files:** none

- [ ] **Step 1: Run the existing test suite**

Run: `cargo test`
Expected: all physics tests pass (b_crit capture/escape, spin=0 degeneracy, spin>0 capture).

- [ ] **Step 2: Confirm the desktop build runs**

Run: `cargo build --release`
Expected: compiles without error.

- [ ] **Step 3: No commit** — verification gate only.

---

## Task 1: Offscreen format → Rgba16Float + camera order + markers

**Files:**
- Modify: `src/render/plugin.rs` (offscreen texture format, offscreen camera order, add `Nudgable`/`QuadScaleFactor`/`CompositeQuad` markers to offscreen + upscale cameras/quads, generalize `nudge_camera`)

This switches the offscreen render target to float, sets up the camera order for the bloom pipeline, and introduces the marker components all later tasks depend on. No bloom yet — the upscale path still works.

- [ ] **Step 1: Define the new marker components**

In `src/render/plugin.rs`, after the `UpscaleQuad` component definition (around line 34), add:

```rust
/// Marker for any camera that must be nudged each frame (Bevy 0.19 #24448
/// workaround). All render cameras carry this.
#[derive(Component)]
pub struct Nudgable;

/// Stores the fraction of the offscreen resolution that this quad's target
/// fills. Used by resize_offscreen to rescale each quad independently.
/// (1.0, 1.0) = full offscreen res; (0.5, 0.5) = half; etc.
#[derive(Component)]
pub struct QuadScaleFactor(pub f32, pub f32);

/// Marks the composite quad (renders to the window, not an offscreen Image).
/// resize_offscreen rescales it against the window, not the offscreen res.
#[derive(Component)]
pub struct CompositeQuad;
```

- [ ] **Step 2: Change the offscreen texture format in `spawn_fullscreen_quad`**

In `src/render/plugin.rs`, find the `Image::new_target_texture` call in `spawn_fullscreen_quad` (around line 84) and change the format:

```rust
    let offscreen = images.add(Image::new_target_texture(
        w,
        h,
        TextureFormat::Rgba16Float,  // was Bgra8UnormSrgb — HDR for bloom headroom
        None,
    ));
```

- [ ] **Step 3: Add `QuadScaleFactor` to the offscreen quad**

In `spawn_fullscreen_quad`, in the offscreen quad spawn (the `FullscreenQuad` `commands.spawn(...)`, around line 111), add `QuadScaleFactor(1.0, 1.0)` to the tuple:

```rust
        Transform::default().with_scale(Vec3::new(half_w, half_h, 1.0)),
        FullscreenQuad,
        QuadScaleFactor(1.0, 1.0),
        RenderLayers::layer(0),
```

- [ ] **Step 4: Set the offscreen camera order to -20 and add `Nudgable`**

In `spawn_fullscreen_quad`, in the offscreen camera spawn (the `commands.spawn((Camera2d, Camera { order: -1, ... }))` around line 119), change `order: -1` to `order: -20` and add `Nudgable,`:

```rust
        Camera {
            order: -20,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.1, 0.1, 0.1)),
            ..default()
        },
        RenderTarget::Image(offscreen.clone().into()),
        Msaa::Off,
        OffscreenCamera,
        Nudgable,
        RenderLayers::layer(0),
```

- [ ] **Step 5: Tag the upscale quad + camera with `CompositeQuad` + `Nudgable`**

In `spawn_fullscreen_quad`, in the upscale quad spawn (around line 133), add `CompositeQuad,`:

```rust
        Transform::default().with_scale(Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0)),
        UpscaleQuad,
        CompositeQuad,
        RenderLayers::layer(1),
```

In the upscale camera spawn (around line 142), add `Nudgable,`:

```rust
    commands.spawn((Camera2d, Msaa::Off, UpscaleCamera, Nudgable, RenderLayers::layer(1)));
```

- [ ] **Step 6: Generalize `nudge_camera` to use the `Nudgable` marker**

In `src/render/plugin.rs`, replace the `nudge_camera` function signature and query:

```rust
fn nudge_camera(
    time: Res<Time>,
    mut camera: Query<&mut Transform, With<Nudgable>>,
) {
    let nudge = (time.elapsed_secs() * 5.0).sin() * 1e-3;
    for mut t in &mut camera {
        t.translation.x = nudge;
    }
}
```

- [ ] **Step 7: Update `resize_offscreen` to use `QuadScaleFactor`**

In `src/render/plugin.rs`, replace the `quads` ParamSet in `resize_offscreen` and the rescale loops. The new version rescales offscreen+bloom quads by their `QuadScaleFactor` against the offscreen resolution, and the composite quad against the window. Replace the existing `mut quads: ParamSet<( ... )>` parameter and the two `for` loops at the end with:

```rust
    mut quads: ParamSet<(
        // p0: offscreen + bloom quads — rescaled against offscreen resolution.
        Query<(&mut Transform, &QuadScaleFactor), Without<CompositeQuad>>,
        // p1: composite quad — rescaled against window resolution.
        Query<&mut Transform, With<CompositeQuad>>,
    )>,
```

and replace the final two `for` loops with:

```rust
    // Rescale offscreen + bloom quads against the offscreen resolution.
    for (mut t, f) in &mut quads.p0() {
        t.scale = Vec3::new(w as f32 * f.0 / 2.0, h as f32 * f.1 / 2.0, 1.0);
    }
    // Rescale the composite quad against the window resolution.
    for mut t in &mut quads.p1() {
        t.scale = Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0);
    }
```

Also change the offscreen image resize format from `Bgra8UnormSrgb` to `Rgba16Float` in `resize_offscreen` (around line 170):

```rust
    let img = Image::new_target_texture(w, h, TextureFormat::Rgba16Float, None);
```

- [ ] **Step 8: Build and visually verify**

Run: `cargo run --release`
Expected: the black hole renders, possibly slightly brighter/blown-out in the disk core (over-bright values now reach the window unclamped). ACES is added in Task 6. No crash; the nudge still works (all cameras carry `Nudgable`).

- [ ] **Step 9: Commit**

```bash
git add src/render/plugin.rs
git commit -m "render: offscreen → Rgba16Float + Nudgable/QuadScaleFactor/CompositeQuad markers

Switch the offscreen target from Bgra8UnormSrgb to Rgba16Float for HDR
headroom (disk brightness can exceed 1.0 without clamping). Set offscreen
camera order to -20 (the bloom pipeline, added next, needs cameras ordered
so offscreen renders first). Introduce three marker components:

- Nudgable: all render cameras carry it; nudge_camera now queries this
  instead of a hardcoded Or<(OffscreenCamera, UpscaleCamera)>.
- QuadScaleFactor(fx, fy): fraction of offscreen res each quad fills;
  resize_offscreen rescales each quad independently via this.
- CompositeQuad: the final quad renders to the window (rescaled against
  window res, not offscreen).

The upscale path still works (textureSample reads float textures);
ACES tone-mapping is added in a later commit."
```

---

## Task 2: Domain-warped FBM disk texture

**Files:**
- Modify: `assets/shaders/black_hole.wgsl:143-171` (`disk_color`) and add `hash33`/`value_noise3`/`fbm3`/`disk_noise` helpers before `disk_color`

Replace the two-sine noise with domain-warped FBM for the feathered/smoky disk texture. The geodesic integrator is untouched.

- [ ] **Step 1: Add FBM helper functions before `disk_color`**

In `assets/shaders/black_hole.wgsl`, insert these functions immediately before `fn disk_color` (before line 143):

```wgsl
// --- disk noise (domain-warped FBM) ---
fn hash33(p: vec3<f32>) -> vec3<f32> {
    let q = vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    );
    return fract(sin(q) * 43758.5453);
}

fn value_noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = dot(hash33(i + vec3<f32>(0.0, 0.0, 0.0)) - 0.5, vec3<f32>(1.0));
    let n100 = dot(hash33(i + vec3<f32>(1.0, 0.0, 0.0)) - 0.5, vec3<f32>(1.0));
    let n010 = dot(hash33(i + vec3<f32>(0.0, 1.0, 0.0)) - 0.5, vec3<f32>(1.0));
    let n110 = dot(hash33(i + vec3<f32>(1.0, 1.0, 0.0)) - 0.5, vec3<f32>(1.0));
    let n001 = dot(hash33(i + vec3<f32>(0.0, 0.0, 1.0)) - 0.5, vec3<f32>(1.0));
    let n101 = dot(hash33(i + vec3<f32>(1.0, 0.0, 1.0)) - 0.5, vec3<f32>(1.0));
    let n011 = dot(hash33(i + vec3<f32>(0.0, 1.0, 1.0)) - 0.5, vec3<f32>(1.0));
    let n111 = dot(hash33(i + vec3<f32>(1.0, 1.0, 1.0)) - 0.5, vec3<f32>(1.0));
    let nx00 = mix(n000, n100, u.x);
    let nx10 = mix(n010, n110, u.x);
    let nx01 = mix(n001, n101, u.x);
    let nx11 = mix(n011, n111, u.x);
    let nxy0 = mix(nx00, nx10, u.y);
    let nxy1 = mix(nx01, nx11, u.y);
    return mix(nxy0, nxy1, u.z) + 0.5;
}

fn fbm3(p: vec3<f32>, octaves: u32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i: u32 = 0u; i < octaves; i = i + 1u) {
        sum = sum + amp * value_noise3(p * freq);
        freq = freq * 2.0;
        amp = amp * 0.5;
    }
    return sum;
}

fn disk_noise(pos: vec3<f32>, t: f32) -> f32 {
    let warp = fbm3(pos * 0.8 + vec3<f32>(0.0, 0.0, t * 0.1), 3u);
    let n = fbm3(pos * 2.0 + warp * 1.5 + vec3<f32>(0.0, 0.0, t * 0.3), 4u);
    return n;
}
```

- [ ] **Step 2: Replace the noise computation in `disk_color`**

In `assets/shaders/black_hole.wgsl`, in `disk_color`, replace these four lines (the `rot`, `n`, `n2`, `noise` computation around lines 147-150):

```wgsl
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    let n = sin(phi * 8.0 + rot) * 0.5 + 0.5;
    let n2 = sin(phi * 23.0 - rot * 1.7 + r * 2.0) * 0.5 + 0.5;
    let noise = mix(n, n2, 0.4);
```

with:

```wgsl
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    // Domain-warped FBM for feathered/smoky gas texture. The Keplerian shear
    // (rot ∝ 1/r^1.5) is folded into the noise flow term so inner radii flow
    // faster than outer — correct differential rotation.
    let noise = disk_noise(vec3<f32>(r * 0.5, phi * 0.3, rot), uniforms.time);
```

- [ ] **Step 3: Build and visually verify**

Run: `cargo run --release`
Expected: the disk shows feathered, turbulent structure instead of banded sine waves. The brightness gradient (white-hot inner → orange outer) and Doppler asymmetry are preserved. If the noise looks too flat or too busy, the `0.5`/`0.3`/`2.0` constants in `disk_noise`/`disk_color` are the tuning knobs.

- [ ] **Step 4: Commit**

```bash
git add assets/shaders/black_hole.wgsl
git commit -m "shader: domain-warped FBM disk texture replaces two-sine noise

The disk's noise was two sin() terms (phi*8, phi*23) producing geometric
bands. Replace with domain-warped FBM: a 3-octave value-noise field warps
the sampling coordinate of a 4-octave FBM, giving the feathered/smoky gas
texture of the Gargantua reference. The Keplerian shear (rot ∝ 1/r^1.5)
is retained as the flow time term so inner radii rotate faster."
```

---

## Task 3: Gaussian-speck stars + star_aa flag + bloom/exposure uniforms

**Files:**
- Modify: `assets/shaders/black_hole.wgsl` (`BlackHoleUniforms` struct + `star_color`)
- Modify: `src/render/material.rs` (`BlackHoleUniforms` fields + defaults)
- Modify: `src/render/plugin.rs` (`mirror_params`)
- Modify: `src/params.rs` (add fields + defaults)

Replace blocky smoothstep stars with a gaussian speck (round, anti-aliased). Add the `star_aa` toggle and the bloom/exposure uniform fields (used by later tasks, with safe defaults).

- [ ] **Step 1: Add uniform fields to the WGSL `BlackHoleUniforms` struct**

In `assets/shaders/black_hole.wgsl`, in the `BlackHoleUniforms` struct, replace the final two lines:

```wgsl
    spin: f32,
    _pad5: f32,
};
```

with:

```wgsl
    spin: f32,
    star_aa: u32,
    bloom_threshold: f32,
    bloom_strength: f32,
    exposure: f32,
    _pad5: f32,
};
```

- [ ] **Step 2: Add the matching fields to the Rust `BlackHoleUniforms` struct**

In `src/render/material.rs`, in the `BlackHoleUniforms` struct, replace:

```rust
    pub spin: f32,       // Phase 2: dimensionless Kerr spin χ = a/M ∈ [0,1].
    pub _pad5: f32,
}
```

with:

```rust
    pub spin: f32,       // Phase 2: dimensionless Kerr spin χ = a/M ∈ [0,1].
    pub star_aa: u32,
    pub bloom_threshold: f32,
    pub bloom_strength: f32,
    pub exposure: f32,
    pub _pad5: f32,
}
```

- [ ] **Step 3: Set defaults for the new fields**

In `src/render/material.rs`, in `impl Default for BlackHoleUniforms`, replace:

```rust
            spin: 0.0,
            _pad5: 0.0,
        }
    }
}
```

with:

```rust
            spin: 0.0,
            star_aa: 1,
            bloom_threshold: 1.0,
            bloom_strength: 0.8,
            exposure: 1.0,
            _pad5: 0.0,
        }
    }
}
```

- [ ] **Step 4: Add the fields to `BlackHoleParams`**

In `src/params.rs`, in the `BlackHoleParams` struct, after `pub spin: f32,`, add:

```rust
    // Quality (Phase 3: cinematic rendering)
    pub star_aa: bool,
    pub bloom_threshold: f32,
    pub bloom_strength: f32,
    pub exposure: f32,
```

- [ ] **Step 5: Set defaults in `BlackHoleParams::default`**

In `src/params.rs`, in `impl Default for BlackHoleParams`, replace:

```rust
            spin: 0.0,
        }
    }
}
```

with:

```rust
            spin: 0.0,
            star_aa: if cfg!(target_arch = "wasm32") { false } else { true },
            bloom_threshold: 1.0,
            bloom_strength: 0.8,
            exposure: 1.0,
        }
    }
}
```

- [ ] **Step 6: Mirror the new fields in `mirror_params`**

In `src/render/plugin.rs`, in `mirror_params`, after the `u.spin = params.spin;` line, add:

```rust
        u.star_aa = params.star_aa as u32;
        u.bloom_threshold = params.bloom_threshold;
        u.bloom_strength = params.bloom_strength;
        u.exposure = params.exposure;
```

- [ ] **Step 7: Replace `star_color` with the gaussian-speck version**

In `assets/shaders/black_hole.wgsl`, replace the entire `star_color` function (lines 83-97):

```wgsl
fn star_color(dir: vec3<f32>, intensity: f32) -> vec3<f32> {
    let scale = 80.0;
    let cell = floor(dir * scale);
    let h = hash13(cell);
    let threshold = 0.985;
    if (h > threshold) {
        let b = (h - threshold) / (1.0 - threshold);
        let col = mix(vec3<f32>(0.6, 0.7, 1.0), vec3<f32>(1.0, 0.9, 0.7), b);
        let f = abs(dir * scale - cell);
        let d = max(f.x, max(f.y, f.z));
        let falloff = smoothstep(0.5, 0.0, d);
        return col * b * falloff * 3.0 * intensity;
    }
    return vec3<f32>(0.0);
}
```

with:

```wgsl
fn star_color(dir: vec3<f32>, intensity: f32) -> vec3<f32> {
    let scale = 80.0;
    let p = dir * scale;
    let cell = floor(p);
    let h = hash13(cell);
    let threshold = 0.985;
    if (h > threshold) {
        let b = (h - threshold) / (1.0 - threshold);
        let col = mix(vec3<f32>(0.6, 0.7, 1.0), vec3<f32>(1.0, 0.9, 0.7), b);
        if (uniforms.star_aa != 0u) {
            // Gaussian speck: distance to cell center, soft radial falloff.
            // Produces a round 2-3 pixel anti-aliased disk instead of a square.
            let center = cell + vec3<f32>(0.5);
            let dist = length(p - center);
            let radius = 0.25 + b * 0.4;
            let falloff = exp(-dist * dist / (radius * radius));
            return col * b * falloff * 4.0 * intensity;
        } else {
            // Original fast path: square-cell smoothstep (blocky but cheap).
            let f = abs(p - cell);
            let d = max(f.x, max(f.y, f.z));
            let falloff = smoothstep(0.5, 0.0, d);
            return col * b * falloff * 3.0 * intensity;
        }
    }
    return vec3<f32>(0.0);
}
```

- [ ] **Step 8: Build and visually verify**

Run: `cargo run --release`
Expected: stars are round points instead of rectangular pixel blobs (desktop, `star_aa` defaults true). The starfield looks sharper.

- [ ] **Step 9: Commit**

```bash
git add assets/shaders/black_hole.wgsl src/render/material.rs src/render/plugin.rs src/params.rs
git commit -m "shader: gaussian-speck stars + star_aa toggle + bloom/exposure uniforms

star_color's smoothstep(0.5,0.0,d) with scale=80 produced blocky rectangles
at low render_scale. Replace with a gaussian speck: distance to cell center
with exp(-d²/r²) falloff, producing a round 2-3 pixel anti-aliased disk.
star_aa uniform toggles between the gaussian path (desktop default) and the
old square-cell fast path (web default).

Also adds bloom_threshold/bloom_strength/exposure uniform fields to
BlackHoleUniforms (used by later bloom tasks) with safe defaults."
```

---

## Task 4: BrightPass material + shader + camera

**Files:**
- Create: `assets/shaders/brightpass.wgsl`
- Modify: `src/render/material.rs` (add `BrightPassMaterial`)
- Modify: `src/render/plugin.rs` (add target/quad/camera marker components, register plugin, spawn brightpass)

Adds stage [2]: extracts luminance > threshold from the HDR offscreen into a half-res float texture.

- [ ] **Step 1: Create the brightpass shader**

Create `assets/shaders/brightpass.wgsl`:

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct BrightPassUniform {
    threshold: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: BrightPassUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var samp: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(tex, samp, in.uv).rgb;
    let lum = dot(hdr, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Soft knee: smooth roll-off instead of a hard threshold cut.
    // Fully-bright passes through; near-threshold tapers to zero.
    let soft = max(lum - u.threshold, 0.0) / (lum + 0.0001);
    let contribution = hdr * soft;
    return vec4<f32>(contribution, 1.0);
}
```

- [ ] **Step 2: Add `BrightPassMaterial` to `src/render/material.rs`**

At the end of `src/render/material.rs`, add:

```rust
/// Extracts luminance above a threshold from the HDR offscreen into a
/// half-res float texture (bloom stage [2]). Soft-knee, not hard cut.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BrightPassMaterial {
    #[uniform(0)]
    pub threshold: f32,
    #[texture(1)]
    #[sampler(2)]
    pub source: Handle<Image>,
}

impl Material2d for BrightPassMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/brightpass.wgsl".into()
    }
}
```

- [ ] **Step 3: Add bloom target/quad/camera marker components**

In `src/render/plugin.rs`, after the `CompositeQuad` component (added in Task 1 Step 1), add:

```rust
/// The half-res Image the bright-pass writes into (bloom stage [2]).
#[derive(Component)]
pub struct BloomTarget0(pub Handle<Image>);

/// Camera + quad markers for the bright-pass (for rebuild queries).
#[derive(Component)]
pub struct BrightPassCamera;
#[derive(Component)]
pub struct BrightPassQuad;

/// Camera + quad markers for the blur passes (for rebuild queries).
#[derive(Component)]
pub struct BlurCamera;
#[derive(Component)]
pub struct BlurQuad;
```

- [ ] **Step 4: Register the BrightPass material plugin**

In `src/render/plugin.rs`, in `BlackHolePlugin::build`, after the `UpscaleMaterial` plugin line (around line 44), add:

```rust
            .add_plugins(Material2dPlugin::<crate::render::material::BrightPassMaterial>::default())
```

- [ ] **Step 5: Spawn the brightpass target, quad, and camera**

In `src/render/plugin.rs`, in `spawn_fullscreen_quad`, after the offscreen camera spawn block (after the `OffscreenCamera` `commands.spawn(...)` ending around line 130), insert:

```rust
    // --- Bright-pass (bloom stage [2]): half-res float target ---
    let bw = ((w as f32 * 0.5) as u32).max(1);
    let bh = ((h as f32 * 0.5) as u32).max(1);
    let bloom0 = images.add(Image::new_target_texture(
        bw, bh, TextureFormat::Rgba16Float, None,
    ));
    commands.spawn(BloomTarget0(bloom0.clone()));
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(crate::render::material::BrightPassMaterial {
            threshold: 1.0,
            source: offscreen.clone(),
        })),
        Transform::default().with_scale(Vec3::new(bw as f32 / 2.0, bh as f32 / 2.0, 1.0)),
        BrightPassQuad,
        QuadScaleFactor(0.5, 0.5),
        RenderLayers::layer(2),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            order: -19,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.0, 0.0, 0.0)),
            ..default()
        },
        RenderTarget::Image(bloom0.clone().into()),
        Msaa::Off,
        BrightPassCamera,
        Nudgable,
        RenderLayers::layer(2),
    ));
```

- [ ] **Step 6: Build and visually verify**

Run: `cargo run --release`
Expected: the main image is unchanged (the brightpass runs but its output isn't composited yet — dead-end texture). Confirm no crash, existing render still shows.

- [ ] **Step 7: Commit**

```bash
git add assets/shaders/brightpass.wgsl src/render/material.rs src/render/plugin.rs
git commit -m "render: bright-pass material + camera (bloom stage [2])

Adds BrightPassMaterial + brightpass.wgsl: extracts luminance above a
soft-knee threshold from the HDR offscreen into a half-res Rgba16Float
target (bloom_0). Camera order -19 (after offscreen at -20). The brightpass
output is not yet composited (Task 6); this commit only adds the pass and
confirms it runs without crashing."
```

---

## Task 5: Blur material + shader + down/up pyramid

**Files:**
- Create: `assets/shaders/blur.wgsl`
- Modify: `src/render/material.rs` (add `BlurMaterial` + `BlurUniform`)
- Modify: `src/render/plugin.rs` (add `BloomTarget1/2` + `BloomFinalTarget`, register plugin, spawn 4 blur passes)

Adds stages [3] and [4]: the downsample then upsample pyramid. For High quality (3 bloom textures): bloom_0 (half, from brightpass) → bloom_1 (quarter) → bloom_2 (eighth), then upsample to bloom_final (half). 2 down passes + 2 up passes = 4 BlurMaterial instances.

- [ ] **Step 1: Create the blur shader**

Create `assets/shaders/blur.wgsl`:

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct BlurUniform {
    mode: u32,           // 0 = downsample, 1 = upsample
    texel_size: vec2<f32>,
    blend: f32,          // upsample blend factor (ignored for down)
    _pad0: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: BlurUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var samp: sampler;

// 13-tap weighted kernel (Gaussian approximation for HDR bloom).
// NOTE: naga rejects dynamic indexing of const arrays ("may only be indexed
// by a constant"), so we use var<private> here — it allows runtime indexing.
var<private> KERNEL_OFFSETS: array<vec2<f32>, 13> = array<vec2<f32>, 13>(
    vec2<f32>( 0.0,  0.0),
    vec2<f32>( 1.0,  0.0), vec2<f32>(-1.0,  0.0),
    vec2<f32>( 0.0,  1.0), vec2<f32>( 0.0, -1.0),
    vec2<f32>( 1.0,  1.0), vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0, -1.0), vec2<f32>(-1.0, -1.0),
    vec2<f32>( 2.0,  0.0), vec2<f32>(-2.0,  0.0),
    vec2<f32>( 0.0,  2.0), vec2<f32>( 0.0, -2.0),
);
var<private> KERNEL_WEIGHTS: array<f32, 13> = array<f32, 13>(
    0.5,
    0.25, 0.25, 0.25, 0.25,
    0.125, 0.125, 0.125, 0.125,
    0.0625, 0.0625, 0.0625, 0.0625,
);

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    // Downsample and upsample share the same 13-tap weighted kernel.
    // mode 0 = down (plain weighted average), mode 1 = up (scaled by blend).
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var i: i32 = 0; i < 13; i = i + 1) {
        let off = KERNEL_OFFSETS[i] * u.texel_size;
        let c = textureSample(tex, samp, uv + off).rgb;
        sum = sum + c * KERNEL_WEIGHTS[i];
        wsum = wsum + KERNEL_WEIGHTS[i];
    }
    let blurred = sum / wsum;
    if (u.mode == 0u) {
        return vec4<f32>(blurred, 1.0);
    } else {
        return vec4<f32>(blurred * u.blend, 1.0);
    }
}
```

- [ ] **Step 2: Add `BlurMaterial` + `BlurUniform` to `src/render/material.rs`**

At the end of `src/render/material.rs`, add:

```rust
/// Uniform for one blur pass (bloom stages [3]/[4]).
#[derive(Clone, ShaderType)]
pub struct BlurUniform {
    pub mode: u32,           // 0 = downsample, 1 = upsample
    pub texel_size: Vec2,
    pub blend: f32,          // upsample blend factor (ignored for down)
    pub _pad0: f32,
}

/// One pass of the bloom down/up pyramid. One material instance per pass
/// (Bevy binds uniforms once per material, not per entity).
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BlurMaterial {
    #[uniform(0)]
    pub uniform: BlurUniform,
    #[texture(1)]
    #[sampler(2)]
    pub source: Handle<Image>,
}

impl Material2d for BlurMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/blur.wgsl".into()
    }
}
```

- [ ] **Step 3: Add the remaining bloom target components + register the Blur plugin**

In `src/render/plugin.rs`, after the `BlurQuad` component (Task 4 Step 3), add:

```rust
/// Bloom pyramid textures bloom_1, bloom_2 (bloom_0 is BloomTarget0).
#[derive(Component)]
pub struct BloomTarget1(pub Handle<Image>);
#[derive(Component)]
pub struct BloomTarget2(pub Handle<Image>);

/// The final up-sampled bloom texture read by the composite pass.
#[derive(Component)]
pub struct BloomFinalTarget(pub Handle<Image>);
```

In `BlackHolePlugin::build`, after the `BrightPassMaterial` plugin line (Task 4 Step 4), add:

```rust
            .add_plugins(Material2dPlugin::<crate::render::material::BlurMaterial>::default())
```

- [ ] **Step 4: Spawn the blur pyramid in `spawn_fullscreen_quad`**

In `src/render/plugin.rs`, in `spawn_fullscreen_quad`, after the brightpass spawn block from Task 4 Step 5, insert:

```rust
    // --- Blur pyramid (bloom stages [3]/[4]): bloom_1, bloom_2 + down/up passes ---
    let b1w = ((w as f32 * 0.25) as u32).max(1);
    let b1h = ((h as f32 * 0.25) as u32).max(1);
    let b2w = ((w as f32 * 0.125) as u32).max(1);
    let b2h = ((h as f32 * 0.125) as u32).max(1);
    let bloom1 = images.add(Image::new_target_texture(
        b1w, b1h, TextureFormat::Rgba16Float, None,
    ));
    let bloom2 = images.add(Image::new_target_texture(
        b2w, b2h, TextureFormat::Rgba16Float, None,
    ));
    commands.spawn(BloomTarget1(bloom1.clone()));
    commands.spawn(BloomTarget2(bloom2.clone()));

    // Down pass 0→1: samples bloom0 (half-res), writes bloom1 (quarter-res).
    let down01_texel = Vec2::new(1.0 / bw as f32, 1.0 / bh as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 0, texel_size: down01_texel, blend: 0.0, _pad0: 0.0,
            },
            source: bloom0.clone(),
        })),
        Transform::default().with_scale(Vec3::new(b1w as f32 / 2.0, b1h as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.25, 0.25),
        RenderLayers::layer(3),
    ));
    // Down pass 1→2: samples bloom1 (quarter), writes bloom2 (eighth).
    let down12_texel = Vec2::new(1.0 / b1w as f32, 1.0 / b1h as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 0, texel_size: down12_texel, blend: 0.0, _pad0: 0.0,
            },
            source: bloom1.clone(),
        })),
        Transform::default().with_scale(Vec3::new(b2w as f32 / 2.0, b2h as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.125, 0.125),
        RenderLayers::layer(4),
    ));
    // Up pass 2→1: samples bloom2 (eighth), writes bloom1 (quarter).
    // The up pass OVERWRITES bloom1 (Camera2d clears before drawing) — that's
    // fine; the composite pass adds the final bloom to the HDR scene.
    let up21_texel = Vec2::new(1.0 / b2w as f32, 1.0 / b2h as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 1, texel_size: up21_texel, blend: 0.6, _pad0: 0.0,
            },
            source: bloom2.clone(),
        })),
        Transform::default().with_scale(Vec3::new(b1w as f32 / 2.0, b1h as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.25, 0.25),
        RenderLayers::layer(5),
    ));
    // Up pass 1→0: samples bloom1 (quarter), writes bloom_final (half).
    let bfw = bw;
    let bfh = bh;
    let bloom_final = images.add(Image::new_target_texture(
        bfw, bfh, TextureFormat::Rgba16Float, None,
    ));
    commands.spawn(BloomFinalTarget(bloom_final.clone()));
    let up10_texel = Vec2::new(1.0 / b1w as f32, 1.0 / b1h as f32);
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(materials.add(crate::render::material::BlurMaterial {
            uniform: crate::render::material::BlurUniform {
                mode: 1, texel_size: up10_texel, blend: 0.8, _pad0: 0.0,
            },
            source: bloom1.clone(),
        })),
        Transform::default().with_scale(Vec3::new(bfw as f32 / 2.0, bfh as f32 / 2.0, 1.0)),
        BlurQuad,
        QuadScaleFactor(0.5, 0.5),
        RenderLayers::layer(6),
    ));
    // Cameras: down01=-18, down12=-17, up21=-16, up10=-15 (after brightpass
    // at -19, before composite at 0). Each renders to its target Image.
    commands.spawn((
        Camera2d, Camera { order: -18, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom1.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(3),
    ));
    commands.spawn((
        Camera2d, Camera { order: -17, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom2.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(4),
    ));
    commands.spawn((
        Camera2d, Camera { order: -16, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom1.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(5),
    ));
    commands.spawn((
        Camera2d, Camera { order: -15, clear_color: ClearColorConfig::Custom(Color::srgb(0.0,0.0,0.0)), ..default() },
        RenderTarget::Image(bloom_final.clone().into()), Msaa::Off, BlurCamera, Nudgable, RenderLayers::layer(6),
    ));
```

- [ ] **Step 5: Build and visually verify**

Run: `cargo run --release`
Expected: main image unchanged (bloom output not composited yet). Confirm no crash; blur passes run into dead-end textures.

- [ ] **Step 6: Commit**

```bash
git add assets/shaders/blur.wgsl src/render/material.rs src/render/plugin.rs
git commit -m "render: blur pyramid (bloom stages [3]/[4]) — down then up

Adds BlurMaterial + blur.wgsl with a 13-tap weighted Gaussian kernel
(var<private> arrays — naga rejects dynamic indexing of const arrays),
run in downsample then upsample passes. For the 3-level pyramid:
bloom_0 (half, from brightpass) → bloom_1 (quarter) → bloom_2 (eighth),
then upsample to bloom_final (half). Camera orders -18/-17/-16/-15.

The bloom output is not yet composited (Task 6); this commit only wires
the passes and confirms they run in sequence without crashing."
```

---

## Task 6: Composite material + shader + ACES (replaces upscale)

**Files:**
- Create: `assets/shaders/composite.wgsl`
- Modify: `src/render/material.rs` (add `CompositeMaterial` + `CompositeUniform`, remove `UpscaleMaterial`)
- Modify: `src/render/plugin.rs` (replace upscale quad/material/camera with composite; register plugin; update `mirror_params`; update `resize_offscreen` for bloom targets)
- Delete: `assets/shaders/upscale.wgsl`

Adds stage [5]: ACES tone-map + bloom blend, writing to the window. Replaces the old upscale path.

- [ ] **Step 1: Create the composite shader**

Create `assets/shaders/composite.wgsl`:

```wgsl
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct CompositeUniform {
    bloom_strength: f32,
    exposure: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: CompositeUniform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var scene_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var scene_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var bloom_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var bloom_samp: sampler;

// ACES Narkowicz fit (5 ops, clamped to [0,1]).
fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(scene_tex, scene_samp, in.uv).rgb;
    let bloom = textureSample(bloom_tex, bloom_samp, in.uv).rgb;
    let combined = hdr + bloom * u.bloom_strength;
    let mapped = aces_tonemap(combined * u.exposure);
    return vec4<f32>(mapped, 1.0);
}
```

- [ ] **Step 2: Add `CompositeMaterial` + `CompositeUniform` to `src/render/material.rs`**

At the end of `src/render/material.rs`, add:

```rust
/// Uniform for the composite pass (bloom stage [5]).
#[derive(Clone, ShaderType)]
pub struct CompositeUniform {
    pub bloom_strength: f32,
    pub exposure: f32,
    pub _pad0: f32,
    pub _pad1: f32,
}

/// Final stage: tone-maps the HDR scene + bloom to LDR for the window.
/// Replaces UpscaleMaterial. ACES (Narkowicz) tone mapping.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct CompositeMaterial {
    #[uniform(0)]
    pub uniform: CompositeUniform,
    #[texture(1)]
    #[sampler(2)]
    pub scene: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub bloom: Handle<Image>,
}

impl Material2d for CompositeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/composite.wgsl".into()
    }
}
```

- [ ] **Step 3: Remove `UpscaleMaterial` from `src/render/material.rs`**

Delete the entire `UpscaleMaterial` struct and its `impl Material2d` block (the lines from `/// Samples the sub-resolution offscreen render...` through the end of its impl, around lines 118-131).

- [ ] **Step 4: Register the Composite material plugin, remove the Upscale plugin**

In `src/render/plugin.rs`, in `BlackHolePlugin::build`, replace the line:

```rust
            .add_plugins(Material2dPlugin::<crate::render::material::UpscaleMaterial>::default())
```

with:

```rust
            .add_plugins(Material2dPlugin::<crate::render::material::CompositeMaterial>::default())
```

- [ ] **Step 5: Replace the upscale quad/camera with composite in `spawn_fullscreen_quad`**

First, update the `spawn_fullscreen_quad` signature — the `upscale_materials` parameter held `UpscaleMaterial`; now it must hold `CompositeMaterial`. Change:

```rust
    mut upscale_materials: ResMut<Assets<crate::render::material::UpscaleMaterial>>,
```

to:

```rust
    mut composite_materials: ResMut<Assets<crate::render::material::CompositeMaterial>>,
```

Then find the upscale quad + camera spawn block (the `--- Upscale quad ---` section, around lines 132-142) and replace it with:

```rust
    // --- Composite quad (draws HDR scene + bloom to the window, ACES tone-mapped) ---
    // bloom_final comes from the blur pyramid (Task 5 Step 4).
    let composite_mat = composite_materials.add(crate::render::material::CompositeMaterial {
        uniform: crate::render::material::CompositeUniform {
            bloom_strength: 0.8, exposure: 1.0, _pad0: 0.0, _pad1: 0.0,
        },
        scene: offscreen.clone(),
        bloom: bloom_final.clone(),
    });
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial2d(composite_mat),
        Transform::default().with_scale(Vec3::new(win.width() / 2.0, win.height() / 2.0, 1.0)),
        UpscaleQuad,
        CompositeQuad,
        RenderLayers::layer(1),
    ));
    commands.spawn((Camera2d, Msaa::Off, UpscaleCamera, Nudgable, RenderLayers::layer(1)));
```

`bloom_final` is the `Handle<Image>` from `BloomFinalTarget` spawned in Task 5 Step 4 — already in scope (same function).

- [ ] **Step 6: Update `mirror_params` to set composite + brightpass uniforms each frame**

In `src/render/plugin.rs`, in `mirror_params`, add the `composite_materials` and `brightpass_materials` parameters and update loops. Change the signature:

```rust
fn mirror_params(
    camera: Res<crate::camera::OrbitCamera>,
    params: Res<crate::params::BlackHoleParams>,
    time: Res<Time>,
    window: Query<&Window>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
) {
```

to:

```rust
fn mirror_params(
    camera: Res<crate::camera::OrbitCamera>,
    params: Res<crate::params::BlackHoleParams>,
    time: Res<Time>,
    window: Query<&Window>,
    mut materials: ResMut<Assets<BlackHoleMaterial>>,
    mut brightpass_materials: ResMut<Assets<crate::render::material::BrightPassMaterial>>,
    mut composite_materials: ResMut<Assets<crate::render::material::CompositeMaterial>>,
) {
```

After the existing `for (_, mat) in materials.iter_mut()` loop, add:

```rust
    // Update brightpass threshold (live-tunable).
    for (_, mat) in brightpass_materials.iter_mut() {
        mat.threshold = params.bloom_threshold;
    }
    // Update composite material uniforms (bloom strength + exposure live-tunable).
    for (_, mat) in composite_materials.iter_mut() {
        mat.uniform.bloom_strength = params.bloom_strength;
        mat.uniform.exposure = params.exposure;
    }
```

- [ ] **Step 7: Update `resize_offscreen` to resize bloom targets**

In `src/render/plugin.rs`, in `resize_offscreen`, add the bloom target queries and resize logic. The signature (built in Task 1 Step 7) already has the `quads` ParamSet with `QuadScaleFactor`/`CompositeQuad`. Add the bloom target queries. Change:

```rust
fn resize_offscreen(
    mut images: ResMut<Assets<Image>>,
    params: Res<crate::params::BlackHoleParams>,
    window: Query<&Window>,
    mut resized: MessageReader<bevy::window::WindowResized>,
    offscreen: Query<&OffscreenTarget>,
    mut quads: ParamSet<(
        Query<(&mut Transform, &QuadScaleFactor), Without<CompositeQuad>>,
        Query<&mut Transform, With<CompositeQuad>>,
    )>,
) {
```

to:

```rust
fn resize_offscreen(
    mut images: ResMut<Assets<Image>>,
    params: Res<crate::params::BlackHoleParams>,
    window: Query<&Window>,
    mut resized: MessageReader<bevy::window::WindowResized>,
    offscreen: Query<&OffscreenTarget>,
    bloom0: Query<&BloomTarget0>,
    bloom1: Query<&BloomTarget1>,
    bloom2: Query<&BloomTarget2>,
    bloom_final: Query<&BloomFinalTarget>,
    mut quads: ParamSet<(
        Query<(&mut Transform, &QuadScaleFactor), Without<CompositeQuad>>,
        Query<&mut Transform, With<CompositeQuad>>,
    )>,
) {
```

Then, after the offscreen image resize block (the `if let Ok(handle) = offscreen.single() { ... }` block), add:

```rust
    // Bloom pyramid (queries return empty when bloom_quality == Off).
    let bw = ((w as f32 * 0.5) as u32).max(1);
    let bh = ((h as f32 * 0.5) as u32).max(1);
    let b1w = ((w as f32 * 0.25) as u32).max(1);
    let b1h = ((h as f32 * 0.25) as u32).max(1);
    let b2w = ((w as f32 * 0.125) as u32).max(1);
    let b2h = ((h as f32 * 0.125) as u32).max(1);
    if let Ok(t) = bloom0.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(bw, bh, TextureFormat::Rgba16Float, None));
    }
    if let Ok(t) = bloom1.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(b1w, b1h, TextureFormat::Rgba16Float, None));
    }
    if let Ok(t) = bloom2.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(b2w, b2h, TextureFormat::Rgba16Float, None));
    }
    if let Ok(t) = bloom_final.single() {
        let _ = images.insert(t.0.id(), Image::new_target_texture(bw, bh, TextureFormat::Rgba16Float, None));
    }
```

- [ ] **Step 8: Delete the old upscale shader**

Run: `git rm assets/shaders/upscale.wgsl`

- [ ] **Step 9: Build and visually verify**

Run: `cargo run --release`
Expected: bloom is now composited. Bright disk regions glow with a soft halo. ACES compresses over-bright cores to a natural white-orange. The image looks dramatically more cinematic. If bloom is too strong, lower `bloom_strength` (default 0.8) — UI control comes in Task 8.

- [ ] **Step 10: Commit**

```bash
git add assets/shaders/composite.wgsl src/render/material.rs src/render/plugin.rs
git rm assets/shaders/upscale.wgsl
git commit -m "render: composite + ACES tone-map (bloom stage [5]), replaces upscale

Adds CompositeMaterial + composite.wgsl: combines the HDR scene with the
bloom pyramid output, applies ACES (Narkowicz) tone mapping, and writes
LDR to the window. Removes UpscaleMaterial + upscale.wgsl.

mirror_params now updates composite (bloom_strength, exposure) and
brightpass (threshold) uniforms each frame. resize_offscreen rebuilds
the full bloom pyramid targets on window resize."
```

---

## Task 7: BloomQuality enum + tiered defaults + bloom-off fallback

**Files:**
- Modify: `src/params.rs` (add `BloomQuality` enum + `bloom_quality` field + tiered defaults)
- Modify: `src/render/plugin.rs` (conditional spawn of bloom passes based on quality; bloom-off fallback to a 1×1 black bloom texture)

Makes bloom quality configurable and adds web/desktop tiered defaults. `Off` skips all bloom passes and composites scene-only with ACES.

- [ ] **Step 1: Add `BloomQuality` enum to `src/params.rs`**

At the top of `src/params.rs` (before `BlackHoleParams`), add:

```rust
/// Bloom pyramid depth (number of bloom textures).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BloomQuality {
    Off,     // no bloom, scene-only ACES composite
    Low,     // 1 level: brightpass → composite (soft halo)
    Medium,  // 2 levels: brightpass → 1 down → 1 up → composite
    #[default]
    High,    // 3 levels: brightpass → 2 down → 2 up → composite (full cinematic)
}

impl BloomQuality {
    pub fn levels(self) -> u32 {
        match self {
            BloomQuality::Off => 0,
            BloomQuality::Low => 1,
            BloomQuality::Medium => 2,
            BloomQuality::High => 3,
        }
    }
}
```

- [ ] **Step 2: Add `bloom_quality` field + tiered default to `BlackHoleParams`**

In `src/params.rs`, in the `BlackHoleParams` struct, after `pub exposure: f32,` (added in Task 3), add:

```rust
    pub bloom_quality: BloomQuality,
```

In `impl Default for BlackHoleParams`, after `exposure: 1.0,`, add:

```rust
            bloom_quality: if cfg!(target_arch = "wasm32") { BloomQuality::Low } else { BloomQuality::High },
```

- [ ] **Step 3: Gate bloom spawn on `bloom_quality` in `spawn_fullscreen_quad`**

In `src/render/plugin.rs`, in `spawn_fullscreen_quad`, wrap the entire bloom spawn block (brightpass from Task 4 + blur pyramid from Task 5) in a conditional based on `params.bloom_quality.levels()`. Introduce a `bloom_final_handle: Option<Handle<Image>>` that is `Some` when bloom is active, `None` when Off:

```rust
    let levels = params.bloom_quality.levels();
    let mut bloom_final_handle: Option<Handle<Image>> = None;
    if levels > 0 {
        // ... the entire brightpass + blur pyramid spawn block from Tasks 4-5 ...
        // At the end of the block, after bloom_final is created:
        bloom_final_handle = Some(bloom_final.clone());
    }
```

Then, for the composite material spawn (Task 6 Step 5), use the bloom handle if present, else a 1×1 black fallback texture:

```rust
    let bloom_handle = bloom_final_handle.unwrap_or_else(|| {
        // 1x1 black fallback — composite samples black, effectively scene-only ACES.
        images.add(Image::new_fill(
            bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba16Float,
            bevy::render::render_asset::RenderAssetUsages::default(),
        ))
    });
    let composite_mat = composite_materials.add(crate::render::material::CompositeMaterial {
        uniform: crate::render::material::CompositeUniform {
            bloom_strength: 0.8, exposure: 1.0, _pad0: 0.0, _pad1: 0.0,
        },
        scene: offscreen.clone(),
        bloom: bloom_handle,
    });
```

(Replace the `bloom: bloom_final.clone(),` line from Task 6 Step 5 with `bloom: bloom_handle,`.)

Note: `params` is already a parameter of `spawn_fullscreen_quad` (`params: Res<crate::params::BlackHoleParams>`). It's currently only used for `render_scale`; now it's also read for `bloom_quality`. The `params` resource is immutable in `spawn_fullscreen_quad` (it's `Res`, not `ResMut`), which is correct for a read.

- [ ] **Step 4: Build and visually verify (Off path)**

Temporarily set `bloom_quality: BloomQuality::Off` in the params default, run `cargo run --release`.
Expected: the image renders with ACES tone mapping but no bloom halo — a clean HDR scene. Restore to the tiered default (`if cfg!(wasm32) { Low } else { High }`) after confirming.

- [ ] **Step 5: Build and visually verify (High path)**

Set `bloom_quality: BloomQuality::High`, run `cargo run --release`.
Expected: full bloom as in Task 6.

- [ ] **Step 6: Commit**

```bash
git add src/params.rs src/render/plugin.rs
git commit -m "params: BloomQuality enum + tiered web/desktop defaults + Off fallback

BloomQuality { Off, Low, Medium, High } controls pyramid depth (0-3).
Web defaults to Low (1 level, minimal float textures), desktop to High
(3 levels, full cinematic). Off skips all bloom passes and composites
scene-only with ACES — the fallback path for WebGPU browsers without
float-filterable support. The composite samples a 1x1 black texture
when bloom is off."
```

---

## Task 8: Quality egui panel + runtime bloom-quality rebuild

**Files:**
- Modify: `src/ui.rs` (add `Quality` collapsible section)
- Modify: `src/render/plugin.rs` (rebuild bloom pyramid when `bloom_quality` changes at runtime)

Wires all quality options to the UI and handles runtime pyramid rebuild.

- [ ] **Step 1: Add the Quality panel section to `src/ui.rs`**

In `src/ui.rs`, in `ui_system`, after the existing `Grid` `CollapsingHeader` (the last one, around line 54), add a new section inside the `egui::Window::show` closure:

```rust
                egui::CollapsingHeader::new("Quality")
                    .default_open(true)
                    .show(ui, |ui| {
                        use crate::params::BloomQuality;
                        let mut q = params.bloom_quality;
                        egui::ComboBox::from_label("Bloom quality")
                            .selected_text(format!("{:?}", q))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut q, BloomQuality::Off, "Off");
                                ui.selectable_value(&mut q, BloomQuality::Low, "Low");
                                ui.selectable_value(&mut q, BloomQuality::Medium, "Medium");
                                ui.selectable_value(&mut q, BloomQuality::High, "High");
                            });
                        params.bloom_quality = q;
                        ui.add_enabled(q != BloomQuality::Off, egui::Slider::new(&mut params.bloom_threshold, 0.0..=3.0).text("Bloom threshold"));
                        ui.add_enabled(q != BloomQuality::Off, egui::Slider::new(&mut params.bloom_strength, 0.0..=2.0).text("Bloom strength"));
                        ui.add(egui::Slider::new(&mut params.exposure, 0.5..=3.0).text("Exposure"));
                        ui.add(egui::Slider::new(&mut params.render_scale, 0.25..=1.0).text("Resolution scale"));
                        ui.checkbox(&mut params.star_aa, "Anti-aliased stars");
                        ui.label("MSAA is decorative on a fullscreen shader (no geometry edges to sample).");
                    });
```

- [ ] **Step 2: Extract bloom spawn into a reusable helper**

In `src/render/plugin.rs`, extract the bloom spawn block (brightpass + blur pyramid, currently inside the `if levels > 0 { ... }` in `spawn_fullscreen_quad`) into a standalone function that both startup and the rebuild system can call:

```rust
/// Spawns the bloom pipeline (brightpass + blur pyramid + bloom_final).
/// Called at startup and when bloom_quality changes at runtime.
/// Returns the bloom_final handle (None if levels == 0).
#[allow(clippy::too_many_arguments)]
fn spawn_bloom_pipeline(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    materials: &mut Assets<crate::render::material::BrightPassMaterial>,
    blur_materials: &mut Assets<crate::render::material::BlurMaterial>,
    meshes: &mut Assets<Mesh>,
    offscreen: &Handle<Image>,
    w: u32,
    h: u32,
    levels: u32,
) -> Option<Handle<Image>> {
    if levels == 0 { return None; }
    // ... the brightpass + blur spawn code, moved verbatim from spawn_fullscreen_quad ...
    // Return Some(bloom_final.clone()) at the end.
    // NOTE: when levels < 3, spawn fewer blur passes. For Low (1 level):
    //   only brightpass → bloom_final (no down/up). For Medium (2 levels):
    //   brightpass → 1 down → 1 up. Branch on `levels`.
    todo!("move the bloom spawn code here, branching on `levels`")
}
```

The `levels` branching: for `levels == 1` (Low), spawn only the brightpass writing directly to bloom_final (skip the blur pyramid). For `levels == 2` (Medium), spawn brightpass + one down pass + one up pass. For `levels == 3` (High), the full 2-down + 2-up pyramid. The composite reads `bloom_final` regardless of how many levels produced it.

Note: this refactor also requires `BlurMaterial` assets in the signature (the `materials: &mut Assets<BrightPassMaterial>` param isn't enough — blur passes need their own). Add `blur_materials: &mut Assets<crate::render::material::BlurMaterial>` and update the calls. The `spawn_fullscreen_quad` function currently uses a single `materials: ResMut<Assets<BlackHoleMaterial>>` for the black-hole material and separate typed asset stores for bloom. Ensure all typed asset stores are passed through.

- [ ] **Step 3: Add the `AppliedBloomQuality` resource + `rebuild_bloom` system**

In `src/render/plugin.rs`, add:

```rust
/// Tracks the bloom quality currently applied to the render pipeline.
/// When it differs from params.bloom_quality, a rebuild is triggered.
#[derive(Resource)]
pub struct AppliedBloomQuality(pub crate::params::BloomQuality);
```

In `BlackHolePlugin::build`, init it (set to the params default at startup):

```rust
            .init_resource::<AppliedBloomQuality>()
```

Wait — `init_resource` uses `Default`, but `BloomQuality` defaults to `High`. The web default is `Low`. So `AppliedBloomQuality` must be initialized from params, not via `init_resource`. Instead, set it in `spawn_fullscreen_quad` after reading params:

```rust
    commands.insert_resource(AppliedBloomQuality(params.bloom_quality));
```

Add the rebuild system:

```rust
/// Detects a bloom_quality change and rebuilds the bloom pipeline.
/// Heavy (despawn + respawn all bloom entities) but only fires when the
/// user changes the dropdown.
fn rebuild_bloom(
    params: Res<crate::params::BlackHoleParams>,
    applied: Res<AppliedBloomQuality>,
    mut commands: Commands,
    // Despawn all bloom entities (cameras, quads, targets).
    bloom_entities: Query<Entity, Or<(
        With<BrightPassCamera>, With<BlurCamera>,
        With<BrightPassQuad>, With<BlurQuad>,
        Or<(With<BloomTarget0>, With<BloomTarget1>, With<BloomTarget2>, With<BloomFinalTarget>)>,
    )>>,
    // Also re-query the composite material to update its bloom handle.
    mut composite_materials: ResMut<Assets<crate::render::material::CompositeMaterial>>,
    // Need the offscreen handle + window size + asset stores to re-spawn.
    offscreen_target: Query<&OffscreenTarget>,
    window: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
    mut bp_materials: ResMut<Assets<crate::render::material::BrightPassMaterial>>,
    mut blur_materials: ResMut<Assets<crate::render::material::BlurMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if params.bloom_quality == applied.0 { return; }
    // Despawn all bloom entities.
    for e in bloom_entities.iter() {
        commands.entity(e).despawn();
    }
    // Re-spawn at the new quality.
    let Ok(win) = window.single() else { return; };
    let scale = params.render_scale.clamp(MIN_RENDER_SCALE, 1.0);
    let w = ((win.width() * scale) as u32).max(1);
    let h = ((win.height() * scale) as u32).max(1);
    let Ok(offscreen_t) = offscreen_target.single() else { return; };
    let offscreen = offscreen_t.0.clone();
    let levels = params.bloom_quality.levels();
    let new_bloom = spawn_bloom_pipeline(
        &mut commands, &mut images, &mut bp_materials, &mut blur_materials,
        &mut meshes, &offscreen, w, h, levels,
    );
    // Update the composite material's bloom handle.
    let bloom_handle = new_bloom.unwrap_or_else(|| {
        images.add(Image::new_fill(
            bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba16Float,
            bevy::render::render_asset::RenderAssetUsages::default(),
        ))
    });
    for (_, mat) in composite_materials.iter_mut() {
        mat.bloom = bloom_handle.clone();
    }
    commands.insert_resource(AppliedBloomQuality(params.bloom_quality));
}
```

Register it in `BlackHolePlugin::build`:

```rust
            .add_systems(Update, rebuild_bloom)
```

Note: the `AppliedBloomQuality` resource must NOT be a plain `Resource` updated via `commands.insert_resource` inside a system that also reads it as `Res` — Bevy's change detection requires it to be `ResMut` to update. Since `rebuild_bloom` both reads (`applied.0`) and writes (`commands.insert_resource`) it, read it as `Res` and write via `commands.insert_resource` (which replaces the resource). This is safe — `insert_resource` doesn't conflict with the `Res` borrow because it runs at command application time, after the system's direct borrows are released.

Actually — `applied: Res<AppliedBloomQuality>` and then `commands.insert_resource(AppliedBloomQuality(...))` in the same system IS safe in Bevy: the `Res` is an immutable borrow for reading, and `insert_resource` is a queued command applied later. Confirmed pattern.

- [ ] **Step 4: Build and visually verify**

Run: `cargo run --release`
Expected: the Quality panel appears with all controls. Changing bloom quality at runtime rebuilds the pyramid (no crash). Sliders for threshold/strength/exposure take effect immediately. Toggling star_aa switches star rendering. Resolution scale changes sharpness.

- [ ] **Step 5: Commit**

```bash
git add src/ui.rs src/render/plugin.rs
git commit -m "ui: Quality panel + runtime bloom-quality rebuild

Adds a dedicated Quality collapsible section: bloom quality dropdown
(Off/Low/Medium/High), bloom threshold/strength, exposure, resolution
scale, star AA toggle. Changing bloom quality at runtime triggers a
pipeline rebuild (despawn bloom entities + respawn via spawn_bloom_pipeline
helper, branching on levels). MSAA honestly labeled as decorative."
```

---

## Task 9: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite**

Run: `cargo test`
Expected: all physics tests pass unchanged (the integrator wasn't touched).

- [ ] **Step 2: Desktop visual verification**

Run: `cargo run --release`
Check against the Gargantua reference:
- Disk has feathered/smoky texture (FBM), not bands.
- Bright disk regions have a soft cinematic halo (bloom).
- White-hot core → deep orange gradient (HDR + ACES).
- Doppler asymmetry: one side brighter than the other.
- Stars are round points, not rectangles.
- Quality panel controls all work live.

- [ ] **Step 3: Web build check (if trunk is installed)**

Run: `trunk build --release`
Expected: builds without error. (Full web visual check requires a WebGPU browser; the build succeeding confirms the float-format code compiles for wasm32.)

- [ ] **Step 4: Commit any final tuning**

If any tuning constants (FBM scales, bloom strengths, kernel weights) were adjusted during verification, commit them:

```bash
git add -A
git commit -m "tweak: final cinematic rendering tuning constants

Adjusted [specific constants] during visual verification against the
Gargantua reference."
```

---

## Self-Review

**1. Spec coverage:**
- HDR color + tone mapping → Task 1 (Rgba16Float) + Task 6 (ACES). ✓
- Bloom post-processing → Tasks 4, 5, 6, 7. ✓
- Smoke/turbulence disk texture → Task 2 (FBM). ✓
- Anti-aliasing + round stars → Task 3 (gaussian speck) + render_scale in Task 8 panel. ✓
- Configurable quality panel → Task 8. ✓
- Tiered web defaults → Task 7. ✓
- Bloom-off fallback → Task 7. ✓
- Nudgable marker → Task 1. ✓

**2. Placeholder scan:** Task 8 Step 2 contains a `todo!("move the bloom spawn code here...")` — this is a genuine refactor instruction (extract existing code into a helper), not a content placeholder. The implementer moves the Task 4/5 spawn code verbatim into `spawn_bloom_pipeline` and adds the `levels` branching. No other TBD/TODO.

**3. Type consistency:**
- `BloomQuality` enum: consistent (Off/Low/Medium/High, `.levels()`) across params.rs, ui.rs, plugin.rs. ✓
- `BlurUniform`: `mode: u32, texel_size: Vec2, blend: f32, _pad0: f32` — matches shader. ✓
- `CompositeUniform`: `bloom_strength, exposure, _pad0, _pad1` — matches shader. ✓
- `QuadScaleFactor(f32, f32)`: consistent across all spawn sites + resize. ✓
- `CompositeQuad` marker: on composite quad, queried in resize + rebuild. ✓
- `Nudgable` marker: applied to all 7 cameras. ✓
- Camera orders: -20/-19/-18/-17/-16/-15/0 — consistent across all tasks. ✓
- RenderLayers: 0-6 — consistent across all tasks. ✓
