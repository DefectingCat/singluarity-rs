//! In-session probe editor and replay state.

mod map;

use std::f64::consts::{FRAC_PI_2, PI};

use bevy_egui::egui::{self, Align, Align2, Button, Color32, Layout, RichText, Ui};

use crate::camera::OrbitCamera;
use crate::params::BlackHoleParams;
use crate::physics::probe::{IntegrationError, Launch, Outcome, ProbeKind, Trajectory, integrate};
use crate::ui::style::{ACCENT, MUTED_TEXT, PANEL_FILL, panel_frame};
use crate::ui::trajectory::clock;

const RUN_STYLES: [(char, Color32); 6] = [
    ('A', ACCENT),
    ('B', Color32::from_rgb(124, 199, 240)),
    ('C', Color32::from_rgb(163, 219, 160)),
    ('D', Color32::from_rgb(225, 159, 203)),
    ('E', Color32::from_rgb(244, 145, 119)),
    ('F', Color32::from_rgb(191, 175, 237)),
];
const RATES: [(f64, &str); 4] = [(0.25, "¼×"), (0.5, "½×"), (1.0, "1×"), (2.0, "2×")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Experiment {
    Capture,
    Clocks,
    FrameDragging,
    Precession,
}

impl Experiment {
    const ALL: [Self; 4] = [
        Self::Capture,
        Self::Clocks,
        Self::FrameDragging,
        Self::Precession,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Capture => "Capture / escape",
            Self::Clocks => "Two clocks",
            Self::FrameDragging => "Frame dragging",
            Self::Precession => "Orbit precession",
        }
    }

    fn explanation(self) -> &'static str {
        match self {
            Self::Capture => {
                "The launches differ by 8°. One pulse reaches the horizon limit and the other escapes."
            }
            Self::Clocks => {
                "Compare the time on two orbiting spacecraft at the same far-away time."
            }
            Self::FrameDragging => {
                "Both spacecraft start locally at rest. Spin carries one around the hole as it falls."
            }
            Self::Precession => {
                "The dots mark successive closest passes. Compare how their directions shift after each orbit."
            }
        }
    }

    fn launches(self) -> [Launch; 2] {
        let light = Launch::default();
        let spacecraft = Launch {
            kind: ProbeKind::Spacecraft,
            aim: FRAC_PI_2,
            ..light
        };
        match self {
            Self::Capture => [
                light,
                Launch {
                    aim: 162.0_f64.to_radians(),
                    ..light
                },
            ],
            Self::Clocks => [4.0, 10.0].map(|radius| Launch {
                radius,
                speed: Launch::circular_speed(radius, 0.0),
                ..spacecraft
            }),
            Self::FrameDragging => [0.0, 0.9].map(|spin| Launch {
                spin,
                radius: 4.0,
                ..spacecraft
            }),
            Self::Precession => [0.0, 0.8].map(|spin| Launch {
                spin,
                radius: 10.0,
                speed: 0.22,
                ..spacecraft
            }),
        }
    }
}

struct Run {
    path: Trajectory,
    visible: bool,
}

#[derive(Default)]
pub(super) struct ProbeLab {
    open: bool,
    initialized: bool,
    draft: Launch,
    preset: Option<Experiment>,
    runs: [Option<Run>; RUN_STYLES.len()],
    error: Option<IntegrationError>,
    time: f64,
    playing: bool,
    rate: usize,
    scene: bool,
    map: map::Map,
}

impl ProbeLab {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn launcher(&mut self, ui: &mut Ui, size: [f32; 2]) {
        if ui
            .add_sized(
                size,
                Button::new(RichText::new("Launch a probe").color(ACCENT)).frame(false),
            )
            .clicked()
        {
            self.open = true;
            if !self.initialized {
                self.load(Experiment::Capture);
                self.rate = 2;
                self.initialized = true;
            }
        }
    }

    fn load(&mut self, experiment: Experiment) {
        let [a, b] = experiment.launches();
        self.clear();
        self.draft = a;
        self.add_run(a);
        self.add_run(b);
        self.preset = Some(experiment);
        self.map.focus_launch(self.draft);
    }

    fn clear(&mut self) {
        self.runs = Default::default();
        self.preset = None;
        self.time = 0.0;
        self.playing = false;
        self.error = None;
    }

