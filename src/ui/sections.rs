//! Parameter widgets shared by the Camera, Scene, Render, and Effects tabs.

use std::{f32::consts::PI, ops::RangeInclusive};

use bevy_egui::egui::{self, Align, Layout, RichText, TextStyle, Ui};

use crate::camera::{OrbitCamera, OrbitInteraction};
use crate::params::{
    AaQuality, BlackHoleParams, BloomQuality, DiskColorMode, DiskQuality, MAX_DISK_CLUMPS,
};
use crate::ui::style::{ACCENT, BORDER, MUTED_TEXT, SURFACE, TEXT};

pub(super) fn slider<Num: egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    value: &mut Num,
    range: RangeInclusive<Num>,
    decimals: usize,
    suffix: &str,
    logarithmic: bool,
) {
    let label = ui
        .horizontal(|ui| {
            let label = ui.label(label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::DragValue::new(value)
                        .range(range.clone())
                        .clamp_existing_to_range(false)
                        .fixed_decimals(decimals)
                        .speed(10_f64.powi(-(decimals as i32)))
                        .suffix(suffix),
                )
                .labelled_by(label.id);
            });
            label
        })
        .inner;
    ui.spacing_mut().slider_width = ui.available_width();
    ui.add(
        egui::Slider::new(value, range)
            .show_value(false)
            // The camera can change between frames and zoom outside the slider
            // range. Only an actual edit may clamp or round a live value.
            .clamping(egui::SliderClamping::Edits)
            .fixed_decimals(decimals)
            .logarithmic(logarithmic),
    )
    .labelled_by(label.id);
}

fn switch(ui: &mut Ui, text: &str, checked: &mut bool) {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 27.0), egui::Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *checked, text)
    });
    if ui.is_rect_visible(rect) {
        let font = TextStyle::Body.resolve(ui.style());
        let painter = ui.painter();
        painter.text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            font,
            TEXT,
        );
        let track = egui::Align2::RIGHT_CENTER.align_size_within_rect(egui::vec2(32.0, 18.0), rect);
        let fill = if *checked { ACCENT } else { BORDER };
        painter.rect_filled(track, 9, fill);
        let thumb_x = if *checked {
            track.right() - 9.0
        } else {
            track.left() + 9.0
        };
        painter.circle_filled(
            egui::pos2(thumb_x, track.center().y),
            6.0,
            if *checked { SURFACE } else { TEXT },
        );
        if response.has_focus() {
            painter.rect_stroke(
                track.expand(2.0),
                10,
                egui::Stroke::new(1.0, ACCENT),
                egui::StrokeKind::Outside,
            );
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
}

fn choose<T: Copy + PartialEq>(ui: &mut Ui, label: &str, selected: &mut T, choices: &[(T, &str)]) {
    let selected_text = choices
        .iter()
        .find_map(|&(value, name)| (value == *selected).then_some(name))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        let label_response = ui.label(label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            egui::ComboBox::from_id_salt(label)
                .width(98.0)
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for &(value, name) in choices {
                        ui.selectable_value(selected, value, name);
                    }
                })
                .response
                .labelled_by(label_response.id);
        });
    });
}

fn heading(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).small().strong().color(MUTED_TEXT));
}

pub fn section_camera(ui: &mut Ui, cam: &mut OrbitCamera, orbit: &mut OrbitInteraction) {
    let before = *cam;
    switch(ui, "Auto orbit", &mut cam.auto_rotate);
    ui.separator();
    slider(
        ui,
        "Distance",
        &mut cam.distance,
        3.0..=200.0,
        1,
        " r_s",
        false,
    );
    slider(ui, "Yaw", &mut cam.yaw, -PI..=PI, 2, " rad", false);
    slider(
        ui,
        "Pitch",
        &mut cam.pitch,
        (-PI + 0.05)..=(PI - 0.05),
        2,
        " rad",
        false,
    );
    slider(
        ui,
        "Field of view",
        &mut cam.fov,
        0.3..=2.0,
        2,
        " rad",
        false,
    );
    if before != *cam {
        orbit.stop_flight();
    }
    ui.separator();
    let active = orbit.flyby_active();
    let label = if active { "Stop flyby" } else { "Play flyby" };
    if ui
        .add_sized([ui.available_width(), 28.0], egui::Button::new(label))
        .on_hover_text("Push in from a wide view over ten seconds")
        .clicked()
    {
        if active {
            orbit.stop_flight();
        } else {
            orbit.start_flyby(cam);
        }
    }
}

