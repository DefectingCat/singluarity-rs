# Volumetric Accretion Disk Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the smooth zero-thickness disk with a Gargantua-style volumetric gas disk — finite-thickness luminous slab, ridged bright filaments, density clumping, logarithmic-spiral arm modulation — all panel-tunable with a quality tier (Off/Low/Medium/High).

**Architecture:** Volumetric integration reuses the existing RK45 adaptive loop (approach A from the spec): each accepted step that lies inside the disk thickness slab samples the new `disk_color_volumetric` and accumulates emission × arc length. A ridged multifractal noise field drives bright filaments; a separate smoothstep-gated FBM drives density; a logarithmic-spiral term riding the Keplerian shear drives large-scale arms. `physics.rs` is untouched (volumetric integration is render-sampling layer only).

**Tech Stack:** Bevy 0.19, WGSL, egui. Natural units (Rs = 1).

**Spec:** `docs/superpowers/specs/2026-07-15-volumetric-disk-design.md`

---

## File Structure

| File | Responsibility | Change |
|------|----------------|--------|
| `src/params.rs` | `DiskQuality` enum + tunable params + defaults | Add enum, 8 f32 fields, tier field, web/desktop defaults |
| `src/render/material.rs` | GPU uniform struct (`BlackHoleUniforms`) | Add 9 fields (8 f32 + 1 u32) to struct + `Default` |
| `assets/shaders/black_hole.wgsl` | The renderer | Add uniform fields, `ridged_fbm`, `DiskSample`, `disk_color_volumetric`, `disk_color_flat`, shared helpers; restructure main-loop disk handling |
| `src/render/plugin.rs` | Per-frame param→uniform mirror | Copy 9 new fields in `mirror_params` |
| `src/ui.rs` | egui Controls panel | Add "Disk turbulence" collapsible section |

**Not touched:** `src/physics.rs`, `src/lib.rs`, bloom/brightpass/blur/composite shaders, `src/camera.rs`, `src/scene/planets.rs`, `src/web.rs`.

---

## Task 1: Add `DiskQuality` enum + params fields

Extend `BlackHoleParams` with the new tunables and a `DiskQuality` tier enum. This is the data layer — nothing reads it yet, so it compiles standalone.

**Files:**
- Modify: `src/params.rs`

- [ ] **Step 1: Add the `DiskQuality` enum**

Insert after the `BloomQuality` impl block (after `src/params.rs:22`, the closing `}` of `impl BloomQuality`):

```rust
/// Disk volumetric rendering quality. Gates noise octave counts.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DiskQuality {
    Off,     // flat zero-thickness fallback (current appearance)
    Low,     // 3/2/2 octaves — web default
    Medium,  // 4/3/3 octaves
    #[default]
    High,    // 5/4/3 octaves — desktop default
}

impl DiskQuality {
    /// Returns (filament_octaves, density_octaves, warp_octaves).
    /// Off returns zeros; the shader dispatches to the flat path instead.
    pub fn octaves(self) -> (u32, u32, u32) {
        match self {
            DiskQuality::Off => (0, 0, 0),
            DiskQuality::Low => (3, 2, 2),
            DiskQuality::Medium => (4, 3, 3),
            DiskQuality::High => (5, 4, 3),
        }
    }
}
```

- [ ] **Step 2: Add the 8 f32 fields + tier field to `BlackHoleParams`**

Add these fields to the `BlackHoleParams` struct, after `bloom_quality: BloomQuality,` (after `src/params.rs:56`):

```rust
    // Disk turbulence (Phase 3.1: volumetric disk)
    pub disk_half_thickness: f32,
    pub filament_freq: f32,
    pub filament_sharpness: f32,
    pub density_freq: f32,
    pub density_strength: f32,
    pub arm_count: f32,
    pub arm_tightness: f32,
    pub arm_strength: f32,
    pub disk_quality: DiskQuality,
```

- [ ] **Step 3: Add the defaults to `impl Default`**

Add these to the `Self { ... }` literal in `Default::default` (after `bloom_quality: ...` at `src/params.rs:82`):

