use bevy::input::mouse::MouseMotion;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

/// Orbit camera state. The black hole is at the origin (Rs=1 units).
/// `distance` is the camera radius; `yaw`/`pitch` orient it.
#[derive(Resource, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Field of view along the shorter viewport axis, in radians.
    pub fov: f32,
    /// Whether the camera should orbit while it is idle.
    pub auto_rotate: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            // Look across the foreground from about 7° above the disk.
            yaw: 0.0,
            pitch: 0.12,
            distance: 22.0,
            fov: 0.84,
            auto_rotate: true,
        }
    }
}

const AUTO_ORBIT_PERIOD: f32 = 45.0;
const AUTO_ORBIT_IDLE_DELAY: f32 = 1.5;
const MAX_PITCH: f32 = PI - 0.05;
const DRAG_RADIANS_PER_PIXEL: f32 = 0.005;
const DRAG_VELOCITY_RESPONSE: f32 = 30.0;
const MAX_DRAG_SPEED: f32 = 4.0;
const COAST_DAMPING: f32 = 8.0;
const COAST_STOP_SPEED: f32 = 0.02;
const FLYBY_START_DISTANCE: f32 = 44.0;
const FLYBY_DURATION: f32 = 10.0;

/// Tracks input and an optional camera flyby.
#[derive(Resource)]
pub struct OrbitInteraction {
    idle_seconds: f32,
    flyby_elapsed: Option<f32>,
    angular_velocity: Vec2,
}

impl Default for OrbitInteraction {
    fn default() -> Self {
        Self {
            idle_seconds: AUTO_ORBIT_IDLE_DELAY,
            flyby_elapsed: None,
            angular_velocity: Vec2::ZERO,
        }
    }
}

impl OrbitInteraction {
    pub(crate) fn flyby_active(&self) -> bool {
        self.flyby_elapsed.is_some()
    }

    pub(crate) fn start_flyby(&mut self, camera: &mut OrbitCamera) {
        *camera = OrbitCamera {
            distance: FLYBY_START_DISTANCE,
            auto_rotate: camera.auto_rotate,
            ..default()
        };
        self.flyby_elapsed = Some(0.0);
        self.angular_velocity = Vec2::ZERO;
    }

    pub(crate) fn stop_flyby(&mut self) {
        self.flyby_elapsed = None;
    }

    fn coast(&mut self, camera: &mut OrbitCamera, seconds: f32) -> bool {
        if self.angular_velocity.length_squared() <= COAST_STOP_SPEED * COAST_STOP_SPEED {
            self.angular_velocity = Vec2::ZERO;
            return false;
        }

        // Integrate the exponentially decaying speed over this frame.
        let speed_loss = -(-COAST_DAMPING * seconds).exp_m1();
        camera.rotate(self.angular_velocity * (speed_loss / COAST_DAMPING));
        self.angular_velocity *= 1.0 - speed_loss;
        if camera.pitch.abs() >= MAX_PITCH {
            self.angular_velocity.y = 0.0;
        }
        self.idle_seconds = 0.0;
        true
    }
}

/// Set by the UI system while a control is actively being manipulated.
#[derive(Resource, Default)]
pub struct UiPointerActivity(pub bool);

