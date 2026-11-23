//! Shared egui styling for the floating controls.

use bevy_egui::egui::{self, Color32, Stroke, TextStyle, Visuals};

pub const PANEL_FILL: Color32 = Color32::from_rgb(28, 26, 24);
pub const SURFACE: Color32 = Color32::from_rgb(40, 37, 33);
pub const BORDER: Color32 = Color32::from_rgb(56, 51, 46);
pub const TEXT: Color32 = Color32::from_rgb(242, 237, 229);
pub const MUTED_TEXT: Color32 = Color32::from_rgb(188, 178, 167);
pub const ACCENT: Color32 = Color32::from_rgb(221, 190, 151);

// The startup screen has its own palette.
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(90, 200, 255);
pub const ACCENT_ORANGE: Color32 = Color32::from_rgb(255, 140, 66);
pub const EXTREME_BG: Color32 = Color32::from_rgb(8, 9, 12);

const HOVER: Color32 = Color32::from_rgb(51, 46, 41);
const SLIDER_RAIL: Color32 = Color32::from_rgb(73, 66, 59);

pub fn panel_frame(radius: u8) -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL_FILL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(radius)
        .shadow(egui::epaint::Shadow {
            offset: [0, 7],
            blur: 22,
            spread: 0,
            color: Color32::from_black_alpha(130),
        })
}

fn controls_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.window_fill = PANEL_FILL;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.faint_bg_color = SURFACE;
    visuals.extreme_bg_color = SURFACE;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, PANEL_FILL);
    visuals.button_frame = false;
    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.noninteractive.fg_stroke.color = MUTED_TEXT;
    widgets.inactive.bg_fill = SLIDER_RAIL;
    widgets.inactive.weak_bg_fill = SURFACE;
    widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.inactive.fg_stroke = Stroke::new(1.0, MUTED_TEXT);
    widgets.inactive.corner_radius = 6.into();
    widgets.hovered.bg_fill = ACCENT;
    widgets.hovered.weak_bg_fill = HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED_TEXT);
    widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.hovered.corner_radius = 6.into();
    widgets.hovered.expansion = 0.0;
    widgets.active = widgets.hovered;
    widgets.active.bg_stroke.color = ACCENT;
    widgets.open = widgets.hovered;
    visuals
}

/// Applied once to the primary egui context.
pub fn setup(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.slider_rail_height = 3.0;
        style.spacing.indent = 10.0;
        style.spacing.scroll.bar_width = 5.0;
        style.spacing.scroll.floating_width = 3.0;
        style.animation_time = 0.12;
        for (text_style, size) in [
            (TextStyle::Body, 13.0),
            (TextStyle::Button, 12.0),
            (TextStyle::Monospace, 12.0),
            (TextStyle::Heading, 22.0),
            (TextStyle::Small, 12.0),
        ] {
            style
                .text_styles
                .insert(text_style, egui::FontId::proportional(size));
        }
        style.visuals = controls_visuals();
    });
}