```rust
            disk_half_thickness: if cfg!(target_arch = "wasm32") { 0.2 } else { 0.3 },
            filament_freq: 1.0,
            filament_sharpness: 2.0,
            density_freq: 0.8,
            density_strength: 1.0,
            arm_count: 2.0,
            arm_tightness: 2.0,
            arm_strength: 0.5,
            disk_quality: if cfg!(target_arch = "wasm32") { DiskQuality::Low } else { DiskQuality::High },
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors (the new fields are unused yet, but `#[allow(dead_code)]` on the struct suppresses warnings).

- [ ] **Step 5: Commit**

```bash
git add src/params.rs
git commit -m "feat(params): add DiskQuality enum + volumetric disk params

Data layer for the volumetric disk. DiskQuality gates octave counts
(Off/Low/Medium/High); Off is a full revert to the flat disk. Eight new
tunable f32 fields for thickness, filament, density, and spiral-arm
control. Web defaults to Low tier + 0.2 half-thickness; desktop to
High + 0.3. Nothing reads these yet."
```

---

## Task 2: Extend the GPU uniform struct

Add the 9 fields (8 f32 + 1 u32 tier) to both the Rust `BlackHoleUniforms` and the WGSL `BlackHoleUniforms`, keeping them in lockstep. The order and types must match exactly between Rust and WGSL.

**Files:**
- Modify: `src/render/material.rs`
- Modify: `assets/shaders/black_hole.wgsl:14-42` (the WGSL uniform struct)

- [ ] **Step 1: Add fields to the Rust `BlackHoleUniforms` struct**

In `src/render/material.rs`, add these fields to `pub struct BlackHoleUniforms` after `pub _pad5: f32,` (after line 45):

```rust
    // Disk volumetric (Phase 3.1)
    pub disk_half_thickness: f32,
    pub filament_freq: f32,
    pub filament_sharpness: f32,
    pub density_freq: f32,
    pub density_strength: f32,
    pub arm_count: f32,
    pub arm_tightness: f32,
    pub arm_strength: f32,
    pub disk_quality: u32,
```

- [ ] **Step 2: Add defaults to the Rust `Default` impl**

In `impl Default for BlackHoleUniforms` (`src/render/material.rs:48-84`), add after `_pad5: 0.0,` (after line 81):

```rust
            disk_half_thickness: 0.3,
            filament_freq: 1.0,
            filament_sharpness: 2.0,
            density_freq: 0.8,
            density_strength: 1.0,
            arm_count: 2.0,
            arm_tightness: 2.0,
            arm_strength: 0.5,
            disk_quality: 3, // High
```

- [ ] **Step 3: Add fields to the WGSL uniform struct**

In `assets/shaders/black_hole.wgsl`, replace the struct tail (lines 38-42):

```wgsl
    bloom_threshold: f32,
    bloom_strength: f32,
    exposure: f32,
    _pad5: f32,
};
```

with:

```wgsl
    bloom_threshold: f32,
    bloom_strength: f32,
    exposure: f32,
    _pad5: f32,
    disk_half_thickness: f32,
    filament_freq: f32,
    filament_sharpness: f32,
    density_freq: f32,
    density_strength: f32,
    arm_count: f32,
    arm_tightness: f32,
    arm_strength: f32,
    disk_quality: u32,
};
```

- [ ] **Step 4: Verify it compiles + shader reflects**

Run: `cargo build`
Expected: compiles. `ShaderType` derive generates the uniform layout from the Rust struct; as long as the WGSL field names/types match, bind group reflection will align. (A mismatch surfaces at runtime as a validation error / grey screen, per the AGENTS.md gotchas — so this match is critical.)

- [ ] **Step 5: Commit**

```bash
git add src/render/material.rs assets/shaders/black_hole.wgsl
git commit -m "feat(uniform): add 9 volumetric-disk fields to BlackHoleUniforms

Rust and WGSL structs kept in lockstep: 8 f32 tunables + disk_quality
u32 tier. Appended after _pad5; scalar fields pack contiguously so no
explicit padding needed. Defaults mirror BlackHoleParams::default."
```

---

## Task 3: Mirror the new params into the uniform each frame

Wire `BlackHoleParams` → `BlackHoleUniforms` in the existing per-frame `mirror_params` system. After this task, the GPU receives the new values (though nothing in the shader uses them yet).

