# Control Panel Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single floating `egui::Window` of 11 `CollapsingHeader` sections with a docked right `SidePanel`, cohesive deep-space cyan/orange theme, layered grouping (4 always-open cards + 6 collapsing sections), a Cinematic/Performance/Web/Custom preset bar, and aligned two-column `Grid` slider rows — without touching physics, shaders, params struct, or camera control.

**Architecture:** Split the current 186-line `src/ui.rs` into a `src/ui/` module directory (`mod.rs` orchestrator + `style.rs` theming + `preset.rs` preset logic + `sections.rs` the 10 per-group render functions). The egui `Context` is themed once at startup via a `Local<bool>`-guarded `setup_egui_style` system in `EguiPrimaryContextPass`. Preset state (`current_preset`, `just_applied_preset`) is `Local` UI-layer state, not added to `BlackHoleParams`.

**Tech Stack:** Bevy 0.19, bevy_egui 0.41 (egui 0.34), Rust edition 2024.

**Spec:** `docs/superpowers/specs/2026-07-17-ui-panel-redesign-design.md`

---

## File Structure

| File | Responsibility |
|---|---|
| `src/ui.rs` → `src/ui/mod.rs` | Module root. `ui_system` orchestrator (chassis: SidePanel + TopBottomPanel + ScrollArea), `group()` / `collapsing()` helpers, `preset_bar()` |
| `src/ui/style.rs` (new) | All egui styling: `setup()`, palette consts, `sci_fi_visuals()`, `text_styles()` |
| `src/ui/preset.rs` (new) | `Preset` enum, `apply()`, `canonical_hash()`, `params_hash()` |
| `src/ui/sections.rs` (new) | 10 `section_*` render functions, one per group |
| `src/main.rs` | `mod ui;` unchanged (module dir works with same decl) |
| `src/render/plugin.rs` | Register `setup_egui_style` in `EguiPrimaryContextPass` |

**Testable surface.** This project's only test harness is `src/physics.rs` + `tests/physics_test.rs` (the CPU↔shader mirror). The UI layer has no existing test infrastructure, and egui widget rendering is not unit-testable in isolation (it needs a live `egui::Context`). The two pure-logic functions that *are* testable — `preset::canonical_hash` / `params_hash` (§3 of the spec, "Custom detection") — get unit tests via a new `tests/preset_test.rs` through the `singularity_rs::ui::preset` lib export. Everything else is verified by the visual/compile acceptance checklist in Task 12. This matches the codebase convention: "The GPU shader is not unit-tested. The whole point of `physics.rs` is to be a CPU mirror that _is_ testable." — the analog here is that `preset.rs`'s hash logic is the testable mirror of the preset-bar UI behavior.

**Lib export requirement.** `src/lib.rs` currently exports only `pub mod physics;`. To make `preset` testable we add `pub mod ui;` and `pub mod params;` to `lib.rs`. (`params.rs` must be exported because `preset::apply` / `params_hash` take `&mut BlackHoleParams` / `&BlackHoleParams`; the test constructs one.)

---

### Task 1: Convert `src/ui.rs` to `src/ui/mod.rs` module directory

This task only moves the file into a directory so subsequent tasks can add sibling modules. No behavior change. The build must still pass at the end.

**Files:**
- Move: `src/ui.rs` → `src/ui/mod.rs`
- No content change yet

- [ ] **Step 1: Move the file into a directory**

```bash
mkdir -p src/ui
git mv src/ui.rs src/ui/mod.rs
```

- [ ] **Step 2: Verify the module still resolves**

`src/main.rs:9` is `mod ui;` — this resolves both `src/ui.rs` and `src/ui/mod.rs`, so no edit to `main.rs` is needed.

Run: `cargo build`
Expected: compiles with no errors (no behavior change).

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor(ui): move ui.rs into ui/ module directory

No content change; sets up for sibling style/preset/sections modules."
```

---

### Task 2: Add `style.rs` — palette constants and `sci_fi_visuals()`

The palette and `Visuals` builder are pure data; landing them first lets later tasks reference the consts by name.

**Files:**
- Create: `src/ui/style.rs`
- Modify: `src/ui/mod.rs` (add `mod style;`)

- [ ] **Step 1: Create `src/ui/style.rs` with palette constants and `sci_fi_visuals()`**

```rust
//! egui styling for the control panel. Deep-space cyan/orange theme,
//! hand-applied over `Visuals::dark()`. See design spec §1.

use egui::{Color32, Stroke, TextStyle, Visuals};

// --- Palette (single source of truth; referenced by every section helper) ---
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(90, 200, 255);   // #5AC8FF
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(255, 140, 66); // #FF8C42
pub const PANEL_FILL: Color32 = Color32::from_rgb(14, 16, 20);      // #0E1014
pub const EXTREME_BG: Color32 = Color32::from_rgb(8, 9, 12);        // #08090C
pub const MUTED_TEXT: Color32 = Color32::from_rgb(140, 140, 140);   // read-only / disabled labels
pub const DIM_TEXT: Color32 = Color32::from_rgb(110, 110, 110);     // section-disabled headers

/// Text sizes applied over egui's default `Proportional` family. No custom
/// font binary is embedded (spec non-goal).
pub fn text_styles() -> Vec<(TextStyle, f32)> {
    vec![
        (TextStyle::Body, 14.0),
        (TextStyle::Monospace, 12.0),
        (TextStyle::Heading, 16.0),
        (TextStyle::Small, 11.0),
    ]
}

/// The themed `Visuals`. Built fresh from `dark()` each call so it is
/// independent of whatever egui's defaults evolve into.
pub fn sci_fi_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL_FILL;
    v.extreme_bg_color = EXTREME_BG;
    v.hyperlink_color = ACCENT_CYAN; // also used as the section-heading color by convention
    v.selection.bg_fill = ACCENT_CYAN;
    v.selection.stroke = Stroke::new(1.0, ACCENT_ORANGE);

    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 36, 48);
    v.widgets.inactive.bg_stroke = Stroke::new(0.5, Color32::from_rgb(60, 70, 90));
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 50, 70);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 220, 255));
    v.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT_CYAN);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(50, 70, 100);
    v.widgets.noninteractive.bg_stroke = Stroke::new(0.5, Color32::from_rgb(40, 46, 60));

    v.window_shadow = epaint::Shadow {
        offset: [0, 4],
        blur: 16.0,
        spread: 0.0,
        color: Color32::from_black_alpha(120),
    };
    v.animation_time = 0.12;
    v
}

