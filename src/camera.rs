use bevy::input::mouse::MouseScrollUnit;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::WindowEvent;
use std::f32::consts::{PI, TAU};
use std::time::Duration;

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
            pitch: 7.0_f32.to_radians(),
            distance: 22.0,
            fov: 0.84,
            auto_rotate: true,
        }
    }
}

/// View-only framing shared by the ray shader and the scene overlays.
#[derive(Resource, Clone, Copy)]
pub struct SceneFraming {
    /// Offset from the viewport center in fractions of its size, with Y down.
    pub(crate) offset: Vec2,
    pub(crate) distance_scale: f32,
}

impl Default for SceneFraming {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            distance_scale: 1.0,
        }
    }
}

impl SceneFraming {
    pub(crate) fn camera(self, camera: &OrbitCamera) -> OrbitCamera {
        OrbitCamera {
            distance: (camera.distance * self.distance_scale).min(500.0),
            ..*camera
        }
    }

    pub(crate) fn approach(&mut self, target: Self, dt: f32) {
        let remaining = (-16.0 * dt).exp();
        let offset_delta = (self.offset - target.offset) * remaining;
        let scale_delta = (self.distance_scale - target.distance_scale) * remaining;
        self.offset = if offset_delta.length_squared() < 0.00025f32.powi(2) {
            target.offset
        } else {
            target.offset + offset_delta
        };
        self.distance_scale = if scale_delta.abs() < 0.001 {
            target.distance_scale
        } else {
            target.distance_scale + scale_delta
        };
    }
}

const AUTO_ORBIT_PERIOD: f32 = 45.0;
const AUTO_ORBIT_IDLE_DELAY: f32 = 1.5;
const MAX_PITCH: f32 = PI - 0.05;
// Event Horizon orbit gains in radians per logical pixel and inverse seconds.
const DRAG_RADIANS_PER_PIXEL: Vec2 = Vec2::new(-0.0055, 0.0045);
// The reference estimates release speed from the last pointer move over 16 ms.
const DRAG_SAMPLE_SECONDS: f32 = 0.016;
const DRAG_FOLLOW_RATE: f32 = 12.0;
const HELD_VELOCITY_DAMPING: f32 = 30.0;
const COAST_DAMPING: f32 = 4.0;
const COAST_STOP_SPEED: f32 = 0.0001;
const ANGLE_EPSILON: f32 = 0.00001;
const OPENING_DISTANCE: f32 = 60.0;
const OPENING_PITCH: f32 = 21.0_f32.to_radians();
const OPENING_DURATION: f32 = 1.8;
const FLYBY_START_DISTANCE: f32 = 44.0;
const FLYBY_DURATION: f32 = 10.0;

enum CameraFlight {
    Opening { started_at: Option<Duration> },
    Flyby { elapsed: f32 },
}

/// Tracks manual input and the current camera flight.
#[derive(Resource)]
pub struct OrbitInteraction {
    idle_seconds: f32,
    flight: Option<CameraFlight>,
    dragging: bool,
    /// Target minus displayed angles. Keeping yaw relative preserves turns across ±π.
    pending_rotation: Vec2,
    angular_velocity: Vec2,
}

impl Default for OrbitInteraction {
    fn default() -> Self {
        Self {
            idle_seconds: AUTO_ORBIT_IDLE_DELAY,
            flight: None,
            dragging: false,
            pending_rotation: Vec2::ZERO,
            angular_velocity: Vec2::ZERO,
        }
    }
}

impl OrbitInteraction {
    pub(crate) fn flyby_active(&self) -> bool {
        matches!(self.flight, Some(CameraFlight::Flyby { .. }))
    }

    fn prepare_opening(&mut self, camera: &mut OrbitCamera) {
        camera.distance = OPENING_DISTANCE;
        camera.pitch = OPENING_PITCH;
        self.flight = Some(CameraFlight::Opening { started_at: None });
    }

    pub(crate) fn start_flyby(&mut self, camera: &mut OrbitCamera) {
        self.clear_drag_motion();
        *camera = OrbitCamera {
            distance: FLYBY_START_DISTANCE,
            auto_rotate: camera.auto_rotate,
            ..default()
        };
        self.flight = Some(CameraFlight::Flyby { elapsed: 0.0 });
    }