**Files:**
- Modify: `src/render/plugin.rs:579-623` (the `mirror_params` copy loop)

- [ ] **Step 1: Add the field copies**

In `src/render/plugin.rs`, inside the `for (_, mat) in materials.iter_mut()` loop in `mirror_params`, add after `u.exposure = params.exposure;` (after line 622):

```rust
        u.disk_half_thickness = params.disk_half_thickness;
        u.filament_freq = params.filament_freq;
        u.filament_sharpness = params.filament_sharpness;
        u.density_freq = params.density_freq;
        u.density_strength = params.density_strength;
        u.arm_count = params.arm_count;
        u.arm_tightness = params.arm_tightness;
        u.arm_strength = params.arm_strength;
        u.disk_quality = params.disk_quality as u32;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles cleanly.

- [ ] **Step 3: Commit**

```bash
git add src/render/plugin.rs
git commit -m "feat(mirror): copy volumetric-disk params into uniform each frame

Extends mirror_params with the 9 new field assignments. DiskQuality enum
is cast to u32 for the WGSL tier selector. GPU now receives live values;
shader does not consume them yet."
```

---

## Task 4: Add `ridged_fbm` noise function to the shader

Add the ridged multifractal noise that produces sharp bright filaments. This is a standalone helper — no caller yet, so it compiles but is inert. Uses the fixed-`MAX_OCTAVES`-with-break pattern for WebGPU driver safety.

**Files:**
- Modify: `assets/shaders/black_hole.wgsl` (add after the existing `fbm3` function, which ends at line 218)

- [ ] **Step 1: Add `ridged_fbm`**

Insert immediately after the closing `}` of `fn fbm3` (`assets/shaders/black_hole.wgsl:218`):

```wgsl
// Ridged multifractal noise: 1 - |2n-1| turns value-noise gradients into
// sharp ridges (peak where n=0.5, zero at n=0 and n=1). Raising to
// `sharpness` thins the ridges into filaments. MAX_OCTAVES-with-break is
// the conservative WebGPU form for a runtime-chosen octave count.
fn ridged_fbm(p: vec3<f32>, octaves: u32, sharpness: f32) -> f32 {
    const MAX_OCTAVES = 6u;
    var sum = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i: u32 = 0u; i < MAX_OCTAVES; i = i + 1u) {
        if (i >= octaves) { break; }
        let n = value_noise3(p * freq);
        let ridge = 1.0 - abs(2.0 * n - 1.0);
        sum = sum + amp * pow(ridge, sharpness);
        freq = freq * 2.0;
        amp = amp * 0.5;
    }
    return sum;
}
```

- [ ] **Step 2: Verify the shader still compiles**

Run: `cargo build`
Expected: compiles. (`ridged_fbm` is unused, but WGSL does not warn on unused functions the way Rust does; `value_noise3` is already defined at line 187.)

- [ ] **Step 3: Commit**

```bash
git add assets/shaders/black_hole.wgsl
git commit -m "feat(shader): add ridged_fbm multifractal noise

Ridged noise (1 - |2n-1|, raised to sharpness) produces the sharp bright
filaments the spec calls for, replacing the smooth FBM blobs. Fixed
MAX_OCTAVES=6 with early break — conservative form for runtime-chosen
octave counts on older WebGPU drivers. Inert; no caller yet."
```

---

## Task 5: Extract shared physics helpers + add `DiskSample` + `disk_color_flat`

Refactor the existing `disk_color` (`assets/shaders/black_hole.wgsl:226-255`) into shared helpers plus a flat fallback that returns a `DiskSample`. This preserves the exact current appearance behind the `Off` tier and sets up the struct that the volumetric path (Task 6) will also return.

**Files:**
- Modify: `assets/shaders/black_hole.wgsl:226-255`

- [ ] **Step 1: Add the `DiskSample` struct**

Insert immediately before `fn disk_color` (before line 226):

```wgsl
// Result of a disk color query: emitted radiance + opacity contribution.
// Both the volumetric and flat paths return this struct so the main loop
// can treat them uniformly.
struct DiskSample {
    color: vec3<f32>,
    density: f32,
}
```

- [ ] **Step 2: Add shared helpers**

Insert immediately after the `DiskSample` struct (before `fn disk_color`):

```wgsl
// Radial temperature gradient: white-hot inner → deep-orange outer.
fn temperature_color(t: f32) -> vec3<f32> {
    return mix(vec3<f32>(1.0, 0.95, 0.85), vec3<f32>(1.0, 0.45, 0.12), clamp(t, 0.0, 1.0));
}

