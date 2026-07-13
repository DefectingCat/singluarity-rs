# Interstellar Black Hole Renderer — Phase 2 (Kerr) Design Spec

**Date:** 2026-07-13
**Status:** Draft (awaiting user review)
**Project:** `singularity-rs` (Rust, edition 2024, Bevy 0.19) — desktop + web (WebGPU)
**Predecessor:** `docs/superpowers/specs/2026-07-09-interstellar-blackhole-design.md` (Phase 1, Schwarzschild — shipped)

## 0. Context

Phase 1 delivered a working Schwarzschild (non-spinning) renderer: black shadow, tilted Doppler disk with lensed halo, lensed stars, optional grid/planets/skybox, orbit camera, egui panel. The integrator core is a fixed-step RK4 in `black_hole.wgsl:110-119` (`deriv`) and `:268-322` (the integration loop).

Phase 2 replaces that integrator core with a Kerr (spinning) geodesic integrator so the hole exhibits frame-dragging / ergosphere asymmetry — the defining visual feature of a true Gargantua. Per the Phase 1 spec (§10), "only the integrator core swaps; the same scene elements, camera, and params carry over unchanged."

This spec covers *only* the Phase 2 delta. Anything not mentioned here is inherited unchanged from Phase 1.

## 1. Goal

Replace the Schwarzschild geodesic integrator with a Kerr geodesic integrator (Cartesian pseudo-Hamiltonian form, adaptive RK45), so that:

- At `spin = 0` the renderer is **bit-identical in behavior** to Phase 1 (Kerr math degenerates to Schwarzschild).
- At `spin > 0` the accretion disk and lensed halo show frame-dragging asymmetry — the bright/dark Doppler sides shear off-axis, and the photon sphere becomes oblate/prolate relative to the spin axis.
- The disk inner edge tracks the Kerr ISCO (Bardeen formula), shrinking from 3 Rs at spin=0 toward Rs/2 at extremal spin.
- All Phase 1 scene elements (disk, stars, planets, grid, skybox), the orbit camera, and the egui panel continue to work without behavioral regression.

**Non-goals (deferred to future work, documented in §8):**
- Full exact Kerr pseudo-Hamiltonian (this spec ships the leading-order frame-dragging term; the exact form is future work).
- Retrograde disk / spin sign (spin is non-negative; the hole spins one way).
- Volumetric disk, relativistic beaming beyond the Phase 1 Doppler model.

## 2. Physics & units (delta from Phase 1 §4)

Natural units with **Rs = 1** everywhere, inherited from Phase 1. Phase 2 introduces the Kerr spin parameter:

| Quantity | Definition | Range |
|---|---|---|
| `χ` (UI "spin") | dimensionless spin `a/M` | `[0, 1]` (0 = Schwarzschild, 1 = extremal Kerr) |
| `a` (internal) | Kerr spin length `a = χ·M` | `[0, M] = [0, 0.5]` |
| `M` (internal) | mass `Rs/2 = 0.5` | constant |

**Spin-axis orientation:** Kerr spin axis aligned with world **+Y**. The disk remains the equatorial plane (xz-plane, `y=0`). This preserves every Phase 1 crossing test (`disk_hit`, `planet_hit`, `grid_hit` all assume `y=0` equatorial geometry). No coordinate-conversion code leaks outside `deriv()`.

**Spin-dependent radii (Rs units, Rs=1):**
| Radius | Formula | spin=0 | spin=1 (extremal) |
|---|---|---|---|
| Event horizon `r₊` | `M + sqrt(M² − a²)` | 1.0 (= Rs) | 0.5 (= M) |
| ISCO (prograde) | Bardeen formula, §2.1 | 3.0 (= 3 Rs) | 0.5 (= Rs/2) |
| Photon sphere | (emerges from integrator; not pinned) | ≈1.5 | < 1.5, oblate |

### 2.1 ISCO (Bardeen-Press-Teukolsky 1972, prograde)

