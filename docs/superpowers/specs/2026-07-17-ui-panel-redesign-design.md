# Control Panel Redesign — Design Spec

**Date:** 2026-07-17
**Phase:** UI (presentation layer only; no physics/shader changes)
**Status:** approved design, pending implementation plan

## Goal

Redesign the egui control panel from a single floating `Window` with 11 `CollapsingHeader` sections of 40+ raw `Slider::new(...).text(...)` calls (no styling) into a docked right-side `SidePanel` with a cohesive dark sci-fi theme, layered grouping, presets, and aligned label/value rows.

This addresses the user-observed problem: "the settings all look bad." The root causes are structural, not cosmetic:

1. **No global styling** — egui defaults render against a busy HDR black-hole render; no accent color, no spacing tuning, default font sizes.
2. **Flat structure** — 11 `CollapsingHeader`s in a single scroll produces 11 collapse arrows and a long scroll, with no priority distinction between always-tuned (Camera, Quality) and rarely-tuned (Grid, Doppler) sections.
3. **Ragged widgets** — `Slider::new(x, r).text("label")` produces misaligned labels and value readouts; no units on any readout.
4. **No presets** — the project already ships two quality tiers (`cfg!(wasm32)` defaults) but exposes them only at startup; the user cannot switch between "cinematic" and "performance" intents without nudging 6 sliders.

## Non-goals

- **No `BlackHoleParams` structural changes.** No new fields. Preset is a UI-layer concept, not stored in the params resource.
- **No shader / physics-mirror changes.** `physics.rs`, `black_hole.wgsl`, `material.rs` are untouched. The CPU↔shader mirror invariant is not at risk.
- **No camera-control logic changes.** `WantsPointer` and the `wants.0 = ctx.egui_wants_pointer_input()` assignment are retained verbatim — this is load-bearing (lets the orbit camera ignore drags over the panel).
- **No custom font binary.** Use egui's default `Proportional` family; only adjust `text_styles` sizes. Embedding Inter would add an asset and a `Trunk.toml`/`include_bytes!` step not justified by the visual gain.
- **No custom-painted toggle switch.** egui has no native iOS-style toggle, but painting one (~30 lines of `Painter`) is maintenance cost the project doesn't need. Use `ui::checkbox` with accent coloring instead.
- **No new dependencies.** `egui_colors` / `egui_widget_ext` / `egui-thematic` are not added. The accent palette is hand-applied via `Visuals` field overrides (§1).

## Approach A — layered grouping (chosen)

Among three approaches considered:

- **A. Layered grouping** (chosen): `SidePanel` + top preset bar + 4 always-open framed cards + 6 collapsing sections, `Grid` two-column rows, hand-tuned sci-fi theme.
- B. Pure cosmetic refresh: keep the 11-`CollapsingHeader` structure, only restyle. Rejected — the structural problem (flat long scroll, no priority) survives.
- C. Tabbed (`TopBottomPanel` of tabs): zero scroll but cross-tab tuning is hostile to explorable debugging.

A balances noise reduction against parameter access. Its preset entry (§3) directly maps onto the existing `cfg!(wasm32)` dual defaults — those two default bundles are essentially two presets already.

## §1 — Chassis and startup styling

### Chassis

`egui::SidePanel::right("controls")`, `default_width(300.0)`, `width_range(260.0..=400.0)`, `resizable(true)`. Inside: a `TopBottomPanel::top("preset_bar")` (fixed) followed by a `ScrollArea::vertical` (the section stack).

### One-shot styling

A new `setup_egui_style` system runs **once** (not per-frame — per-frame `set_style`/`set_visuals` dirties font/layout caches every frame). It is registered in `EguiPrimaryContextPass` (not `Startup` — see gotcha below), guarded by a `Local<bool>` flag so it runs exactly once on the first tick where `ctx_mut()` succeeds. It applies three things to the `egui::Context`:

**Fonts** — keep default `Proportional` family; override `text_styles` sizes:
- `TextStyle::Body` → 14
- `TextStyle::Monospace` → 12
- `TextStyle::Heading` → 16
- `TextStyle::Small` → 11

**Spacing** (`Style::spacing`):
- `item_spacing = (8.0, 6.0)` — tighter gutters than default
- `slider_width = 150.0` — uniform rail length (the single biggest alignment fix)
- `indent = 18.0`
- `button_padding = (10.0, 4.0)`
- `indent_ends_with_horizontal_line = false` — removes the collapsing-header divider noise