/// Set by the UI system each frame; the orbit controller ignores input when true.
#[derive(Resource, Default)]
pub struct WantsPointer(pub bool);
impl OrbitCamera {
    fn rotate(&mut self, delta: Vec2) {
        // Keep the angles within the UI slider ranges. Yaw wraps seamlessly.
        self.yaw = (self.yaw + delta.x + PI).rem_euclid(TAU) - PI;
        self.pitch = (self.pitch + delta.y).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// The ray generator expects a vertical angle even in portrait windows.
    pub(crate) fn vertical_fov(&self, aspect: f32) -> f32 {
        if aspect >= 1.0 {
            self.fov
        } else {
            2.0 * ((self.fov * 0.5).tan() / aspect).atan()
        }
    }

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
    let left_pressed = mouse.pressed(MouseButton::Left);
    let mut active_interaction = ui_activity.0 || left_pressed;
    let seconds = time.delta_secs();

    if left_pressed && !wants.0 {
        if mouse.just_pressed(MouseButton::Left) {
            interaction.angular_velocity = Vec2::ZERO;
        }
        let start_pitch = camera.pitch;
        let mut yaw_delta = 0.0;
        for ev in motion.read() {
            interaction.stop_flyby();
            // Orbit opposite the pointer so the foreground follows the drag.
            let delta = Vec2::new(-ev.delta.x, ev.delta.y) * DRAG_RADIANS_PER_PIXEL;
            camera.rotate(delta);
            yaw_delta += delta.x;
        }
        if seconds > 0.0 {
            // Sample the applied rotation. Holding still lets the estimate decay.
            let delta = Vec2::new(yaw_delta, camera.pitch - start_pitch);
            let velocity = (delta / seconds).clamp_length_max(MAX_DRAG_SPEED);
            let weight = -(-DRAG_VELOCITY_RESPONSE * seconds).exp_m1();
            interaction.angular_velocity = interaction.angular_velocity.lerp(velocity, weight);
        }
    } else {
        // Discard hover and UI motion before the next scene drag.
        motion.clear();
        if active_interaction {
            interaction.angular_velocity = Vec2::ZERO;
        }
    }
    for ev in wheel.read() {
        active_interaction = true;
        interaction.angular_velocity = Vec2::ZERO;
        if wants.0 {
            continue;
        }
        interaction.stop_flyby();
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

    interaction.idle_seconds = (interaction.idle_seconds + seconds).min(AUTO_ORBIT_IDLE_DELAY);
    if let Some(elapsed) = interaction.flyby_elapsed.as_mut() {
        *elapsed += seconds;
        let t = (*elapsed / FLYBY_DURATION).min(1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        camera.distance = FLYBY_START_DISTANCE.lerp(OrbitCamera::default().distance, eased);
        if t == 1.0 {
            interaction.stop_flyby();
        }
        return;
    }
    if interaction.coast(&mut camera, seconds) {
        return;
    }
    if camera.auto_rotate && interaction.idle_seconds >= AUTO_ORBIT_IDLE_DELAY {
        camera.rotate(Vec2::new(seconds * TAU / AUTO_ORBIT_PERIOD, 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut schedule = Schedule::new(Update);
        schedule.add_systems(orbit_controller);
        world.add_schedule(schedule);
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
        advance(world, 0.0);
        world.resource::<OrbitCamera>().distance
    }

    fn advance(world: &mut World, seconds: f32) {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(seconds));
        world.run_schedule(Update);
        world.resource_mut::<ButtonInput<MouseButton>>().clear();
    }

    fn drag(world: &mut World, delta: Vec2, seconds: f32) {
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        world.write_message(MouseMotion { delta });
        advance(world, seconds);
    }

    fn release_drag(world: &mut World) {
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
    }

    #[test]
    fn auto_orbit_starts_immediately_at_fixed_rate_by_default() {
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
    fn disabling_auto_orbit_holds_the_camera_framing() {
        let mut world = wheel_world(30.0);
        world.resource_mut::<OrbitCamera>().auto_rotate = false;
        let start = *world.resource::<OrbitCamera>();

        advance(&mut world, 1.0);

        assert!(*world.resource::<OrbitCamera>() == start);
    }

    fn start_flyby(world: &mut World) {
        world.resource_scope(|world, mut interaction: Mut<OrbitInteraction>| {
            interaction.start_flyby(&mut world.resource_mut::<OrbitCamera>());
        });
    }

    #[test]
    fn flyby_eases_to_the_default_camera_then_auto_orbit_resumes() {
        let mut world = wheel_world(5.0);
        start_flyby(&mut world);
        let near = OrbitCamera::default().distance;
        assert_eq!(
            world.resource::<OrbitCamera>().distance,
            FLYBY_START_DISTANCE
        );

        advance(&mut world, FLYBY_DURATION / 2.0);
        assert_eq!(
            world.resource::<OrbitCamera>().distance,
            (FLYBY_START_DISTANCE + near) / 2.0
        );
        advance(&mut world, FLYBY_DURATION / 2.0);

        assert!(!world.resource::<OrbitInteraction>().flyby_active());
        assert!(*world.resource::<OrbitCamera>() == OrbitCamera::default());

        advance(&mut world, 0.1);
        assert!(world.resource::<OrbitCamera>().yaw > OrbitCamera::default().yaw);
    }

    #[test]
    fn zoom_cancels_the_flyby_at_the_manual_distance() {
        let mut world = wheel_world(5.0);
        world.resource_mut::<OrbitCamera>().auto_rotate = false;
        start_flyby(&mut world);
        advance(&mut world, 1.0);
        let zoomed = scroll(&mut world, MouseScrollUnit::Line, &[1.0]);
        advance(&mut world, FLYBY_DURATION);
        assert!(!world.resource::<OrbitInteraction>().flyby_active());
        let camera = world.resource::<OrbitCamera>();
        assert_eq!(camera.distance, zoomed);
        assert!(!camera.auto_rotate);
    }

    #[test]
    fn portrait_framing_preserves_the_short_axis_angle() {
        let camera = OrbitCamera::default();
        let aspect = 9.0 / 16.0;
        let half_horizontal_tangent = (camera.vertical_fov(aspect) * 0.5).tan() * aspect;
        assert!((half_horizontal_tangent - (camera.fov * 0.5).tan()).abs() < 1e-6);
        assert_eq!(camera.vertical_fov(16.0 / 9.0), camera.fov);
    }

    #[test]
    fn left_drag_moves_the_foreground_with_the_cursor() {
        let mut world = wheel_world(30.0);
        let (start_eye, _, _, _) = world.resource::<OrbitCamera>().basis();
        // Start with a point centered in the view, in front of the orbit pivot.
        let foreground = start_eye * 0.5;

        drag(&mut world, Vec2::new(100.0, 100.0), 0.5);

        let (eye, _, right, up) = world.resource::<OrbitCamera>().basis();
        let direction = foreground - eye;
        assert!(direction.dot(right) > 0.0, "foreground should move right");
        assert!(direction.dot(up) < 0.0, "foreground should move down");
    }

    #[test]
    fn drag_coasts_then_waits_before_resuming_auto_orbit() {
        let mut world = wheel_world(30.0);
        drag(&mut world, Vec2::new(30.0, 15.0), 1.0 / 60.0);
        release_drag(&mut world);
        let released = *world.resource::<OrbitCamera>();

        advance(&mut world, 0.1);
        let first = *world.resource::<OrbitCamera>();
        assert!(first.yaw < released.yaw);
        assert!(first.pitch > released.pitch);

        advance(&mut world, 0.1);
        let second = *world.resource::<OrbitCamera>();
        assert!(second.yaw < first.yaw);
        assert!(first.yaw - second.yaw < released.yaw - first.yaw);

        advance(&mut world, 0.8);
        let stopped = *world.resource::<OrbitCamera>();
        advance(&mut world, 1.4);
        assert!(*world.resource::<OrbitCamera>() == stopped);

        advance(&mut world, 0.2);
        assert!(world.resource::<OrbitCamera>().yaw > stopped.yaw);
    }

    #[test]
    fn holding_the_mouse_still_before_release_stops_the_coast() {
        let mut world = wheel_world(30.0);
        drag(&mut world, Vec2::new(30.0, 15.0), 1.0 / 60.0);
        let stopped = *world.resource::<OrbitCamera>();

        advance(&mut world, 0.3);
        release_drag(&mut world);
        advance(&mut world, 0.1);

        assert!(*world.resource::<OrbitCamera>() == stopped);
    }

    #[test]
    fn wheel_input_cancels_the_coast() {
        let mut world = wheel_world(30.0);
        drag(&mut world, Vec2::new(30.0, 15.0), 1.0 / 60.0);
        release_drag(&mut world);
        advance(&mut world, 0.1);

        scroll(&mut world, MouseScrollUnit::Line, &[1.0]);
        let zoomed = *world.resource::<OrbitCamera>();
        advance(&mut world, 0.1);

        assert!(*world.resource::<OrbitCamera>() == zoomed);
    }

    #[test]
    fn coast_covers_the_same_angle_with_different_frame_times() {
        let coast = |frames| {
            let mut world = wheel_world(30.0);
            drag(&mut world, Vec2::new(30.0, 15.0), 1.0 / 60.0);
            release_drag(&mut world);
            for _ in 0..frames {
                advance(&mut world, 0.2 / frames as f32);
            }
            *world.resource::<OrbitCamera>()
        };
        let coarse = coast(6);
        let fine = coast(24);
        assert!((coarse.yaw - fine.yaw).abs() < 1e-5);
        assert!((coarse.pitch - fine.pitch).abs() < 1e-5);
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