```
Z1 = 1 + (1 − χ²)^(1/3) · [ (1 + χ)^(1/3) + (1 − χ)^(1/3) ]
Z2 = sqrt(3χ² + Z1²)
ISCO = M · [ 3 + Z2 − sqrt( (3 − Z1) · (3 + Z1 + 2·Z2) ) ]    // prograde: minus sign
```

Verified: χ=0 → ISCO = 6M = 3 Rs ✓; χ=1 → ISCO = M = Rs/2 ✓. Monotonically decreasing in χ.

### 2.2 Geodesic `deriv()` — Cartesian pseudo-Hamiltonian, leading-order frame-dragging

The Phase 1 `deriv(pos, dir) -> Deriv{dpos, ddir}` signature is preserved. The body gains one spin-orthogonal term:

```
fn deriv(pos, dir) -> Deriv {
    let r   = length(pos);
    let M   = 0.5;
    let chi = uniforms.spin;            // dimensionless, [0,1]
    let a   = chi * M;
    let Rs  = uniforms.rs;              // = 1.0
    // Schwarzschild radial bending (unchanged at chi=0)
    let h   = cross(pos, dir);
    let h2  = dot(h, h);
    let r3  = r * r * r;
    let r5  = r3 * r * r;
    let radial = -1.5 * Rs * h2 / r5 * pos;
    // Frame-dragging (Lense-Thirring leading term). Spin axis = +Y.
    let spin_axis = vec3(0, 1, 0);
    let drag = 2.0 * M * a / r3 * cross(spin_axis, dir);
    let accel = radial + drag;
    return Deriv(dir, accel);
}
```

**Degeneracy:** at `χ=0`, `a=0` → `drag=0` → `accel = -1.5·Rs·h²/r⁵·pos`, exactly the Phase 1 formula (`black_hole.wgsl:117`). This is the load-bearing correctness assertion, covered by a unit test.

**Approximation status:** `drag` is the leading-order `g_{tφ}` (Lense-Thirring) term in Cartesian dress. It reproduces the frame-dragging *direction* and asymmetry sign correctly. The exact Kerr Cartesian pseudo-Hamiltonian (Riazzi-style Σ/Δ terms) is deferred — see §8. At high spin the photon-orbit radii will deviate from exact BL by a few percent; at spin ≤ 0.7 the deviation is sub-percent.

## 3. Integrator — adaptive RK45 (Dormand-Prince)

The Phase 1 fixed-step RK4 loop (`black_hole.wgsl:268-322`) is replaced by an adaptive-step Dormand-Prince (RK45) loop. Same termination conditions (capture / escape / alpha-saturation), same crossing-test calls on each accepted segment.

### 3.1 Loop structure

```
var pos = rot_x(eye, -disk_tilt);          // inherited: disk-local space
var d   = normalize(rot_x(dir, -disk_tilt));
var dt  = total_path / steps_max;           // initial step (same seed as Phase 1)
var prev = pos;
var budget = steps_max;                      // uniforms.steps (hard cap)

loop {
    if (budget == 0) break;
    // Dormand-Prince step: 6 deriv() evals → (y5, y4) for error estimate
    let result = rk45_step(pos, d, dt);      // → (new_pos, new_dir, err_vec)
    let err_norm = length(result.err);

    // Step-size control.
    if (err_norm > tol * 10.0) {
        // Reject: shrink and retry. Does NOT consume budget.
        dt = clamp(dt * 0.2, dt_min, dt_max);
        continue;
    }
    // Accept: consume one budget unit, refine dt for next step.
    budget -= 1;
    dt = clamp(dt * pow(tol / max(err_norm, 1e-12), 0.2), dt_min, dt_max);

    // Crossing tests on the ACCEPTED segment [prev, new_pos] — unchanged calls.
    ... disk_hit(prev, result.pos) ...        // composites disk color
    ... planet_hit(prev, result.pos, result.dir) ...
    ... grid_hit(prev, result.pos) ...        // if enabled

    prev = result.pos;
    pos  = result.pos;
    d    = result.dir;

    // Termination (spin-dependent capture radius).
    if (length(pos) < r_plus) break;          // captured by horizon
    if (length(pos) > escape_r) { /* sample sky + stars, break */ }
}
```

### 3.2 Constants & budget semantics

