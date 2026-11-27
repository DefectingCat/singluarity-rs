use bevy::math::VectorSpace;
use std::f32::consts::{PI, TAU};

use crate::camera::{OrbitCamera, SceneFraming};
use crate::params::{BlackHoleParams, KerrGeometry};

// Only the camera and the effects staged by the tour belong in a shot.
// Resolution, sample counts, planets and the disk clock keep their live values.
#[derive(Clone, Copy)]
struct Shot {
    yaw: f32,
    pitch: f32,
    distance: f32,
    fov: f32,
    tilt: f32,
    brightness: f32,
    outer: f32,
    flow: f32,
    clumps: f32,
    clump_strength: f32,
    beaming: f32,
    thickness: f32,
    jets: f32,
    spin: f32,
}

const DISK: Shot = Shot {
    yaw: 0.0,
    pitch: 7.0_f32.to_radians(),
    distance: 22.0,
    fov: 0.84,
    tilt: 0.0,
    brightness: 1.25,
    outer: 24.0,
    flow: 1.0,
    clumps: 4.0,
    clump_strength: 0.8,
    beaming: 0.72,
    thickness: 0.075,
    jets: 0.0,
    spin: 0.0,
};

impl Shot {
    fn capture(camera: &OrbitCamera, p: &BlackHoleParams) -> Self {
        Self {
            yaw: camera.yaw,
            pitch: camera.pitch,
            distance: camera.distance,
            fov: camera.fov,
            tilt: p.disk_tilt,
            brightness: p.disk_brightness,
            outer: p.disk_outer,
            flow: p.disk_rotation_speed,
            clumps: p.arm_count,
            clump_strength: p.arm_strength,
            beaming: if p.doppler_enabled {
                p.doppler_strength
            } else {
                0.0
            },
            thickness: p.disk_half_thickness,
            jets: if p.jets_enabled { p.jets_strength } else { 0.0 },
            spin: p.spin,
        }
    }

    fn blend(self, to: Self, t: f32) -> Self {
        if t <= 0.0 {
            return self;
        }
        if t >= 1.0 {
            return to;
        }
        let yaw_delta = (to.yaw - self.yaw + PI).rem_euclid(TAU) - PI;
        Self {
            yaw: self.yaw + yaw_delta * t,
            pitch: self.pitch.lerp(to.pitch, t),
            distance: self.distance * (to.distance / self.distance).powf(t),
            fov: self.fov.lerp(to.fov, t),
            tilt: self.tilt.lerp(to.tilt, t),
            brightness: self.brightness.lerp(to.brightness, t),
            outer: self.outer.lerp(to.outer, t),
            flow: self.flow.lerp(to.flow, t),
            clumps: self.clumps.lerp(to.clumps, t),
            clump_strength: self.clump_strength.lerp(to.clump_strength, t),
            beaming: self.beaming.lerp(to.beaming, t),
            thickness: self.thickness.lerp(to.thickness, t),
            jets: self.jets.lerp(to.jets, t),
            spin: self.spin.lerp(to.spin, t),
        }
    }

    fn apply(self, camera: &mut OrbitCamera, p: &mut BlackHoleParams, follow_camera: bool) {
        if follow_camera {
            camera.yaw = self.yaw;
            camera.pitch = self.pitch;
            camera.distance = self.distance;
            camera.fov = self.fov;
        }
        p.disk_tilt = self.tilt;
        p.disk_brightness = self.brightness;
        p.disk_outer = self.outer;
        p.disk_rotation_speed = self.flow;
        p.arm_count = self.clumps;
        p.arm_strength = self.clump_strength;
        p.doppler_enabled = self.beaming > 0.0;
        p.doppler_strength = self.beaming;
        p.disk_half_thickness = self.thickness;
        p.jets_enabled = self.jets > 0.0;
        p.jets_strength = self.jets;
        p.spin = self.spin;
    }
}

#[derive(Clone, Copy)]
enum Sweep {
    Distance,
    Elevation,
    Flow,
    Beaming,
    OuterRadius,
    Thickness,
    Jets,
    Spin,
}

pub(super) struct Chapter {
    pub title: &'static str,
    pub text: &'static str,
    shot: Shot,
    sweep: Sweep,
    start: f32,
    end: f32,
}

