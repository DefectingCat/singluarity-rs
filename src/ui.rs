use bevy::prelude::*;
use bevy_egui::egui;

pub fn ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut params: ResMut<crate::params::BlackHoleParams>,
) {
    if let Ok(ctx) = contexts.ctx_mut() {
        egui::Window::new("Controls").show(ctx, |ui| {
            ui.add(
                egui::Slider::new(&mut params.disk_brightness, 0.0..=3.0)
                    .text("Disk brightness"),
            );
            ui.add(egui::Slider::new(&mut params.steps, 50..=600).text("Steps"));
        });
    }
}
