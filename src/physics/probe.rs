//! Equatorial Kerr test-particle and light trajectories in Boyer–Lindquist coordinates.

pub use super::geodesic::{IntegrationError, Outcome, Sample, horizon_radius};

use super::geodesic::{self, Geodesic, HORIZON_MARGIN, Limits, MASS, Path, local_frame};

const ESCAPE_RADIUS: f64 = 120.0;
const TIME_LIMIT: f64 = 600.0;
const MAX_STEPS: usize = 24_000;
const MAX_TIME_STEP: f64 = 0.75;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProbeKind {
    #[default]
    Light,
    Spacecraft,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Launch {
    pub kind: ProbeKind,
    pub solar_masses: f64,
    pub spin: f64,
    pub radius: f64,
    /// Azimuth in radians around the spin axis.
    pub bearing: f64,
    /// Local heading in radians, measured from radially outward toward increasing azimuth.
    pub aim: f64,
    /// Local speed in units of c, used for spacecraft only.
    pub speed: f64,
}

impl Default for Launch {
    fn default() -> Self {
        Self {
            kind: ProbeKind::Light,
            solar_masses: 4_300_000.0,
            spin: 0.0,
            radius: 12.0,
            bearing: 0.0,
            aim: 170.0_f64.to_radians(),
            speed: 0.0,
        }
    }
}

impl Launch {
    pub const MAX_SPIN: f64 = 0.998;
    pub const MAX_START_RADIUS: f64 = 60.0;
    pub const MAX_SPEED: f64 = 0.999;
    pub const MAX_SOLAR_MASSES: f64 = 1e11;

    pub fn min_start_radius(spin: f64) -> f64 {
        horizon_radius(spin) + 0.02
    }

    pub fn seconds(self, time: f64) -> f64 {
        geodesic::seconds(self.solar_masses, time)
    }

    /// Local speed for a positive-azimuth circular orbit, where one exists.
    pub(crate) fn circular_speed(r: f64, spin: f64) -> f64 {
        let a = spin * MASS;
        let (lapse, omega, azimuth_scale) = local_frame(r, a);
        let orbital = MASS.sqrt() / (r.powf(1.5) + a * MASS.sqrt());
        (orbital - omega) * azimuth_scale / lapse
    }

    fn valid(self) -> bool {
        (-Self::MAX_SPIN..=Self::MAX_SPIN).contains(&self.spin)
            && (Self::min_start_radius(self.spin)..=Self::MAX_START_RADIUS).contains(&self.radius)
            && (1.0..=Self::MAX_SOLAR_MASSES).contains(&self.solar_masses)
            && self.bearing.is_finite()
            && self.aim.is_finite()
            && (self.kind == ProbeKind::Light || (0.0..=Self::MAX_SPEED).contains(&self.speed))
    }
}

#[derive(Debug)]
pub struct Trajectory {
    launch: Launch,
    path: Path,
}

impl Trajectory {
    pub fn launch(&self) -> Launch {
        self.launch
    }

    pub fn samples(&self) -> &[Sample] {
        self.path.samples()
    }

    pub fn end(&self) -> Sample {
        self.path.end()
    }

    pub fn outcome(&self) -> Outcome {
        self.path.outcome
    }

    pub fn closest_radius(&self) -> f64 {
        self.path.closest_radius
    }

    /// Sample at a shared coordinate time, holding the initial and final positions outside the run.
    pub fn at(&self, time: f64) -> Sample {
        self.path.at(time)
    }
}

pub fn integrate(launch: Launch) -> Result<Trajectory, IntegrationError> {
    integrate_with_budget(launch, MAX_STEPS)
}

fn integrate_with_budget(launch: Launch, max_steps: usize) -> Result<Trajectory, IntegrationError> {
    if !launch.valid() {
        return Err(IntegrationError::InvalidLaunch);
    }

    let (equations, state) = Geodesic::from_local(
        launch.spin,
        launch.radius,
        launch.bearing,
        0.0,
        (launch.kind == ProbeKind::Spacecraft).then_some(launch.speed),
        launch.aim,
    );
    let horizon = horizon_radius(launch.spin);
    let limits = Limits {
        inner_radius: horizon + HORIZON_MARGIN,
        outer_radius: equations.unbound().then_some(ESCAPE_RADIUS),
        end_time: TIME_LIMIT,
        replay_step: MAX_TIME_STEP,
        max_steps,
        inbound_radius: None,
    };
    let path = geodesic::integrate(equations, state, horizon, limits)?;
    Ok(Trajectory { launch, path })
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use super::*;

    #[test]
    fn eight_degrees_separate_capture_from_escape() {
        let launch = Launch::default();
        let captured = integrate(launch).unwrap();
        let escaped = integrate(Launch {
            aim: 162.0_f64.to_radians(),
            ..launch
        })
        .unwrap();
        assert_eq!(captured.outcome(), Outcome::HorizonLimit);
        assert!((captured.end().radius - 1.004).abs() < 1e-10);
        assert_eq!(escaped.outcome(), Outcome::Escaped);
        assert!((escaped.end().radius - 120.0).abs() < 1e-10);
        assert!(escaped.closest_radius() > 3.0 && escaped.closest_radius() < 3.4);
        assert!(
            escaped
                .samples()
                .iter()
                .all(|sample| sample.proper_time.is_none())
        );
        assert!(
            escaped
                .samples()
                .windows(2)
                .all(|pair| pair[1].coordinate_time > pair[0].coordinate_time)
        );
    }

    #[test]
    fn circular_orbits_preserve_radius_and_follow_schwarzschild_clock_rate() {
        let clocks = [4.0, 10.0].map(|radius| {
            let launch = Launch {
                kind: ProbeKind::Spacecraft,
                radius,
                aim: std::f64::consts::FRAC_PI_2,
                speed: Launch::circular_speed(radius, 0.0),
                ..Launch::default()
            };
            let orbit = integrate(launch).unwrap();
            assert_eq!(orbit.outcome(), Outcome::TimeLimit);
            assert!(
                orbit
                    .samples()
                    .iter()
                    .all(|sample| (sample.radius - radius).abs() < 1e-7)
            );
            let sample = orbit.at(199.123);
            let expected = sample.coordinate_time * (1.0 - 1.5 / radius).sqrt();
            let clock = sample.proper_time.unwrap();
            assert!((clock - expected).abs() < 1e-7);
            clock
        });
        assert!(clocks[0] < clocks[1]);
    }

    #[test]
    fn spin_drags_a_zero_angular_momentum_launch_in_either_direction() {
        let launch = Launch {
            kind: ProbeKind::Spacecraft,
            radius: 4.0,
            ..Launch::default()
        };
        let azimuths = [-0.9, 0.0, 0.9].map(|spin| {
            integrate(Launch { spin, ..launch })
                .unwrap()
                .at(8.0)
                .azimuth
        });
        assert!(azimuths[0] < -0.01);
        assert!(azimuths[1].abs() < 1e-12);
        assert!((azimuths[2] + azimuths[0]).abs() < 1e-9);
    }

    #[test]
    fn eccentric_orbit_turns_and_advances_its_periapsis() {
        let orbit = integrate(Launch {
            kind: ProbeKind::Spacecraft,
            radius: 10.0,
            aim: std::f64::consts::FRAC_PI_2,
            speed: 0.22,
            ..Launch::default()
        })
        .unwrap();
        let mut periapses = orbit
            .samples()
            .windows(3)
            .filter(|w| w[1].radius < w[0].radius && w[1].radius < w[2].radius)
            .map(|w| w[1]);
        let first = periapses.next().unwrap();
        let second = periapses.next().unwrap();
        assert!(first.radius > 3.0 && first.radius < 9.0);
        assert!((first.radius - second.radius).abs() < 0.01);
        assert!(second.azimuth - first.azimuth > TAU + 0.2);
    }

    #[test]
    fn maximum_spin_and_speed_can_launch_near_the_horizon() {
        for spin in [-Launch::MAX_SPIN, Launch::MAX_SPIN] {
            let run = integrate(Launch {
                kind: ProbeKind::Spacecraft,
                spin,
                radius: Launch::min_start_radius(spin),
                aim: std::f64::consts::PI,
                speed: Launch::MAX_SPEED,
                ..Launch::default()
            })
            .unwrap();
            assert_eq!(run.outcome(), Outcome::HorizonLimit);
            assert!(run.end().coordinate_time.is_finite());
            assert!(run.end().proper_time.unwrap() > 0.0);
        }
    }

    #[test]
    fn rejects_invalid_launch_and_reports_step_exhaustion() {
        for launch in [
            Launch {
                spin: 1.0,
                ..Launch::default()
            },
            Launch {
                radius: 1.0,
                ..Launch::default()
            },
            Launch {
                solar_masses: 0.0,
                ..Launch::default()
            },
            Launch {
                aim: f64::NAN,
                ..Launch::default()
            },
            Launch {
                kind: ProbeKind::Spacecraft,
                speed: 1.0,
                ..Launch::default()
            },
        ] {
            assert_eq!(
                integrate(launch).unwrap_err(),
                IntegrationError::InvalidLaunch
            );
        }
        assert_eq!(
            integrate_with_budget(Launch::default(), 1).unwrap_err(),
            IntegrationError::StepLimit
        );
    }

    #[test]
    fn mass_scales_seconds_and_leaves_dimensionless_motion_unchanged() {
        let light = Launch::default();
        let massive = Launch {
            solar_masses: light.solar_masses * 2.0,
            ..light
        };
        let a = integrate(light).unwrap().end();
        let b = integrate(massive).unwrap().end();
        assert!((a.coordinate_time - b.coordinate_time).abs() < 1e-12);
        assert!(
            (massive.seconds(b.coordinate_time) / light.seconds(a.coordinate_time) - 2.0).abs()
                < 1e-12
        );
    }
}