/// Apply fonts + spacing + visuals to a context. Called exactly once from
/// `setup_egui_style` (plugin.rs).
pub fn setup(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.slider_width = 150.0;
    style.spacing.indent = 18.0;
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.indent_ends_with_horizontal_line = false;
    for (ts, size) in text_styles() {
        style.text_styles_mut().insert(ts, egui::FontId::proportional(size));
    }
    ctx.set_style(style);
    ctx.set_visuals(sci_fi_visuals());
}
```

- [ ] **Step 2: Register the module in `src/ui/mod.rs`**

Add at the very top of `src/ui/mod.rs` (above any existing `use`):

```rust
mod style;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles. `style` module is declared but not yet called from `ui_system`, so it may emit `dead_code` warnings for unused items — that is expected and acceptable for now (Task 4 wires them up). If the build hard-errors on `epaint::Shadow` field names, check `cargo doc --document-private-items -p epaint` for the current field set; egui 0.34's `Shadow` has `offset: [i32;2]`, `blur`, `spread`, `color`.

- [ ] **Step 4: Commit**

```bash
git add src/ui/style.rs src/ui/mod.rs
git commit -m "feat(ui): add style module — deep-space cyan/orange theme

Palette constants + sci_fi_visuals() + setup(). Hand-applied over
Visuals::dark(); no custom font, no new deps. Not yet wired into
ui_system (Task 4 does that via setup_egui_style)."
```

---

### Task 3: Add `preset.rs` with hash-based Custom detection + its test

The preset logic is the one piece of this UI layer that IS unit-testable (pure functions over `BlackHoleParams`). We build it TDD-style so the hash field set is pinned by a test, not guessed.

**Files:**
- Create: `src/ui/preset.rs`
- Create: `tests/preset_test.rs`
- Modify: `src/lib.rs` (export `ui` + `params` modules for the test)
- Modify: `src/ui/mod.rs` (add `mod preset;`)

- [ ] **Step 1: Export `ui` and `params` from `src/lib.rs`**

Current `src/lib.rs` is a single line `pub mod physics;`. Replace its contents with:

```rust
pub mod params;
pub mod physics;
pub mod ui;
```

- [ ] **Step 2: Write the failing test first**

Create `tests/preset_test.rs`:

```rust
use singularity_rs::params::BlackHoleParams;
use singularity_rs::ui::preset::{Preset, apply, canonical_hash, params_hash};

#[test]
fn canonical_hash_matches_just_applied_params() {
    // After applying a preset, params_hash of the result must equal that
    // preset's canonical_hash — otherwise the Custom-detection logic would
    // immediately flip a freshly-applied preset back to Custom.
    let mut p = BlackHoleParams::default();
    for preset in [Preset::Cinematic, Preset::Performance, Preset::Web] {
        apply(preset, &mut p);
        assert_eq!(
            canonical_hash(preset),
            params_hash(&p),
            "preset {:?}: apply() did not reproduce canonical_hash",
            preset
        );
    }
}

#[test]
fn non_preset_field_change_flips_to_custom() {
    // Editing a field that NO preset touches (camera distance lives on
    // OrbitCamera, not BlackHoleParams — use disk_tilt, also not preset-touched)
    // must NOT change params_hash. This guards the "hash only preset fields"
    // invariant: non-preset edits must not spuriously flip to Custom.
    let mut p = BlackHoleParams::default();
    let h0 = params_hash(&p);
    p.disk_tilt = 1.0; // not in any preset bundle
    assert_eq!(h0, params_hash(&p), "non-preset field leaked into hash");
}

#[test]
fn preset_field_change_differs_from_canonical() {
    // Editing a preset-touched field after applying must change the hash
    // away from the preset's canonical_hash (i.e. flip to Custom).
    let mut p = BlackHoleParams::default();
    apply(Preset::Cinematic, &mut p);
    let h_canonical = canonical_hash(Preset::Cinematic);
    p.steps = 299; // off by one from the Cinematic bundle
    assert_ne!(h_canonical, params_hash(&p));
}
```

- [ ] **Step 3: Run the test to verify it fails (module not found)**

Run: `cargo test --test preset_test`
Expected: FAIL — `unresolved module` / `cannot find` errors for `singularity_rs::ui::preset`.

- [ ] **Step 4: Create `src/ui/preset.rs`**

```rust
//! Preset bundles + Custom-edit detection (design spec §3).
//!
//! `params_hash` hashes ONLY the fields that presets touch — otherwise a
//! non-preset edit (e.g. disk_tilt) would spuriously flip the bar to Custom.
//! The field set below is the single source of truth; if you add a field to
//! a preset bundle, add it to `hashed_fields` too or the test
//! `non_preset_field_change_flips_to_custom` will not catch the leak.

use crate::params::{AaQuality, BlackHoleParams, BloomQuality, DiskQuality};

use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    Cinematic,
    Performance,
    Web,
    /// Read-only marker set when any preset-touched field is hand-edited
    /// away from a preset bundle. `apply(Custom, _)` is a no-op.
    Custom,
}

/// The exact params each preset writes. `Custom` writes nothing.
fn bundle(p: Preset) -> Option<HashedParams> {
    // Defaults chosen to mirror cfg!(wasm32) dual-defaults already in the codebase:
    // Cinematic = desktop default, Web = wasm default, Performance = a low tier.
    Some(match p {
        Preset::Cinematic => HashedParams {
            steps: 300, render_scale: 0.75,
            bloom_quality: BloomQuality::High, disk_quality: DiskQuality::High, aa_quality: AaQuality::High,
        },
        Preset::Performance => HashedParams {
            steps: 150, render_scale: 0.5,
            bloom_quality: BloomQuality::Low, disk_quality: DiskQuality::Low, aa_quality: AaQuality::Off,
        },
        Preset::Web => HashedParams {
            steps: 200, render_scale: 0.5,
            bloom_quality: BloomQuality::Low, disk_quality: DiskQuality::Low, aa_quality: AaQuality::Off,
        },
        Preset::Custom => return None,
    })
}

/// Apply a preset's bundle to params. `Custom` is a no-op.
pub fn apply(p: Preset, params: &mut BlackHoleParams) {
    if let Some(b) = bundle(p) {
        params.steps = b.steps;
        params.render_scale = b.render_scale;
        params.bloom_quality = b.bloom_quality;
        params.disk_quality = b.disk_quality;
        params.aa_quality = b.aa_quality;
    }
}

/// Stable hash of a preset's canonical bundle. `Custom` returns 0 (it never
/// matches any real params state, by construction — see `hashed_fields`).
pub fn canonical_hash(p: Preset) -> u64 {
    match bundle(p) {
        Some(b) => b.hash(),
        None => 0,
    }
}

/// Hash of the preset-touched fields of a live params. Used by `ui_system`
/// to detect hand-edits and flip the bar to Custom.
pub fn params_hash(params: &BlackHoleParams) -> u64 {
    let h = HashedParams {
        steps: params.steps,
        render_scale: params.render_scale,
        bloom_quality: params.bloom_quality,
        disk_quality: params.disk_quality,
        aa_quality: params.aa_quality,
    };
    h.hash()
}

// --- internals ---

#[derive(Clone, Copy)]
struct HashedParams {
    steps: u32,
    render_scale: f32,
    bloom_quality: BloomQuality,
    disk_quality: DiskQuality,
    aa_quality: AaQuality,
}

impl HashedParams {
    fn hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.steps.hash(&mut hasher);
        self.render_scale.to_bits().hash(&mut hasher); // f32: hash bit pattern, not value
        self.bloom_quality.as_u32_().hash(&mut hasher);
        self.disk_quality.as_u32().hash(&mut hasher);
        self.aa_quality.samples().hash(&mut hasher);
        hasher.finish()
    }
}

// Local trait adapters: BloomQuality has levels(), DiskQuality has as_u32(),
// AaQuality has samples(). Unify under one name for hashing.
trait AsU32ForHash { fn as_u32_(self) -> u32; }
impl AsU32ForHash for BloomQuality { fn as_u32_(self) -> u32 { self.levels() } }
```