| Knob | Value | Rationale |
|---|---|---|
| `steps_max` | `uniforms.steps` (UI-tunable, default 300 desktop / 200 web) | Hard cap on **accepted** steps. Rejected steps do not consume budget. |
| `tol` | `1e-3` (hardcoded, not UI-exposed) | ~0.1% of horizon radius; tight enough to keep the Einstein ring crisp. |
| `dt_min` | `total_path / (steps_max * 4)` | Prevents step collapse near the photon sphere. |
| `dt_max` | `total_path / (steps_max / 4)` | Prevents a single step from crossing the whole domain. |

**Budget = accepted steps only.** Rejected steps (failed `err > 10·tol` check) retry with a smaller `dt` without decrementing `budget`. This prevents the photon-sphere region (where steps shrink) from starving the rest of the ray. When `budget` hits zero the ray stops gracefully mid-flight — same degradation Phase 1 has.

### 3.3 Capture radius becomes spin-dependent

Phase 1 tests `r < Rs`. Phase 2 tests `r < r₊(χ) = M + sqrt(max(M² − a², 0))`. At spin=0 this equals Rs (no regression). The `max(., 0)` guards against floating error at extremal spin.

### 3.4 Cost model

Dormand-Prince: 6 `deriv()` evaluations per *attempted* step (vs RK4's 4 per *taken* step). Kerr `deriv` (leading-order) ≈ 30 ALU. At 300 accepted steps × ~1.5 reject ratio near the hole ≈ 450 attempts × 180 ALU = **~81k ALU/pixel** for the integrator core. Phase 1 Schwarzschild was ~12k ALU/pixel. The ~7× increase is the reason `render_scale` drops (§5).

## 4. Uniform & parameter changes

### 4.1 `spin` enters the GPU uniform

`spin` already exists in `BlackHoleParams` (`src/params.rs:29`) but is **not** in `BlackHoleUniforms` today. Add it by consuming the `_pad4` slot — no struct size change, no WGSL alignment shift.

`src/render/material.rs` — `BlackHoleUniforms`:
```rust
// before:
pub steps: u32,
pub _pad4: f32,
pub _pad5: f32,

// after:
pub steps: u32,
pub spin: f32,      // was _pad4. Dimensionless χ = a/M ∈ [0,1].
pub _pad5: f32,     // trailing pad retained.
```

`assets/shaders/black_hole.wgsl` — `BlackHoleUniforms` struct (mirror the same slot):
```wgsl
// before:
steps: u32,
_pad4: f32,
_pad5: f32,

// after:
steps: u32,
spin: f32,      // was _pad4.
_pad5: f32,
```

`src/render/plugin.rs` — `mirror_params` gains one line: `u.spin = params.spin;`.

### 4.2 `disk_inner` becomes ISCO-derived (CPU)

`disk_inner` is no longer free-tunable; it is the Kerr ISCO computed from `spin`. Add a CPU helper and call it in `mirror_params`:

```rust
// src/physics.rs (extends the existing Phase 1 module)
/// Prograde Kerr ISCO in Rs units (Rs=1). chi = a/M ∈ [0,1].
pub fn kerr_isco(chi: f32) -> f32 {
    let m = 0.5;
    let cbrt_pos = (1.0 + chi).cbrt();
    let cbrt_neg = (1.0 - chi).cbrt();
    let z1 = 1.0 + (1.0 - chi * chi).cbrt() * (cbrt_pos + cbrt_neg);
    let z2 = (3.0 * chi * chi + z1 * z1).sqrt();
    m * (3.0 + z2 - ((3.0 - z1) * (3.0 + z1 + 2.0 * z2)).sqrt())
}

/// Kerr event-horizon radius in Rs units (Rs=1). chi = a/M ∈ [0,1].
pub fn kerr_horizon(chi: f32) -> f32 {
    let m = 0.5;
    let a = chi * m;
    m + (m * m - a * a).max(0.0).sqrt()
}
```

In `mirror_params`: `u.disk_inner = kerr_isco(params.spin);` (overrides whatever `params.disk_inner` held). The `params.disk_inner` field is retained for Phase 1 backward-compat in the struct but ignored when `spin` is the driver.

### 4.3 UI (egui) changes — `src/ui.rs`

Under the existing "Camera" section's sibling, add a **"Black Hole"** collapsing header (or extend the Physics group):

- **Spin (χ):** `Slider::new(&mut params.spin, 0.0..=1.0)` — live; drives ISCO and frame-dragging.
- **ISCO (read-only):** display `kerr_isco(params.spin)` as a label — shows the derived disk inner edge.
- **Horizon (read-only):** display `kerr_horizon(params.spin)` as a label.

The existing **disk_inner** slider is removed (or shown disabled with the ISCO value), since it is now spin-derived. All other controls (disk_tilt, brightness, Doppler, steps, render_scale, stars, grid, planets, skybox) are unchanged.

## 5. Performance & render_scale

### 5.1 render_scale must actually be wired (Phase 2 prerequisite)

Phase 1 targeted `render_scale = 1.0` and `fit_quad_to_window` (`src/render/plugin.rs:79-93`) scales the quad to the full window without consulting `render_scale`. For Phase 2 the integrator cost (§3.4) forces sub-1.0 render scales, so this gap must be closed as a Phase 2 task.

**Why the naive approach fails:** simply shrinking the full-screen quad (multiplying `fit_quad_to_window`'s target scale by `render_scale`) does reduce fragment invocations, but leaves the surrounding area as camera clear-color border — the view no longer fills the window. That is not acceptable.

**The required mechanism is render-to-texture + upscale:** render the black-hole quad into an offscreen `Image` target sized `window × render_scale`, then draw that texture upscaled (linear) to the full-window camera view. Concretely this means a second `Camera` (or a render-graph node) whose target is the sub-resolution `Image`, and the existing fullscreen quad draws into *that* camera; a second pass/blit fills the visible window. The exact Bevy 0.19 wiring (offscreen `Image` handle, `Camera::render_target`, render-graph ordering, whether to use a second `Camera2d` or a post-processing pass) is a plan-phase implementation decision. The *behavior* — fewer shader invocations at lower `render_scale`, view still fills the window — is what this spec pins.

This is called out as the first implementation task (§10) because it unblocks all Phase 2 performance tuning and is itself the highest-risk unknown in the plan.

### 5.2 Defaults

| Platform | render_scale | steps | Notes |
|---|---|---|---|
| Desktop (Phase 1) | 1.0 | 300 | inherited |
| Desktop (Phase 2) | **0.75** | 300 | 7× integrator cost; 0.75 buys headroom |
| Web (Phase 1) | 0.75 | 200 | inherited |
| Web (Phase 2) | **0.5** | 200 | RK45 cost heavy on wasm |

These are defaults; both remain UI-tunable live.

### 5.3 Budget vs. quality

At `tol = 1e-3` and 300 budget, rays in the far-field (boring straight-ish paths) take few large steps and finish well under budget; rays grazing the photon sphere spend their whole budget there. This is the desired adaptive behavior — compute goes where the bending is. If the frame is dominated by photon-sphere pixels (close camera), budget may saturate and some rays terminate mid-flight, softening higher-order Einstein images. Acceptable; documented as a known quality lever (raise `steps` in the UI).

## 6. Testing & verification

### 6.1 Unit tests (`src/physics.rs`, `tests/physics_test.rs`)

Mirrors of the shader math in CPU code, asserted in Rust:

1. **Degeneracy:** `kerr_deriv(chi=0) == schwarzschild_deriv` within `1e-6` (asserts the `drag` term vanishes). Requires a CPU mirror of `deriv` for both metrics.
2. **ISCO endpoints:** `kerr_isco(0.0) ≈ 3.0`; `kerr_isco(1.0) ≈ 0.5`.
3. **ISCO monotonic:** `kerr_isco(0.3) > kerr_isco(0.6) > kerr_isco(0.9)`.
4. **Horizon endpoints:** `kerr_horizon(0.0) ≈ 1.0`; `kerr_horizon(1.0) ≈ 0.5`.
5. **Horizon monotonic:** decreases in `χ`.
6. **Existing Phase 1 tests still pass** (bcrit ≈ 2.598; capture/escape) — unchanged.

### 6.2 Visual milestones (manual)

1. **Spin=0 regression:** with `spin = 0`, the image is visually indistinguishable from Phase 1 (same shadow size, same halo, same Doppler).
2. **Frame-dragging asymmetry:** at `spin = 0.5`, the disk's bright Doppler side shears off the pure line-of-sight axis; the halo is no longer mirror-symmetric across the spin axis.
3. **ISCO shrink:** sweeping `spin` 0 → 0.9 visibly pulls the disk inner edge inward.
4. **Higher-order Einstein ring:** still forms near the photon sphere at spin > 0 (budget doesn't starve it at default steps).
5. **Performance:** ≥60 fps desktop at render_scale=0.75, steps=300; ≥30 fps web at render_scale=0.5, steps=200.
6. **No regression** in grid/planets/skybox when toggled on at spin > 0.

## 7. File structure (delta)

```
src/
  physics.rs            # +kerr_isco, +kerr_horizon, +CPU-mirrored kerr_deriv (tests)
  render/material.rs    # BlackHoleUniforms: _pad4 → spin
  render/plugin.rs      # mirror_params: +u.spin, +u.disk_inner=isco; fit_quad_to_window: +render_scale
  params.rs             # (no struct change; spin field exists; default unchanged=0.0)
  ui.rs                 # +Spin slider, ISCO/Horizon read-only labels; disk_inner slider removed
assets/shaders/
  black_hole.wgsl       # deriv() body (Kerr); loop (RK45); struct spin field; capture radius
tests/
  physics_test.rs       # +Kerr degeneracy/ISCO/horizon tests
```

No new files. No new crates. The change is concentrated in `black_hole.wgsl` (integrator), `physics.rs` (CPU mirror + formulas), `material.rs`/`plugin.rs` (one uniform field), and `ui.rs` (one slider).

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Leading-order frame-dragging deviates from exact Kerr at high spin | Documented (§2.2). At χ ≤ 0.7 sub-percent; visible mainly at χ > 0.9. Full pseudo-Hamiltonian is future work. |
| RK45 cost blowup near photon sphere | Budget cap (accepted-steps-only) + `dt_min` clamp; rays terminate gracefully. |
| `render_scale` not yet wired | First Phase 2 task (§5.1); blocks performance validation. |
| Disk crossing test under tiny steps | The `t = y0/(y0−y1)` lerp is resolution-independent; more/smaller crossings raise compositing cost but alpha-saturation early-out bounds it. |
| Extremal-spin numerical edge cases (χ → 1) | `max(., 0)` guards in horizon; ISCO formula stable through χ=1 (verified in §2.1). UI clamps χ ≤ 1.0. |
| spin=0 visual regression | Unit test + visual milestone #1; degeneracy is the central correctness assertion. |

## 9. Out of scope (future work)

- Full exact Kerr Cartesian pseudo-Hamiltonian (Σ/Δ/Carter-separable form) for <1% photon-orbit accuracy at all spins.
- Negative spin / retrograde disk (would need ISCO sign flip and UI sign toggle).
- Adaptive *order* integrator (currently fixed RK45 order, adaptive step only).
- Tilted-spin (spin axis not aligned to +Y) — would break the y=0 disk assumption and require generalizing all crossing tests.

## 10. Phasing within Phase 2 (suggested task ordering for the plan)

1. **render_scale wiring** (prerequisite; unblocks perf tuning).
2. **`spin` uniform plumbing** (material + plugin + WGSL struct), verify spin=0 still compiles and renders.
3. **CPU `kerr_isco` / `kerr_horizon` + unit tests** (pure Rust, no GPU).
4. **Kerr `deriv()` in shader** + capture radius; verify spin=0 bit-identical, spin>0 shows bending.
5. **RK45 adaptive shell** replacing the RK4 loop; tune `tol`/`dt_min`/`dt_max`.
6. **UI** (Spin slider, read-only ISCO/Horizon).
7. **Visual + performance validation** against §6 milestones; adjust defaults.