    fn add_run(&mut self, launch: Launch) -> bool {
        let Some(slot) = self.runs.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };
        match integrate(launch) {
            Ok(path) => {
                *slot = Some(Run {
                    path,
                    visible: true,
                });
                self.error = None;
                self.preset = None;
                self.time = 0.0;
                self.playing = false;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    fn duration(&self) -> f64 {
        self.runs
            .iter()
            .flatten()
            .map(|run| run.path.end().coordinate_time)
            .fold(0.0, f64::max)
    }

    fn update_playback(&mut self, ctx: &egui::Context) {
        let duration = self.duration();
        self.time = self.time.min(duration);
        if self.playing {
            // Give each comparison a readable playback length, regardless of its simulation span.
            let dt = f64::from(ctx.input(|input| input.stable_dt)).min(0.1);
            self.time = (self.time + dt * duration / 16.0 * RATES[self.rate].0).min(duration);
            self.playing = self.time < duration;
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        camera: &OrbitCamera,
        params: &BlackHoleParams,
    ) -> egui::Rect {
        let viewport = ctx.content_rect();
        let compact = viewport.width() <= 720.0;
        let inset = if compact { 12.0 } else { 20.0 };
        let width = (viewport.width() - 2.0 * inset).clamp(0.0, 375.0);
        let max_height = if compact {
            viewport.height() * if self.scene { 0.40 } else { 0.72 }
        } else {
            viewport.height() - 2.0 * inset
        };
        let (anchor, y) = if compact {
            (Align2::RIGHT_BOTTOM, -inset)
        } else {
            (Align2::RIGHT_TOP, inset)
        };
        self.update_playback(ctx);

        let response = egui::Area::new(egui::Id::new("probe_lab"))
            .anchor(anchor, egui::vec2(-inset, y))
            .default_size(egui::vec2(width, max_height))
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_width(width);
                panel_frame(16).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    self.header(ui);
                    let before = self.draft;
                    if self.scene {
                        ui.label(RichText::new("Coordinate paths over the current scene. Drag the scene to orbit.").small().color(MUTED_TEXT));
                    } else {
                        let height = (max_height * 0.38).clamp(65.0, 230.0);
                        self.map.show(ui, height, &mut self.draft, &self.runs, self.time, self.preset == Some(Experiment::Precession));
                    }
                    if self.draft != before {
                        self.error = None;
                    }
                    self.replay(ui);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("probe_settings")
                        .auto_shrink([false, true])
                        .max_height((max_height - ui.min_size().y - 24.0).max(28.0))
                        .show(ui, |ui| {
                            self.readings(ui);
                            ui.separator();
                            self.launch_settings(ui);
                        });
                });
            }).response;