Note on the `AsU32ForHash` trait: it exists only because `BloomQuality` exposes `levels()` while `DiskQuality` exposes `as_u32()`. Rather than touch `params.rs` (a non-goal), we adapt locally. `AaQuality` uses its existing `samples()` directly.

- [ ] **Step 5: Register the module in `src/ui/mod.rs`**

Add below `mod style;`:

```rust
mod preset;
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test --test preset_test`
Expected: PASS — all three tests green.

If it fails on `canonical_hash_matches_just_applied_params`, the `apply()` field assignments don't line up with `HashedParams` — re-check that both reference the same five fields.

- [ ] **Step 7: Run the full test suite to confirm no regressions**

Run: `cargo test`
Expected: all tests pass (physics mirror + new preset tests).

- [ ] **Step 8: Commit**

```bash
git add src/ui/preset.rs src/ui/mod.rs src/lib.rs tests/preset_test.rs
git commit -m "feat(ui): add preset module with hash-based Custom detection

Preset::Cinematic/Performance/Web write 5 params (steps, render_scale,
bloom/disk/aa quality); Custom is a no-op marker. params_hash hashes
ONLY preset-touched fields so non-preset edits (disk_tilt, etc.) don't
spuriously flip the bar to Custom. Test-pinned via tests/preset_test.rs.

Exports ui + params from lib.rs so the test can construct params."
```

---

### Task 4: Wire `setup_egui_style` into the plugin

Register the one-shot styling system in `EguiPrimaryContextPass`. `Local<bool>` guard makes it retry until the egui context exists, then run once.

**Files:**
- Modify: `src/render/plugin.rs` (add system + registration)
- Modify: `src/ui/mod.rs` (add `pub fn setup_egui_style`)

- [ ] **Step 1: Add `setup_egui_style` to `src/ui/mod.rs`**

Add this function to `src/ui/mod.rs` (anywhere at module scope, after the `mod` declarations):

```rust
/// One-shot egui styling. Registered in `EguiPrimaryContextPass` with a
/// `Local<bool>` guard so it retries until `ctx_mut()` first succeeds, then
/// runs exactly once. Per-frame set_style/set_visuals would dirty layout
/// caches every frame; this avoids that.
pub fn setup_egui_style(
    mut contexts: bevy_egui::EguiContexts,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    if let Ok(ctx) = contexts.ctx_mut() {
        crate::ui::style::setup(&ctx);
        *done = true;
    }
}
```

- [ ] **Step 2: Register it in `src/render/plugin.rs`**

Find line 134 (current):
```rust
            .add_systems(bevy_egui::EguiPrimaryContextPass, crate::ui::ui_system);
```

Replace with:
```rust
            .add_systems(
                bevy_egui::EguiPrimaryContextPass,
                (crate::ui::setup_egui_style, crate::ui::ui_system).chain(),
            );
```

`.chain()` ensures the style is applied before the UI renders on the very first successful frame. The comment on lines 132-133 ("bevy_egui 0.41 requires UI systems to run inside the egui context pass") already documents why these live here — add a sibling note for the new system.

Update the comment block above to read:
```rust
            // bevy_egui 0.41 requires UI systems to run inside the egui context
            // pass (fonts/ctx are initialized there); placing them in Update panics.
            // setup_egui_style is a one-shot (Local<bool> guard): retries until the
            // context exists, applies the theme once, never runs again.
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 4: Verify visually that styling now applies**

Run: `cargo run --release`
Expected: the Controls window is now dark blue-black with cyan accents (panel bg `#0E1014`, sliders cyan-on-hover). The layout is still the old 11-CollapsingHeader structure — that's fine; this task only verifies the theme is wired. Quit after confirming the colors changed.

If the window is still default-styled, `setup_egui_style` isn't running — check that `*done` starts false and `ctx_mut()` returns `Ok`.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs src/render/plugin.rs
git commit -m "feat(ui): wire setup_egui_style one-shot into EguiPrimaryContextPass

Local<bool> guard retries until ctx_mut() first succeeds, applies the
sci-fi theme once, then never runs again. .chain()'d before ui_system so
the very first successful frame is already themed. Per-frame set_style
would dirty layout caches; this avoids that."
```

---

### Task 5: Add `group()` and `collapsing()` section helpers

The two chassis helpers that eliminate the 11× `CollapsingHeader` boilerplate. Once these exist, Task 8's section rewrite uses them.

**Files:**
- Modify: `src/ui/mod.rs` (add the two helpers)

- [ ] **Step 1: Add the helpers to `src/ui/mod.rs`**

Add at module scope (after `setup_egui_style`):

```rust
use crate::ui::style::ACCENT_CYAN;

/// An always-open framed card. Title in cyan RichText::strong().small().
/// Used for the 4 most-tuned groups (Camera / Black Hole / Disk / Quality).
fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .corner_radius(6.0)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().window_stroke.color,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .strong()
                    .small()
                    .color(ACCENT_CYAN),
            );
            ui.add_space(2.0);
            body(ui);
        });
}

/// A collapsible section (default_open controls initial state).
/// Used for the 6 secondary groups. The body closure is only invoked when open.
fn collapsing(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::CollapsingHeader::new(title)
        .default_open(default_open)
        .id_salt(id)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}
```

Notes:
- `.corner_radius(6.0)` — egui 0.34 renamed `Rounding` to `CornerRadius`; `Frame::corner_radius(f32)` is the current builder method.
- `.id_salt(id)` makes the open/closed state stable across frames independent of header text.
- `collapsing` here is the simple form (no header toggle). Task 9 adds a second variant `collapsing_with_toggle` for the four groups that have an enable checkbox in the header.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles. Helpers are unused so far → `dead_code` warnings are fine (cleared in Task 8).

- [ ] **Step 3: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): add group()/collapsing() section helpers

group() = always-open framed card with cyan heading.
collapsing() = CollapsingHeader wrapper with stable id_salt.
These eliminate the 11x boilerplate in the upcoming section rewrite."
```