**Visuals** (deep-space cyan/orange, hand-applied over `Visuals::dark()`):
- `panel_fill = #0E1014` (blue-black, not pure black)
- `extreme_bg_color = #08090C`
- `hyperlink_color = #5AC8FF` (cyan; by convention also used for section headings)
- `selection.bg_fill = #5AC8FF`
- `selection.stroke = Stroke::new(1.0, #FF8C42)` (orange selection outline)
- `widgets.inactive.weak_bg_fill = #1E2430`, `bg_stroke = Stroke::new(0.5, #3C4646)`
- `widgets.hovered.weak_bg_fill = #283246`, `fg_stroke = #C8DCFF`
- `widgets.active.fg_stroke = #5AC8FF`, `weak_bg_fill = #324664`
- `widgets.noninteractive.bg_stroke = Stroke::new(0.5, #282E3C)`
- `animation_time = 0.12` (snappier than default 0.25)

Color constants are defined once in `src/ui/style.rs` (`ACCENT_CYAN`, `ACCENT_ORANGE`, `PANEL_FILL`, `MUTED_TEXT`, …) and reused by every section helper.

### bevy_egui 0.41 startup gotcha

The egui context is exposed via `EguiContexts`, which is reliable in `EguiPrimaryContextPass` (where `ui_system` already runs) but not guaranteed available in `Startup`. So `setup_egui_style` is registered in `EguiPrimaryContextPass`, guarded by a `Local<bool>`: it retries each tick until `ctx_mut()` first succeeds, applies the style once, sets the flag, and never runs again.

## §2 — Section skeleton and `section()` abstraction

Two helper functions in `src/ui/mod.rs` eliminate the current 11× `CollapsingHeader::new(...).default_open(...).show(...)` boilerplate:

```rust
/// Always-open framed card. Title in cyan RichText::strong().small().
fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui));

/// Collapsible section. Title row can host an enable toggle (§4).
fn collapsing(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
);
```

### Section roster (11 → 10 groups)

The current Renderer section is removed; its contents merge into Quality (see bug fix below).

| Group | Form | Contents |
|---|---|---|
| Camera | always-open card | distance / yaw / pitch / fov |
| Black Hole | always-open card | spin slider + ISCO / horizon r+ read-only |
| Accretion Disk | always-open card | outer / tilt / brightness / rotation / color model / temp |
| Quality | always-open card | bloom + exposure + **steps** + **render_scale** + AA + star_aa |
| Disk Turbulence | collapsing (closed) | disk_quality + 7 turbulence params (longest section) |
| Doppler | collapsing | enabled + strength |
| Jets | collapsing | enabled + strength (spin<0.05 warning retained) |
| Planets | collapsing | 6 params (dirty-detection retained) |
| Background | collapsing | star_intensity / skybox_intensity |
| Grid | collapsing | enabled + density |

Always-open: the 4 most-frequently-tuned. Collapsing: 6 secondary sections, most defaulting closed. This halves the visible scroll length and drops collapse-arrow count from 11 to 6.

### Bug fix: duplicate `render_scale` slider

Current `ui.rs` binds `params.render_scale` twice — once in Renderer (`ui.rs:131`, "Render scale") and once in Quality (`ui.rs:158`, "Resolution scale"). Both write the same field; whichever runs second wins the frame, and the two labels disagree. The redesign consolidates this into a single Quality-row entry labeled "Resolution scale" with `.logarithmic(true)`. This is a latent bug fix included opportunistically in the refactor (the AGENTS.md guidance to "improve code you're working in").

## §3 — Preset bar and Grid two-column rows

### Preset bar

`TopBottomPanel::top("preset_bar")` fixed above the `ScrollArea`:

```
┌─────────────────────────────┐
│ Preset: [Cinematic ▾]  [↺] │
├─────────────────────────────┤
│ (ScrollArea sections…)      │
```

**Four presets**, each writes a bundle of params:

| Preset | Intent | Key overrides |
|---|---|---|
| Cinematic | desktop showpiece | steps=300, render_scale=0.75, bloom=High, disk=High, aa=High |
| Performance | high fps | steps=150, render_scale=0.5, bloom=Low, disk=Low, aa=Off |
| Web | wasm default bundle | the existing `cfg!(wasm32)` default set |
| Custom | read-only marker | written when any slider is hand-edited; writes nothing |

