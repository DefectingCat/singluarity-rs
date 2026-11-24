//! Shared equatorial Kerr integrator in units of the Schwarzschild radius.

use std::f64::consts::TAU;

pub(super) const MASS: f64 = 0.5;
const MIN_STEP: f64 = 1e-9;
const TOLERANCE: f64 = 1e-9;
pub(crate) const HORIZON_MARGIN: f64 = 0.004;
const SOLAR_LIGHT_CROSSING_SECONDS: f64 = 9.850_981_9e-6;

pub(crate) fn seconds(solar_masses: f64, coordinate_time: f64) -> f64 {
    coordinate_time * solar_masses * SOLAR_LIGHT_CROSSING_SECONDS
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
pub struct Path {
    samples: Vec<Sample>,
    pub(super) outcome: Outcome,
    pub(super) closest_radius: f64,
    pub(super) periapsis: Option<Sample>,
    pub(super) inbound_crossing: Option<Crossing>,
}

impl Path {
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn end(&self) -> Sample {
        self.samples[self.samples.len() - 1]
    }

    /// Interpolate at a shared coordinate time, holding the endpoints outside the run.
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

#[derive(Debug)]
pub(super) struct Crossing {
    pub sample: Sample,
    pub speed: f64,
    pub aim: f64,
}

pub(super) struct Limits {
    pub inner_radius: f64,
    pub outer_radius: Option<f64>,
    pub end_time: f64,
    pub replay_step: f64,
    pub max_steps: usize,
    pub inbound_radius: Option<f64>,
}

// Lapse, local frame rotation, and azimuthal length per radian at the equator.
pub(super) fn local_frame(r: f64, a: f64) -> (f64, f64, f64) {
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
pub(super) type State = [f64; 4];

pub(super) struct Geodesic {
    a: f64,
    energy: f64,
    angular_momentum: f64,
    mu_squared: f64,
}

impl Geodesic {
    pub(super) fn from_local(
        spin: f64,
        r: f64,
        bearing: f64,
        time: f64,
        speed: Option<f64>,
        aim: f64,
    ) -> (Self, State) {
        let a = spin * MASS;
        let (lapse, omega, azimuth_scale) = local_frame(r, a);
        let (local_time, local_speed, mu_squared) = match speed {
            None => (1.0, 1.0, 0.0),
            Some(speed) => ((1.0 - speed * speed).sqrt().recip(), speed, 1.0),
        };
        let (sin, cos) = aim.sin_cos();
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
            [r, radial_velocity, bearing.rem_euclid(TAU), time],
        )
    }

    pub(super) fn parabolic(r: f64, angular_momentum: f64) -> (Self, State) {
        let radial_velocity =
            -(r.recip() - (1.0 - r.recip()) * (angular_momentum / r).powi(2)).sqrt();
        (
            Self {
                a: 0.0,
                energy: 1.0,
                angular_momentum,
                mu_squared: 1.0,
            },
            [r, radial_velocity, 0.0, 0.0],
        )
    }

    pub(super) fn unbound(&self) -> bool {
        self.energy > self.mu_squared
    }

    fn crossing(&self, sample: Sample, radial_velocity: f64) -> Crossing {
        let (lapse, omega, azimuth_scale) = local_frame(sample.radius, self.a);
        let local_time = (self.energy - omega * self.angular_momentum) / lapse;
        let radial_scale =
            (1.0 - 2.0 * MASS / sample.radius + (self.a / sample.radius).powi(2)).sqrt();
        let radial = radial_velocity / radial_scale;
        let azimuthal = self.angular_momentum / azimuth_scale;
        Crossing {
            sample,
            speed: radial.hypot(azimuthal) / local_time,
            aim: azimuthal.atan2(radial),
        }
    }

    fn derivative(&self, [r, v, _, _]: State) -> State {
        if self.a == 0.0 && self.mu_squared == 1.0 {
            let inv_r = r.recip();
            let angular = self.angular_momentum * inv_r;
            return [
                v,
                inv_r * inv_r * (-MASS + angular * angular * (r - 3.0 * MASS)),
                angular * inv_r,
                self.energy / (1.0 - 2.0 * MASS * inv_r),
            ];
        }
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

impl Geodesic {
    fn advance(&self, state: State, step: f64) -> State {
        self.rk4(self.rk4(state, step * 0.5), step * 0.5)
    }

    // The states bracket an event within a single accepted step. Return its
    // full state so the debris inherits the velocity at the tidal crossing.
    fn locate_event(
        &self,
        state: State,
        step: f64,
        crossed: impl Fn(State) -> bool,
    ) -> (State, f64) {
        let mut lo = 0.0;
        let mut hi = step;
        for _ in 0..32 {
            let mid = (lo + hi) * 0.5;
            if crossed(self.advance(state, mid)) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        (self.advance(state, hi), hi)
    }
}

pub(super) fn integrate(
    equations: Geodesic,
    mut state: State,
    horizon: f64,
    limits: Limits,
) -> Result<Path, IntegrationError> {
    let mut elapsed = 0.0;
    let mut closest_radius = state[0];
    let sample = |state: State, elapsed| Sample {
        radius: state[0],
        azimuth: state[2],
        coordinate_time: state[3],
        proper_time: (equations.mu_squared == 1.0).then_some(elapsed),
    };
    let mut samples = vec![sample(state, elapsed)];
    let mut step = 0.1_f64;
    let mut periapsis = None;
    let mut inbound_crossing = None;

    for _ in 0..limits.max_steps {
        let derivative = equations.derivative(state);
        // Resolve the final approach and limit the interval between replay samples.
        let radial_step = (state[0] - horizon) * 0.08 / derivative[0].abs().max(1e-6);
        let clock_step = limits.replay_step / derivative[3];
        step = step.min(radial_step).min(clock_step);
        if !step.is_finite() || step < MIN_STEP {
            return Err(IntegrationError::NumericalFailure);
        }
        let full = equations.rk4(state, step);
        let half = equations.advance(state, step);
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
        if half[0] <= limits.inner_radius {
            stop_at(0, limits.inner_radius, Outcome::HorizonLimit);
        }
        if let Some(outer) = limits.outer_radius
            && half[0] >= outer
            && state[0] <= outer
            && half[1] > 0.0
        {
            stop_at(0, outer, Outcome::Escaped);
        }
        if half[3] >= limits.end_time {
            stop_at(3, limits.end_time, Outcome::TimeLimit);
        }
        let (fraction, outcome) = terminal;
        let next = std::array::from_fn(|i| state[i] + fraction * (half[i] - state[i]));
        let accepted_step = fraction * step;

        let turn = if limits.inbound_radius.is_some()
            && periapsis.is_none()
            && state[1] < 0.0
            && next[1] >= 0.0
        {
            let turn = equations.locate_event(state, accepted_step, |at| at[1] >= 0.0);
            periapsis = Some(sample(turn.0, elapsed + turn.1));
            Some(turn)
        } else {
            None
        };
        let (inbound_end, inbound_step) = turn.unwrap_or((next, accepted_step));
        if let Some(radius) = limits.inbound_radius
            && inbound_crossing.is_none()
            && state[1] < 0.0
            && state[0] > radius
            && inbound_end[0] <= radius
        {
            let (at, offset) = equations.locate_event(state, inbound_step, |at| at[0] <= radius);
            inbound_crossing = Some(equations.crossing(sample(at, elapsed + offset), at[1]));
        }
        // Keep the exact event positions in the replay. A jump to periapsis
        // then lands on the map's marker, even when replay samples are sparse.
        for event in [
            inbound_crossing.as_ref().map(|crossing| crossing.sample),
            periapsis,
        ]
        .into_iter()
        .flatten()
        {
            if event.coordinate_time > samples.last().unwrap().coordinate_time
                && event.coordinate_time < next[3]
            {
                samples.push(event);
            }
        }

        state = next;
        elapsed += accepted_step;
        closest_radius = closest_radius.min(state[0]);
        samples.push(sample(state, elapsed));
        if let Some(outcome) = outcome {
            return Ok(Path {
                samples,
                outcome,
                closest_radius,
                periapsis,
                inbound_crossing,
            });
        }
        step *= growth;
    }
    Err(IntegrationError::StepLimit)
}
