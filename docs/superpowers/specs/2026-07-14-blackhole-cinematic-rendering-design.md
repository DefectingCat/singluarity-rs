# Cinematic Rendering — Design Spec

**Date:** 2026-07-14
**Phase:** 3 (visual fidelity), builds on Phase 2 (Kerr)
**Status:** approved design, pending implementation plan

## Goal

Raise the renderer from "geometrically correct but flat" (current screenshot) to the visual fidelity of the Gargantua reference image (Figure 1). Four target traits, all user-selected:

1. **HDR color + tone mapping** — white-hot core → deep orange, strong Doppler left/right asymmetry.
2. **Bloom post-processing** — cinematic glow around the bright disk.
3. **Smoke/turbulence disk texture** — feathered, flowing gas instead of flat color blocks.
4. **Anti-aliasing + round stars** — smooth edges, sharp circular stars instead of jagged pixel blobs.

All quality options must be **configurable** via a dedicated egui panel, with **tiered web defaults** (conservative on web, full on desktop).

## Non-goals (explicit scope boundaries)

- **Volumetric disk thickness** — disk stays a zero-thickness plane. Volume ray-marching is a separate future project.
- **Exact Kerr Hamiltonian** — keep the leading-pole (Lense-Thirring) approximation. Phase 3 physics is out of scope.
- **Retrograde / tilted spin axis** — out of scope.
- **MSAA on the fullscreen quad** — documented as decorative (the quad is a single fragment covering the screen; MSAA samples geometry edges, not shader internals). Retained as a visible option for future geometry; the real AA is `render_scale` + `star_aa`.

## Architecture: the new render pipeline

The current pipeline is 2 passes: `OffscreenQuad → Upscale`. The new pipeline is 5 stages chained through `Image` targets:

```
[1] Offscreen (HDR)        black_hole.wgsl
    Rgba16Float @ render_scale
    geodesic + disk + stars, linear HDR output (no tone-map yet)
       ↓
[2] Bright-pass             brightpass.wgsl (new material)
    Rgba16Float @ render_scale/2
    extracts luminance > threshold, soft-knee, pre-blur
       ↓
[3] Downsample pyramid      blur.wgsl (down mode) × N passes
    Rgba16Float @ render_scale/2, /4, /8 (N = BloomQuality::levels)
    each level samples previous with 13-tap weighted kernel
       ↓
[4] Upsample pyramid        blur.wgsl (up mode) × N passes
    back up to render_scale/2, additive blend with lerp factor
       ↓
[5] Composite + Tone-map    composite.wgsl (replaces upscale.wgsl)
    @ window res, Bgra8UnormSrgb
    final = ACES(scene_hdr + bloom * bloom_strength) * exposure
```

### Key format decision

The offscreen target switches from `Bgra8UnormSrgb` to `Rgba16Float`. This is mandatory — bloom needs over-bright disk values (>1.0) that 8-bit srgb would clamp away. The final LDR conversion is deferred to ACES tone-mapping in stage [5].

### Why a down-then-up pyramid instead of a single blur

Multi-pass downsampling gives isotropic wide-frequency bloom (13 taps around the source ≈ a Gaussian). A single bilinear tap gives a soft halo, not the broad cinematic glow of the reference. The number of levels is configurable (1–3) so web can run 1 (soft halo) and desktop 3 (full pyramid).

### Image targets (created in the plugin)

| Target        | Format          | Scale            | Written by      | Read by                |
|---------------|-----------------|------------------|-----------------|------------------------|
| `offscreen_hdr` | Rgba16Float   | render_scale     | BH shader       | brightpass             |
| `bloom_0`     | Rgba16Float     | render_scale/2   | brightpass      | blur down[0]           |
| `bloom_1`     | Rgba16Float     | render_scale/4   | blur down[0]    | blur down[1] / up[1]   |
| `bloom_2`     | Rgba16Float     | render_scale/8   | blur down[1]    | blur up[1]             |
| (window)      | Bgra8UnormSrgb  | 1.0              | composite       | —                      |

