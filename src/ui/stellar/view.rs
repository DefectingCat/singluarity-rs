use bevy_egui::egui::{self, Color32, Painter, Pos2, Sense, Stroke, Ui, Vec2};

use crate::camera::{OrbitCamera, SceneFraming};
use crate::params::BlackHoleParams;
use crate::physics::stellar::{Experiment, HORIZON, Outcome, Pass, Sample};
use crate::ui::style::{EXTREME_BG, MUTED_TEXT};
use crate::ui::trajectory::{MapProjection, SceneProjection, draw_line, grid};

use super::{COLORS, Mode, Settings, TIDAL_RING};
use crate::ui::lab::TRANSITION_SECONDS;

pub(super) fn map(ui: &mut Ui, settings: &mut Settings, experiment: &Experiment, height: f32) {
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut settings.overlay, "Scene overlay")
            .on_hover_text("Project the trajectories over the current scene");
        if ui.button("−").on_hover_text("Zoom orbit map out").clicked() {
            settings.zoom = (settings.zoom * 1.5).min(5.0);
        }
        if ui.button("+").on_hover_text("Zoom orbit map in").clicked() {
            settings.zoom = (settings.zoom / 1.5).max(0.03);
        }
        if ui
            .button("Fit")
            .on_hover_text("Fit the encounter region")
            .clicked()
        {
            settings.zoom = 1.0;
        }
    });

    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let base = experiment.launch_radius as f32 * 1.10;
    let zoom = ui
        .ctx()
        .animate_value_with_time(
            ui.id().with("orbit_zoom"),
            settings.zoom.ln(),
            TRANSITION_SECONDS,
        )
        .exp();
    let extent = (base * zoom).max(1.25);
    let projection = MapProjection {
        center: rect.center(),
        scale: rect.width().min(rect.height()) * 0.5 / extent,
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 7, EXTREME_BG);
    grid(&painter, rect, projection, extent);
    let tidal_radius = experiment.input.tidal_radius();
    painter.circle_stroke(
        projection.center,
        tidal_radius as f32 * projection.scale,
        Stroke::new(1.25, TIDAL_RING.gamma_multiply(0.8)),
    );
    painter.circle(
        projection.center,
        HORIZON as f32 * projection.scale,
        Color32::BLACK,
        Stroke::new(1.2, MUTED_TEXT),
    );

    draw_passes(&painter, settings, experiment, |sample| {
        Some(projection.project(sample.xy()))
    });
    for (pass, color, opacity) in visible_passes(ui.ctx(), settings, experiment) {
        if let Some(event) = pass.events.encounter
            && matches!(
                pass.outcome,
                Outcome::DistantPass | Outcome::GrazingPass | Outcome::Disrupted
            )
        {
            painter.circle(
                projection.project(event.xy()),
                3.3,
                EXTREME_BG.gamma_multiply(opacity),
                Stroke::new(1.4, color.gamma_multiply(opacity)),
            );
        }
    }
    ui.horizontal_wrapped(|ui| {
        legend(ui, MUTED_TEXT, "Horizon");
        legend(
            ui,
            TIDAL_RING,
            format!("Tidal radius {tidal_radius:.2} r_s"),
        );
        legend(ui, COLORS[2], "Closest point");
    });
}

fn legend(ui: &mut Ui, color: Color32, label: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (mark, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter()
            .circle_stroke(mark.center(), 3.5, Stroke::new(1.2, color));
        ui.small(label.into());
    });
}

pub(super) fn scene(
    ctx: &egui::Context,
    camera: &OrbitCamera,
    params: &BlackHoleParams,
    settings: &Settings,
    experiment: &Experiment,
    framing: SceneFraming,
    opacity: f32,
) {
    let rect = ctx.viewport_rect();
    let mut painter = ctx
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(rect);
    painter.multiply_opacity(opacity);
    let projection = SceneProjection::new(rect, camera, params, framing);
    draw_passes(&painter, settings, experiment, |sample| {
        projection.project(sample)
    });
}

