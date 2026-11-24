//! First encounters on equatorial, marginally bound Schwarzschild geodesics.
//! Radii and coordinate times are in units of r_s and r_s/c, respectively.

use std::ops::RangeInclusive;

use super::geodesic::{self, Crossing, Geodesic, HORIZON_MARGIN, Limits, Outcome as Stop};
pub use super::geodesic::{Path, Sample};

pub const HORIZON: f64 = 1.0;
pub const CRITICAL_RADIUS: f64 = 2.0;
pub const DEBRIS_COUNT: usize = 48;
const CRITICAL_MOMENTUM: f64 = 2.0;
const CRITICAL_MARGIN: f64 = 0.01;
const SOLAR_RADIUS_IN_SOLAR_RS: f64 = 235_564.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    pub black_hole_mass: f64,
    pub star_mass: f64,
    pub star_radius: f64,
    /// Target penetration depth, even if the orbit can't turn at that radius.
    pub beta: f64,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            black_hole_mass: 1e7,
            star_mass: 1.0,
            star_radius: 1.0,
            beta: 1.7,
        }
    }
}

impl Input {
    pub const BLACK_HOLE_MASS: RangeInclusive<f64> = 1e5..=1e10;
    pub const STAR_MASS: RangeInclusive<f64> = 0.1..=30.0;
    pub const STAR_RADIUS: RangeInclusive<f64> = 0.1..=50.0;
    pub const BETA: RangeInclusive<f64> = 0.1..=10.0;

    pub fn valid(self) -> bool {
        Self::BLACK_HOLE_MASS.contains(&self.black_hole_mass)
            && Self::STAR_MASS.contains(&self.star_mass)
            && Self::STAR_RADIUS.contains(&self.star_radius)
            && Self::BETA.contains(&self.beta)
    }

    pub fn tidal_radius(self) -> f64 {
        self.radius() * (self.black_hole_mass / self.star_mass).cbrt()
    }

    pub fn seconds(self, coordinate_time: f64) -> f64 {
        geodesic::seconds(self.black_hole_mass, coordinate_time)
    }

    fn radius(self) -> f64 {
        self.star_radius * SOLAR_RADIUS_IN_SOLAR_RS / self.black_hole_mass
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    DistantPass,
    GrazingPass,
    Disrupted,
    DirectCapture,
    Critical,
    Unresolved,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Events {
    pub approach: Option<f64>,
    /// Periapsis for a scattering orbit, or the integration limit for capture or the critical orbit.
    pub encounter: Option<Sample>,
    pub disruption: Option<Sample>,
    pub after: Option<f64>,
}

#[derive(Debug)]
pub struct Pass {
    pub target_radius: f64,
    pub launch_aim: f64,
    pub outcome: Outcome,
    pub events: Events,
    pub path: Option<Path>,
    debris: Option<Vec<Path>>,
}

impl Pass {
    pub fn nominal_target(&self) -> bool {
        self.target_radius <= CRITICAL_RADIUS || self.outcome == Outcome::Critical
    }

    /// Present only for a disruption whose debris could not be integrated.
    pub fn debris_unresolved(&self) -> bool {
        self.outcome == Outcome::Disrupted && self.debris.is_none()
    }

    /// The debris is released simultaneously in Schwarzschild coordinate time.
    /// Captured fragments cease to appear once they reach the horizon limit.
    pub fn fragments_at(&self, time: f64) -> impl Iterator<Item = Sample> + '_ {
        self.active_fragments(time).map(move |path| path.at(time))
    }

    pub fn fragment_trails_at(
        &self,
        time: f64,
        tail_time: f64,
    ) -> impl Iterator<Item = (Sample, Sample)> + '_ {
        self.active_fragments(time)
            .map(move |path| (path.at(time), path.at(tail_time.min(time))))
    }

    fn active_fragments(&self, time: f64) -> impl Iterator<Item = &Path> + '_ {
        self.debris
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(move |path| {
                time >= path.samples()[0].coordinate_time
                    && (path.outcome != Stop::HorizonLimit || time <= path.end().coordinate_time)
            })
    }
}

#[derive(Debug)]
pub struct Experiment {
    pub input: Input,
    pub launch_radius: f64,
    pub passes: Vec<Pass>,
    pub duration: f64,
}

impl Experiment {
    pub fn four_passes() -> Self {
        let input = Input::default();
        let rt = input.tidal_radius();
        Self::run(input, &[1.7 * rt, 1.12 * rt, rt / input.beta, 1.45])
    }

    pub fn custom(input: Input) -> Option<Self> {
        input
            .valid()
            .then(|| Self::run(input, &[input.tidal_radius() / input.beta]))
    }

    pub fn launch_speed(&self) -> f64 {
        self.launch_radius.sqrt().recip()
    }

    fn run(input: Input, targets: &[f64]) -> Self {
        let rt = input.tidal_radius();
        let outer_target = targets.iter().copied().fold(rt, f64::max);
        let launch_radius = (1.7 * outer_target).max(16.0);
        let flight_scale = launch_radius.powf(1.5);
        let limits = Limits {
            inner_radius: HORIZON + HORIZON_MARGIN,
            outer_radius: Some(launch_radius),
            end_time: 4.0 * flight_scale + 240.0,
            replay_step: (flight_scale / 500.0).max(0.12),
            max_steps: 24_000,
            inbound_radius: Some(rt),
        };
        let mut passes: Vec<_> = targets
            .iter()
            .map(|&target| integrate_pass(target, rt, launch_radius, &limits))
            .collect();
        let duration = passes
            .iter()
            .filter_map(|pass| pass.path.as_ref().map(Path::end))
            .map(|end| end.coordinate_time)
            .chain(passes.iter().filter_map(|pass| pass.events.after))
            .fold(0.0, f64::max);

        for pass in &mut passes {
            if let Some(crossing) = pass
                .events
                .disruption
                .and_then(|_| pass.path.as_ref()?.inbound_crossing.as_ref())
            {
                pass.debris = integrate_debris(input.radius(), crossing, duration, &limits);
            }
        }
        Self {
            input,
            launch_radius,
            passes,
            duration,
        }
    }
}