pub(super) const CHAPTERS: [Chapter; 9] = [
    Chapter {
        title: "Approach the shadow",
        text: "Watch the dark patch grow as you move closer. Light aimed into it is captured by the hole. The glowing rim belongs to gas outside the horizon.",
        shot: DISK,
        sweep: Sweep::Distance,
        start: 48.0,
        end: 22.0,
    },
    Chapter {
        title: "Rise above the disk",
        text: "Climb above the equator and follow the bright band as it opens into a ring. You are seeing the same disk from a new angle.",
        shot: Shot {
            distance: 29.0,
            ..DISK
        },
        sweep: Sweep::Elevation,
        start: 7.0,
        end: 72.0,
    },
    Chapter {
        title: "Find the far side",
        text: "Descend until the foreground gas looks almost edge-on. The far side stays visible in an arc above the shadow because its light bends around the hole.",
        shot: DISK,
        sweep: Sweep::Elevation,
        start: 48.0,
        end: 3.0,
    },
    Chapter {
        title: "Follow a bright patch",
        text: "Pick out a bright patch while the gas pattern is still. As the flow starts, its inner edge overtakes its outer edge and stretches the patch into an arc.",
        shot: Shot {
            pitch: 38.0_f32.to_radians(),
            clump_strength: 1.5,
            ..DISK
        },
        sweep: Sweep::Flow,
        start: 0.0,
        end: 2.5,
    },
    Chapter {
        title: "Light from the approaching gas",
        text: "Compare the two sides of the disk as Doppler beaming builds. Gas moving toward you sends more light in your direction, making that side brighter.",
        shot: DISK,
        sweep: Sweep::Beaming,
        start: 0.0,
        end: 1.0,
    },
    Chapter {
        title: "Move the outer edge",
        text: "Watch the disk spread outward while the central shadow stays put. The outer radius sets how far the emitting gas extends.",
        shot: Shot {
            distance: 35.0,
            pitch: 36.0_f32.to_radians(),
            ..DISK
        },
        sweep: Sweep::OuterRadius,
        start: 8.0,
        end: 30.0,
    },
    Chapter {
        title: "Give the disk depth",
        text: "Look along the foreground band as the gas layer thickens. More glowing material now lies above and below the equatorial plane.",
        shot: Shot {
            pitch: 3.0_f32.to_radians(),
            ..DISK
        },
        sweep: Sweep::Thickness,
        start: 0.025,
        end: 0.16,
    },
    Chapter {
        title: "Look along the poles",
        text: "Two jets stream away from the spinning hole along its axis. Turn up their emission and follow the blue light above and below the warmer disk.",
        shot: Shot {
            distance: 34.0,
            pitch: 18.0_f32.to_radians(),
            spin: 0.65,
            ..DISK
        },
        sweep: Sweep::Jets,
        start: 0.0,
        end: 3.0,
    },
    Chapter {
        title: "Spin up the black hole",
        text: "Follow the inner rim as spin rises to 95% of the theoretical maximum. Gas orbiting with the hole can stay on a stable orbit closer in. Watch the inner radius fall.",
        shot: Shot {
            distance: 19.0,
            ..DISK
        },
        sweep: Sweep::Spin,
        start: 0.0,
        end: 0.95,
    },
];

impl Chapter {
    fn at(&self, progress: f32, yaw: f32) -> Shot {
        let mut shot = Shot { yaw, ..self.shot };
        let value = self.start.lerp(self.end, progress);
        match self.sweep {
            Sweep::Distance => shot.distance = self.start * (self.end / self.start).powf(progress),
            Sweep::Elevation => shot.pitch = value.to_radians(),
            Sweep::Flow => shot.flow = value,
            Sweep::Beaming => shot.beaming = value,
            Sweep::OuterRadius => shot.outer = value,
            Sweep::Thickness => shot.thickness = value,
            Sweep::Jets => shot.jets = value,
            Sweep::Spin => shot.spin = value,
        }
        shot
    }

