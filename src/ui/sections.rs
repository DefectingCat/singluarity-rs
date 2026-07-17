//! Per-group section render functions. Each takes the `&mut egui::Ui` it
//! draws into plus the params/camera refs it needs. The `ui_system`
//! orchestrator (mod.rs) calls them inside `group()` / `collapsing()`.

use bevy_egui::egui;

use crate::camera::OrbitCamera;
use crate::params::{
    AaQuality, BlackHoleParams, BloomQuality, DiskColorMode,
};
use crate::ui::style::{ACCENT_ORANGE, MUTED_TEXT};

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
    let blackbody = params.disk_color_mode == DiskColorMode::Blackbody;
    ui.add_enabled(
        blackbody,
        egui::Slider::new(&mut params.disk_temp, 1000.0..=50000.0)
            .suffix(" K")
            .logarithmic(true)
            .fixed_decimals(0)
            .text("Temperature"),
    );
}

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

pub fn section_planets(ui: &mut egui::Ui, params: &mut BlackHoleParams, enabled: bool) {
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