pub fn section_scene(ui: &mut Ui, params: &mut BlackHoleParams) {
    heading(ui, "Black hole");
    slider(ui, "Spin (χ)", &mut params.spin, 0.0..=1.0, 2, "", false);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("ISCO").small().color(MUTED_TEXT))
            .on_hover_text("Inner edge of the accretion disk");
        ui.label(RichText::new(format!("{:.3}", crate::physics::kerr_isco(params.spin))).small());
        ui.label(RichText::new("Horizon r+").small().color(MUTED_TEXT));
        ui.label(
            RichText::new(format!("{:.3}", crate::physics::kerr_horizon(params.spin))).small(),
        );
    });
    ui.separator();
    heading(ui, "Accretion disk");
    slider(
        ui,
        "Outer radius",
        &mut params.disk_outer,
        6.0..=50.0,
        1,
        " r_s",
        false,
    );
    slider(
        ui,
        "Tilt",
        &mut params.disk_tilt,
        0.0..=PI,
        2,
        " rad",
        false,
    );
    slider(
        ui,
        "Brightness",
        &mut params.disk_brightness,
        0.0..=3.0,
        2,
        "",
        false,
    );
    slider(
        ui,
        "Flow speed",
        &mut params.disk_rotation_speed,
        0.0..=3.0,
        2,
        "×",
        false,
    );
    choose(
        ui,
        "Color model",
        &mut params.disk_color_mode,
        &[
            (DiskColorMode::Cinematic, "Cinematic"),
            (DiskColorMode::Gradient, "Gradient"),
            (DiskColorMode::Blackbody, "Blackbody"),
        ],
    );
    ui.add_enabled_ui(params.disk_color_mode == DiskColorMode::Blackbody, |ui| {
        slider(
            ui,
            "Temperature",
            &mut params.disk_temp,
            1000.0..=50000.0,
            0,
            " K",
            true,
        );
    });
}

pub fn section_render(ui: &mut Ui, params: &mut BlackHoleParams) {
    heading(ui, "Image quality");
    slider(
        ui,
        "Resolution scale",
        &mut params.render_scale,
        0.25..=1.0,
        2,
        "×",
        true,
    );
    slider(ui, "Ray steps", &mut params.steps, 50..=600, 0, "", true);
    slider(
        ui,
        "Exposure",
        &mut params.exposure,
        0.5..=3.0,
        2,
        "",
        false,
    );
    switch(ui, "Star antialiasing", &mut params.star_aa);
    choose(
        ui,
        "Ring antialiasing",
        &mut params.aa_quality,
        &[
            (AaQuality::Off, "Off (1×)"),
            (AaQuality::Low, "Low (2–4×)"),
            (AaQuality::High, "High (4×)"),
        ],
    );
    ui.separator();
    heading(ui, "Bloom");
    choose(
        ui,
        "Quality",
        &mut params.bloom_quality,
        &[
            (BloomQuality::Off, "Off"),
            (BloomQuality::Low, "Low"),
            (BloomQuality::Medium, "Medium"),
            (BloomQuality::High, "High"),
        ],
    );
    if params.bloom_quality != BloomQuality::Off {
        slider(
            ui,
            "Threshold",
            &mut params.bloom_threshold,
            0.0..=3.0,
            2,
            "",
            false,
        );
        slider(
            ui,
            "Strength",
            &mut params.bloom_strength,
            0.0..=2.0,
            2,
            "",
            false,
        );
    }
}

pub fn section_effects(ui: &mut Ui, params: &mut BlackHoleParams) {
    egui::CollapsingHeader::new("Accretion flow").show(ui, |ui| {
        section_turbulence(ui, params);
    });
    ui.separator();
    egui::CollapsingHeader::new("Doppler").show(ui, |ui| {
        switch(ui, "Enable Doppler", &mut params.doppler_enabled);
        ui.add_enabled_ui(params.doppler_enabled, |ui| {
            slider(
                ui,
                "Strength",
                &mut params.doppler_strength,
                0.0..=1.0,
                2,
                "",
                false,
            );
        });
    });
    ui.separator();
    egui::CollapsingHeader::new("Jets").show(ui, |ui| section_jets(ui, params));
    ui.separator();
    egui::CollapsingHeader::new("Planets").show(ui, |ui| section_planets(ui, params));
    ui.separator();
    egui::CollapsingHeader::new("Background").show(ui, |ui| {
        slider(
            ui,
            "Star intensity",
            &mut params.star_intensity,
            0.0..=3.0,
            2,
            "",
            false,
        );
        slider(
            ui,
            "Skybox",
            &mut params.skybox_intensity,
            0.0..=3.0,
            2,
            "",
            false,
        );
    });
    ui.separator();
    egui::CollapsingHeader::new("Grid").show(ui, |ui| {
        switch(ui, "Show grid", &mut params.grid_enabled);
        ui.add_enabled_ui(params.grid_enabled, |ui| {
            slider(
                ui,
                "Density",
                &mut params.grid_density,
                0.1..=4.0,
                2,
                "",
                false,
            );
        });
    });
}

