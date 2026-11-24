//! Equatorial Kerr test-particle and light trajectories in Boyer–Lindquist coordinates.

use std::f64::consts::TAU;

const MASS: f64 = 0.5;
const HORIZON_MARGIN: f64 = 0.004;
const ESCAPE_RADIUS: f64 = 120.0;
const TIME_LIMIT: f64 = 600.0;
const MAX_STEPS: usize = 24_000;
const MAX_TIME_STEP: f64 = 0.75;
const MIN_STEP: f64 = 1e-9;
const TOLERANCE: f64 = 1e-9;
const SOLAR_LIGHT_CROSSING_SECONDS: f64 = 9.850_981_9e-6;

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
        time * self.solar_masses * SOLAR_LIGHT_CROSSING_SECONDS
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

pub fn horizon_radius(spin: f64) -> f64 {
    MASS * (1.0 + (1.0 - spin * spin).max(0.0).sqrt())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    HorizonLimit,
    Escaped,
    TimeLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegrationError {
    InvalidLaunch,
    StepLimit,
    NumericalFailure,
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub radius: f64,
    pub azimuth: f64,
    pub coordinate_time: f64,
    pub proper_time: Option<f64>,
}

impl Sample {
    pub fn xy(self) -> [f64; 2] {
        [
            self.radius * self.azimuth.cos(),
            self.radius * self.azimuth.sin(),
        ]
    }

    fn interpolate(self, next: Self, time: f64) -> Self {
        let fraction =
            (time - self.coordinate_time) / (next.coordinate_time - self.coordinate_time);
        Self {
            radius: self.radius + fraction * (next.radius - self.radius),
            azimuth: self.azimuth + fraction * (next.azimuth - self.azimuth),
            coordinate_time: time,
            proper_time: self
                .proper_time
                .zip(next.proper_time)
                .map(|(a, b)| a + fraction * (b - a)),
        }
    }
}

#[derive(Debug)]
pub struct Trajectory {
    launch: Launch,
    samples: Vec<Sample>,
    outcome: Outcome,
    closest_radius: f64,
}

impl Trajectory {
    pub fn launch(&self) -> Launch {
        self.launch
    }

    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn end(&self) -> Sample {
        self.samples[self.samples.len() - 1]
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    pub fn closest_radius(&self) -> f64 {
        self.closest_radius
    }

    /// Sample at a shared coordinate time, holding the initial and final positions outside the run.
    pub fn at(&self, time: f64) -> Sample {
        let i = self
            .samples
            .partition_point(|sample| sample.coordinate_time < time);
        match i {
            0 => self.samples[0],
            i if i == self.samples.len() => self.end(),
            i => self.samples[i - 1].interpolate(self.samples[i], time),
        }
    }
}

// Lapse, local frame rotation, and azimuthal length per radian at the equator.
fn local_frame(r: f64, a: f64) -> (f64, f64, f64) {
    let r2 = r * r;
    let delta = r2 - 2.0 * MASS * r + a * a;
    let big_a = (r2 + a * a).powi(2) - a * a * delta;
    (
        r * (delta / big_a).sqrt(),
        2.0 * MASS * a * r / big_a,
        big_a.sqrt() / r,
    )
}

// Radius, radial velocity, azimuth and coordinate time. The integration
// parameter is proper time for a spacecraft and affine time for light.
type State = [f64; 4];

struct Geodesic {
    a: f64,
    energy: f64,
    angular_momentum: f64,
    mu_squared: f64,
}

impl Geodesic {
    fn start(launch: Launch) -> (Self, State) {
        let a = launch.spin * MASS;
        let r = launch.radius;
        let (lapse, omega, azimuth_scale) = local_frame(r, a);
        let (local_time, local_speed, mu_squared) = match launch.kind {
            ProbeKind::Light => (1.0, 1.0, 0.0),
            ProbeKind::Spacecraft => {
                let gamma = (1.0 - launch.speed * launch.speed).sqrt().recip();
                (gamma, launch.speed, 1.0)
            }
        };
        let (sin, cos) = launch.aim.sin_cos();
        let angular_momentum = azimuth_scale * local_time * local_speed * sin;
        let energy = lapse * local_time + omega * angular_momentum;
        let delta = r * r - 2.0 * MASS * r + a * a;
        let radial_velocity = delta.sqrt() / r * local_time * local_speed * cos;
        (
            Self {
                a,
                energy,
                angular_momentum,
                mu_squared,
            },
            [r, radial_velocity, launch.bearing.rem_euclid(TAU), 0.0],
        )
    }

    fn derivative(&self, [r, v, _, _]: State) -> State {
        // Carter's separated equatorial equations. Differentiate R/r⁴ to
        // advance v through turning points without choosing a square-root sign.
        let r2 = r * r;
        let r4 = r2 * r2;
        let a = self.a;
        let e = self.energy;
        let l_a_e = self.angular_momentum - a * e;
        let delta = r2 - 2.0 * MASS * r + a * a;
        let p = r2 * e - a * l_a_e;
        let q = self.mu_squared * r2 + l_a_e * l_a_e;
        let radial = p * p - delta * q;
        let radial_derivative =
            4.0 * r * e * p - 2.0 * (r - MASS) * q - 2.0 * self.mu_squared * r * delta;
        [
            v,
            (radial_derivative - 4.0 * radial / r) / (2.0 * r4),
            (l_a_e + a * p / delta) / r2,
            (a * l_a_e + (r2 + a * a) * p / delta) / r2,
        ]
    }

    fn rk4(&self, state: State, h: f64) -> State {
        let add = |slope: State, step| std::array::from_fn(|i| state[i] + step * slope[i]);
        let k1 = self.derivative(state);
        let k2 = self.derivative(add(k1, h * 0.5));
        let k3 = self.derivative(add(k2, h * 0.5));
        let k4 = self.derivative(add(k3, h));
        std::array::from_fn(|i| state[i] + h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]))
    }
}