---

### Task 6: Add `collapsing_with_toggle()` for header-row enable checkboxes

Doppler / Jets / Grid / Planets put their enable checkbox in the collapsing header, not as a body row. This needs `CollapsingState::show_header`, which the plain `collapsing()` from Task 5 doesn't expose.

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Add the variant helper**

Add below `collapsing()`:

```rust
/// Collapsible section whose header hosts an enable toggle on the right.
/// Used by Doppler / Jets / Grid / Planets (design §4.1). The body closure
/// receives the current `enabled` state so it can `add_enabled` its rows.
fn collapsing_with_toggle(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    default_open: bool,
    enabled: &mut bool,
    body: impl FnOnce(&mut egui::Ui, bool),
) {
    let id = ui.make_persistent_id(id);
    let mut state =
        egui::CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    state.show_header(ui, |ui| {
        ui.checkbox(enabled, title);
    })
    .body(|ui| {
        ui.set_width(ui.available_width());
        body(ui, *enabled);
    });
    state.store(ui.ctx());
}
```

Design note: the checkbox *is* the header label (no separate title text + checkbox). `ui.checkbox(&mut enabled, title)` reads as "Doppler ☑" in one row — cleaner than title+separate-checkbox. `toggle_value` would render as a button; `checkbox` renders as the conventional box-with-check, which the spec §4.4 chose.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): add collapsing_with_toggle() for header-row enables

Doppler/Jets/Grid/Planets get their enable checkbox in the collapsing
header (not a body row). Uses CollapsingState::show_header. Body closure
receives the enabled flag so it can add_enabled() its rows."
```

---

### Task 7: Create `sections.rs` skeleton with the 4 always-open card functions

Start the section module with the always-open cards (Camera / Black Hole / Disk / Quality). Each is a `pub fn section_*` that takes `&mut egui::Ui` and the params/camera refs it needs. Land Camera + Black Hole + Quality here; Disk follows in Task 8 because it has the color-mode/temp interaction.

**Files:**
- Create: `src/ui/sections.rs`
- Modify: `src/ui/mod.rs` (add `mod sections;`)

- [ ] **Step 1: Create `src/ui/sections.rs` with Camera, Black Hole, Quality**

```rust
//! Per-group section render functions. Each takes the `&mut egui::Ui` it
//! draws into plus the params/camera refs it needs. The `ui_system`
//! orchestrator (mod.rs) calls them inside `group()` / `collapsing()`.

use crate::camera::OrbitCamera;
use crate::params::{
    AaQuality, BlackHoleParams, BloomQuality, DiskColorMode,
};
use crate::ui::style::{ACCENT_ORANGE, MUTED_TEXT};

use std::f32::consts::PI;

/// Shared two-column row helper: label + sized slider.
fn row(
    ui: &mut egui::Ui,
    label: &str,
    add_widget: impl FnOnce(&mut egui::Ui) -> egui::Response,
) {
    ui.label(label);
    add_widget(ui);
    ui.end_row();
}

// ============================ Always-open cards ============================

pub fn section_camera(ui: &mut egui::Ui, cam: &mut OrbitCamera) {
    egui::Grid::new("camera_grid").num_columns(2).spacing([8.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Distance", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut cam.distance, 3.0..=200.0)
                    .suffix(" r_g").fixed_decimals(1)));
            row(ui, "Yaw", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut cam.yaw, -PI..=PI)
                    .suffix(" rad").fixed_decimals(2)));
            row(ui, "Pitch", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut cam.pitch, (-PI + 0.05)..=(PI - 0.05))
                    .suffix(" rad").fixed_decimals(2)));
            row(ui, "FOV", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut cam.fov, 0.3..=2.0).fixed_decimals(2)));
        });
}

pub fn section_black_hole(ui: &mut egui::Ui, params: &mut BlackHoleParams) {
    egui::Grid::new("bh_grid").num_columns(2).spacing([8.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Spin (χ)", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.spin, 0.0..=1.0).fixed_decimals(2)));
            // Read-only derived values: right column, muted color.
            ui.label(egui::RichText::new("ISCO (disk inner)").color(MUTED_TEXT));
            ui.label(egui::RichText::new(format!("{:.3}", crate::physics::kerr_isco(params.spin)))
                .color(MUTED_TEXT));
            ui.end_row();
            ui.label(egui::RichText::new("Horizon r+").color(MUTED_TEXT));
            ui.label(egui::RichText::new(format!("{:.3}", crate::physics::kerr_horizon(params.spin)))
                .color(MUTED_TEXT));
            ui.end_row();
        });
}

pub fn section_quality(ui: &mut egui::Ui, params: &mut BlackHoleParams) {
    // Bloom quality combobox + threshold/strength (hidden when Off — §4.3).
    {
        let mut q = params.bloom_quality;
        egui::Grid::new("bloom_grid").num_columns(2).spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Bloom");
                egui::ComboBox::from_id_salt("bloom_combo")
                    .selected_text(format!("{:?}", q))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut q, BloomQuality::Off, "Off");
                        ui.selectable_value(&mut q, BloomQuality::Low, "Low");
                        ui.selectable_value(&mut q, BloomQuality::Medium, "Medium");
                        ui.selectable_value(&mut q, BloomQuality::High, "High");
                    });
                ui.end_row();
            });
        params.bloom_quality = q;
    }
    if params.bloom_quality != BloomQuality::Off {
        egui::Grid::new("bloom_detail_grid").num_columns(2).spacing([8.0, 4.0])
            .show(ui, |ui| {
                row(ui, "Threshold", |ui| ui.add_sized([140.0, 16.0],
                    egui::Slider::new(&mut params.bloom_threshold, 0.0..=3.0).fixed_decimals(2)));
                row(ui, "Strength", |ui| ui.add_sized([140.0, 16.0],
                    egui::Slider::new(&mut params.bloom_strength, 0.0..=2.0).fixed_decimals(2)));
            });
    }
    egui::Grid::new("renderer_grid").num_columns(2).spacing([8.0, 4.0])
        .show(ui, |ui| {
            // steps + render_scale migrated here from the deleted Renderer section.
            row(ui, "Steps", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.steps, 50..=600)
                    .logarithmic(true).fixed_decimals(0)));
            row(ui, "Resolution", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.render_scale, 0.25..=1.0)
                    .logarithmic(true).fixed_decimals(2)));
            row(ui, "Exposure", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.exposure, 0.5..=3.0).fixed_decimals(2)));
            row(ui, "Star AA", |ui| ui.checkbox(&mut params.star_aa, ""));
            // Ring anti-alias (supersampling for lensed-image rings).
            let mut a = params.aa_quality;
            ui.label("Ring AA");
            egui::ComboBox::from_id_salt("aa_combo")
                .selected_text(format!("{:?} ({}×)", a, a.samples()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut a, AaQuality::Off, "Off (1×)");
                    ui.selectable_value(&mut a, AaQuality::Low, "Low (2×)");
                    ui.selectable_value(&mut a, AaQuality::High, "High (4×)");
                });
            ui.end_row();
            params.aa_quality = a;
        });
    ui.label(
        egui::RichText::new("MSAA is decorative on a fullscreen shader (no geometry edges).")
            .small().color(MUTED_TEXT),
    );
}
```

Note: `_disk_color_mode` import will be used in Task 8; mark `#[allow(unused_imports)]` on the `use crate::params::{...}` line if the compiler warns in this task (it won't — `DiskColorMode` is only imported in Task 8's file edit). To keep the import list stable across tasks, this task imports only what it uses; Task 8 adds `DiskColorMode` to the same `use` line.

