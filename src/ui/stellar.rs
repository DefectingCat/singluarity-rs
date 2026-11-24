//! Feed a star controls and shared playback across the active passes.

mod view;

use bevy_egui::egui::{self, Align, Align2, Button, Color32, Layout, RichText, Ui};

use crate::camera::OrbitCamera;
use crate::params::BlackHoleParams;
use crate::physics::stellar::{CRITICAL_RADIUS, Experiment, Input, Outcome, Pass};
use crate::ui::sections::slider;
use crate::ui::style::{ACCENT, MUTED_TEXT, PANEL_FILL, panel_frame};
use crate::ui::trajectory::clock;

const COLORS: [Color32; 4] = [
    Color32::from_rgb(108, 193, 249),
    Color32::from_rgb(130, 223, 159),
    Color32::from_rgb(255, 191, 109),
    Color32::from_rgb(242, 127, 142),
];
const RATES: [(f64, &str); 4] = [(0.25, "¼×"), (0.5, "½×"), (1.0, "1×"), (2.0, "2×")];
const TIDAL_RING: Color32 = Color32::from_rgb(176, 137, 222);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Comparison,
    Custom,
}

#[derive(Clone, Copy)]
enum Phase {
    Approach,
    Encounter,
    After,
}

impl Phase {
    fn time(self, pass: &Pass) -> Option<f64> {
        match self {
            Self::Approach => pass.events.approach,
            Self::Encounter => pass.events.encounter.map(|sample| sample.coordinate_time),
            Self::After => pass.events.after,
        }
    }
}

struct Playback {
    time: f64,
    playing: bool,
    rate: usize,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            time: 0.0,
            playing: false,
            rate: 2,
        }
    }
}

impl Playback {
    fn advance(&mut self, dt: f64, duration: f64) {
        if self.playing {
            self.time =
                (self.time + dt.min(0.1) * duration / 18.0 * RATES[self.rate].0).min(duration);
            self.playing = self.time < duration;
        }
    }

    fn seek(&mut self, time: f64) {
        self.time = time;
        self.playing = false;
    }
}

struct Settings {
    mode: Mode,
    draft: Input,
    visible: [bool; 4],
    focus: Option<usize>,
    overlay: bool,
    compact_map: bool,
    zoom: f32,
    replay: Playback,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Comparison,
            draft: Input::default(),
            visible: [true; 4],
            focus: None,
            overlay: true,
            compact_map: true,
            zoom: 1.0,
            replay: Playback::default(),
        }
    }
}

impl Settings {
    fn shows(&self, index: usize) -> bool {
        self.mode == Mode::Custom
            || (self.visible[index] && self.focus.is_none_or(|focus| index == focus))
    }

    fn toggle_focus(&mut self, index: usize) {
        self.focus = (self.focus != Some(index)).then_some(index);
        self.visible[index] = true;
    }

    fn reference(&self, experiment: &Experiment) -> usize {
        // Use the disruption as the reference in the comparison until a different
        // visible pass is selected. Visibility never changes the replay duration.
        let preferred = self.focus.unwrap_or(2).min(experiment.passes.len() - 1);
        if self.shows(preferred) {
            preferred
        } else {
            (0..experiment.passes.len())
                .find(|&index| self.shows(index))
                .unwrap_or(preferred)
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.zoom = 1.0;
            self.replay.seek(0.0);
        }
    }
}

struct Experiments {
    comparison: Experiment,
    custom: Experiment,
    settings: Settings,
}

impl Default for Experiments {
    fn default() -> Self {
        Self {
            comparison: Experiment::four_passes(),
            custom: Experiment::custom(Input::default()).expect("valid default stellar input"),
            settings: Settings::default(),
        }
    }
}

#[derive(Default)]
pub(super) struct StellarLab {
    open: bool,
    experiments: Option<Experiments>,
}