// Radial brightness falloff (∝ 1/r² from the inner edge).
fn radial_falloff(r: f32, inner: f32) -> f32 {
    return 1.0 / pow(r / inner, 2.0);
}

// Relativistic Doppler beaming. `dir` is the ray direction (disk-local).
fn apply_doppler(col: vec3<f32>, pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let phi = atan2(pos.z, pos.x);
    let v_orbital = sqrt(uniforms.rs / (2.0 * r_of(pos)));
    let tangent = normalize(vec3<f32>(-sin(phi), 0.0, cos(phi)));
    let vdotn = dot(tangent * v_orbital, -dir);
    let gamma = 1.0 / sqrt(max(1.0 - v_orbital * v_orbital, 1e-4));
    if (uniforms.doppler_enabled == 0u) {
        return col;
    }
    let delta = 1.0 / (gamma * (1.0 - vdotn));
    let doppler = pow(delta, 3.0) * uniforms.doppler_strength;
    return col * doppler;
}

// Cylindrical radius in the disk plane.
fn r_of(pos: vec3<f32>) -> f32 {
    return length(vec2<f32>(pos.x, pos.z));
}
```

- [ ] **Step 3: Replace `disk_color` with `disk_color_flat`**

Replace the entire `fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32>` (lines 226-255) with:

```wgsl
// Off-tier fallback: zero-thickness disk, single sample, fixed alpha.
// Preserves the exact pre-volumetric appearance. Returns DiskSample so the
// main loop dispatches both paths uniformly.
fn disk_color_flat(pos: vec3<f32>, dir: vec3<f32>) -> DiskSample {
    let r = r_of(pos);
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    let noise = disk_noise(vec3<f32>(pos.x * 0.3, pos.z * 0.3, rot), uniforms.time);

    let t = (r - uniforms.disk_inner) / (uniforms.disk_outer - uniforms.disk_inner);
    let tcol = temperature_color(t);
    let falloff = radial_falloff(r, uniforms.disk_inner);

    var col = tcol * (0.6 + 0.4 * noise) * falloff * uniforms.disk_brightness;
    col = apply_doppler(col, pos, dir);

    return DiskSample(vec3<f32>(col), 0.85);
}
```

- [ ] **Step 4: Add a temporary `disk_color` shim to keep the build green**

`disk_color` is renamed to `disk_color_flat`, but the main loop (line 450) still calls `disk_color`. To keep every task independently compilable, add a one-line shim right after `disk_color_flat`. (Removed in Task 7 when the main loop is restructured to call the new functions directly.)

Insert immediately after `disk_color_flat`:

```wgsl
// TEMPORARY shim — removed in Task 7 when the main loop is restructured.
fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    return disk_color_flat(pos, dir).color;
}
```

- [ ] **Step 5: Verify it compiles + app runs identically**

Run: `cargo build`
Expected: compiles.

Run: `cargo run --release`
Expected: the disk looks identical to before (the shim routes through `disk_color_flat`, which reproduces the old math).

- [ ] **Step 6: Commit**

```bash
git add assets/shaders/black_hole.wgsl
git commit -m "refactor(shader): extract disk helpers, add DiskSample + flat fallback