Pyramid depth = number of bloom textures = `BloomQuality::levels` (Off=0, Low=1, Medium=2, High=3). With L levels there are L−1 down passes and L−1 up passes. For High (3 textures: `bloom_0/1/2`): down passes `0→1` and `1→2` (2 instances) + up passes `2→1` and `1→0` (2 instances) = 4 `BlurMaterial` instances total. `Off` = no bloom targets/materials at all (falls back to a plain LDR upscale path, no float textures).

## Shader changes (black_hole.wgsl)

Two changes, both localized to `disk_color` and the output path. The geodesic integrator (`deriv`, `rk45_step`, the loop at lines 320–390) is **untouched**.

### Disk noise: domain-warped FBM

Replace the two-sine noise (current `disk_color` lines 148–150) with:

```
fn value_noise3(p: vec3<f32>) -> f32   // smoothed 3D value noise (hash → smoothstep interp)
fn fbm3(p: vec3<f32>, octaves: u32) -> f32  // FBM layered sum of value_noise3
fn disk_noise(pos: vec3<f32>, t: f32) -> f32  // domain warp
    let warp = fbm3(pos * 0.8 + t * 0.1, 3)
    let n = fbm3(pos * 2.0 + warp * 1.5 + t * 0.3, 4)
    return n
```

`disk_color` uses `disk_noise` in place of the sines. The Keplerian shear (`rot = time*speed / r^1.5`) is retained as the time term — it drives inner-faster-than-outer rotation, which is correct physics. Domain warping produces the feathered/smoky structure the user flagged as missing from the reference.

Cost: current `disk_color` ≈ 2 `sin` calls; FBM+warp ≈ 50 ALU ops/hit (7× (3 hash + 3 interp)). A ray hits the disk at most ~2 times, so this is acceptable.

### Linear HDR output

With a `Rgba16Float` target, disk color is no longer implicitly clamped to [0,1]. The white-hot inner disk radiates at 3–5×, the bright Doppler channel reaches 5–8×. `disk_brightness` now legitimately exceeds 1.0 and acts as a scale. Stars stay low-intensity (below bloom threshold); background space stays black. Only the disk (and skybox when its intensity is high) drives bloom.

### New uniform fields (added to BlackHoleUniforms)

- `bloom_threshold: f32` (bright-pass cutoff, default 1.0)
- `bloom_strength: f32` (composite blend, default 0.8)
- `tone_map_exposure: f32` (ACES pre-multiplier, default 1.0)

These are part of the existing `BlackHoleUniforms` struct (binding 0), mirrored from `BlackHoleParams` each frame. They are **not** physics fields and do not affect the `physics.rs` mirror.

## New shaders

### brightpass.wgsl (stage [2])

```wgsl
@fragment
fn fragment(in) -> vec4<f32> {
    let hdr = textureSample(source, samp, in.uv).rgb;
    let lum = dot(hdr, vec3(0.2126, 0.7152, 0.0722));
    // soft knee, not hard threshold
    let soft = max(lum - threshold, 0.0) / (lum + 0.0001);
    let contribution = hdr * soft;
    return vec4(contribution, 1.0);
}
```

Hard thresholds create hard bloom edges. The soft knee `(lum - t)/(lum + ε)` gives a smooth roll-off — fully bright passes through, near-threshold tapers to zero. Sampling at half-res cell centers gives a free 2×2 box downsample via bilinear.

### blur.wgsl (stages [3] and [4])

One shader, two modes via a uniform `u_mode` (0=down, 1=up):

```wgsl
struct BlurUniform { mode: u32, texel_size: vec2<f32>, blend: f32, _pad: f32 };
// 13-tap weighted kernel (Gaussian approximation from HDR bloom literature)
// Down: sample previous-larger-level → write to smaller level
// Up:   sample next-smaller-level → additive-blend with `blend` factor
```

`texel_size` drives the tap stride. One shader, instantiated once per pass (L−1 down + L−1 up for L bloom textures).