impl StellarLab {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn launcher(&mut self, ui: &mut Ui, size: [f32; 2]) {
        if ui
            .add_sized(
                size,
                Button::new(RichText::new("Feed a star").color(ACCENT)).frame(false),
            )
            .clicked()
        {
            self.experiments.get_or_insert_with(Experiments::default);
            self.open = true;
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        camera: &OrbitCamera,
        params: &BlackHoleParams,
    ) -> egui::Rect {
        let Some(lab) = &mut self.experiments else {
            return egui::Rect::NOTHING;
        };
        let viewport = ctx.content_rect();
        let compact = viewport.width() <= 700.0;
        let inset = if compact { 10.0 } else { 20.0 };
        let width = (viewport.width() - 2.0 * inset).clamp(0.0, 394.0);
        let available_height = (viewport.height() - 2.0 * inset).max(0.0);
        let (pivot, y) = if compact {
            (Align2::LEFT_BOTTOM, viewport.bottom() - inset)
        } else {
            (Align2::LEFT_TOP, viewport.top() + inset)
        };
        let Experiments {
            comparison,
            custom,
            settings,
        } = lab;

        let response = egui::Area::new(egui::Id::new("stellar_lab"))
            .pivot(pivot)
            .fixed_pos(egui::pos2(viewport.right() - inset - width, y))
            .default_size(egui::vec2(width, available_height))
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.set_max_height(available_height);
                panel_frame(16)
                    .inner_margin(egui::Margin::same(11))
                    .show(ui, |ui| {
                        let close = header(ui, settings, compact);
                        self.open &= !close;
                        if settings.mode == Mode::Custom {
                            custom_summary(ui, custom, settings);
                        }

                        let experiment = match settings.mode {
                            Mode::Comparison => &*comparison,
                            Mode::Custom => &*custom,
                        };
                        let dt = f64::from(ctx.input(|input| input.stable_dt));
                        settings.replay.advance(dt, experiment.duration);
                        replay(ui, settings, experiment);
                        let show_map = !compact || settings.compact_map;
                        let max_height = if compact {
                            let fraction = if show_map { 0.70 } else { 0.43 };
                            let minimum = if show_map { 285.0 } else { 175.0 };
                            (viewport.height() * fraction).max(minimum).min(available_height)
                        } else {
                            available_height
                        };
                        if show_map {
                            let height = (max_height * if compact { 0.25 } else { 0.29 })
                                .clamp(60.0, 230.0);
                            view::map(ui, settings, experiment, height);
                        }
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .id_salt(match settings.mode {
                                Mode::Comparison => "stellar_comparison",
                                Mode::Custom => "stellar_custom",
                            })
                            .auto_shrink([false, true])
                            .max_height((max_height - ui.min_size().y - 30.0).max(26.0))
                            .min_scrolled_height(0.0)
                            .show(ui, |ui| {
                                if settings.mode == Mode::Comparison {
                                    comparison_controls(ui, comparison, settings);
                                } else {
                                    custom_controls(ui, &mut settings.draft);
                                }
                                ui.separator();
                                note(ui, "The overlay uses the current scene settings. Star sizes are visually enlarged, and fragments follow simplified ballistic paths.");
                            });
                    });
            })
            .response;

        if !ctx.text_edit_focused()
            && !ctx.any_popup_open()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.open = false;
        }
        if self.open && (settings.overlay || compact && !settings.compact_map) {
            let experiment = match settings.mode {
                Mode::Comparison => &*comparison,
                Mode::Custom => &*custom,
            };
            view::scene(ctx, camera, params, settings, experiment);
        }
        if !self.open {
            settings.replay.playing = false;
        }
        response.rect
    }
}

fn header(ui: &mut Ui, settings: &mut Settings, compact: bool) -> bool {
    let close = ui
        .horizontal(|ui| {
            ui.heading("Feed a star");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let close = ui.button("Close").on_hover_text("Close (Esc)").clicked();
                if compact {
                    let label = if settings.compact_map {
                        "Scene view"
                    } else {
                        "Orbit map"
                    };
                    if ui.button(label).clicked() {
                        settings.compact_map = !settings.compact_map;
                    }
                }
                close
            })
            .inner
        })
        .inner;
    ui.horizontal_wrapped(|ui| {
        for (mode, name) in [
            (Mode::Comparison, "Four passes"),
            (Mode::Custom, "Your experiment"),
        ] {
            if choice(ui, settings.mode == mode, name).clicked() {
                settings.set_mode(mode);
            }
        }
    });
    close
}

fn choice(ui: &mut Ui, selected: bool, label: &str) -> egui::Response {
    let text = RichText::new(label);
    ui.selectable_label(
        selected,
        if selected {
            text.color(PANEL_FILL).strong()
        } else {
            text
        },
    )
}

fn note(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).small().color(MUTED_TEXT));
}