fn visible_passes<'a>(
    ctx: &'a egui::Context,
    settings: &'a Settings,
    experiment: &'a Experiment,
) -> impl Iterator<Item = (&'a Pass, Color32, f32)> {
    experiment
        .passes
        .iter()
        .enumerate()
        .filter_map(|(index, pass)| {
            let opacity = ctx.animate_bool_with_time(
                egui::Id::new(("stellar_pass", settings.mode, index)),
                settings.shows(index),
                TRANSITION_SECONDS,
            );
            if opacity <= 0.0 {
                return None;
            }
            let color = if settings.mode == Mode::Custom {
                match pass.outcome {
                    Outcome::DistantPass => COLORS[0],
                    Outcome::GrazingPass => COLORS[1],
                    Outcome::Disrupted => COLORS[2],
                    Outcome::DirectCapture => COLORS[3],
                    Outcome::Critical | Outcome::Unresolved => MUTED_TEXT,
                }
            } else {
                COLORS[index]
            };
            Some((pass, color, opacity))
        })
}

fn draw_passes(
    painter: &Painter,
    settings: &Settings,
    experiment: &Experiment,
    project: impl Fn(Sample) -> Option<Pos2>,
) {
    // Draw the paths first, so a body's marker stays readable at intersections.
    for (pass, color, opacity) in visible_passes(painter.ctx(), settings, experiment) {
        let Some(path) = &pass.path else { continue };
        let mut painter = painter.clone();
        painter.multiply_opacity(opacity);
        let time = settings.replay.time;
        let passed = path.samples().partition_point(|s| s.coordinate_time < time);
        let current = path.at(time);
        let near_capture = |s: Sample| pass.outcome == Outcome::DirectCapture && s.radius < 1.55;
        let outer = |s: Sample| if near_capture(s) { None } else { project(s) };
        draw_line(
            &painter,
            path.samples().iter().copied(),
            Stroke::new(1.1, color.gamma_multiply(0.42)),
            &outer,
        );
        draw_line(
            &painter,
            path.samples()[..passed].iter().copied().chain([current]),
            Stroke::new(1.8, color.gamma_multiply(0.84)),
            &outer,
        );
        if pass.outcome == Outcome::DirectCapture {
            let start = path.samples().partition_point(|s| s.radius >= 1.55);
            for pair in path.samples()[start.saturating_sub(1)..].windows(2) {
                if let (Some(a), Some(b)) = (project(pair[0]), project(pair[1])) {
                    let opacity = if pair[1].coordinate_time > time {
                        0.42
                    } else {
                        0.84
                    };
                    painter.line_segment(
                        [a, b],
                        Stroke::new(1.4, color.gamma_multiply(opacity * capture_fade(pair[1]))),
                    );
                }
            }
        }
    }
    for (pass, color, opacity) in visible_passes(painter.ctx(), settings, experiment) {
        let Some(path) = &pass.path else { continue };
        let mut painter = painter.clone();
        painter.multiply_opacity(opacity);
        let time = settings.replay.time;
        if pass
            .events
            .disruption
            .is_some_and(|event| time >= event.coordinate_time)
        {
            let tail_time = time - experiment.duration * 0.014;
            for (sample, tail) in pass.fragment_trails_at(time, tail_time) {
                if let Some(point) = project(sample) {
                    if let Some(tail) = project(tail) {
                        painter.line_segment(
                            [tail, point],
                            Stroke::new(1.1, color.gamma_multiply(0.40)),
                        );
                    }
                    painter.circle_filled(point, 2.1, color.gamma_multiply(0.15));
                    painter.circle_filled(point, 1.0, color);
                }
            }
        } else if pass.outcome != Outcome::DirectCapture || time < path.end().coordinate_time {
            let sample = path.at(time);
            if let Some(point) = project(sample) {
                let fade = if pass.outcome == Outcome::DirectCapture {
                    capture_fade(sample)
                } else {
                    1.0
                };
                painter.circle_filled(point, 9.0, color.gamma_multiply(0.17 * fade));
                painter.circle_filled(point, 5.0, color.gamma_multiply(fade));
                painter.circle_filled(point, 2.25, Color32::WHITE.gamma_multiply(fade));
            }
        }
    }
}

fn capture_fade(sample: Sample) -> f32 {
    ((sample.radius - HORIZON) as f32 / 0.55).clamp(0.0, 1.0)
}