### composite.wgsl (stage [5], replaces upscale.wgsl)

```wgsl
@fragment
fn fragment(in) -> vec4<f32> {
    let hdr = textureSample(scene, scene_samp, in.uv).rgb;       // full-res HDR
    let bloom = textureSample(bloom, bloom_samp, in.uv).rgb;     // top of pyramid
    let combined = hdr + bloom * bloom_strength;
    let mapped = aces(combined * exposure);   // Narkowicz fit, 5 ops
    return vec4(clamp(mapped, 0.0, 1.0), 1.0);  // → Bgra8UnormSrgb
}
```

ACES Narkowicz fit: `(x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14)`, clamped to [0,1]. 5 ops/pixel, negligible at 4K.

## Rust-side changes

### New materials (render/material.rs)

Three new `Material2d` structs mirroring the `UpscaleMaterial` pattern:

- `BrightPassMaterial { source: Handle<Image>, threshold: f32 }` — uniform at binding 0, texture at 1+2
- `BlurMaterial { source: Handle<Image>, mode: u32, texel_size: Vec2, blend: f32 }` — one material instance per pyramid pass. With L bloom textures there are L−1 down passes and L−1 up passes. For High (3 textures): 2 down + 2 up = 4 instances. (Bevy `Material2d` binds uniforms once per material, not per entity, so each pass needs its own material asset.)
- `CompositeMaterial { scene: Handle<Image>, bloom: Handle<Image>, bloom_strength: f32, exposure: f32 }` — replaces `UpscaleMaterial`

`UpscaleMaterial` and `upscale.wgsl` are removed.

### New params + BloomQuality enum (params.rs)

```rust
pub enum BloomQuality { Off, Low, Medium, High }  // 0 / 1 / 2 / 3 levels

pub struct BlackHoleParams {
    // ... existing fields unchanged ...
    // Quality (new section)
    pub bloom_quality: BloomQuality,
    pub bloom_threshold: f32,   // 1.0
    pub bloom_strength: f32,    // 0.8
    pub exposure: f32,          // 1.0
    pub msaa: u32,              // 1 (Off) — 1/2/4
    pub star_aa: bool,          // anti-aliased star rendering
}
```

Defaults differ via `cfg!(target_arch = "wasm32")`:

| Param             | Desktop       | Web         |
|-------------------|---------------|-------------|
| `bloom_quality`   | High (3)      | Low (1)     |
| `msaa`            | 4             | 1           |
| `star_aa`         | true          | false       |

`msaa` toggles the offscreen camera between `Msaa::Off` / `Msaa::Sample2` / `Msaa::Sample4`. `star_aa` toggles a shader branch in `black_hole.wgsl`.

### Plugin rewiring (render/plugin.rs)

`spawn_fullscreen_quad` now also spawns:
- `bloom_0/1/2` Images (sized by the `bloom_quality` upper bound)
- 4 `BlurMaterial` instances + quads (2 down + 2 up for High; fewer for lower quality)
- 1 `BrightPassMaterial` quad
- 1 `CompositeMaterial` quad (replaces the Upscale quad)

Camera order: offscreen=-3, brightpass=-2, blur chain=-1, composite=0.

`resize_offscreen` must rebuild bloom pyramid dimensions on resize (all targets share the scale). When `bloom_quality` changes at runtime via the UI, a one-shot pyramid rebuild is triggered (same logic as the resize path, but driven by a param-change flag rather than a `WindowResized` message).

### Nudgable marker component

