//! egui styling for the control panel. Deep-space cyan/orange theme,
//! hand-applied over `Visuals::dark()`. See design spec §1.
//!
//! Pinned to egui 0.35: the `Shadow` struct packs fields into 8-bit integers,
//! and the context style API is `set_theme` + `global_style_mut` (not the
//! `ctx.style()`/`set_style()` of 0.34). See inline comments at each site.

use bevy_egui::egui::{self, Color32, Stroke, TextStyle, Visuals};

// --- Palette (single source of truth; referenced by every section helper) ---
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(90, 200, 255);   // #5AC8FF
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(255, 140, 66); // #FF8C42
pub const PANEL_FILL: Color32 = Color32::from_rgb(14, 16, 20);      // #0E1014
pub const EXTREME_BG: Color32 = Color32::from_rgb(8, 9, 12);        // #08090C
pub const MUTED_TEXT: Color32 = Color32::from_rgb(140, 140, 140);   // read-only / disabled labels

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
///
/// Note: `animation_time` lives on `Style`, not `Visuals`, since egui 0.35;
/// it is applied in [`setup`].
pub fn sci_fi_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL_FILL;
    v.extreme_bg_color = EXTREME_BG;
    v.hyperlink_color = ACCENT_CYAN; // also used as the section-heading color by convention
    v.selection.bg_fill = ACCENT_CYAN;
    v.selection.stroke = Stroke::new(1.0, ACCENT_ORANGE);

    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 36, 48); // #1E2430
    v.widgets.inactive.bg_stroke = Stroke::new(0.5, Color32::from_rgb(60, 70, 70)); // #3C4646
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 50, 70); // #283246
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 220, 255)); // #C8DCFF
    v.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT_CYAN);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(50, 70, 100); // #324664
    v.widgets.noninteractive.bg_stroke = Stroke::new(0.5, Color32::from_rgb(40, 46, 60)); // #282E3C

    // egui 0.35's `epaint::Shadow` packs its fields into 8-bit integers
    // (`offset: [i8; 2]`, `blur: u8`, `spread: u8`) to keep the struct at 8
    // bytes. Use integer literals; the f32 form would not type-check.
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v
}

/// Apply fonts + spacing + visuals to a context. Called exactly once from
/// `setup_egui_style` (plugin.rs).
///
/// egui 0.35 replaced `ctx.style()` / `ctx.set_style()` with theme-scoped
/// accessors. We pin the active theme to Dark and mutate the active `Style`
/// in place; `Visuals` and `animation_time` are both fields of `Style`, so
/// they go through the same closure. `text_styles` is now a public
/// `BTreeMap` field (no `text_styles_mut()`).
pub fn setup(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.slider_width = 150.0;
        style.spacing.indent = 18.0;
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.indent_ends_with_horizontal_line = false;
        style.animation_time = 0.12;
        for (ts, size) in text_styles() {
            style.text_styles.insert(ts, egui::FontId::proportional(size));
        }
        style.visuals = sci_fi_visuals();
    });
}