        if self.scene && self.open {
            map::draw_scene(ctx, camera, params, &self.runs, self.time);
        }
        if !ctx.text_edit_focused()
            && !ctx.any_popup_open()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.open = false;
        }
        if !self.open {
            self.playing = false;
            self.map.end_drag();
        }
        response.rect
    }

    fn header(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Probe experiments");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("Close")
                    .on_hover_text("Close probe experiments (Esc)")
                    .clicked()
                {
                    self.open = false;
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let mut selection = None;
            egui::ComboBox::from_id_salt("experiment")
                .selected_text(self.preset.map_or("Custom experiment", Experiment::name))
                .width(132.0)
                .show_ui(ui, |ui| {
                    for experiment in Experiment::ALL {
                        if ui
                            .selectable_label(
                                self.preset == Some(experiment),
                                choice(experiment.name(), self.preset == Some(experiment)),
                            )
                            .clicked()
                        {
                            selection = Some(experiment);
                        }
                    }
                });
            if let Some(experiment) = selection {
                self.load(experiment);
            }
            let scene = self.scene;
            ui.selectable_value(&mut self.scene, false, choice("Map", !scene));
            ui.selectable_value(&mut self.scene, true, choice("Scene", scene));
        });
        if let Some(preset) = self.preset {
            ui.label(
                RichText::new(preset.explanation())
                    .small()
                    .color(MUTED_TEXT),
            );
        }
    }

    fn replay(&mut self, ui: &mut Ui) {
        let duration = self.duration();
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(duration > 0.0, |ui| {
                if ui
                    .button(if self.playing { "Pause" } else { "Play" })
                    .clicked()
                {
                    if self.time >= duration {
                        self.time = 0.0;
                    }
                    self.playing = !self.playing;
                }
                if ui.button("Rewind").clicked() {
                    self.time = 0.0;
                    self.playing = false;
                }
            });
            egui::ComboBox::from_id_salt("probe_rate")
                .width(42.0)
                .selected_text(RATES[self.rate].1)
                .show_ui(ui, |ui| {
                    for (index, (_, label)) in RATES.iter().enumerate() {
                        let selected = self.rate == index;
                        ui.selectable_value(&mut self.rate, index, choice(label, selected));
                    }
                });
            ui.label(RichText::new(format!("{:.1} r_s/c", self.time)).monospace());
        });
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = ui.available_width();
            let response = ui
                .add_enabled(
                    duration > 0.0,
                    egui::Slider::new(&mut self.time, 0.0..=duration)
                        .clamping(egui::SliderClamping::Edits)
                        .show_value(false),
                )
                .on_hover_text("Shared coordinate time");
            if response.changed() {
                self.playing = false;
            }
        });
    }

    fn readings(&mut self, ui: &mut Ui) {
        let count = self.runs.iter().flatten().count();
        ui.horizontal(|ui| {
            ui.strong(format!("Runs {count}/{}", self.runs.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(count > 0, Button::new("Clear runs"))
                    .clicked()
                {
                    self.clear();
                }
            });
        });
        for (slot, (name, color)) in self.runs.iter_mut().zip(RUN_STYLES) {
            let Some(run) = slot else { continue };
            let launch = run.path.launch();
            let sample = run.path.at(self.time);
            let status = if self.time < run.path.end().coordinate_time {
                "In flight"
            } else {
                match run.path.outcome() {
                    Outcome::HorizonLimit => "At horizon limit",
                    Outcome::Escaped => "Escaped",
                    Outcome::TimeLimit => "Simulation time limit",
                }
            };
            let mut remove = false;
            ui.push_id(name, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut run.visible,
                        RichText::new(format!("Run {name}")).strong().color(color),
                    )
                    .on_hover_text(format!(
                        "{} · {:.3e} Suns\nStart {:.1}° · Local speed {:.3} c",
                        if launch.kind == ProbeKind::Light {
                            "Light pulse"
                        } else {
                            "Spacecraft"
                        },
                        launch.solar_masses,
                        launch.bearing.to_degrees(),
                        if launch.kind == ProbeKind::Light {
                            1.0
                        } else {
                            launch.speed
                        }
                    ));
                    ui.label(format!("{status} · {:.3} r_s", sample.radius));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        remove = ui
                            .button("×")
                            .on_hover_text(format!("Remove run {name}"))
                            .clicked();
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "Far away {} · Onboard {}",
                        clock(launch.seconds(sample.coordinate_time)),
                        sample
                            .proper_time
                            .map_or_else(|| "—".into(), |t| clock(launch.seconds(t)))
                    ))
                    .small(),
                );
                ui.label(
                    RichText::new(format!(
                        "Closest {:.3} r_s · Start {:.2} r_s · Aim {:.1}° · χ {:+.3}",
                        run.path.closest_radius(),
                        launch.radius,
                        launch.aim.to_degrees(),
                        launch.spin
                    ))
                    .small()
                    .color(MUTED_TEXT),
                );
            });
            if remove {
                *slot = None;
                self.preset = None;
            }
        }
    }

    fn launch_settings(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.strong("Next launch");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let available = self.runs.iter().any(Option::is_none);
                if ui
                    .add_enabled(
                        available,
                        Button::new(RichText::new("Launch and compare").color(ACCENT)).frame(true),
                    )
                    .clicked()
                {
                    self.add_run(self.draft);
                }
            });
        });
        if let Some(error) = self.error {
            ui.colored_label(
                Color32::LIGHT_RED,
                match error {
                    IntegrationError::InvalidLaunch => {
                        "The launch settings are outside the supported range."
                    }
                    IntegrationError::StepLimit => {
                        "The solver reached its step limit before finishing this run."
                    }
                    IntegrationError::NumericalFailure => {
                        "The solver could not resolve this trajectory reliably."
                    }
                },
            );
        }
        let before = self.draft;
        ui.horizontal_wrapped(|ui| {
            let light = self.draft.kind == ProbeKind::Light;
            ui.selectable_value(
                &mut self.draft.kind,
                ProbeKind::Light,
                choice("Light pulse · c", light),
            );
            ui.selectable_value(
                &mut self.draft.kind,
                ProbeKind::Spacecraft,
                choice("Spacecraft", !light),
            );
        });
        super::sections::slider(
            ui,
            "Black-hole mass",
            &mut self.draft.solar_masses,
            1.0..=Launch::MAX_SOLAR_MASSES,
            0,
            " Suns",
            true,
        );
        super::sections::slider(
            ui,
            "Spin and direction (χ)",
            &mut self.draft.spin,
            -Launch::MAX_SPIN..=Launch::MAX_SPIN,
            3,
            "",
            false,
        );
        let min_radius = Launch::min_start_radius(self.draft.spin);
        self.draft.radius = self.draft.radius.max(min_radius);
        super::sections::slider(
            ui,
            "Start distance",
            &mut self.draft.radius,
            min_radius..=Launch::MAX_START_RADIUS,
            3,
            " r_s",
            true,
        );
        angle_slider(ui, "Start position", &mut self.draft.bearing);
        angle_slider(ui, "Aim · 0° out, ±180° in", &mut self.draft.aim);
        if self.draft.kind == ProbeKind::Spacecraft {
            super::sections::slider(
                ui,
                "Local speed",
                &mut self.draft.speed,
                0.0..=Launch::MAX_SPEED,
                3,
                " c",
                false,
            );
        }
        if self.draft != before {
            self.error = None;
        }
        ui.collapsing("Model and units", |ui| {
            ui.label("Light and spacecraft follow equatorial Kerr geodesics. Launch speed and direction use a local zero-angular-momentum frame. The solver stops 0.004 r_s outside the horizon, on an unbound outward crossing of 120 r_s, or at time 600 r_s/c. The unit r_s is the Schwarzschild radius for each run’s mass. Each run uses its own mass to convert coordinate and onboard times to seconds.");
        });
    }
}