    pub(crate) fn stop_motion(&mut self) {
        self.flight = None;
        self.clear_drag_motion();
    }

    fn advance_flight(&mut self, camera: &mut OrbitCamera, now: Duration, seconds: f32) -> bool {
        let settled = OrbitCamera::default();
        match &mut self.flight {
            Some(CameraFlight::Opening { started_at }) => {
                // The controller first runs when the distant scene can actually be drawn.
                let elapsed = now - *started_at.get_or_insert(now);
                let t = (elapsed.as_secs_f32() / OPENING_DURATION).min(1.0);
                if t < 1.0 {
                    let ease = if t < 0.5 {
                        4.0 * t.powi(3)
                    } else {
                        1.0 - 4.0 * (1.0 - t).powi(3)
                    };
                    // Equal changes of log radius give equal apparent changes of scale.
                    camera.distance =
                        OPENING_DISTANCE * (settled.distance / OPENING_DISTANCE).powf(ease);
                    camera.pitch = OPENING_PITCH.lerp(settled.pitch, ease);
                    return true;
                }
                camera.distance = settled.distance;
                camera.pitch = settled.pitch;
                self.idle_seconds = 0.0;
            }
            Some(CameraFlight::Flyby { elapsed }) => {
                *elapsed += seconds;
                let t = (*elapsed / FLYBY_DURATION).min(1.0);
                camera.distance =
                    FLYBY_START_DISTANCE.lerp(settled.distance, t * t * (3.0 - 2.0 * t));
                if t < 1.0 {
                    return true;
                }
            }
            None => return false,
        }
        self.flight = None;
        true
    }

    fn clear_drag_motion(&mut self) {
        self.pending_rotation = Vec2::ZERO;
        self.angular_velocity = Vec2::ZERO;
    }

    fn move_target(&mut self, camera: &OrbitCamera, mut delta: Vec2) -> Vec2 {
        let pitch = camera.pitch + self.pending_rotation.y;
        delta.y = delta.y.clamp(-MAX_PITCH - pitch, MAX_PITCH - pitch);
        self.pending_rotation += delta;
        delta
    }

    fn follow_target(&mut self, camera: &mut OrbitCamera, mut remaining: Vec2) {
        if remaining.abs().max_element() < ANGLE_EPSILON {
            remaining = Vec2::ZERO;
        }
        camera.rotate(self.pending_rotation - remaining);
        self.pending_rotation = remaining;
    }

    fn drag_by(&mut self, camera: &OrbitCamera, pixels: Vec2) {
        self.flight = None;
        let delta = self.move_target(camera, pixels * DRAG_RADIANS_PER_PIXEL);
        self.angular_velocity = delta / DRAG_SAMPLE_SECONDS;
    }

    fn follow_drag(&mut self, camera: &mut OrbitCamera, moved: bool, seconds: f32) {
        if !moved {
            // A deliberate hold before release settles the view without a stale flick.
            self.angular_velocity *= (-HELD_VELOCITY_DAMPING * seconds).exp();
        }
        if self.pending_rotation != Vec2::ZERO {
            let remaining = self.pending_rotation * (-DRAG_FOLLOW_RATE * seconds).exp();
            self.follow_target(camera, remaining);
        }
    }

    fn coast(&mut self, camera: &mut OrbitCamera, seconds: f32) -> bool {
        if self.angular_velocity.length_squared() <= COAST_STOP_SPEED * COAST_STOP_SPEED {
            self.angular_velocity = Vec2::ZERO;
        }
        if self.angular_velocity == Vec2::ZERO && self.pending_rotation == Vec2::ZERO {
            return false;
        }

        let follow = -(-DRAG_FOLLOW_RATE * seconds).exp_m1();
        let mut remaining = self.pending_rotation * (1.0 - follow);
        if seconds > 0.0 && self.angular_velocity != Vec2::ZERO {
            let speed_loss = -(-COAST_DAMPING * seconds).exp_m1();
            let coast_time = speed_loss / COAST_DAMPING;
            let requested = self.angular_velocity * coast_time;
            let delta = self.move_target(camera, requested);
            // Integrate the camera following a moving, exponentially slowing target.
            // This fraction of the new displacement is still pending at frame end.
            let lag_fraction =
                (follow - speed_loss) / ((DRAG_FOLLOW_RATE - COAST_DAMPING) * coast_time);
            remaining += delta * lag_fraction;
            self.angular_velocity *= 1.0 - speed_loss;
            if delta.y != requested.y {
                self.angular_velocity.y = 0.0;
            }
        }
        self.follow_target(camera, remaining);
        self.idle_seconds = 0.0;
        true
    }
}