fn angular_momentum(target: f64) -> f64 {
    if target >= CRITICAL_RADIUS {
        target / (target - HORIZON).sqrt()
    } else {
        // Continue from the marginally bound orbit with a progressively more radial entry.
        CRITICAL_MOMENTUM * target / CRITICAL_RADIUS
    }
}

fn integrate_pass(target: f64, rt: f64, launch_radius: f64, limits: &Limits) -> Pass {
    let critical = (target - CRITICAL_RADIUS).abs() < 1e-10;
    let momentum = if critical {
        CRITICAL_MOMENTUM
    } else {
        angular_momentum(target)
    };
    let (equations, state) = Geodesic::parabolic(launch_radius, momentum);
    // E = 1 gives |v| = sqrt(r_s/r) for a local static observer.
    let sin_aim = momentum * ((1.0 - HORIZON / launch_radius) / launch_radius).sqrt();
    let launch_aim = std::f64::consts::PI - sin_aim.asin();
    let path = geodesic::integrate(
        equations,
        state,
        HORIZON,
        Limits {
            inner_radius: if critical {
                CRITICAL_RADIUS + CRITICAL_MARGIN
            } else {
                limits.inner_radius
            },
            ..*limits
        },
    )
    .ok();
    let mut pass = Pass {
        target_radius: target,
        launch_aim,
        outcome: Outcome::Unresolved,
        events: Events::default(),
        path,
        debris: None,
    };
    if let Some(path) = &pass.path {
        let (outcome, encounter) = match path.outcome {
            Stop::HorizonLimit if critical => (Outcome::Critical, path.end()),
            Stop::HorizonLimit if momentum < CRITICAL_MOMENTUM => {
                (Outcome::DirectCapture, path.end())
            }
            Stop::Escaped => {
                let Some(periapsis) = path.periapsis else {
                    return pass;
                };
                if (periapsis.radius - target).abs() > 1e-5 * target
                    || momentum <= CRITICAL_MOMENTUM
                {
                    return pass;
                }
                // Once the numerical turn is verified, use its analytic radius
                // for the exact boundaries at r_t and 1.25 r_t.
                let outcome = if target < rt {
                    if path.inbound_crossing.is_none() {
                        return pass;
                    }
                    Outcome::Disrupted
                } else if target <= 1.25 * rt {
                    Outcome::GrazingPass
                } else {
                    Outcome::DistantPass
                };
                (outcome, periapsis)
            }
            _ => return pass,
        };
        let disruption = path
            .inbound_crossing
            .as_ref()
            .filter(|_| outcome == Outcome::Disrupted)
            .map(|crossing| crossing.sample);
        let t = encounter.coordinate_time;
        let after = match outcome {
            Outcome::Critical => None,
            Outcome::DirectCapture => Some(t + 16.0),
            Outcome::Disrupted => Some(t + (2.5 * rt.powf(1.5)).max(24.0)),
            _ => Some(t + (path.end().coordinate_time - t) * 0.65),
        };
        pass.outcome = outcome;
        pass.events = Events {
            approach: Some(disruption.map_or(t, |event| event.coordinate_time) * 0.72),
            encounter: Some(encounter),
            disruption,
            after,
        };
    }
    pass
}

fn integrate_debris(
    star_radius: f64,
    crossing: &Crossing,
    end_time: f64,
    limits: &Limits,
) -> Option<Vec<Path>> {
    let Sample {
        radius,
        azimuth,
        coordinate_time,
        ..
    } = crossing.sample;
    // Convert offsets along the star's local radial diameter into coordinate
    // offsets using the Schwarzschild radial metric at its center.
    let coordinate_radius = star_radius * (1.0 - HORIZON / radius).sqrt();
    (0..DEBRIS_COUNT)
        .map(|i| {
            let offset = 2.0 * i as f64 / (DEBRIS_COUNT - 1) as f64 - 1.0;
            let (equations, state) = Geodesic::from_local(
                0.0,
                radius + offset * coordinate_radius,
                azimuth,
                coordinate_time,
                Some(crossing.speed),
                crossing.aim,
            );
            geodesic::integrate(
                equations,
                state,
                HORIZON,
                Limits {
                    outer_radius: None,
                    end_time,
                    inbound_radius: None,
                    ..*limits
                },
            )
            .ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_limit_is_an_unresolved_encounter() {
        let pass = integrate_pass(
            4.0,
            5.0,
            16.0,
            &Limits {
                inner_radius: HORIZON + HORIZON_MARGIN,
                outer_radius: Some(16.0),
                end_time: 1.0,
                replay_step: 0.12,
                max_steps: 100,
                inbound_radius: Some(5.0),
            },
        );
        assert_eq!(pass.path.as_ref().unwrap().outcome, Stop::TimeLimit);
        assert_eq!(pass.outcome, Outcome::Unresolved);
        assert!(pass.events.encounter.is_none());
        assert!(pass.events.disruption.is_none());
    }
}