fn replay(ui: &mut Ui, settings: &mut Settings, experiment: &Experiment) {
    let reference = settings.reference(experiment);
    let pass = &experiment.passes[reference];
    let duration = experiment.duration;
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(duration > 0.0, |ui| {
            let replay = &mut settings.replay;
            if ui
                .button(if replay.playing { "Pause" } else { "Play" })
                .clicked()
            {
                if replay.time >= duration {
                    replay.seek(0.0);
                }
                replay.playing = !replay.playing;
            }
            if ui.button("Replay").clicked() {
                replay.seek(0.0);
                replay.playing = true;
            }
            egui::ComboBox::from_id_salt("stellar_rate")
                .width(38.0)
                .selected_text(RATES[replay.rate].1)
                .show_ui(ui, |ui| {
                    for (index, (_, name)) in RATES.iter().enumerate() {
                        if choice(ui, replay.rate == index, name).clicked() {
                            replay.rate = index;
                        }
                    }
                });
        });
        let seconds = experiment.input.seconds(settings.replay.time);
        let elapsed = if seconds > 0.0 {
            clock(seconds)
        } else {
            "0.00 s".into()
        };
        ui.label(RichText::new(elapsed).monospace()).on_hover_text(
            "Elapsed Schwarzschild coordinate time, as measured far from the black hole",
        );
    });
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = ui.available_width();
        let mut time = settings.replay.time;
        if ui
            .add_enabled(
                duration > 0.0,
                egui::Slider::new(&mut time, 0.0..=duration)
                    .clamping(egui::SliderClamping::Edits)
                    .show_value(false),
            )
            .on_hover_text("Shared coordinate time")
            .changed()
        {
            settings.replay.seek(time);
        }
    });
    ui.horizontal_wrapped(|ui| {
        let encounter = match pass.outcome {
            Outcome::DirectCapture => "Horizon limit",
            Outcome::Critical => "Critical orbit",
            _ => "Closest point",
        };
        for (phase, label) in [
            (Phase::Approach, "Approach"),
            (Phase::Encounter, encounter),
            (Phase::After, "After encounter"),
        ] {
            let time = phase.time(pass);
            if ui
                .add_enabled(time.is_some(), Button::new(label))
                .on_hover_text(outcome_label(pass.outcome))
                .clicked()
                && let Some(time) = time
            {
                settings.replay.seek(time);
            }
        }
    });
}

fn comparison_controls(ui: &mut Ui, experiment: &Experiment, settings: &mut Settings) {
    note(
        ui,
        format!(
            "10 million M☉ Schwarzschild black hole · Sun-like star\nShared start at {:.1} r_s and {:.2} c. Only the entry direction changes.",
            experiment.launch_radius,
            experiment.launch_speed(),
        ),
    );
    for (index, pass) in experiment.passes.iter().enumerate() {
        ui.push_id(index, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut settings.visible[index],
                    RichText::new(outcome_label(pass.outcome)).color(COLORS[index]),
                )
                .on_hover_text(pass_detail(pass));
                if !settings.visible[index] && settings.focus == Some(index) {
                    settings.focus = None;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let focused = settings.focus == Some(index);
                    if choice(ui, focused, if focused { "Unfocus" } else { "Focus" })
                        .on_hover_text("Focus this pass in both views and use its phase times")
                        .clicked()
                    {
                        settings.toggle_focus(index);
                    }
                });
            });
        });
    }
}

fn custom_summary(ui: &mut Ui, experiment: &mut Experiment, settings: &mut Settings) {
    let dirty = settings.draft != experiment.input;
    ui.horizontal(|ui| {
        ui.strong(RichText::new(outcome_label(experiment.passes[0].outcome)).color(COLORS[2]));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    dirty && settings.draft.valid(),
                    Button::new("Apply parameters").frame(true),
                )
                .clicked()
                && let Some(applied) = Experiment::custom(settings.draft)
            {
                *experiment = applied;
                settings.zoom = 1.0;
                settings.replay.seek(0.0);
            }
        });
    });
    note(ui, pass_detail(&experiment.passes[0]));
}