pub(crate) fn prepare_opening_view(
    mut camera: ResMut<OrbitCamera>,
    mut orbit: ResMut<OrbitInteraction>,
) {
    #[cfg(target_arch = "wasm32")]
    if crate::web::prefers_reduced_motion() {
        return;
    }
    orbit.prepare_opening(&mut camera);
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
    real_time: Res<Time<Real>>,
    mut events: MessageReader<WindowEvent>,
    mut wheel: MessageReader<MouseWheel>,
) {
    let left_pressed = mouse.pressed(MouseButton::Left);
    let seconds = time.delta_secs();

    // Cursor deltas are logical pixels on both native and web. Read button edges
    // in order so a final move and release in the same frame still count as a flick.
    let mut moved = false;
    for event in events.read() {
        match event {
            WindowEvent::MouseButtonInput(button) if button.button == MouseButton::Left => {
                interaction.dragging = button.state.is_pressed() && !wants.0;
                if button.state.is_pressed() {
                    interaction.clear_drag_motion();
                    moved = false;
                }
            }
            WindowEvent::CursorMoved(cursor) if interaction.dragging && !wants.0 => {
                if let Some(delta) = cursor.delta
                    && delta != Vec2::ZERO
                {
                    interaction.drag_by(&camera, delta);
                    moved = true;
                }
            }
            WindowEvent::WindowFocused(focus) if !focus.focused => {
                interaction.dragging = false;
                interaction.clear_drag_motion();
                moved = false;
            }
            _ => {}
        }
    }
    let mut active_interaction =
        ui_activity.0 || left_pressed || mouse.just_pressed(MouseButton::Left) || moved;
    if moved || (left_pressed && interaction.dragging && !wants.0) {
        interaction.follow_drag(&mut camera, moved, seconds);
    } else if active_interaction {
        interaction.clear_drag_motion();
    }
    for ev in wheel.read() {
        active_interaction = true;
        interaction.clear_drag_motion();
        if wants.0 {
            continue;
        }
        interaction.flight = None;
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
        if matches!(interaction.flight, Some(CameraFlight::Opening { .. })) {
            interaction.flight = None;
        }
        interaction.idle_seconds = 0.0;
        return;
    }

    interaction.idle_seconds = (interaction.idle_seconds + seconds).min(AUTO_ORBIT_IDLE_DELAY);
    if interaction.advance_flight(&mut camera, real_time.elapsed(), seconds) {
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
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::{ButtonState, mouse::MouseButtonInput};
    use bevy::window::CursorMoved;

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
        world.insert_resource(Time::<Real>::default());
        world.init_resource::<Messages<WindowEvent>>();
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
        let delta = Duration::from_secs_f32(seconds);
        world.resource_mut::<Time>().advance_by(delta);
        world.resource_mut::<Time<Real>>().advance_by(delta);
        world.run_schedule(Update);
        world.resource_mut::<ButtonInput<MouseButton>>().clear();
    }

    fn left_button(world: &mut World, state: ButtonState) {
        let button = MouseButton::Left;
        let mut mouse = world.resource_mut::<ButtonInput<MouseButton>>();
        match state {
            ButtonState::Pressed => mouse.press(button),
            ButtonState::Released => mouse.release(button),
        }
        world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
            button,
            state,
            window: Entity::PLACEHOLDER,
        }));
    }

    fn move_cursor(world: &mut World, delta: Vec2) {
        world.write_message(WindowEvent::CursorMoved(CursorMoved {
            window: Entity::PLACEHOLDER,
            position: delta,
            delta: Some(delta),
        }));
    }

    fn drag(world: &mut World, delta: Vec2, seconds: f32) {
        if !world
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
        {
            left_button(world, ButtonState::Pressed);
        }
        move_cursor(world, delta);
        advance(world, seconds);
    }

    fn release_drag(world: &mut World) {
        left_button(world, ButtonState::Released);
    }

    #[test]
    fn opening_starts_on_the_first_scene_frame_and_eases_to_the_classic_view() {
        let mut world = wheel_world(22.0);
        world.run_system_once(prepare_opening_view).unwrap();
        // A long GPU warmup must not consume the shot.
        advance(&mut world, 8.0);
        let start = *world.resource::<OrbitCamera>();
        assert_eq!(start.distance, 60.0);
        assert!((start.pitch.to_degrees() - 21.0).abs() < 1e-5);

        advance(&mut world, 0.45);
        let early = *world.resource::<OrbitCamera>();
        advance(&mut world, 0.45);
        let middle = *world.resource::<OrbitCamera>();
        assert!(start.distance > early.distance && early.distance > middle.distance);
        assert!(start.pitch - early.pitch < early.pitch - middle.pitch);
        assert!((middle.distance - (60.0_f32 * 22.0).sqrt()).abs() < 1e-4);
        assert!((middle.pitch.to_degrees() - 14.0).abs() < 1e-5);
        assert_eq!(middle.yaw, start.yaw);
        assert_eq!(middle.fov, start.fov);

        advance(&mut world, 0.9);
        assert!(*world.resource::<OrbitCamera>() == OrbitCamera::default());
        advance(&mut world, 1.0);
        assert!(*world.resource::<OrbitCamera>() == OrbitCamera::default());
    }

    #[test]
    fn drag_and_scroll_take_over_the_opening_from_its_current_pose() {
        for use_drag in [true, false] {
            let mut world = wheel_world(22.0);
            world.run_system_once(prepare_opening_view).unwrap();
            advance(&mut world, 0.0);
            advance(&mut world, 0.9);
            let before = *world.resource::<OrbitCamera>();
            if use_drag {
                drag(&mut world, Vec2::new(30.0, 4.0), 0.016);
                assert!(world.resource::<OrbitCamera>().yaw < before.yaw);
                release_drag(&mut world);
            } else {
                let distance = scroll(&mut world, MouseScrollUnit::Line, &[1.0]);
                assert!((distance - before.distance / 1.1).abs() < 1e-4);
            }
            let manual_distance = world.resource::<OrbitCamera>().distance;
            advance(&mut world, 2.0);
            assert_eq!(world.resource::<OrbitCamera>().distance, manual_distance);
            assert!(world.resource::<OrbitInteraction>().flight.is_none());
        }
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
        drag(&mut world, Vec2::new(6.0, 3.0), 1.0 / 60.0);
        release_drag(&mut world);
        let released = *world.resource::<OrbitCamera>();
        let speed = world.resource::<OrbitInteraction>().angular_velocity;

        advance(&mut world, std::f32::consts::LN_2 / 4.0);
        let first = *world.resource::<OrbitCamera>();
        let half_speed = world.resource::<OrbitInteraction>().angular_velocity;
        assert!((half_speed - speed * 0.5).length() < 1e-5);
        assert!(first.yaw < released.yaw);
        assert!(first.pitch > released.pitch);

        advance(&mut world, 0.2);
        let second = *world.resource::<OrbitCamera>();
        advance(&mut world, 0.2);
        let third = *world.resource::<OrbitCamera>();
        assert!(third.yaw < second.yaw && second.yaw < first.yaw);
        assert!(second.yaw - third.yaw < first.yaw - second.yaw);

        for _ in 0..180 {
            advance(&mut world, 1.0 / 60.0);
            let orbit = world.resource::<OrbitInteraction>();
            if orbit.angular_velocity == Vec2::ZERO && orbit.pending_rotation == Vec2::ZERO {
                break;
            }
        }
        let stopped = *world.resource::<OrbitCamera>();
        advance(&mut world, 1.4);
        assert!(*world.resource::<OrbitCamera>() == stopped);

        advance(&mut world, 0.2);
        assert!(world.resource::<OrbitCamera>().yaw > stopped.yaw);
    }

    #[test]
    fn held_drag_follows_the_target_then_releases_without_a_stale_flick() {
        let mut world = wheel_world(30.0);
        let pitch = world.resource::<OrbitCamera>().pitch;
        drag(&mut world, Vec2::splat(20.0), 1.0 / 60.0);
        let target = Vec2::new(-0.110, pitch + 0.090);
        let angles = |world: &World| {
            let camera = world.resource::<OrbitCamera>();
            Vec2::new(camera.yaw, camera.pitch)
        };
        let gap = target - angles(&world);
        assert!(gap.x < 0.0 && gap.x > -0.110 && gap.y > 0.0 && gap.y < 0.090);
        advance(&mut world, 0.1);
        assert!((target - angles(&world) - gap * (-1.2_f32).exp()).length() < 1e-5);

        advance(&mut world, 1.0);
        assert!((angles(&world) - target).length() < 1e-5);
        let stopped = *world.resource::<OrbitCamera>();
        release_drag(&mut world);
        advance(&mut world, 0.1);

        assert!(*world.resource::<OrbitCamera>() == stopped);
    }

    #[test]
    fn quick_reversal_coasts_with_the_last_move_before_release() {
        let mut world = wheel_world(30.0);
        move_cursor(&mut world, Vec2::splat(100.0));
        left_button(&mut world, ButtonState::Pressed);
        move_cursor(&mut world, Vec2::new(4.0, 2.0));
        move_cursor(&mut world, Vec2::new(-4.0, -2.0));
        release_drag(&mut world);
        move_cursor(&mut world, Vec2::splat(100.0));
        advance(&mut world, 0.02);
        let speed = world.resource::<OrbitInteraction>().angular_velocity;
        assert!((speed - Vec2::new(1.375, -0.5625)).length() < 1e-5);
        let released = *world.resource::<OrbitCamera>();
        assert_eq!(released.yaw, OrbitCamera::default().yaw);
        assert_eq!(released.pitch, OrbitCamera::default().pitch);
        advance(&mut world, 0.05);
        let coasting = *world.resource::<OrbitCamera>();
        assert!(coasting.yaw > released.yaw && coasting.pitch < released.pitch);

        left_button(&mut world, ButtonState::Pressed);
        advance(&mut world, 0.1);
        assert!(*world.resource::<OrbitCamera>() == coasting);
    }

    #[test]
    fn a_drag_started_on_a_panel_does_not_rotate_the_scene() {
        let mut world = wheel_world(30.0);
        let start = *world.resource::<OrbitCamera>();
        world.resource_mut::<WantsPointer>().0 = true;
        drag(&mut world, Vec2::splat(20.0), 0.02);
        world.resource_mut::<WantsPointer>().0 = false;
        drag(&mut world, Vec2::splat(20.0), 0.02);
        release_drag(&mut world);
        advance(&mut world, 0.2);
        assert!(*world.resource::<OrbitCamera>() == start);
    }

    #[test]
    fn damping_crosses_the_yaw_seam_and_settles_at_the_pitch_limit() {
        let mut world = wheel_world(30.0);
        let mut camera = world.resource_mut::<OrbitCamera>();
        camera.yaw = -PI + 0.02;
        camera.pitch = MAX_PITCH - 0.02;
        camera.auto_rotate = false;
        drag(&mut world, Vec2::splat(12.0), 0.02);
        release_drag(&mut world);
        for _ in 0..180 {
            let before = *world.resource::<OrbitCamera>();
            advance(&mut world, 1.0 / 60.0);
            let after = world.resource::<OrbitCamera>();
            let yaw_step = (after.yaw - before.yaw + PI).rem_euclid(TAU) - PI;
            assert!((-0.06..=0.0).contains(&yaw_step));
            assert!((before.pitch..=MAX_PITCH).contains(&after.pitch));
        }
        let camera = world.resource::<OrbitCamera>();
        assert!(camera.yaw > 0.0);
        assert!((camera.pitch - MAX_PITCH).abs() < ANGLE_EPSILON);
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