**Custom detection.** egui's `Slider` has no `on_change` callback. The reliable cross-widget "user modified something" signal is a per-frame hash: at the top of `ui_system`, compute a hash of the params fields that presets touch; if it differs from last frame's hash and does not equal the current preset's canonical hash, set `current_preset = Custom`. Applying a preset sets a `just_applied_preset` flag for one frame to skip the hash comparison (the apply itself changes params; that change must not re-trigger Custom).

**[↺] reset button** resets only the **currently displayed section** to its `Default::default()` values, not all params. Resetting the whole panel is destructive (erases hand-tuned values across sections); per-section reset is the safer granularity and matches the section's visual scope.

**Preset state location.** `current_preset: Local<Preset>` and `just_applied_preset: Local<bool>` live as `Local` system params in `ui_system`. They are UI-layer state, not added to `BlackHoleParams` (non-goal).

### Grid two-column rows

Every numeric row converts from:

```rust
ui.add(egui::Slider::new(&mut cam.distance, 3.0..=200.0).text("Distance"));
```

to a two-column `Grid` with label + sized slider:

```rust
egui::Grid::new("camera").num_columns(2).spacing([8.0, 4.0])
    .show(ui, |ui| {
        ui.label("Distance");
        ui.add_sized([140.0, 16.0],
            egui::Slider::new(&mut cam.distance, 3.0..=200.0)
                .suffix(" r_g").fixed_decimals(1));
        ui.end_row();
        // …
    });
```

This produces aligned label/value gutters and uniform slider rails — the biggest single visual upgrade.

### Units and scales

Every slider gains a unit suffix to disambiguate readouts:

- Angles (yaw / pitch / tilt) → `.suffix(" rad")` + `.fixed_decimals(2)`
- `disk_temp` → `.suffix(" K")` + `.logarithmic(true)` (range 1000–50000; linear wastes the low end)
- `steps` (50–600) and `render_scale` (0.25–1.0) → `.logarithmic(true)`
- `planet_radius_factor` → `.prefix("× ")`

Read-only derived values (ISCO / horizon r+ / planet orbit radius) stay as `ui.label(format!(...))` but are placed in the Grid's right column for alignment, colored `MUTED_TEXT` (`Color32::from_gray(140)`) to signal "not editable."

## §4 — Toggles, disabled state, and warning text

The current panel has three inconsistent enable/disable patterns:
- `ui.checkbox(&mut x, "Enable")` on its own row (Doppler/Jets/Grid/Planets)
- `ui.add_enabled(cond, slider)` scattered (turbulence / bloom / temp / jets strength)
- Inline warning text (Jets only)

Unified into one rule set:

### 1. Group-enable toggle in the header row

Doppler / Jets / Grid / Planets — the four "whole section disableable" groups — get their enable checkbox moved into the collapsing header row via `CollapsingState::show_header`, not as a separate row inside the body:

```
▼ Doppler                          [✓]
   └ Strength
▼ Jets                             [✓]   (spin<0.05 → checkbox disabled, title dimmed)
   └ Strength
```

The body's widgets are wrapped in `ui.add_enabled(group_enabled && extra_cond, ...)`.

### 2. Disabled-state visualization

Currently disabled rows are merely non-interactive — indistinguishable from enabled. Two changes:
- Disabled widgets pick up `widgets.noninteractive.weak_bg_fill` (darker gray) automatically via egui's `add_enabled`.
- Section title and row labels drop to `Color32::from_gray(110)` when their section is disabled.

### 3. Inline warning text, templated

The existing Jets warning ("Spin too low — jets need χ ≥ 0.05") is the template. Standardized to orange `RichText::small()` at the top of any section whose controls are gated:

- Jets + spin<0.05 → "Jets need χ ≥ 0.05 (Blandford-Znajek is spin-powered)"
- Disk quality=Off → "Disk quality Off → flat zero-thickness disk rendered" (turbulence sliders below stay visible-but-disabled)
- Bloom=Off → threshold/strength rows are **hidden** (not disabled). Rationale: those two params are meaningless when bloom is off; hiding is cleaner than greying. This is an intentional asymmetry vs. the turbulence case (§4 note below).