- [ ] **Step 2: Register the module in `src/ui/mod.rs`**

Add below `mod preset;`:

```rust
mod sections;
```

- [ ] **Step 3: Trim the unused import in `sections.rs` for this task**

Since Task 7 doesn't yet reference `DiskColorMode` or `ACCENT_ORANGE`, change the import line in `sections.rs` to only what this task uses, to avoid warnings:

```rust
use crate::params::{
    AaQuality, BlackHoleParams, BloomQuality,
};
use crate::ui::style::MUTED_TEXT;
```

(Task 8 will re-add `DiskColorMode` and `ACCENT_ORANGE` to these lines.)

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: compiles. `dead_code` warnings expected (sections not called yet).

- [ ] **Step 5: Commit**

```bash
git add src/ui/sections.rs src/ui/mod.rs
git commit -m "feat(ui): add sections.rs — Camera/BlackHole/Quality cards

Two-column Grid rows with unit suffixes + log scales on wide ranges.
Bloom threshold/strength hidden when Bloom=Off (spec §4.3). steps and
render_scale migrated here from the deleted Renderer section, fixing
the duplicate-render_scale bug."
```

---

### Task 8: Add the Accretion Disk card (color mode + temp interaction)

The Disk section has the `DiskColorMode` ↔ `disk_temp` enable interaction; landing it separately from Task 7 keeps the diffs reviewable.

**Files:**
- Modify: `src/ui/sections.rs`

- [ ] **Step 1: Add `DiskColorMode` + `ACCENT_ORANGE` to the section imports**

Change the import lines at the top of `src/ui/sections.rs` from:

```rust
use crate::params::{
    AaQuality, BlackHoleParams, BloomQuality,
};
use crate::ui::style::MUTED_TEXT;
```

to:

```rust
use crate::params::{
    AaQuality, BlackHoleParams, BloomQuality, DiskColorMode,
};
use crate::ui::style::{ACCENT_ORANGE, MUTED_TEXT};
```

- [ ] **Step 2: Add `section_disk` at the end of `src/ui/sections.rs`**

```rust
pub fn section_disk(ui: &mut egui::Ui, params: &mut BlackHoleParams) {
    egui::Grid::new("disk_grid").num_columns(2).spacing([8.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Outer radius", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.disk_outer, 6.0..=50.0)
                    .suffix(" r_g").fixed_decimals(1)));
            row(ui, "Tilt", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.disk_tilt, 0.0..=PI)
                    .suffix(" rad").fixed_decimals(2)));
            row(ui, "Brightness", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.disk_brightness, 0.0..=3.0).fixed_decimals(2)));
            row(ui, "Rotation", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.disk_rotation_speed, 0.0..=3.0).fixed_decimals(2)));
            // Color model combobox.
            let mut cm = params.disk_color_mode;
            ui.label("Color model");
            egui::ComboBox::from_id_salt("disk_color_combo")
                .selected_text(format!("{:?}", cm))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut cm, DiskColorMode::Gradient, "Gradient");
                    ui.selectable_value(&mut cm, DiskColorMode::Blackbody, "Blackbody");
                });
            ui.end_row();
            params.disk_color_mode = cm;
        });
    // Temperature only meaningful in Blackbody mode — disable (not hide) so
    // the user sees their value while briefly in Gradient.
    ui.add_enabled(
        cm_blackbody(params),
        egui::Slider::new(&mut params.disk_temp, 1000.0..=50000.0)
            .suffix(" K")
            .logarithmic(true)
            .fixed_decimals(0)
            .text("Temperature"),
    );
}

fn cm_blackbody(params: &BlackHoleParams) -> bool {
    params.disk_color_mode == DiskColorMode::Blackbody
}
```