`nudge_camera` (the Bevy 0.19 #24448 workaround) currently targets `Or<(With<OffscreenCamera>, With<UpscaleCamera>)>`. It must extend to all new cameras or they freeze too. Introduce a marker component `Nudgable`; all render cameras carry it; `nudge_camera` queries `With<Nudgable>`. (The component is zero-sized and carry-only; no logic.)

### Anti-aliasing + round stars (black_hole.wgsl)

**MSAA caveat (documented):** MSAA on a fullscreen quad is decorative — the quad is a single fragment covering the screen, so MSAA's geometry-edge sampling does nothing for shader-internal aliasing. Retained as a visible option (useful if real geometry is added later) but honestly labeled. The real AA is:

**`render_scale`** (existing, renamed "Resolution scale" in the Quality panel) — at 1.0 the offscreen is full-res and the bilinear upscale becomes identity, edges as sharp as possible. Already a lever; just surfaced in the Quality panel.

**`star_color` rewrite (gaussian speck):** the current `smoothstep(0.5, 0.0, d)` with `scale=80` produces blocky rectangles at low resolution. Replace with a gaussian speck per cell:
- One star per cell (existing hash decides presence).
- Distance to cell star-center, `smoothstep(radius, 0.0, dist)` where `radius ≈ 0.15` cell.
- Naturally produces a 2–3 pixel anti-aliased disk instead of a rectangle.
- `star_aa` flag switches between the old fast path and the gaussian speck path.

### Quality panel (ui.rs)

A new top-level `CollapsingHeader::new("Quality")` (separate from the existing param-category sections), containing:
- Bloom quality dropdown (Off/Low/Medium/High)
- Bloom threshold slider (0.0–3.0)
- Bloom strength slider (0.0–2.0)
- Exposure slider (0.5–3.0)
- Resolution scale slider (0.25–1.0, moved from Renderer)
- MSAA dropdown (1/2/4) with a "limited effect on fullscreen shader" note label
- Star AA checkbox
- Per-option perf-hint labels

This satisfies "all quality options configurable, quality in its own panel."

## Gotcha reconciliation (AGENTS.md)

1. **`nudge_camera`** — extended to all bloom cameras via the `Nudgable` marker. Without this, the new cameras freeze and bloom silently stops updating.
2. **bevy_egui context** — the Quality panel lives in the same `ui_system`, already in `EguiPrimaryContextPass`. No change.
3. **Storage buffer / RetryNextUpdate** — only the BH material uses a storage buffer; unchanged. Bloom materials are plain texture binds, no risk.

## Testing

The CPU mirror contract (`deriv` / `rk45_step` / `is_captured_rk45` in `physics.rs` ↔ shader) is **untouched** — none of the changes touch the geodesic integrator. Existing tests (`b < b_crit` captured, `b > b_crit` escapes, spin=0 degeneracy, spin>0 capture) pass unchanged. No new tests: per AGENTS.md, "The GPU shader is not unit-tested."

`cargo test` is run first to establish a green baseline before any change.

## Risk: Rgba16Float on WebGPU

`Rgba16Float` as a filterable render target requires WebGPU's float-filterable feature. On desktop this is standard. On web, most modern browsers support `rgba16float`, but if unsupported, the `trunk` build fails at context creation — surfacing the existing WebGPU fallback message rather than a blank canvas.

Mitigation:
- Web defaults to `BloomQuality::Low` (1 bloom level), minimizing float textures.
- `BloomQuality::Off` falls back to the original LDR upscale path (no float targets, no bloom materials). A web user on an unsupported browser can disable bloom and run the LDR path with zero float-texture requirement.

## Implementation order (per-commit blocks)

Each block compiles standalone and is visually checkable via `cargo run --release`.

1. Run `cargo test` — green baseline (no code change).
2. Offscreen format → `Rgba16Float`; adjust BH shader output (remove implicit clamp). Commit.
3. FBM + domain-warp noise in `disk_color`. Commit.
4. `star_color` gaussian speck + `star_aa` flag. Commit.
5. `BrightPassMaterial` + `brightpass.wgsl` + camera. Commit.
6. `BlurMaterial` + `blur.wgsl` + pyramid spawn. Commit.
7. `CompositeMaterial` + `composite.wgsl` (ACES); remove `UpscaleMaterial`. Commit.
8. `BloomQuality` param + `Nudgable` marker + Quality panel. Commit.
9. Web defaults + bloom-off fallback path. Commit.