pub fn integrate(launch: Launch) -> Result<Trajectory, IntegrationError> {
    integrate_with_budget(launch, MAX_STEPS)
}

fn integrate_with_budget(launch: Launch, max_steps: usize) -> Result<Trajectory, IntegrationError> {
    if !launch.valid() {
        return Err(IntegrationError::InvalidLaunch);
    }

    let (equations, mut state) = Geodesic::start(launch);
    let horizon = horizon_radius(launch.spin);
    let capture = horizon + HORIZON_MARGIN;
    let mut elapsed = 0.0;
    let mut closest_radius = launch.radius;
    let sample = |state: State, elapsed| Sample {
        radius: state[0],
        azimuth: state[2],
        coordinate_time: state[3],
        proper_time: (launch.kind == ProbeKind::Spacecraft).then_some(elapsed),
    };
    let mut samples = vec![sample(state, elapsed)];
    let mut step = 0.1_f64;

    for _ in 0..max_steps {
        let derivative = equations.derivative(state);
        // Resolve the final approach and limit the interval between replay samples.
        let radial_step = (state[0] - horizon) * 0.08 / derivative[0].abs().max(1e-6);
        let clock_step = MAX_TIME_STEP / derivative[3];
        step = step.min(radial_step).min(clock_step);
        if !step.is_finite() || step < MIN_STEP {
            return Err(IntegrationError::NumericalFailure);
        }
        let full = equations.rk4(state, step);
        let half = equations.rk4(equations.rk4(state, step * 0.5), step * 0.5);
        if !full.into_iter().chain(half).all(f64::is_finite)
            || half[0] <= horizon
            || half[3] <= state[3]
        {
            step *= 0.25;
            continue;
        }
        let error = (0..state.len())
            .map(|i| (half[i] - full[i]).abs() / (1.0 + (half[i] - state[i]).abs()))
            .fold(0.0_f64, f64::max);
        let growth = (0.9 * (TOLERANCE / error).powf(0.2)).clamp(0.2, 2.0);
        if error > TOLERANCE {
            step *= growth;
            continue;
        }

        // Take the first termination event within the accepted step.
        let mut terminal = (1.0, None);
        let mut stop_at = |i: usize, boundary: f64, outcome| {
            let fraction = (boundary - state[i]) / (half[i] - state[i]);
            if fraction <= terminal.0 {
                terminal = (fraction, Some(outcome));
            }
        };
        if half[0] <= capture {
            stop_at(0, capture, Outcome::HorizonLimit);
        }
        if half[0] >= ESCAPE_RADIUS
            && state[0] < ESCAPE_RADIUS
            && half[1] > 0.0
            && equations.energy > equations.mu_squared
        {
            stop_at(0, ESCAPE_RADIUS, Outcome::Escaped);
        }
        if half[3] >= TIME_LIMIT {
            stop_at(3, TIME_LIMIT, Outcome::TimeLimit);
        }
        let (fraction, outcome) = terminal;
        state = std::array::from_fn(|i| state[i] + fraction * (half[i] - state[i]));
        elapsed += fraction * step;
        closest_radius = closest_radius.min(state[0]);
        samples.push(sample(state, elapsed));
        if let Some(outcome) = outcome {
            return Ok(Trajectory {
                launch,
                samples,
                outcome,
                closest_radius,
            });
        }
        step *= growth;
    }
    Err(IntegrationError::StepLimit)
}

#[cfg(test)]
mod tests {
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