Note: `disk_temp` slider uses `.text("Temperature")` (label inside the slider widget) here rather than a Grid row, because the log-scale slider reads better with its label inline. This is an intentional local deviation from the pure two-column pattern; the visual still aligns because the slider width is the same 150px.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles. (`ACCENT_ORANGE` is now imported but not yet used until Task 9 adds warning text — `#[allow(unused_imports)]` is NOT added; Task 9 clears it within the same module. If the warning bothers you, you may land Task 9's first warning-text usage first. Prefer: leave the warning for one task.)

- [ ] **Step 4: Commit**

```bash
git add src/ui/sections.rs
git commit -m "feat(ui): add Accretion Disk card with color-mode/temp interaction

Temperature disabled (not hidden) when color mode = Gradient, so the
user sees their value. log-scale slider for the 1000-50000K range.
"
```

---

### Task 9: Add the 6 collapsing sections (Turbulence, Doppler, Jets, Planets, Background, Grid)

All six secondary sections. The four with header toggles use `collapsing_with_toggle`; Turbulence and Background use plain `collapsing`. Warning text uses `ACCENT_ORANGE` (clears the unused-import from Task 8).

**Files:**
- Modify: `src/ui/sections.rs`

- [ ] **Step 1: Add the six sections at the end of `src/ui/sections.rs`**

```rust
// ============================ Collapsing sections ==========================

pub fn section_turbulence(ui: &mut egui::Ui, params: &mut BlackHoleParams) {
    use crate::params::DiskQuality;
    let mut q = params.disk_quality;
    egui::Grid::new("diskq_grid").num_columns(2).spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Disk quality");
            egui::ComboBox::from_id_salt("diskq_combo")
                .selected_text(format!("{:?}", q))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut q, DiskQuality::Off, "Off");
                    ui.selectable_value(&mut q, DiskQuality::Low, "Low");
                    ui.selectable_value(&mut q, DiskQuality::Medium, "Medium");
                    ui.selectable_value(&mut q, DiskQuality::High, "High");
                });
            ui.end_row();
        });
    params.disk_quality = q;

    let on = q != DiskQuality::Off;
    if !on {
        ui.label(egui::RichText::new(
            "Disk quality Off → flat zero-thickness disk rendered.",
        ).small().color(ACCENT_ORANGE));
    }
    egui::Grid::new("turb_grid").num_columns(2).spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.add_enabled(on, egui::Slider::new(&mut params.disk_half_thickness, 0.02..=0.3).text("Thickness (H/R)"));
            ui.end_row();
            ui.add_enabled(on, egui::Slider::new(&mut params.filament_freq, 0.2..=4.0).text("Filament frequency"));
            ui.end_row();
            ui.add_enabled(on, egui::Slider::new(&mut params.filament_sharpness, 1.0..=6.0).text("Filament sharpness"));
            ui.end_row();
            ui.add_enabled(on, egui::Slider::new(&mut params.density_freq, 0.2..=3.0).text("Density frequency"));
            ui.end_row();
            ui.add_enabled(on, egui::Slider::new(&mut params.density_strength, 0.0..=2.0).text("Density strength"));
            ui.end_row();
            ui.add_enabled(on, egui::Slider::new(&mut params.arm_count, 0.0..=6.0).text("Arm count"));
            ui.end_row();
            ui.add_enabled(on, egui::Slider::new(&mut params.arm_tightness, 0.0..=6.0).text("Arm tightness"));
            ui.end_row();
            ui.add_enabled(on, egui::Slider::new(&mut params.arm_strength, 0.0..=1.0).text("Arm strength"));
            ui.end_row();
        });
}

pub fn section_doppler(ui: &mut egui::Ui, params: &mut BlackHoleParams, enabled: bool) {
    ui.add_enabled(enabled, egui::Slider::new(&mut params.doppler_strength, 0.0..=3.0).text("Strength"));
}

pub fn section_jets(ui: &mut egui::Ui, params: &mut BlackHoleParams, enabled: bool) {
    // Mirror the shader's spin gate: jets render only for χ ≥ 0.05.
    let jets_renderable = params.spin >= 0.05;
    if enabled && !jets_renderable {
        ui.label(egui::RichText::new(
            "Jets need χ ≥ 0.05 (Blandford-Znajek is spin-powered).",
        ).small().color(ACCENT_ORANGE));
    }
    ui.add_enabled(
        enabled && jets_renderable,
        egui::Slider::new(&mut params.jets_strength, 0.0..=3.0).text("Strength"),
    );
}

pub fn section_planets(
    ui: &mut egui::Ui,
    params: &mut BlackHoleParams,
    enabled: bool,
    planet_dirty: &mut crate::scene::planets::PlanetSystemDirty,
) {
    ui.add_enabled(enabled, egui::Slider::new(&mut params.planet_count_target, 0..=8).text("Count"));
    ui.add_enabled(enabled, egui::Slider::new(&mut params.planet_radius_factor, 1.1..=2.0).prefix("× ").text("Radius (× disk outer)"));
    ui.add_enabled(
        enabled,
        egui::Label::new(
            egui::RichText::new(format!(
                "Orbit r = {:.2} (disk outer: {:.1})",
                params.planet_radius_factor * params.disk_outer,
                params.disk_outer
            )).color(MUTED_TEXT),
        ),
    );
    ui.add_enabled(enabled, egui::Slider::new(&mut params.planet_seed, 0..=1000).text("Seed"));
    ui.add_enabled(enabled, egui::Slider::new(&mut params.planet_time_scale, 1.0..=200.0).text("Time scale"));
}

pub fn section_background(ui: &mut egui::Ui, params: &mut BlackHoleParams) {
    egui::Grid::new("bg_grid").num_columns(2).spacing([8.0, 4.0])
        .show(ui, |ui| {
            row(ui, "Star intensity", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.star_intensity, 0.0..=3.0).fixed_decimals(2)));
            row(ui, "Skybox", |ui| ui.add_sized([140.0, 16.0],
                egui::Slider::new(&mut params.skybox_intensity, 0.0..=3.0).fixed_decimals(2)));
        });
}

pub fn section_grid(ui: &mut egui::Ui, params: &mut BlackHoleParams, enabled: bool) {
    ui.add_enabled(enabled, egui::Slider::new(&mut params.grid_density, 0.1..=4.0).text("Density"));
}
```

Design note on `section_planets`: the dirty-detection (compare prev vs curr of `planets_enabled/count/radius/seed`) moves OUT of the section function into `ui_system` (Task 11), because the comparison needs the values *before* the section draws them. The section function only renders; the orchestrator decides dirty. `planet_time_scale` is excluded from dirty (per the existing comment at `ui.rs:62-63`).

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles. `ACCENT_ORANGE` now used → no warning.

- [ ] **Step 3: Commit**

```bash
git add src/ui/sections.rs
git commit -m "feat(ui): add 6 collapsing sections

Turbulence (default closed, longest; orange warning when disk quality Off),
Doppler/Jets/Grid/Planets (header enable toggle passed in as 'enabled'),
Background. Planets dirty-detection moves to ui_system (Task 11) since it
needs pre-render values. Jets retains its spin<0.05 warning."
```

---

### Task 10: Add `preset_bar()` to `mod.rs`

The top bar: a preset combobox + a global reset. Reads/writes `Local<Preset>` + `Local<bool>` state that `ui_system` owns.

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Add `preset_bar` to `src/ui/mod.rs`**

Add below the helpers:

```rust
use crate::ui::preset::{Preset, apply, canonical_hash, params_hash};

/// The top preset bar. `current` is the UI-layer state (which preset is
/// shown as selected); `just_applied` is set for one frame after a preset
/// is chosen, to skip the Custom-detection hash compare on that frame
/// (applying a preset changes params; that change must not flip to Custom).
fn preset_bar(
    ui: &mut egui::Ui,
    params: &mut crate::params::BlackHoleParams,
    current: &mut Preset,
    just_applied: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Preset:");
        let prev = *current;
        egui::ComboBox::from_id_salt("preset_combo")
            .selected_text(format!("{:?}", prev))
            .show_ui(ui, |ui| {
                ui.selectable_value(current, Preset::Cinematic, "Cinematic");
                ui.selectable_value(current, Preset::Performance, "Performance");
                ui.selectable_value(current, Preset::Web, "Web");
                ui.selectable_value(current, Preset::Custom, "Custom");
            });
        if *current != prev && *current != Preset::Custom {
            // User picked a concrete preset → apply its bundle.
            apply(*current, params);
            *just_applied = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Global reset-to-default for all params. Destructive, so a
            // confirmation button is not added — the panel is exploratory
            // tooling and re-tuning is cheap.
            if ui.button("↺ all").clicked() {
                *params = crate::params::BlackHoleParams::default();
                *current = Preset::Custom;
                *just_applied = true;
            }
        });
    });
    ui.separator();
}
```

**Spec deviation noted.** The spec §3 says [↺] is a per-section reset. On review during plan-writing, per-section reset requires either (a) threading a reset closure into every section function signature (10 signatures change), or (b) storing per-section default snapshots (extra state). Both are heavy for a feature the spec itself flagged as "the safer granularity." For a single-panel tool where re-tuning is cheap, a single global "↺ all" reset is simpler and the destructive cost is low. This deviation is flagged here for the human; if they want per-section reset instead, swap this function's body for a closure-based approach. (Acceptance checklist Task 12 step 3 asks the human to confirm this choice.)

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles. `preset_bar` is unused until Task 11.

- [ ] **Step 3: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): add preset_bar() — combobox + global reset

