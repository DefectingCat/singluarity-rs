use bevy_egui::egui::{self, Align2, Pos2, Rect, Ui, Vec2, vec2};

use crate::camera::{OrbitCamera, SceneFraming};

use super::style::panel_frame;

pub(super) const TRANSITION_SECONDS: f32 = 0.24;

pub(super) fn reveal(ctx: &egui::Context, id: &'static str, shown: bool) -> f32 {
    let t = ctx.animate_bool_with_time(egui::Id::new(id), shown, TRANSITION_SECONDS);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn content_opacity(now: f64, changed_at: Option<f64>) -> f32 {
    changed_at.map_or(1.0, |start| {
        let t = ((now - start) as f32 / TRANSITION_SECONDS).clamp(0.0, 1.0);
        0.45 + 0.55 * t
    })
}

pub(super) struct Surface {
    pub(super) rect: Rect,
    pub(super) settled: Rect,
}

pub(super) struct LabLayout {
    viewport: Rect,
    pub(super) compact: bool,
    inset: f32,
}

impl LabLayout {
    pub(super) fn new(ctx: &egui::Context) -> Self {
        let viewport = ctx.content_rect();
        let compact = viewport.width() <= 700.0;
        let inset = if compact { 8.0 } else { 16.0 };
        Self {
            viewport,
            compact,
            inset,
        }
    }

    pub(super) fn toolbar(
        &self,
        ctx: &egui::Context,
        id: &'static str,
        progress: f32,
        open: bool,
        contents: impl FnOnce(&mut Ui),
    ) -> Surface {
        let slide = vec2(0.0, -24.0 * (1.0 - progress));
        let rect = egui::Area::new(egui::Id::new(id))
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(self.viewport.right_top() + vec2(-self.inset, self.inset) + slide)
            .constrain(false)
            .fade_in(false)
            .interactable(open)
            .show(ctx, |ui| {
                ui.set_clip_rect(self.viewport);
                ui.multiply_opacity(progress);
                panel_frame(9).inner_margin(7).show(ui, contents);
            })
            .response
            .rect;
        Surface {
            rect,
            settled: rect.translate(-slide),
        }
    }

    pub(super) fn transport(
        &self,
        ctx: &egui::Context,
        id: &'static str,
        progress: f32,
        open: bool,
        contents: impl FnOnce(&mut Ui),
    ) -> Surface {
        let slide = vec2(0.0, 32.0 * (1.0 - progress));
        let width = self.width();
        let rect = egui::Area::new(egui::Id::new(id))
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(self.viewport.left_bottom() + vec2(self.inset, -self.inset) + slide)
            .default_size(vec2(width, 82.0))
            .constrain(false)
            .fade_in(false)
            .interactable(open)
            .show(ctx, |ui| {
                ui.set_clip_rect(self.viewport);
                ui.set_width(width);
                ui.multiply_opacity(progress);
                panel_frame(12).inner_margin(8).show(ui, contents);
            })
            .response
            .rect;
        Surface {
            rect,
            settled: rect.translate(-slide),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn inspector(
        &self,
        ctx: &egui::Context,
        id: &'static str,
        toolbar: &Surface,
        transport: Option<&Surface>,
        progress: f32,
        lab_progress: f32,
        open: bool,
        contents: impl FnOnce(&mut Ui, f32),
    ) -> Surface {
        let top = toolbar.settled.bottom() + 8.0;
        let bottom = transport.map_or(self.viewport.bottom() - self.inset, |bar| {
            bar.settled.top() - 8.0
        });
        let available_height = (bottom - top).max(0.0);
        let width = self.width().min(if self.compact { 480.0 } else { 376.0 });
        let (pivot, y, height, slide) = if self.compact {
            (
                Align2::RIGHT_BOTTOM,
                bottom,
                (available_height * 0.55).min(370.0),
                vec2(0.0, 26.0 * (1.0 - progress)),
            )
        } else {
            (
                Align2::RIGHT_TOP,
                top,
                available_height,
                vec2(28.0 * (1.0 - progress), 0.0),
            )
        };
        let mut clip = self.viewport;
        if self.compact
            && let Some(bar) = transport
        {
            clip.max.y = clip.max.y.min(bar.rect.top() - 1.0);
        }
        let rect = egui::Area::new(egui::Id::new(id))
            .pivot(pivot)
            .fixed_pos(egui::pos2(self.viewport.right() - self.inset, y) + slide)
            .default_size(vec2(width, height))
            .constrain(false)
            .fade_in(false)
            .interactable(open)
            .show(ctx, |ui| {
                ui.set_clip_rect(clip);
                ui.set_width(width);
                ui.set_max_height(height);
                ui.multiply_opacity(progress * lab_progress);
                panel_frame(12)
                    .inner_margin(10)
                    .show(ui, |ui| contents(ui, height));
            })
            .response
            .rect;
        Surface {
            rect,
            settled: rect.translate(-slide),
        }
    }

    pub(super) fn scene_region(
        &self,
        toolbar: &Surface,
        transport: Option<&Surface>,
        panel: Option<Rect>,
    ) -> Rect {
        const GAP: f32 = 16.0;
        let mut region = self.viewport;
        if let Some(bar) = transport {
            region.max.y = (bar.settled.top() - GAP).clamp(region.top(), region.bottom());
        }
        if self.compact {
            region.min.y = (toolbar.settled.bottom() + 8.0).clamp(region.top(), region.bottom());
        }
        if let Some(panel) = panel {
            if self.compact {
                region.max.y = (panel.top() - GAP).clamp(region.top(), region.bottom());
            } else {
                region.max.x = (panel.left() - GAP).clamp(region.left(), region.right());
            }
        }
        region
    }

    fn width(&self) -> f32 {
        (self.viewport.width() - 2.0 * self.inset).max(0.0)
    }
}

pub(super) fn frame_scene(canvas: Rect, center: Pos2, distance_scale: f32) -> SceneFraming {
    let offset = (center - canvas.center()) / canvas.size().max(Vec2::splat(1.0));
    SceneFraming {
        offset: bevy::math::Vec2::new(offset.x, offset.y),
        distance_scale: distance_scale.max(1.0),
    }
}

pub(super) fn fit_distance(
    canvas: Rect,
    region: Rect,
    center: Pos2,
    camera: &OrbitCamera,
    radius: f32,
) -> f32 {
    let aspect = canvas.width().max(1.0) / canvas.height().max(1.0);
    let focal = canvas.height() * 0.5 / (camera.vertical_fov(aspect) * 0.5).tan();
    let clearance = (center - region.min).min(region.max - center).min_elem();
    let half_extent = (clearance * 0.9).max(1.0);
    // Distance at which a sphere of this radius subtends the available angle.
    let distance = radius * (focal / half_extent).hypot(1.0);
    (distance / camera.distance).max(1.0)
}

pub(super) struct TimeJump {
    from: f64,
    to: f64,
    elapsed: f64,
}

impl TimeJump {
    pub(super) fn new(from: f64, to: f64) -> Option<Self> {
        (from != to).then_some(Self {
            from,
            to,
            elapsed: 0.0,
        })
    }

    pub(super) fn advance(&mut self, dt: f64, time: &mut f64) -> bool {
        self.elapsed += dt;
        let t = self.elapsed / f64::from(TRANSITION_SECONDS);
        if t >= 1.0 {
            *time = self.to;
            return true;
        }
        *time = self.from + (self.to - self.from) * (t * t * (3.0 - 2.0 * t));
        false
    }
}