fn custom_controls(ui: &mut Ui, draft: &mut Input) {
    // Use log mass for a usable slider across five orders of magnitude.
    slider(
        ui,
        "Black-hole mass",
        &mut draft.black_hole_mass,
        Input::BLACK_HOLE_MASS,
        0,
        " M☉",
        true,
    );
    slider(
        ui,
        "Stellar mass",
        &mut draft.star_mass,
        Input::STAR_MASS,
        2,
        " M☉",
        true,
    );
    slider(
        ui,
        "Stellar radius",
        &mut draft.star_radius,
        Input::STAR_RADIUS,
        2,
        " R☉",
        true,
    );
    slider(
        ui,
        "Encounter depth β",
        &mut draft.beta,
        Input::BETA,
        3,
        "",
        true,
    );
    if draft.valid() {
        let target = draft.tidal_radius() / draft.beta;
        note(
            ui,
            format!(
                "β = tidal radius / target radius · {:.3} r_s target{}",
                target,
                if target <= CRITICAL_RADIUS {
                    " (nominal)"
                } else {
                    ""
                }
            ),
        );
    }
}

fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::DistantPass => "Distant flyby",
        Outcome::GrazingPass => "Grazing flyby",
        Outcome::Disrupted => "Tidal disruption",
        Outcome::DirectCapture => "Direct capture",
        Outcome::Critical => "Critical orbit",
        Outcome::Unresolved => "Trajectory unresolved",
    }
}

fn pass_detail(pass: &Pass) -> String {
    match pass.outcome {
        Outcome::DirectCapture => format!(
            "Nominal target {:.3} r_s · Falls to the horizon limit",
            pass.target_radius
        ),
        Outcome::Critical => {
            "Nominal depth · Approaches the marginally bound orbit at 2 r_s".into()
        }
        Outcome::Unresolved => "The solver did not resolve this encounter.".into(),
        _ => {
            let radius = pass
                .events
                .encounter
                .map_or(pass.target_radius, |sample| sample.radius);
            let details = match pass.outcome {
                Outcome::Disrupted if pass.debris_unresolved() => " · Debris unresolved",
                Outcome::Disrupted => " · 48 debris particles",
                _ => "",
            };
            format!("Closest point {radius:.3} r_s{details}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_focus_and_jumps_keep_one_clock_for_every_pass() {
        let experiment = Experiment::four_passes();
        let mut settings = Settings::default();
        let encounter = Phase::Encounter
            .time(&experiment.passes[settings.reference(&experiment)])
            .unwrap();
        settings.replay.playing = true;
        settings.replay.seek(encounter);
        settings.replay.advance(0.05, experiment.duration);
        assert!((settings.replay.time - encounter).abs() < 1e-12);
        assert!(!settings.replay.playing);

        settings.visible[2] = false;
        assert!(!settings.shows(2));
        settings.toggle_focus(0);
        assert_eq!(settings.reference(&experiment), 0);
        assert!(!settings.shows(1));
        assert!((settings.replay.time - encounter).abs() < 1e-12);

        let after = Phase::After.time(&experiment.passes[0]).unwrap();
        settings.replay.seek(after);
        settings.toggle_focus(0);
        settings.visible[2] = true;
        assert!(settings.shows(1) && settings.shows(2));
        for pass in &experiment.passes {
            let path = pass.path.as_ref().unwrap();
            let sample = path.at(settings.replay.time);
            assert!((sample.coordinate_time - after.min(path.end().coordinate_time)).abs() < 1e-12);
        }

        settings.replay.playing = true;
        settings.replay.advance(0.05, experiment.duration);
        assert!(settings.replay.time > after);
    }

    #[test]
    fn the_panel_fits_narrow_portrait_viewports_in_both_modes() {
        let context = egui::Context::default();
        crate::ui::style::setup(&context);
        let mut lab = StellarLab {
            open: true,
            experiments: Some(Experiments::default()),
        };
        for size in [egui::vec2(320.0, 560.0), egui::vec2(360.0, 680.0)] {
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            for mode in [Mode::Comparison, Mode::Custom] {
                lab.experiments.as_mut().unwrap().settings.set_mode(mode);
                for show_map in [true, false] {
                    lab.experiments.as_mut().unwrap().settings.compact_map = show_map;
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(viewport),
                            ..Default::default()
                        },
                        |_| {
                            let panel = lab.show(
                                &context,
                                &OrbitCamera::default(),
                                &BlackHoleParams::default(),
                            );
                            assert!(
                                viewport.contains_rect(panel),
                                "{panel:?} outside {viewport:?}"
                            );
                        },
                    );
                    output.textures_delta.clear();
                }
            }
        }
    }
}