Selecting a concrete preset applies its bundle and sets just_applied
for one frame to suppress Custom-detection on that frame. Global '↺ all'
reset (deviation from spec's per-section reset, flagged in plan for review)."
```

---

### Task 11: Rewrite `ui_system` as the chassis orchestrator

Replace the 186-line body of `ui_system` with: SidePanel → preset_bar → ScrollArea → section calls. Adds the `Local<Preset>` / `Local<bool>` state and the planets dirty-detection (relocated from the old Planets section).

**Files:**
- Modify: `src/ui/mod.rs` (replace `ui_system` body)

- [ ] **Step 1: Replace `ui_system` in `src/ui/mod.rs`**

Replace the entire current `pub fn ui_system(...)` function with:

```rust
pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
    mut wants: ResMut<crate::camera::WantsPointer>,
    mut planet_dirty: ResMut<crate::scene::planets::PlanetSystemDirty>,
    mut current_preset: Local<Preset>,
    mut just_applied: Local<bool>,
    mut last_hash: Local<u64>,
) {
    // Default the preset state on the first frame. Local<T: Default> would
    // require deriving Default for Preset; we instead initialize manually
    // via a sentinel hash of u64::MAX.
    if *last_hash == u64::MAX {
        *current_preset = Preset::Custom;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        wants.0 = false;
        return;
    };

    // --- Custom-detection (skip on the frame a preset was just applied) ---
    let now_hash = params_hash(&params);
    if !*just_applied && *last_hash != u64::MAX && now_hash != *last_hash {
        // Some preset-touched field changed by hand. If it no longer matches
        // any concrete preset's canonical bundle, flip to Custom.
        let matches_any = matches!(
            *current_preset,
            Preset::Cinematic | Preset::Performance | Preset::Web
        ) && canonical_hash(*current_preset) == now_hash;
        if !matches_any && *current_preset != Preset::Custom {
            *current_preset = Preset::Custom;
        }
    }
    *just_applied = false;
    *last_hash = now_hash;

    // --- Chassis ---
    egui::SidePanel::right("controls")
        .default_width(300.0)
        .width_range(260.0..=400.0)
        .resizable(true)
        .show(ctx.clone(), |ui| {
            // Top bar (fixed).
            preset_bar(ui, &mut params, &mut current_preset, &mut just_applied);

            // Section stack (scrolls).
            egui::ScrollArea::vertical().show(ui, |ui| {
                use crate::ui::sections::*;

                // --- Always-open cards ---
                group(ui, "Camera", |ui| section_camera(ui, &mut camera));
                group(ui, "Black Hole", |ui| section_black_hole(ui, &mut params));
                group(ui, "Accretion Disk", |ui| section_disk(ui, &mut params));
                group(ui, "Quality", |ui| section_quality(ui, &mut params));

                // --- Collapsing sections ---
                collapsing(ui, "turbulence", "Disk Turbulence", false,
                    |ui| section_turbulence(ui, &mut params));

                collapsing_with_toggle(ui, "doppler", "Doppler", false,
                    &mut params.doppler_enabled,
                    |ui, en| section_doppler(ui, &mut params, en));

                collapsing_with_toggle(ui, "jets", "Jets", false,
                    &mut params.jets_enabled,
                    |ui, en| section_jets(ui, &mut params, en));

                // Planets: snapshot dirty-relevant fields before rendering so
                // we can detect changes (relocated from the old Planets block).
                let prev_planet = (
                    params.planets_enabled,
                    params.planet_count_target,
                    params.planet_radius_factor,
                    params.planet_seed,
                );
                collapsing_with_toggle(ui, "planets", "Planets", false,
                    &mut params.planets_enabled,
                    |ui, en| section_planets(ui, &mut params, en, &mut planet_dirty));
                let curr_planet = (
                    params.planets_enabled,
                    params.planet_count_target,
                    params.planet_radius_factor,
                    params.planet_seed,
                );
                if curr_planet != prev_planet {
                    planet_dirty.0 = true;
                }

                collapsing(ui, "background", "Background", false,
                    |ui| section_background(ui, &mut params));

                collapsing_with_toggle(ui, "grid", "Grid", false,
                    &mut params.grid_enabled,
                    |ui, en| section_grid(ui, &mut params, en));
            });
        });

    // egui captures pointer when the cursor is over a window or being
    // interacted with. MUST stay last — load-bearing for orbit camera.
    wants.0 = ctx.egui_wants_pointer_input();
}
```

Key points:
- `SidePanel::show` takes `ctx.clone()` — `SidePanel::show` requires owned `Context`, but `ui_system` still needs `ctx` for the final `egui_wants_pointer_input()` call. Cloning the `Context` is cheap (it's an `Arc` internally).
- The old `if let Ok(ctx) = ... { ... } else { wants.0 = false }` is preserved as an early-return guard.
- `Local<Preset>` / `Local<bool>` / `Local<u64>` initialize to `Preset::default()` / `false` / `0` — but `Preset` has no `Default`, and `0` is a valid hash. Use `*last_hash == u64::MAX` as the "uninitialized" sentinel: initialize `last_hash` to `MAX` on first frame... **but `Local<u64>` defaults to `0`, not `MAX`.** Fix: derive `Default` for `Preset` returning `Custom`, and use `Option<u64>` for `last_hash`. Apply that fix now before this task's commit:

- [ ] **Step 2: Fix the Local initialization properly**

Step 1's `u64::MAX` sentinel is fragile. Replace with `Option<u64>`:

(a) At the top of `src/ui/preset.rs`, add `Default` for `Preset`:

Change the `Preset` enum declaration to:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Preset {
    #[default]
    Custom,
    Cinematic,
    Performance,
    Web,
}
```

(Moving `Custom` to first position with `#[default]` so `Local<Preset>` initializes to `Custom`.)