fn section_turbulence(ui: &mut Ui, params: &mut BlackHoleParams) {
    choose(
        ui,
        "Disk quality",
        &mut params.disk_quality,
        &[
            (DiskQuality::Off, "Off"),
            (DiskQuality::Low, "Low"),
            (DiskQuality::Medium, "Medium"),
            (DiskQuality::High, "High"),
        ],
    );
    let enabled = params.disk_quality != DiskQuality::Off;
    if !enabled {
        ui.label(
            RichText::new("Off renders a flat disk.")
                .small()
                .color(MUTED_TEXT),
        );
    }
    ui.add_enabled_ui(enabled, |ui| {
        slider(
            ui,
            "Thickness (H/R)",
            &mut params.disk_half_thickness,
            0.015..=0.30,
            3,
            "",
            false,
        );
    });
    let has_filaments = matches!(params.disk_quality, DiskQuality::Medium | DiskQuality::High);
    ui.add_enabled_ui(has_filaments, |ui| {
        slider(
            ui,
            "Filament frequency",
            &mut params.filament_freq,
            0.2..=4.0,
            2,
            "",
            false,
        );
        slider(
            ui,
            "Filament sharpness",
            &mut params.filament_sharpness,
            1.0..=6.0,
            2,
            "",
            false,
        );
    });
    slider(
        ui,
        "Radial frequency",
        &mut params.density_freq,
        0.2..=3.0,
        2,
        "",
        false,
    );
    slider(
        ui,
        "Absorption",
        &mut params.density_strength,
        0.0..=2.0,
        2,
        "",
        false,
    );
    slider(
        ui,
        "Bright clumps",
        &mut params.arm_count,
        0.0..=MAX_DISK_CLUMPS as f32,
        0,
        "",
        false,
    );
    slider(
        ui,
        "Curve tightness",
        &mut params.arm_tightness,
        0.0..=6.0,
        2,
        "",
        false,
    );
    slider(
        ui,
        "Flow contrast",
        &mut params.arm_strength,
        0.0..=1.0,
        2,
        "",
        false,
    );
}

fn section_jets(ui: &mut Ui, params: &mut BlackHoleParams) {
    switch(ui, "Enable jets", &mut params.jets_enabled);
    let jets_renderable = params.spin >= 0.05;
    if params.jets_enabled && !jets_renderable {
        ui.label(
            RichText::new("Jets require spin χ ≥ 0.05.")
                .small()
                .color(MUTED_TEXT),
        );
    }
    ui.add_enabled_ui(params.jets_enabled && jets_renderable, |ui| {
        slider(
            ui,
            "Strength",
            &mut params.jets_strength,
            0.0..=3.0,
            2,
            "",
            false,
        );
    });
}

fn section_planets(ui: &mut Ui, params: &mut BlackHoleParams) {
    switch(ui, "Enable planets", &mut params.planets_enabled);
    ui.add_enabled_ui(params.planets_enabled, |ui| {
        slider(
            ui,
            "Count",
            &mut params.planet_count_target,
            0..=8,
            0,
            "",
            false,
        );
        slider(
            ui,
            "Orbit radius",
            &mut params.planet_radius_factor,
            1.1..=2.0,
            2,
            "×",
            false,
        );
        ui.label(
            RichText::new(format!(
                "Orbit r = {:.2} (disk outer: {:.1})",
                params.planet_radius_factor * params.disk_outer,
                params.disk_outer,
            ))
            .small()
            .color(MUTED_TEXT),
        );
        slider(ui, "Seed", &mut params.planet_seed, 0..=1000, 0, "", false);
        slider(
            ui,
            "Time scale",
            &mut params.planet_time_scale,
            1.0..=200.0,
            0,
            "×",
            false,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for "auto-orbit broken: stars shake, no rotation".
    ///
    /// The camera sliders bind directly to the live `OrbitCamera` resource,
    /// which `orbit_controller` also writes every frame. egui's default
    /// `SliderClamping::Always` makes `Slider::add_contents` call
    /// `set_value(get_value())` on every IDLE frame, and `set_value` rounds to
    /// `fixed_decimals` — snapping the live camera to the slider's decimal
    /// grid each frame. Auto-orbit's per-frame yaw step (~0.0023 rad at
    /// 60 fps) is below the 0.005 half-quantum of the 2-decimal grid, so yaw
    /// was pinned forever (no rotation) while the frame-time-jittered residue
    /// strobed the starfield (shaking). The sliders now use
    /// `SliderClamping::Edits`, so an idle UI frame must leave an off-grid
    /// `OrbitCamera` bit-for-bit untouched.
    #[test]
    fn idle_camera_panel_does_not_mutate_camera() {
        // Every value deliberately off its slider's decimal grid, in-range:
        // under the old code the first idle frame rounds all four.
        let mut cam = OrbitCamera {
            yaw: -1.0623,    // 0.01 grid → would snap to -1.06
            pitch: 0.1234,   // 0.01 grid → would snap to 0.12
            distance: 30.04, // 0.1 grid → would snap to 30.0
            fov: 1.007,      // 0.01 grid → would snap to 1.01
            auto_rotate: true,
        };
        let original = cam;

        let ctx = egui::Context::default();
        let mut orbit = OrbitInteraction::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            section_camera(ui, &mut cam, &mut orbit)
        });
        output.textures_delta.clear();

        assert_eq!(cam.yaw, original.yaw, "idle Yaw slider wrote back");
        assert_eq!(cam.pitch, original.pitch, "idle Pitch slider wrote back");
        assert_eq!(
            cam.distance, original.distance,
            "idle Distance slider wrote back"
        );
        assert_eq!(cam.fov, original.fov, "idle FOV slider wrote back");
        assert_eq!(cam.auto_rotate, original.auto_rotate);
    }
}
