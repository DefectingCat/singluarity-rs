use crate::state::LoadingProgress;
use crate::ui::style::{ACCENT_CYAN, ACCENT_ORANGE, EXTREME_BG, MUTED_TEXT, PANEL_FILL};
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, LayerId, Margin, Stroke, UiBuilder, Vec2};

/// Renders the HUD loading progress card during `AppState::Loading`.
pub fn loading_ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut progress: ResMut<LoadingProgress>,
    time: Res<Time>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    // Smoothly interpolate display_progress towards raw progress
    let dt = time.delta_secs();
    let target = progress.progress.clamp(0.0, 1.0);
    progress.display_progress += (target - progress.display_progress) * (dt * 8.0).min(1.0);

    let display_pct = (progress.display_progress * 100.0) as u32;

    let mut root_ui = egui::Ui::new(
        ctx.clone(),
        "loading_root".into(),
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(Color32::from_black_alpha(180)))
        .show(&mut root_ui, |ui| {
            let screen_rect = ui.max_rect();
            let center = screen_rect.center();

            let card_size = Vec2::new(380.0, 170.0);
            let card_rect = egui::Rect::from_center_size(center, card_size);

            ui.scope_builder(UiBuilder::new().max_rect(card_rect), |ui| {
                egui::Frame::window(ui.style())
                    .fill(PANEL_FILL)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(40, 50, 70)))
                    .corner_radius(10.0)
                    .inner_margin(Margin::same(18))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(200),
                    })
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            // Title Header
                            ui.label(
                                egui::RichText::new("SINGULARITY")
                                    .strong()
                                    .size(18.0)
                                    .color(Color32::WHITE)
                                    .monospace(),
                            );
                            ui.label(
                                egui::RichText::new("KERR BLACK HOLE ENGINE")
                                    .small()
                                    .color(ACCENT_CYAN)
                                    .monospace(),
                            );

                            ui.add_space(14.0);

                            // Progress bar status & percentage
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&progress.status)
                                        .small()
                                        .color(MUTED_TEXT)
                                        .monospace(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{}%", display_pct))
                                                .strong()
                                                .size(13.0)
                                                .color(ACCENT_ORANGE)
                                                .monospace(),
                                        );
                                    },
                                );
                            });

                            ui.add_space(6.0);

                            // Custom glowing progress bar
                            let progress_bar_height = 6.0;
                            let (rect, _response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), progress_bar_height),
                                egui::Sense::hover(),
                            );

                            // Background track
                            ui.painter().rect_filled(rect, 3.0, EXTREME_BG);

                            // Filled portion
                            let fill_width =
                                rect.width() * progress.display_progress.clamp(0.01, 1.0);
                            let fill_rect = egui::Rect::from_min_size(
                                rect.min,
                                Vec2::new(fill_width, progress_bar_height),
                            );

                            // Gradient / Accent fill
                            ui.painter().rect_filled(fill_rect, 3.0, ACCENT_CYAN);

                            ui.add_space(14.0);

                            // Footer info
                            ui.label(
                                egui::RichText::new("BEVY 0.19 | WEBGPU / WGPU ENGINE")
                                    .small()
                                    .color(Color32::from_rgb(80, 90, 110))
                                    .monospace(),
                            );
                        });
                    });
            });
        });
}