(b) In `ui_system`'s signature, change `mut last_hash: Local<u64>` to `mut last_hash: Local<Option<u64>>`, and replace the two blocks:

Replace:
```rust
    // Default the preset state on the first frame. Local<T: Default> would
    // require deriving Default for Preset; we instead initialize manually
    // via a sentinel hash of u64::MAX.
    if *last_hash == u64::MAX {
        *current_preset = Preset::Custom;
    }
```
with:
```rust
    // Local<Preset> defaults to Custom (Preset::default()).
    // Local<Option<u64>> defaults to None — first-frame sentinel.
```

Replace:
```rust
    if !*just_applied && *last_hash != u64::MAX && now_hash != *last_hash {
```
with:
```rust
    if !*just_applied && last_hash.is_some() && now_hash != last_hash.unwrap() {
```

Replace:
```rust
    *last_hash = now_hash;
```
with:
```rust
    *last_hash = Some(now_hash);
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles cleanly.

- [ ] **Step 4: Run tests (preset test must still pass after the Default change)**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs src/ui/preset.rs
git commit -m "feat(ui): rewrite ui_system as SidePanel chassis orchestrator

SidePanel::right(300, resizable 260-400) → preset_bar (top, fixed) →
ScrollArea (section stack). 4 always-open group() cards + 6 collapsing
sections (4 with header toggles). Planets dirty-detection relocated here
(needs pre-render field snapshot). Preset Custom-detection runs each
frame unless just_applied. WantsPointer assignment preserved verbatim.

Preset now derives Default=Custom; last_hash is Option<u64> (None=first
frame) to avoid the u64::MAX sentinel fragility."
```

---

### Task 12: Acceptance — build, test, visual, web

No new code. This task runs the spec's acceptance checklist (§5) and confirms every item.

**Files:** none

- [ ] **Step 1: Clean release build**

Run: `cargo build --release`
Expected: compiles with no errors.

- [ ] **Step 2: Full test suite**

Run: `cargo test`
Expected: all tests pass — the existing `physics` mirror tests + the three new `preset_test` tests.

- [ ] **Step 3: Desktop visual check**

Run: `cargo run --release`

Confirm each of these (tick as you see it):
- [ ] SidePanel docked on the right edge, draggable width in 260–400px range
- [ ] 4 always-open cards visible: Camera, Black Hole, Accretion Disk, Quality
- [ ] 6 collapsing headers visible: Disk Turbulence, Doppler, Jets, Planets, Background, Grid
- [ ] No section named "Renderer" (it was merged into Quality)
- [ ] Only ONE "Resolution"/`render_scale` slider exists (in Quality) — the duplicate-slider bug is gone
- [ ] Preset bar at top: dropdown shows "Custom" initially
- [ ] Select "Cinematic" → sliders jump (steps=300, render_scale=0.75, bloom=High, disk=High, aa=High), bar shows "Cinematic"
- [ ] Drag any preset-touched slider (e.g. steps) → bar auto-switches to "Custom"
- [ ] Drag a non-preset slider (e.g. camera Distance, or disk Tilt) → bar STAYS on current preset (does NOT flip to Custom)
- [ ] Click "↺ all" → all params reset to default, bar shows "Custom"
- [ ] Open Jets, set spin to 0 → header checkbox still checkable but the warning "Jets need χ ≥ 0.05" appears in orange, strength slider disabled
- [ ] In Quality, set Bloom to "Off" → Threshold and Strength rows disappear entirely
- [ ] Open Disk Turbulence, set Disk quality to "Off" → orange warning "flat zero-thickness disk rendered", 7 turbulence sliders grey out but stay visible
- [ ] **Regression-critical**: drag a slider in the panel → the orbit camera does NOT rotate
- [ ] **Regression-critical**: no grey screen / frozen view — `nudge_camera` still working (the view updates as you change params)
- [ ] Panel background is the dark blue-black `#0E1014`, accents are cyan, warnings orange
- [ ] Confirm with the human: global "↺ all" reset is acceptable vs. the spec's per-section reset (see Task 10 deviation note). If they want per-section, file as a follow-up.

- [ ] **Step 4: Web build boots without panic**

Run: `trunk serve` → open http://127.0.0.1:8080
Expected:
- [ ] Page loads, no console panic
- [ ] SidePanel renders on the right
- [ ] No egui 0.34 `Rounding`/`CornerRadius` or `Frame` stroke-in-padding errors in the console
- [ ] Preset bar works, sections expand/collapse

If the build fails to compile for wasm, the most likely cause is an egui 0.34 API surface difference — check the error against the research findings (egui 0.34 renamed `Rounding`→`CornerRadius`, `Frame` counts stroke width in padding).

- [ ] **Step 5: Final commit (only if any fixups were needed during acceptance)**

If Steps 1–4 needed fixes, commit them. Otherwise no commit — the implementation is complete as of Task 11.

```bash
git add -A
git commit -m "fix(ui): acceptance pass fixups

(If empty, skip this commit.)"
```

---

## Self-Review (run before handing off)

**1. Spec coverage** — every spec section maps to a task:
- §1 Chassis + startup styling → Tasks 2 (style.rs) + 4 (wire) + 11 (SidePanel)
- §2 Section skeleton + `section()` → Tasks 5 (group/collapsing) + 6 (collapsing_with_toggle) + 7-9 (10 sections)
- §3 Preset bar + Grid rows → Tasks 3 (preset.rs + test) + 10 (preset_bar) + 11 (orchestrator wiring) + Grid rows throughout 7-9
- §4 Toggles / disabled / warnings → Tasks 6 (header toggle) + 9 (orange warnings, hide vs disable)
- §5 File structure + acceptance → Task 1 (module dir) + 12 (acceptance)

**2. Placeholder scan** — no TBDs/TODOs. Each step has complete code or an exact command. The one "deviation from spec" (Task 10 global reset) is flagged, not hidden.

**3. Type consistency** — checked signatures across tasks:
- `group(ui, title, body)` used in Task 11 matches Task 5's definition
- `collapsing(ui, id, title, default_open, body)` matches
- `collapsing_with_toggle(ui, id, title, default_open, enabled, body)` — Task 6 defines `body: FnOnce(&mut Ui, bool)`, Task 11 calls it with `|ui, en| section_*(ui, &mut params, en)` ✓
- `section_planets(ui, params, enabled, planet_dirty)` — Task 9 signature matches Task 11 call ✓
- `Preset::default() == Custom` (Task 11.2) is consistent with `Local<Preset>` initialization
- `params_hash` / `canonical_hash` / `apply` (Task 3) match preset.rs and the Task 11 detection logic

**4. Bite-size** — each task is one cohesive change, each step is one action, commits are granular and explain what+why.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-17-ui-panel-redesign.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
