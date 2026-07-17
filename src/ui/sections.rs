//! Per-group section render functions. Each takes the `&mut egui::Ui` it
//! draws into plus the params/camera refs it needs. The `ui_system`
//! orchestrator (mod.rs) calls them inside `group()` / `collapsing()`.

use bevy_egui::egui;

use crate::camera::OrbitCamera;
use crate::params::{
    AaQuality, BlackHoleParams, BloomQuality,
};
use crate::ui::style::MUTED_TEXT;

use std::f32::consts::PI;

/// Shared two-column row helper: label + sized widget.
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
