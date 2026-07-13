use bevy::prelude::*;
use bevy_egui::egui;

pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
    mut camera: ResMut<crate::camera::OrbitCamera>,
    mut wants: ResMut<crate::camera::WantsPointer>,
) {
    if let Ok(ctx) = contexts.ctx_mut() {
        egui::Window::new("Controls")
            .collapsible(true)
            .default_pos([16.0, 16.0])
            .show(ctx, |ui| {
                egui::CollapsingHeader::new("Camera")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add(egui::Slider::new(&mut camera.distance, 3.0..=200.0).text("Distance"));
                        ui.add(egui::Slider::new(&mut camera.yaw, -3.14..=3.14).text("Yaw"));
                        ui.add(egui::Slider::new(&mut camera.pitch, 0.05..=3.09).text("Pitch"));
                        ui.add(egui::Slider::new(&mut camera.fov, 0.3..=2.0).text("FOV"));
                    });
                egui::CollapsingHeader::new("Accretion Disk")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add(egui::Slider::new(&mut params.disk_inner, 1.5..=6.0).text("Inner radius"));
                        ui.add(egui::Slider::new(&mut params.disk_outer, 6.0..=40.0).text("Outer radius"));
                        ui.add(egui::Slider::new(&mut params.disk_tilt, 0.0..=3.14).text("Tilt"));
                        ui.add(egui::Slider::new(&mut params.disk_brightness, 0.0..=3.0).text("Brightness"));
                        ui.add(egui::Slider::new(&mut params.disk_rotation_speed, 0.0..=3.0).text("Rotation speed"));
                    });
                egui::CollapsingHeader::new("Doppler").show(ui, |ui| {
                    ui.checkbox(&mut params.doppler_enabled, "Enabled");
                    ui.add_enabled(params.doppler_enabled, egui::Slider::new(&mut params.doppler_strength, 0.0..=3.0).text("Strength"));
                });
                egui::CollapsingHeader::new("Renderer").show(ui, |ui| {
                    ui.add(egui::Slider::new(&mut params.steps, 50..=600).text("Steps"));
                    // NOTE: render_scale is intentionally not exposed — it isn't wired
                    // to a real sub-resolution render target in Phase 1 (the full-screen
                    // quad always renders at window resolution). Lower `Steps` to gain FPS.
                });
                egui::CollapsingHeader::new("Background").show(ui, |ui| {
                    ui.add(egui::Slider::new(&mut params.star_intensity, 0.0..=3.0).text("Star intensity"));
                    ui.add(egui::Slider::new(&mut params.skybox_intensity, 0.0..=3.0).text("Skybox intensity"));
                });
                egui::CollapsingHeader::new("Grid").show(ui, |ui| {
                    ui.checkbox(&mut params.grid_enabled, "Enabled");
                    ui.add_enabled(params.grid_enabled, egui::Slider::new(&mut params.grid_density, 0.1..=4.0).text("Density"));
                });
            });
        // egui captures pointer when the cursor is over a window or being interacted with.
        wants.0 = ctx.egui_wants_pointer_input();
    } else {
        wants.0 = false;
    }
}