**Asymmetry rationale.** Bloom's child params (threshold/strength) have no interpretation when bloom is off → hide. Turbulence's child params (the 7 noise sliders) retain their values as user-tuned state worth seeing while quality=Off briefly → disable-but-show. The distinction is "param meaningless" vs. "param currently not applied but still yours."

### 4. Toggle visual

No custom-painted iOS toggle (non-goal). `ui.checkbox` in the header row, with accent coloring: cyan ✓ when on, gray when off. This reads as a switch in context without the maintenance cost of a `Painter` widget.

## §5 — File structure, migration, and acceptance

### File changes

**New** `src/ui/style.rs` — all egui styling:
- `pub fn setup(ctx: &egui::Context)` — fonts + spacing + visuals; called once from the startup system
- `pub const ACCENT_CYAN / ACCENT_ORANGE / PANEL_FILL / MUTED_TEXT / …` — palette constants
- `pub fn sci_fi_visuals() -> egui::Visuals` — returns the themed Visuals (unit-testable in isolation)
- `pub fn text_styles() -> Vec<(egui::TextStyle, f32)>` — the size table

**New** `src/ui/preset.rs` — preset logic:
- `pub enum Preset { Cinematic, Performance, Web, Custom }`
- `pub fn apply(p: Preset, params: &mut BlackHoleParams)`
- `pub fn canonical_hash(p: Preset) -> u64` — the hash each preset's param bundle produces
- `pub fn params_hash(params: &BlackHoleParams) -> u64` — current-state hash for Custom detection

**New** `src/ui/sections.rs` — the 10 `section_*` render functions, one per group. Each takes `(ui, params, …)` and mutates params in place. Keeps `mod.rs` a thin orchestrator.

**Refactor** `src/ui.rs` → `src/ui/mod.rs`:
- `mod style; mod preset; mod sections;`
- `pub fn ui_system(...)` — thinned to the chassis: `SidePanel::right` → `TopBottomPanel`(preset bar) + `ScrollArea`(section calls)
- `fn group(...)` / `fn collapsing(...)` helpers (§2)
- `fn preset_bar(...)` — top bar UI
- `Local<Preset>` / `Local<bool>` (current_preset / just_applied_preset) state

**Edit** `src/render/plugin.rs`:
- Register `setup_egui_style` as a one-shot system in `EguiPrimaryContextPass` (with `Local<bool>` guard, §1). Must be `EguiPrimaryContextPass`, not `Startup` — the egui context is reliable there but not guaranteed in `Startup`.

**Unchanged**: `params.rs`, `lib.rs`, all shaders, `physics.rs`, `camera.rs`, `material.rs`, `scene/planets.rs`, `web.rs`, `main.rs`.

### Out of scope (explicit)

- No `BlackHoleParams` field changes or default value changes
- No shader / physics mirror edits
- No camera control / `WantsPointer` changes
- No embedded font binary
- No custom-painted toggle
- No new crate dependencies

### Acceptance / regression checklist

Run after implementation:

1. `cargo build --release` compiles
2. `cargo test` passes (`physics.rs` mirror unaffected; confirms `lib.rs` export intact)
3. `cargo run --release` visual checks:
   - SidePanel docked right, draggable width 260–400
   - 4 always-open cards + 6 collapsing headers visible
   - Preset bar at top: select Cinematic → sliders jump to its bundle, bar shows "Cinematic"
   - Drag any slider → bar auto-switches to "Custom"
   - [↺] in a section header → that section's params reset to default
   - Jets + spin=0 → header checkbox disabled + orange warning text
   - Bloom=Off → threshold/strength rows disappear
   - Disk quality=Off → turbulence sliders disabled-but-visible + warning
   - **Regression-critical**: dragging a slider does NOT rotate the orbit camera (`WantsPointer` still correct)
   - **Regression-critical**: no "grey screen" / frozen-frame — the `nudge_camera` workaround is untouched
4. `trunk serve` web build boots without panic; SidePanel renders; egui 0.34 breaking changes (`Rounding→CornerRadius`, `Frame` stroke-in-padding) surface here if missed

## Open questions for implementation plan

- The hash function for preset detection (`params_hash`) needs to hash only the fields presets touch, not all of `BlackHoleParams` — otherwise non-preset edits (e.g. camera distance) would spuriously flip to Custom. The plan should enumerate the exact hashed field set.