fn choice(label: &str, selected: bool) -> RichText {
    let text = RichText::new(label);
    if selected {
        text.color(PANEL_FILL).strong()
    } else {
        text
    }
}

fn angle_slider(ui: &mut Ui, label: &str, angle: &mut f64) {
    let before = *angle;
    let wrapped = (before + PI).rem_euclid(2.0 * PI) - PI;
    let mut degrees = if wrapped == -PI && before > 0.0 {
        180.0
    } else {
        wrapped.to_degrees()
    };
    let initial = degrees;
    super::sections::slider(ui, label, &mut degrees, -180.0..=180.0, 1, "°", false);
    if degrees != initial {
        *angle = degrees.to_radians();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_load_two_runs_and_reset_replay() {
        let mut lab = ProbeLab::default();
        for experiment in Experiment::ALL {
            lab.time = 100.0;
            lab.playing = true;
            lab.load(experiment);
            assert!(lab.error.is_none());
            assert_eq!(lab.runs.iter().flatten().count(), 2);
            assert_eq!(lab.preset, Some(experiment));
            assert!(lab.time.abs() < f64::EPSILON && !lab.playing);
        }
    }

    #[test]
    fn runs_keep_their_launch_snapshot_and_reuse_empty_slots() {
        let mut lab = ProbeLab::default();
        lab.load(Experiment::Capture);
        let first = lab.draft;
        lab.draft.spin = 0.5;
        for _ in 2..RUN_STYLES.len() {
            assert!(lab.add_run(lab.draft));
        }
        assert!(!lab.add_run(lab.draft));
        assert_eq!(lab.runs[0].as_ref().unwrap().path.launch(), first);
        lab.runs[1] = None;
        assert!(lab.add_run(lab.draft));
        assert_eq!(lab.runs[1].as_ref().unwrap().path.launch(), lab.draft);
    }

    #[test]
    fn shorter_run_holds_its_final_state_while_the_comparison_continues() {
        let mut lab = ProbeLab::default();
        lab.load(Experiment::Capture);
        let captured = &lab.runs[0].as_ref().unwrap().path;
        let last = captured.end();
        let duration = lab.duration();
        assert!(duration > last.coordinate_time);
        let settled = captured.at(duration);
        assert!((settled.coordinate_time - last.coordinate_time).abs() < f64::EPSILON);
        assert!((settled.radius - last.radius).abs() < f64::EPSILON);
        let in_flight = captured.at(1.23);
        assert!((in_flight.coordinate_time - 1.23).abs() < f64::EPSILON);
        assert!(in_flight.radius < captured.launch().radius && in_flight.radius > last.radius);
    }
}
