# Volumetric Accretion Disk — Design Spec

**Date:** 2026-07-15
**Phase:** 3.1 (disk visual fidelity), builds on Phase 3 (cinematic)
**Status:** approved design, pending implementation plan

## Goal

Replace the current smooth, zero-thickness disk with a Gargantua-style volumetric gas disk: a finite-thickness luminous layer with sharp bright filaments embedded in darker gas, large-scale spiral-arm modulation, and Keplerian-shear-driven flow. All new parameters configurable via the egui panel, with tiered web/desktop defaults.

This is the direct follow-up to the user-observed problem: the current disk looks "very smooth." The root cause is not a bug but five stacked design choices (zero thickness, fixed alpha, single surface sample, low-frequency low-contrast FBM, bloom smoothing) — this spec replaces the first four with volumetric physics and ridged turbulence.

## Non-goals

- **No Kerr-physics changes.** The geodesic integrator (`deriv` / `rk45_step` / the main integration loop's step-accept logic) is untouched. Volumetric integration happens in the render-sampling layer, layered *on top of* accepted RK45 steps, and never enters the geodesic solver.
- **Therefore `physics.rs` is unchanged**, and the existing `is_captured_rk45` / integration tests stay green. The CPU mirror covers capture-vs-escape, not disk appearance.
- **No real MHD.** Turbulence is procedurally faked with noise for visual effect, not magnetohydrodynamic simulation.
- **No changes to the post-processing pipeline** (bloom/brightpass/blur/composite). The disk feeds stage [1] HDR output as before; bloom then operates on the new higher-detail disk.

## Explicit supersession of a Phase 3 non-goal

Phase 3 spec (`2026-07-14-blackhole-cinematic-rendering-design.md`, line 20) states:

> **Volumetric disk thickness** — disk stays a zero-thickness plane. Volume ray-marching is a separate future project.

**This design supersedes that line.** Rationale: user feedback identified the disk as too smooth, and zero thickness is a primary root cause — without intra-volume density variation, no amount of brightness modulation produces the rolling/turbulent structure of the reference image. The volumetric integration approach chosen here (§2, approach A) is specifically the one that reuses the existing RK45 adaptive step rather than a separate ray-march, keeping the cost bounded and the architecture coherent. All other Phase 3 decisions (HDR pipeline, tone mapping, bloom, AA) remain in force.

## Architecture: volumetric integration in the RK45 loop

### Approach A — in-step volumetric sampling (chosen)

The main loop (`black_hole.wgsl:404-474`) currently calls `disk_hit(prev, new_pos)` once per accepted step to test `y=0` plane crossing. The new logic splits disk handling into two cooperating parts:

**A. In-disk step sampling (new).** On every accepted step, at `new_pos`, test whether the point lies within the disk thickness slab:

```wgsl
let r = length(vec2<f32>(new_pos.x, new_pos.z));
if (abs(new_pos.y) < disk_half_thickness && r >= disk_inner && r <= disk_outer) {
    let s = disk_color_volumetric(new_pos, d);
    let step_len = length(new_pos - prev);
    accum_color += (1.0 - accum_alpha) * s.color * step_len;
    accum_alpha  += (1.0 - accum_alpha) * s.density * step_len;
}
```

Multiplying by `step_len` makes emission integrate over arc length. This is the key to approach A: the RK45 adaptive step already shrinks where light bends sharply (exactly where fine detail should appear) and grows where it travels straight. Volumetric contribution is therefore dense where the disk's light path curves and sparse where it doesn't — the adaptivity is reused, not duplicated. No extra geometry intersection, no separate march.

**B. Midplane edge capture (retained, repurposed).** The existing `disk_hit(prev, new_pos)` test is kept but its meaning changes from "the only sample" to "edge catch": when a single step straddles the `y=0` midplane and the step is large enough to skip over the thin slab, one precise at-plane sample is added:

```wgsl
if (disk_hit(prev, new_pos)) {
    let ty = prev.y / (prev.y - new_pos.y);
    let hit = mix(prev, new_pos, vec3<f32>(ty));
    let s = disk_color_volumetric(hit, d);
    let thickness_proj = disk_half_thickness / max(abs(d.y), 1e-3);
    accum_color += (1.0 - accum_alpha) * s.color * thickness_proj;
    accum_alpha  += (1.0 - accum_alpha) * s.density * thickness_proj;
}
```

`thickness_proj` is the slab depth projected along the ray, compensating for the midplane crossing a step skipped. The two parts do not conflict: A handles thick-slab traversal (many in-slab steps), B handles a straddling step that jumps the midplane.

**Parameters:** `disk_half_thickness` (default 0.3 desktop / 0.2 web → total thickness 0.6 / 0.4 Rs; with `disk_inner = 3.0` this is a ~20% / ~13% thickness-to-radius ratio, close to a real thin disk).

**Termination:** the existing `if (accum_alpha > 0.99) { break; }` is retained; opacity saturates naturally over a few in-disk steps.

### Why not B (fixed-step march) or C (analytic falloff)

- **B** opens a separate fixed-step mini-loop per plane crossing. It divorces sampling from the RK45 adaptivity, re-marches on every secondary-image crossing, and charges every ray uniformly regardless of viewing angle. Strictly more expensive for no visual gain over A.
- **C** (thickness only scales a single sample by `1/cos(angle)`) is precisely the current smooth disk plus a slope falloff — it cannot produce intra-volume structure, so it fails the entire goal. A false economy.

## Turbulence texture: ridged filaments + spiral arms

`disk_color_volumetric` produces three signals: **color, density, and brightness**. The noise is layered.

### Layer 1 — ridged bright filaments (replaces current FBM blobs)

The current `disk_noise` (`black_hole.wgsl:220`) is domain-warped FBM outputting [0,1] smooth blobs. It is replaced by ridged multifractal noise:

```wgsl
fn ridged_fbm(p: vec3<f32>, octaves: u32) -> f32 {
    var sum = 0.0; var amp = 0.5; var freq = 1.0;
    for (var i: u32 = 0u; i < octaves; i = i + 1u) {
        let n = value_noise3(p * freq);
        let ridge = 1.0 - abs(2.0 * n - 1.0);  // peak at n=0.5, valleys at 0 and 1
        sum += amp * pow(ridge, filament_sharpness);
        freq *= 2.0; amp *= 0.5;
    }
    return sum;
}
```

`1 - |2n−1|` inverts the smooth value-noise gradient into a sharp ridge — where the noise used to transition smoothly it now peaks, falling off to zero on both sides. Raising to `filament_sharpness` (default **2.0**, `ridge²`) thins and sharpens the ridges into filaments.

The default of 2.0 is chosen to match Gargantua: its bright structures are feathered smoke streaks — clearly defined bright lines that retain a soft, gaseous quality. Higher powers (4+) turn filaments into needle-thin hard lines and lose the gas feel; lower or no power widens them back toward the current blobs. 2.0 is panel-tunable up (sharper) or down (softer).

Domain warping (one layer of plain FBM distorting the sample coordinate) is retained so filaments curve and turbulence-bend rather than running straight.

### Layer 2 — density (decides where gas is, where it is void)

Density is no longer the fixed 0.85. A separate low-frequency FBM generates a volume density field:

```wgsl
let density_noise = fbm3(pos * density_freq + warp, density_octaves);
let density = smoothstep(0.3, 0.7, density_noise) * density_strength;
```

`smoothstep` produces a definite boundary: density noise above 0.7 is solid gas, below 0.3 is near-vacuum, between is a feathered edge. This gives the disk clumping — dense bright knots and thin dark gaps — instead of a uniform sheet.

### Layer 3 — spiral-arm modulation (large-scale winding)

A density modulation evolving with angle and radius, layered on top of the turbulence:

```wgsl
let phi = atan2(pos.z, pos.x);
let arm_phase = phi * arm_count + log(r) * arm_tightness - rot;
let arm = 0.5 + 0.5 * cos(arm_phase);          // [0,1]; cos peaks = arms
let arm_mod = mix(1.0, pow(arm, 2.0), arm_strength);
```

`phi * arm_count` sets the number of arms; `log(r) * arm_tightness` winds them with radius (a logarithmic spiral, the physically-motivated shape for differentially-rotating disks); the existing Keplerian `rot` term (`time * disk_rotation_speed / r^1.5`, already computed in `disk_color`) advects the arms at the disk's own differential rotation so inner radii wind faster — no separate arm-speed parameter, the arms simply ride the flow. `arm_strength` interpolates between no arms (`mix(…, …, 0) = 1.0`, modulation absent) and full arms. The modulation multiplies density: arms are dense, inter-arm is tenuous.

### Composition

```wgsl
let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);  // Keplerian shear (already in disk_color)
let flow = vec3<f32>(0.0, 0.0, rot);                                   // advect noise along the Keplerian flow
let warp = fbm3(pos * 0.8 + flow * 0.1, warp_octaves);
let filament = ridged_fbm(pos * filament_freq + warp * 1.5 + flow * 0.3, filament_octaves);
let density_noise = fbm3(pos * density_freq + warp, density_octaves);
let base_density = smoothstep(0.3, 0.7, density_noise) * density_strength;
let arm_mod = spiral_arm_modulation(pos, r, rot);

let brightness = filament;                          // filaments drive luminance
let total_density = base_density * arm_mod;         // gas × spiral arms
let col = temperature_color(r) * brightness * radial_falloff(r) * disk_brightness;
col = apply_doppler(col, pos, dir);                 // Doppler asymmetry preserved
return DiskSample { color: col, density: total_density };
```

Physically motivated separation: density decides *where matter is*, filaments decide *how bright that matter is*, spiral arms decide *how the large-scale structure is distributed*. The three multiply, never add.

**Retained unchanged:** radial temperature color `tcol`, radial falloff `falloff`, Doppler beaming, Keplerian shear `rot`. These physical terms are extracted into shared helpers (`temperature_color`, `radial_falloff`, `apply_doppler`) used by both the volumetric path and the flat fallback, ensuring the two modes share identical physics.

## Disk quality tiers and performance budget

A new `DiskQuality` enum (mirroring `BloomQuality`) gates octave counts and provides a full escape hatch:

| Tier | Filament octaves | Density octaves | Warp octaves | Half-thickness default |
|------|------------------|-----------------|--------------|------------------------|
| `Off` | — (flat fallback) | — | — | — |
| `Low` (web default) | 3 | 2 | 2 | 0.2 |
| `Medium` | 4 | 3 | 3 | 0.3 |
| `High` (desktop default) | 5 | 4 | 3 | 0.3 |

`Off` reverts to the current zero-thickness disk: single midplane `disk_color_flat` sample, fixed alpha 0.85, identical to today's appearance. It is both a performance escape hatch and a visual A/B reference.

**Per-fragment cost.** Each in-disk RK45 step pays one `disk_color_volumetric` call:

- domain warp FBM (warp_octaves × ~7 ALU)
- ridged filament (filament_octaves × ~11 ALU)
- density FBM (density_octaves × ~7 ALU)
- spiral-arm trig + pow ≈ 15 ALU

At High (5/4/3): ≈ 21 + 55 + 28 + 15 ≈ **120 ALU per in-disk step**. A ray crosses the slab in roughly 5–20 accepted steps (dense where bent, sparse where straight), so **≈ 600–2400 ALU per ray that hits the disk**. Disk-missing rays pay zero. The midplane edge capture (part B) runs at most twice per ray (primary + secondary image), ≈ 240 ALU/ray, constant.

This is ~10× the current disk cost, but only on disk-intersecting rays and only at High tier. Low tier (3/2/2) drops to ≈ **60 ALU per in-disk step**.

**Levers untouched:** `steps` (200 web / 300 desktop) and `render_scale` (0.5 web / 0.75 desktop) keep their Phase 3 defaults — they are tuned for the black-hole silhouette and secondary-image quality. Disk cost is absorbed by octave tiering, not resolution cuts.

### WGSL octave-loop safety

WGSL permits dynamic `for` bounds, but older WebGPU drivers have bugs with non-constant loop bounds. Octave loops use a fixed upper bound with early break:

```wgsl
const MAX_OCTAVES = 6u;
for (var i: u32 = 0u; i < MAX_OCTAVES; i = i + 1u) {
    if (i >= actual_octaves) { break; }
    ...
}
```

`MAX_OCTAVES = 6` covers the highest tier (5) with headroom.

## Data flow and interface changes

### Uniform extension (`BlackHoleUniforms`, `src/render/material.rs`)

Nine new fields appended to the existing `#[derive(ShaderType)]` struct:

```wgsl
disk_half_thickness: f32,
filament_freq: f32,
filament_sharpness: f32,
density_freq: f32,
density_strength: f32,
arm_count: f32,
arm_tightness: f32,
arm_strength: f32,
disk_quality: u32,   // 0=Off, 1=Low, 2=Medium, 3=High
```

`disk_quality` is a single `u32` encoding the tier; the shader selects the octave triplet from it rather than receiving three separate octave integers (fewer fields, panel is one dropdown not three sliders). WGSL-side `if/else` selects the octave set. Total uniform struct stays far under the 16384-byte limit.

### Rust params (`BlackHoleParams`, `src/params.rs`)

Eight new `pub f32` fields plus a `DiskQuality` enum. `Default`:

- desktop: `DiskQuality::High`, `disk_half_thickness` 0.3
- web: `DiskQuality::Low`, `disk_half_thickness` 0.2
- the other seven noise params share web/desktop defaults (table in §3)

```rust
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DiskQuality {
    Off,
    Low,
    Medium,
    #[default]
    High,
}

impl DiskQuality {
    pub fn octaves(self) -> (u32, u32, u32) {  // (filament, density, warp)
        match self {
            DiskQuality::Off => (0, 0, 0),
            DiskQuality::Low => (3, 2, 2),
            DiskQuality::Medium => (4, 3, 3),
            DiskQuality::High => (5, 4, 3),
        }
    }
}
```

### Mirror (`mirror_params`, `src/render/plugin.rs:579`)

The existing per-frame copy loop appends nine assignments mirroring the new params into `u`. `disk_quality` is copied as `params.disk_quality as u32`.

### egui panel (`src/ui.rs`)

A new collapsible "Disk turbulence" section after the existing disk params block, holding eight sliders plus a `DiskQuality` `ComboBox` (pattern identical to the existing bloom-quality combo at `ui.rs:58-68`). Sliders are disabled when `DiskQuality::Off`.

### Shader function changes (`assets/shaders/black_hole.wgsl`)

- New `struct DiskSample { color: vec3<f32>, density: f32 }`.
- New `fn disk_color_volumetric(pos, dir) -> DiskSample` (ridged + density + spiral-arm path).
- Renamed `disk_color` → `fn disk_color_flat(pos, dir) -> DiskSample` returning `{ color: <existing calc>, density: 0.85 }` (the `Off` fallback).
- Extracted shared helpers `temperature_color(r)`, `radial_falloff(r)`, `apply_doppler(col, pos, dir)` used by both paths.
- New `fn ridged_fbm(p, octaves) -> f32`.
- Main loop `:447-455` restructured per §2, dispatching on `disk_quality`: `Off` → single `disk_color_flat` midplane sample (current behavior); non-`Off` → in-disk step sampling (part A) + edge capture (part B).

## Files touched

| File | Change |
|------|--------|
| `assets/shaders/black_hole.wgsl` | `ridged_fbm`, `DiskSample`, `disk_color_volumetric`, `disk_color_flat`, shared helpers, main-loop §2 restructure |
| `src/render/material.rs` | `BlackHoleUniforms`: +9 fields, `Default` |
| `src/params.rs` | `DiskQuality` enum + octaves; `BlackHoleParams`: +8 f32 + tier field; web/desktop `Default` |
| `src/ui.rs` | "Disk turbulence" collapsible: 8 sliders + quality combo |
| `src/render/plugin.rs` | `mirror_params`: +9 field copies |

**Not touched:** `src/physics.rs`, `src/lib.rs`, `assets/shaders/brightpass.wgsl` / `blur.wgsl` / `composite.wgsl`, `src/camera.rs`, `src/scene/planets.rs`, `src/web.rs`.

## Risks and mitigation

1. **Web frame rate (top risk).** Volumetric integration atop the already-heavy RK45 may drop web below playable.
   - Mitigation: `Off` escape hatch; web defaults `Low` + thin slab (0.2); octaves cut to 3/2/2. After implementation, web frame rate must be measured at Low; if it still drops, consider further lowering the web default `filament_freq`. This is part of the existing Phase 3 human visual/perf validation gate.
2. **`DiskSample` struct return overhead.** WGSL returns structs by value; the compiler expands to registers. Negligible; acceptable.
3. **Octave count must be runtime-known.** Addressed by the fixed-`MAX_OCTAVES`-with-break pattern above (conservative WebGPU form).
4. **Secondary-image disk texture.** Rays that loop around the hole and re-cross the disk re-trigger volumetric integration — desirable (secondary image should also show texture), and naturally bounded by `accum_alpha > 0.99`. Cost is at most doubled on those rays; no special handling needed.
5. **`accum_alpha` now carries units.** Today alpha is the dimensionless constant 0.85. Under volumetric integration it becomes `density × step_len` (length units). Numerically: density ∈ [0,1], step_len ∈ [0.1, 1.0] typically, so a single in-disk step contributes ≈ 0.0–1.0 to alpha, and a few steps saturate to the 0.99 break. Magnitudes are reasonable, but `density_strength` default may need tuning after first visual check to avoid an over-transparent or over-opaque disk.

## Validation (human, per Phase 3 Task 8 process)

- **Desktop** `cargo run --release`: compare against the Gargantua reference — check that bright filaments, spiral-arm winding, slab thickness edge-feather, and Doppler left/right asymmetry all read correctly at High tier.
- **Web** `trunk serve`: confirm Low-tier frame rate is acceptable and that `Off` reverts exactly to the current disk appearance.
- **`cargo test`**: confirm `physics.rs` tests are unaffected (expected all green — `physics.rs` is not modified by this spec).