Splits disk_color into shared helpers (temperature_color, radial_falloff,
apply_doppler, r_of) reused by both the flat and volumetric paths.
disk_color_flat returns a DiskSample with the old fixed 0.85 alpha,
preserving the exact pre-volumetric appearance behind the Off tier. A
temporary disk_color shim keeps the main-loop call site compiling until
Task 7 restructures it."
```

---

## Task 6: Add `disk_color_volumetric`

The volumetric color function: ridged filaments for brightness, smoothstep-gated FBM for density, logarithmic-spiral arm modulation. Returns the same `DiskSample` struct.

**Files:**
- Modify: `assets/shaders/black_hole.wgsl` (add after `disk_color_flat` + its shim)

- [ ] **Step 1: Add `disk_color_volumetric`**

Insert immediately after the temporary `disk_color` shim from Task 5:

```wgsl
// Volumetric disk color: ridged filaments drive brightness, a smoothstep-
// gated FBM drives density clumping, and a logarithmic-spiral term (riding
// the Keplerian shear `rot`) drives large-scale arm structure. The three
// signals multiply — density says where matter is, filaments say how bright,
// arms say how it's distributed.
fn disk_color_volumetric(pos: vec3<f32>, dir: vec3<f32>) -> DiskSample {
    let r = r_of(pos);
    let rot = uniforms.time * uniforms.disk_rotation_speed / pow(r, 1.5);
    let flow = vec3<f32>(0.0, 0.0, rot);

    // Octave triplet from the quality tier.
    let q = uniforms.disk_quality;
    var filament_octaves = 5u; var density_octaves = 4u; var warp_octaves = 3u;
    if (q == 1u) { filament_octaves = 3u; density_octaves = 2u; warp_octaves = 2u; }
    else if (q == 2u) { filament_octaves = 4u; density_octaves = 3u; warp_octaves = 3u; }
    // q == 3u keeps the High defaults above; q == 0u is never passed here.

    // Domain warp: distorts sample coords so filaments curve and bend.
    let warp = fbm3(pos * 0.8 + flow * 0.1, warp_octaves);

    // Layer 1: ridged bright filaments.
    let filament = ridged_fbm(pos * uniforms.filament_freq + warp * 1.5 + flow * 0.3,
                              filament_octaves, uniforms.filament_sharpness);

    // Layer 2: density clumping (smoothstep makes a definite gas/void boundary).
    let density_noise = fbm3(pos * uniforms.density_freq + warp, density_octaves);
    let base_density = smoothstep(0.3, 0.7, density_noise) * uniforms.density_strength;

    // Layer 3: logarithmic-spiral arm modulation, advected by Keplerian shear.
    let phi = atan2(pos.z, pos.x);
    let arm_phase = phi * uniforms.arm_count + log(r) * uniforms.arm_tightness - rot;
    let arm = 0.5 + 0.5 * cos(arm_phase);
    let arm_mod = mix(1.0, pow(arm, 2.0), uniforms.arm_strength);

    let total_density = base_density * arm_mod;
    let brightness = filament;

    let t = (r - uniforms.disk_inner) / (uniforms.disk_outer - uniforms.disk_inner);
    let tcol = temperature_color(t);
    let falloff = radial_falloff(r, uniforms.disk_inner);

    var col = tcol * brightness * falloff * uniforms.disk_brightness;
    col = apply_doppler(col, pos, dir);

    return DiskSample(vec3<f32>(col), total_density);
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles. (Function is unused; `ridged_fbm`, `fbm3`, `value_noise3` all defined.)

- [ ] **Step 3: Commit**

```bash
git add assets/shaders/black_hole.wgsl
git commit -m "feat(shader): add disk_color_volumetric (ridged + density + arms)

Three multiplying signal layers: ridged_fbm filaments for brightness,
smoothstep-gated FBM for density clumping, logarithmic-spiral arm
modulation riding the Keplerian shear. Octave triplet selected from
disk_quality tier. Returns DiskSample; inert until Task 7 wires it in."
```

---

## Task 7: Restructure the main loop for volumetric integration

This is the core integration change. Replace the single `disk_hit` sample with: (A) in-slab per-step sampling, (B) midplane edge-capture, dispatching between flat and volumetric paths by tier. Remove the Task-5 shim.

**Files:**
- Modify: `assets/shaders/black_hole.wgsl:447-455` (the disk block in the main loop)
- Modify: the temporary `disk_color` shim (remove it)

- [ ] **Step 1: Remove the temporary `disk_color` shim**

Delete the shim added in Task 5 Step 4:

```wgsl
// TEMPORARY shim — removed in Task 7 when the main loop is restructured.
fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    return disk_color_flat(pos, dir).color;
}
```

- [ ] **Step 2: Replace the disk-handling block in the main loop**

In `assets/shaders/black_hole.wgsl`, replace the current disk block (lines 447-455):

```wgsl
        if (disk_hit(prev, new_pos)) {
            let ty = prev.y / (prev.y - new_pos.y);
            let hit = mix(prev, new_pos, vec3<f32>(ty));
            let dc = disk_color(hit, new_dir);
            let a = 0.85;
            accum_color += (1.0 - accum_alpha) * dc * a;
            accum_alpha += (1.0 - accum_alpha) * a;
            if (accum_alpha > 0.99) { break; }
        }
```

with:

```wgsl
        // --- volumetric disk ---
        if (uniforms.disk_quality == 0u) {
            // Off tier: zero-thickness single midplane sample, fixed alpha.
            if (disk_hit(prev, new_pos)) {
                let ty = prev.y / (prev.y - new_pos.y);
                let hit = mix(prev, new_pos, vec3<f32>(ty));
                let s = disk_color_flat(hit, new_dir);
                accum_color += (1.0 - accum_alpha) * s.color * s.density;
                accum_alpha += (1.0 - accum_alpha) * s.density;
                if (accum_alpha > 0.99) { break; }
            }
        } else {
            // Volumetric tier.
            // (A) In-slab per-step sampling: if this step ends inside the
            // thickness slab, accumulate emission × arc length. Reuses the
            // RK45 adaptive step — dense where light bends, sparse where straight.
            let slab_r = r_of(new_pos);
            if (abs(new_pos.y) < uniforms.disk_half_thickness
                && slab_r >= uniforms.disk_inner
                && slab_r <= uniforms.disk_outer) {
                let s = disk_color_volumetric(new_pos, new_dir);
                let step_len = length(new_pos - prev);
                accum_color += (1.0 - accum_alpha) * s.color * s.density * step_len;
                accum_alpha += (1.0 - accum_alpha) * s.density * step_len;
            }
            // (B) Midplane edge-capture: if a step straddles y=0, add one
            // precise at-plane sample weighted by the slab depth along the ray.
            if (disk_hit(prev, new_pos)) {
                let ty = prev.y / (prev.y - new_pos.y);
                let hit = mix(prev, new_pos, vec3<f32>(ty));
                let s = disk_color_volumetric(hit, new_dir);
                let thickness_proj = uniforms.disk_half_thickness / max(abs(new_dir.y), 1e-3);
                accum_color += (1.0 - accum_alpha) * s.color * s.density * thickness_proj;
                accum_alpha += (1.0 - accum_alpha) * s.density * thickness_proj;
            }
            if (accum_alpha > 0.99) { break; }
        }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors. (Both `disk_color_flat` and `disk_color_volumetric` return `DiskSample`; the main loop reads `.color` and `.density`.)

- [ ] **Step 4: Verify visually at all tiers**

Run: `cargo run --release`
Expected:
- With default params (High tier), the disk shows bright filaments, density clumping, and spiral-arm structure — visibly different from the smooth original.
- Switching the panel to `Off` (once Task 8 adds the panel) reverts to the smooth disk. (If running this step before Task 8, temporarily edit `params.rs` default `disk_quality` to `DiskQuality::Off` to confirm the flat path works, then revert.)

- [ ] **Step 5: Verify physics tests are unaffected**

Run: `cargo test`
Expected: all tests pass (the mirror in `physics.rs` is not modified by this plan; this confirms no accidental regression).

- [ ] **Step 6: Commit**

```bash
git add assets/shaders/black_hole.wgsl
git commit -m "feat(render): volumetric disk integration in the RK45 loop

Restructures the disk block in the main loop:
- Off tier: flat disk_color_flat single midplane sample (old behavior).
- Volumetric tiers: (A) in-slab per-step sampling accumulates emission ×
  arc length, reusing the RK45 adaptive step density; (B) midplane edge-
  capture weights one at-plane sample by slab depth along the ray.
Removes the Task-5 disk_color shim. physics.rs untouched; cargo test green."
```

---

## Task 8: Add the "Disk turbulence" egui panel section

Wire all 8 sliders + the quality dropdown into the Controls window so the user can tune live. Sliders disable when quality is `Off`.

**Files:**
- Modify: `src/ui.rs` (insert a new `CollapsingHeader` after the existing "Accretion Disk" section, which ends at line 38)

- [ ] **Step 1: Add the panel section**

In `src/ui.rs`, insert this new `CollapsingHeader` immediately after the "Accretion Disk" block's closing `});` (after line 38, before the "Doppler" header at line 39):

```rust
                egui::CollapsingHeader::new("Disk Turbulence")
                    .default_open(true)
                    .show(ui, |ui| {
                        use crate::params::DiskQuality;
                        let mut q = params.disk_quality;
                        egui::ComboBox::from_label("Disk quality")
                            .selected_text(format!("{:?}", q))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut q, DiskQuality::Off, "Off");
                                ui.selectable_value(&mut q, DiskQuality::Low, "Low");
                                ui.selectable_value(&mut q, DiskQuality::Medium, "Medium");
                                ui.selectable_value(&mut q, DiskQuality::High, "High");
                            });
                        params.disk_quality = q;
                        let on = q != DiskQuality::Off;
                        ui.add_enabled(on, egui::Slider::new(&mut params.disk_half_thickness, 0.05..=1.0).text("Half thickness"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.filament_freq, 0.2..=4.0).text("Filament frequency"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.filament_sharpness, 1.0..=6.0).text("Filament sharpness"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.density_freq, 0.2..=3.0).text("Density frequency"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.density_strength, 0.0..=2.0).text("Density strength"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.arm_count, 0.0..=6.0).text("Arm count"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.arm_tightness, 0.0..=6.0).text("Arm tightness"));
                        ui.add_enabled(on, egui::Slider::new(&mut params.arm_strength, 0.0..=1.0).text("Arm strength"));
                    });
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 3: Verify the panel works live**

