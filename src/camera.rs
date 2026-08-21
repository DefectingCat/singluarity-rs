use bevy::input::mouse::MouseMotion;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

/// Orbit camera state. The black hole is at the origin (Rs=1 units).
/// `distance` is the camera radius; `yaw`/`pitch` orient it.
#[derive(Resource, Clone, Copy)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
    /// Whether the camera should orbit while it is idle.
    pub auto_rotate: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: -1.065,
            // ~0.335 rad (19°) below the disk plane. The lensed far side of the
            // disk then arcs over the top of the shadow — the iconic framing. The
            // basis() has no gimbal pole, so any pitch is safe; this is just a nice
            // default angle, not a pole-avoidance choice.
            pitch: -0.335,
            distance: 30.0,
            fov: 1.0, // radians
            auto_rotate: true,
        }
    }
}

const AUTO_ORBIT_PERIOD: f32 = 45.0;
const AUTO_ORBIT_IDLE_DELAY: f32 = 1.5;

/// Tracks time since the last camera or UI interaction.
#[derive(Resource)]
pub struct OrbitInteraction {
    idle_seconds: f32,
}

impl Default for OrbitInteraction {
    fn default() -> Self {
        // Treat startup as already idle so auto-orbit begins on the first frame.
        Self {
            idle_seconds: AUTO_ORBIT_IDLE_DELAY,
        }
    }
}

/// Set by the UI system while a control is actively being manipulated.
#[derive(Resource, Default)]
pub struct UiPointerActivity(pub bool);