    pub fn reading(
        &self,
        camera: &OrbitCamera,
        p: &BlackHoleParams,
        geometry: KerrGeometry,
    ) -> (&'static str, String) {
        match self.sweep {
            Sweep::Distance => ("Observer distance", format!("{:.1} Rs", camera.distance)),
            Sweep::Elevation => (
                "View above disk",
                format!("{:.0}°", camera.pitch.to_degrees()),
            ),
            Sweep::Flow => ("Flow speed", format!("{:.2}×", p.disk_rotation_speed)),
            Sweep::Beaming => (
                "Doppler strength",
                format!("{:.0}%", p.doppler_strength * 100.0),
            ),
            Sweep::OuterRadius => ("Outer radius", format!("{:.1} Rs", p.disk_outer)),
            Sweep::Thickness => (
                "Disk height / radius",
                format!("{:.1}%", p.disk_half_thickness * 100.0),
            ),
            Sweep::Jets => ("Jet strength", format!("{:.2}×", p.jets_strength)),
            Sweep::Spin => (
                "Spin",
                format!(
                    "{:.0}%   ·   Inner edge {:.2} Rs",
                    geometry.spin * 100.0,
                    geometry.isco
                ),
            ),
        }
    }
}

const SHOT_CHANGE: f32 = 0.9;
const RETURN_TIME: f32 = 0.55;
const DEMO_TIME: f32 = 4.5;

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub(super) struct Playback {
    original: Shot,
    auto_rotate: bool,
    // Keep the user's stored strengths even for effects that were switched off.
    doppler: (bool, f32),
    jets: (bool, f32),
    pub original_framing: SceneFraming,
    source: Shot,
    source_progress: f32,
    transition: f32,
    last_camera: OrbitCamera,
    follow_camera: bool,
    pub chapter: usize,
    pub progress: f32,
    pub playing: bool,
    pub returning: bool,
}

impl Playback {
    pub fn new(camera: &mut OrbitCamera, p: &BlackHoleParams, framing: SceneFraming) -> Self {
        let original = Shot::capture(camera, p);
        let auto_rotate = camera.auto_rotate;
        camera.auto_rotate = false;
        Self {
            original,
            auto_rotate,
            doppler: (p.doppler_enabled, p.doppler_strength),
            jets: (p.jets_enabled, p.jets_strength),
            original_framing: framing,
            source: original,
            source_progress: 0.0,
            last_camera: *camera,
            transition: 0.0,
            follow_camera: true,
            chapter: 0,
            progress: 0.0,
            playing: true,
            returning: false,
        }
    }

    pub fn begin(&mut self, chapter: usize, camera: &OrbitCamera, p: &BlackHoleParams) {
        self.source = Shot::capture(camera, p);
        self.source_progress = self.slider_position();
        self.last_camera = *camera;
        self.follow_camera = true;
        self.transition = 0.0;
        self.progress = 0.0;
        self.playing = true;
        self.chapter = chapter;
    }

    pub fn slider_position(&self) -> f32 {
        self.source_progress
            .lerp(self.progress, smooth(self.transition))
    }

    pub fn exit(&mut self, camera: &OrbitCamera, p: &BlackHoleParams) {
        self.begin(self.chapter, camera, p);
        self.returning = true;
    }

    pub fn scrub(&mut self, progress: f32, camera: &OrbitCamera) {
        self.progress = progress;
        self.playing = false;
        self.transition = 1.0;
        if matches!(
            CHAPTERS[self.chapter].sweep,
            Sweep::Distance | Sweep::Elevation
        ) {
            self.follow_camera = true;
        }
        self.source.yaw = camera.yaw;
        self.last_camera = *camera;
    }