Run: `cargo run --release`
Expected:
- A "Disk Turbulence" collapsible appears in the Controls window between "Accretion Disk" and "Doppler".
- Selecting `Off` disables the 8 sliders and the disk reverts to smooth.
- Selecting `High` re-enables them; dragging each slider visibly changes the disk in real time.
- The quality combo and slider-enable pattern mirror the existing Bloom quality block.

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs
git commit -m "feat(ui): Disk Turbulence panel with quality tier + 8 sliders

Live-tunable volumetric disk params. Quality ComboBox (Off/Low/Medium/
High) mirrors the Bloom quality pattern; the 8 sliders disable when Off.
Off reverts to the flat disk for perf escape + visual A/B."
```

---

## Task 9: Final verification + web check

Confirm the full system works end-to-end on both targets and that no regression slipped in.

**Files:** none modified.

- [ ] **Step 1: Full test suite**

Run: `cargo test`
Expected: all tests pass. (`physics.rs` is untouched by this entire plan; this is a regression guard.)

- [ ] **Step 2: Desktop release build + visual check**

Run: `cargo run --release`
Expected:
- Default High tier: disk shows ridged bright filaments, density clumping (bright knots + dark gaps), and logarithmic-spiral arm structure winding with differential rotation. Doppler left/right asymmetry preserved. Slab thickness produces feathered edges at the disk limb.
- Compare against the Gargantua reference for the intended aesthetic.
- `Off` tier: disk reverts to the smooth pre-volumetric appearance.
- Tune the 8 sliders; each produces a visible, sensible response.

- [ ] **Step 3: Web build + Low-tier frame-rate check**

Run: `trunk serve`
Expected:
- App loads at http://127.0.0.1:8080 with WebGPU.
- Low tier (web default) renders the volumetric disk at an acceptable frame rate.
- `Off` reverts to flat and is fastest.

If Low tier frame rate is poor on web, the spec's mitigation applies: lower the web-default `filament_freq` in `src/params.rs`. Document the finding.

- [ ] **Step 4: Density-strength tuning check**

Per spec risk #5, inspect whether the disk is over-transparent (background bleeds through too much) or over-opaque (a solid ring). Adjust `density_strength` default in `src/params.rs` if needed and re-check. Commit any default change separately:

```bash
git add src/params.rs
git commit -m "tune(params): adjust density_strength default after visual check"
```

- [ ] **Step 5: Final commit (only if any tuning changes were made in Step 4)**

No commit needed if defaults held. This step exists only to capture tuning.