/// Set by the UI system each frame; the orbit controller ignores input when true.
#[derive(Resource, Default)]
pub struct WantsPointer(pub bool);
impl OrbitCamera {
    /// Compute the camera eye position and an orthonormal basis (forward/right/up)
    /// in Bevy's right-handed Y-up coordinate system. The black hole sits at origin.
    /// Disk plane is the xz-plane (y=0); the disk tilt is applied in the shader
    /// via the params, so the camera basis here is in world space.
    ///
    /// `right` is derived from `yaw` alone (always a unit vector in the y=0 plane),
    /// NOT from `forward × world_up`. The old cross-product form degenerated to a
    /// zero vector when forward aligned with world-Y (pitch = ±π/2), flipping the
    /// image. Decoupling right from forward removes that singularity, so pitch can
    /// cross both poles with no roll/flip. This also keeps right exactly horizontal
    /// for all pitch, eliminating a subtle roll the old form introduced.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        // Eye position on a sphere around the origin.
        let eye = Vec3::new(
            self.distance * cp * sy,
            self.distance * sp,
            self.distance * cp * cy,
        );
        // Forward points from eye toward the origin.
        let forward = (-eye).normalize();
        // Right depends on yaw only: a unit vector in the y=0 plane, never zero.
        let right = Vec3::new(cy, 0.0, -sy);
        let up = right.cross(forward).normalize();
        (eye, forward, right, up)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn orbit_controller(
    wants: Res<WantsPointer>,
    ui_activity: Res<UiPointerActivity>,
    mut camera: ResMut<OrbitCamera>,
    mut interaction: ResMut<OrbitInteraction>,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
) {
    let mut active_interaction = ui_activity.0;

    if wants.0 {
        // egui owns the pointer: drain motion so it cannot pile up, but continue
        // the idle timer so merely hovering the panel does not stop auto-orbit.
        motion.clear();
    }
    if mouse.pressed(MouseButton::Left) {
        active_interaction = true;
        if !wants.0 {
            for ev in motion.read() {
                // View-follows-cursor: dragging right rotates the view right,
                // dragging down tilts it down (inverts the old grab-the-scene mapping).
                camera.yaw += ev.delta.x * 0.005;
                // Keep yaw in (-π, π] so it never drifts out of the UI slider's range
                // (which would make touching the slider snap the view). cos/sin are
                // periodic, so the wrap is visually lossless.
                camera.yaw = (camera.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                // Full pitch range: the basis() has no gimbal pole now (right is yaw-only),
                // so crossing ±π/2 no longer flips. The ±0.05 margin keeps eye off the
                // exact origin-axis singularity where forward = ∓Y and yaw is undefined.
                camera.pitch = (camera.pitch - ev.delta.y * 0.005)
                    .clamp(-std::f32::consts::PI + 0.05, std::f32::consts::PI - 0.05);
            }
        }
    }
    for ev in wheel.read() {
        active_interaction = true;
        if wants.0 {
            continue;
        }
        // Trackpads/Magic Mouse deliver PIXEL deltas (tens of px per event);
        // wheel mice deliver LINE deltas (±1 per notch). Normalize pixels to
        // notch-equivalents — treating raw pixels as lines made a single
        // smooth-scroll tick divide the distance several-fold.
        let lines = match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y / 50.0,
        };
        // Exponential zoom: +y (scroll up) shrinks distance, symmetric in/out,
        // and free of the old singularity — distance/(1 + y*0.1) blew up at
        // y ≤ -10 (÷0 → ∞, or negative → clamped to the 2.6 floor), so a fast
        // scroll-out teleported the camera to the CLOSEST point and macOS
        // momentum bursts jittered it between the clamp bounds.
        camera.distance = (camera.distance * 1.1f32.powf(-lines)).clamp(2.6, 500.0);
        // 2.6 ≈ bcrit; don't let the camera pass through the shadow.
    }
    if active_interaction {
        interaction.idle_seconds = 0.0;
        return;
    }

    interaction.idle_seconds =
        (interaction.idle_seconds + time.delta_secs()).min(AUTO_ORBIT_IDLE_DELAY);
    if camera.auto_rotate && interaction.idle_seconds >= AUTO_ORBIT_IDLE_DELAY {
        // Positive yaw is the same direction as dragging the view to the right.
        camera.yaw = (camera.yaw
            + time.delta_secs() * std::f32::consts::TAU / AUTO_ORBIT_PERIOD
            + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::mouse::MouseScrollUnit;
    use std::time::Duration;

    fn wheel_world(distance: f32) -> World {
        let mut world = World::new();
        world.insert_resource(WantsPointer(false));
        world.insert_resource(UiPointerActivity(false));
        world.insert_resource(OrbitInteraction::default());
        world.insert_resource(OrbitCamera {
            distance,
            ..Default::default()
        });
        world.insert_resource(ButtonInput::<MouseButton>::default());
        world.insert_resource(Time::<()>::default());
        world.init_resource::<Messages<MouseMotion>>();
        world.init_resource::<Messages<MouseWheel>>();
        world
    }

    /// Feed wheel events (one batch = one frame), run the controller, return distance.
    fn scroll(world: &mut World, unit: MouseScrollUnit, ys: &[f32]) -> f32 {
        for &y in ys {
            world.write_message(MouseWheel {
                unit,
                x: 0.0,
                y,
                window: Entity::PLACEHOLDER,
                phase: bevy::input::touch::TouchPhase::Moved,
            });
        }
        world.run_system_once(orbit_controller).unwrap();
        let distance = world.resource::<OrbitCamera>().distance;
        world.resource_mut::<Messages<MouseWheel>>().clear();
        distance
    }

    fn advance(world: &mut World, seconds: f32) {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(seconds));
        world.run_system_once(orbit_controller).unwrap();
    }

    #[test]
    fn auto_orbit_starts_immediately_at_fixed_rate() {
        let mut world = wheel_world(30.0);
        let start = world.resource::<OrbitCamera>().yaw;

        advance(&mut world, 0.5);

        let expected = start + std::f32::consts::TAU * 0.5 / 45.0;
        let actual = world.resource::<OrbitCamera>().yaw;
        assert!(
            (actual - expected).abs() < 1e-5,
            "got {actual}, expected {expected}"
        );
    }

    #[test]
    fn wheel_interaction_pauses_then_resumes_after_idle_delay() {
        let mut world = wheel_world(30.0);
        advance(&mut world, 0.5);
        let paused_at = world.resource::<OrbitCamera>().yaw;

        scroll(&mut world, MouseScrollUnit::Line, &[1.0]);
        advance(&mut world, 1.4);
        assert_eq!(world.resource::<OrbitCamera>().yaw, paused_at);

        advance(&mut world, 0.2);
        assert!(world.resource::<OrbitCamera>().yaw > paused_at);
    }

    #[test]
    fn active_ui_interaction_pauses_auto_orbit() {
        let mut world = wheel_world(30.0);
        world.resource_mut::<UiPointerActivity>().0 = true;
        let start = world.resource::<OrbitCamera>().yaw;

        advance(&mut world, 1.0);

        assert_eq!(world.resource::<OrbitCamera>().yaw, start);
    }

    #[test]
    fn disabling_auto_orbit_keeps_manual_camera_behavior_only() {
        let mut world = wheel_world(30.0);
        world.resource_mut::<OrbitCamera>().auto_rotate = false;
        let start = world.resource::<OrbitCamera>().yaw;

        advance(&mut world, 1.0);

        assert_eq!(world.resource::<OrbitCamera>().yaw, start);
    }
    #[test]
    fn left_drag_applies_manual_rotation_without_auto_rotation() {
        let mut world = wheel_world(30.0);
        let start = world.resource::<OrbitCamera>().yaw;
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        world.write_message(MouseMotion {
            delta: Vec2::new(100.0, 0.0),
        });

        advance(&mut world, 0.5);

        let actual = world.resource::<OrbitCamera>().yaw;
        assert!((actual - (start + 0.5)).abs() < 1e-5, "got {actual}");
    }

    #[test]
    fn wheel_line_notch_zooms_in_one_step() {
        let mut w = wheel_world(30.0);
        let d = scroll(&mut w, MouseScrollUnit::Line, &[1.0]);
        assert!((d - 30.0 / 1.1).abs() < 1e-4, "got {d}");
    }

    #[test]
    fn wheel_pixel_tick_is_notch_sized_not_explosive() {
        let mut w = wheel_world(30.0);
        let d = scroll(&mut w, MouseScrollUnit::Pixel, &[30.0]);
        assert!(d > 30.0 * 0.7 && d < 30.0, "pixel tick overshot: {d}");
    }

    #[test]
    fn wheel_fast_scroll_out_does_not_teleport_inward() {
        let mut w = wheel_world(30.0);
        let d = scroll(&mut w, MouseScrollUnit::Pixel, &[-30.0]);
        assert!(
            (30.0..=500.0).contains(&d),
            "scroll-out moved camera inward: {d}"
        );
    }

    #[test]
    fn wheel_alternating_momentum_burst_stays_bounded() {
        let mut w = wheel_world(30.0);
        let d = scroll(
            &mut w,
            MouseScrollUnit::Pixel,
            &[40.0, -30.0, 25.0, -20.0, 10.0, -5.0],
        );
        assert!(d > 10.0 && d < 30.0, "alternating burst jittered: {d}");
    }
}