    /// Returns true once the free-exploration view has been restored.
    pub fn advance(&mut self, dt: f32, camera: &mut OrbitCamera, p: &mut BlackHoleParams) -> bool {
        // Orbit and zoom input run before egui. Hand over the camera for the
        // rest of this shot while its physical demonstration can keep playing.
        if *camera != self.last_camera {
            self.follow_camera = false;
        }
        let duration = if self.returning {
            RETURN_TIME
        } else {
            SHOT_CHANGE
        };
        if self.returning || self.playing {
            let travel = dt.min((1.0 - self.transition) * duration);
            self.transition = (self.transition + travel / duration).min(1.0);
            self.progress = (self.progress + (dt - travel) / DEMO_TIME).min(1.0);
            if self.progress == 1.0 {
                self.playing = false;
            }
        }
        let target = if self.returning {
            self.original
        } else {
            CHAPTERS[self.chapter].at(smooth(self.progress), self.source.yaw)
        };
        self.source
            .blend(target, smooth(self.transition))
            .apply(camera, p, self.follow_camera);
        self.last_camera = *camera;
        if self.returning && self.transition >= 1.0 {
            camera.auto_rotate = self.auto_rotate;
            (p.doppler_enabled, p.doppler_strength) = self.doppler;
            (p.jets_enabled, p.jets_strength) = self.jets;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chapter_stages_an_animated_observable_and_holds_the_final_shot() {
        for (index, chapter) in CHAPTERS.iter().enumerate() {
            let mut camera = OrbitCamera::default();
            let mut p = BlackHoleParams::default();
            let mut player = Playback::new(&mut camera, &p, SceneFraming::default());
            player.begin(index, &camera, &p);
            player.advance(SHOT_CHANGE, &mut camera, &mut p);
            let reading = |camera: &OrbitCamera, p: &BlackHoleParams| {
                chapter
                    .reading(camera, p, KerrGeometry::from_spin(p.spin))
                    .1
            };
            let first = reading(&camera, &p);
            player.advance(DEMO_TIME * 0.5, &mut camera, &mut p);
            let middle = reading(&camera, &p);
            player.advance(DEMO_TIME * 0.5, &mut camera, &mut p);
            let end = reading(&camera, &p);
            assert!(
                first != middle && middle != end && first != end,
                "{}",
                chapter.title
            );
            assert!(!player.playing);
            player.advance(2.0, &mut camera, &mut p);
            assert_eq!(reading(&camera, &p), end);
        }
    }

    #[test]
    fn retarget_replay_and_exit_start_at_the_live_view() {
        let mut camera = OrbitCamera {
            distance: 39.0,
            pitch: -0.4,
            yaw: 3.1,
            fov: 0.7,
            ..Default::default()
        };
        let original = camera;
        let mut p = BlackHoleParams {
            spin: 0.63,
            jets_enabled: false,
            jets_strength: 2.4,
            doppler_enabled: false,
            doppler_strength: 0.39,
            render_scale: 0.43,
            disk_tilt: 0.3,
            disk_rotation_speed: 0.0,
            arm_count: 0.0,
            ..Default::default()
        };
        let mut s = Playback::new(&mut camera, &p, SceneFraming::default());
        s.advance(SHOT_CHANGE * 0.4, &mut camera, &mut p);
        let intermediate = camera;
        assert!(camera.distance > original.distance && camera.distance < 48.0);
        s.begin(8, &camera, &p);
        s.advance(0.0, &mut camera, &mut p);
        assert!(camera == intermediate);
        s.advance(SHOT_CHANGE + DEMO_TIME * 0.5, &mut camera, &mut p);
        let halfway = p.spin;
        assert!((halfway - 0.475).abs() < 1e-5);
        s.playing = false;
        s.advance(3.0, &mut camera, &mut p);
        assert_eq!(p.spin, halfway);

        camera.distance = 27.0;
        camera.yaw = -3.1;
        s.advance(0.02, &mut camera, &mut p);
        s.scrub(0.8, &camera);
        s.advance(0.0, &mut camera, &mut p);
        assert_eq!(camera.distance, 27.0);
        let scrubbed = p.spin;
        assert!(scrubbed > halfway);
        s.begin(8, &camera, &p);
        s.advance(SHOT_CHANGE * 0.5, &mut camera, &mut p);
        assert!(p.spin > 0.0 && p.spin < scrubbed);
        assert!(camera.distance < 27.0 && camera.distance > 19.0);
        let before_exit = camera;
        s.exit(&camera, &p);
        assert!(!s.advance(0.0, &mut camera, &mut p));
        assert!(camera == before_exit);
        assert!(!s.advance(RETURN_TIME * 0.5, &mut camera, &mut p));
        assert!(camera.distance > before_exit.distance && camera.distance < original.distance);
        assert!(camera.yaw.abs() > 3.0);
        assert!(s.advance(RETURN_TIME, &mut camera, &mut p));
        assert!(camera == original);
        assert_eq!(p.spin, 0.63);
        assert_eq!((p.doppler_enabled, p.doppler_strength), (false, 0.39));
        assert_eq!((p.jets_enabled, p.jets_strength), (false, 2.4));
        assert_eq!(
            (p.disk_tilt, p.disk_rotation_speed, p.arm_count),
            (0.3, 0.0, 0.0)
        );
        assert_eq!(p.render_scale, 0.43);
    }
}
