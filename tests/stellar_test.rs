use singularity_rs::physics::stellar::{
    CRITICAL_RADIUS, DEBRIS_COUNT, Experiment, HORIZON, Input, Outcome,
};

fn custom(input: Input) -> Experiment {
    Experiment::custom(input).expect("valid stellar input")
}

#[test]
fn four_passes_share_launch_and_have_distinct_outcomes() {
    let experiment = Experiment::four_passes();
    let expected = [
        Outcome::DistantPass,
        Outcome::GrazingPass,
        Outcome::Disrupted,
        Outcome::DirectCapture,
    ];
    let mut previous_aim = 0.0;
    for (pass, expected) in experiment.passes.iter().zip(expected) {
        assert_eq!(pass.outcome, expected);
        assert!(pass.launch_aim > previous_aim);
        previous_aim = pass.launch_aim;
        let path = pass.path.as_ref().unwrap();
        let launch = path.samples()[0];
        assert!((launch.radius - experiment.launch_radius).abs() < 1e-12);
        assert!(launch.azimuth.abs() < 1e-12);
        assert!(launch.coordinate_time.abs() < 1e-12);
        assert!(
            path.samples()
                .windows(2)
                .all(|w| w[0].coordinate_time < w[1].coordinate_time)
        );

        let encounter = pass.events.encounter.unwrap();
        assert!(pass.events.approach.unwrap() < encounter.coordinate_time);
        assert!(pass.events.after.unwrap() > encounter.coordinate_time);
        assert!(pass.events.after.unwrap() <= experiment.duration);
        if pass.nominal_target() {
            assert!(encounter.radius < 1.01 && encounter.radius > HORIZON);
            assert!(encounter.radius < pass.target_radius);
        } else {
            assert!((encounter.radius - pass.target_radius).abs() < 1e-6);
        }
    }
    assert!((experiment.launch_speed() - experiment.launch_radius.sqrt().recip()).abs() < 1e-12);
}

#[test]
fn tides_scale_with_stellar_density_and_black_hole_mass() {
    let input = Input::default();
    let rt = input.tidal_radius();
    assert!((5.0..5.1).contains(&rt));
    let ratio = |variant: Input| variant.tidal_radius() / rt;
    assert!(
        (ratio(Input {
            black_hole_mass: input.black_hole_mass * 8.0,
            ..input
        }) - 0.25)
            .abs()
            < 1e-12
    );
    assert!(
        (ratio(Input {
            star_mass: input.star_mass * 8.0,
            ..input
        }) - 0.5)
            .abs()
            < 1e-12
    );
    assert!(
        (ratio(Input {
            star_radius: input.star_radius * 2.0,
            ..input
        }) - 2.0)
            .abs()
            < 1e-12
    );
    let heavier = Input {
        black_hole_mass: input.black_hole_mass * 2.0,
        ..input
    };
    assert!((heavier.seconds(100.0) / input.seconds(100.0) - 2.0).abs() < 1e-12);
}

#[test]
fn classifications_include_the_grazing_boundary_and_capture_separatrix() {
    let input = Input::default();
    let rt = input.tidal_radius();
    for (target, outcome) in [
        (1.26 * rt, Outcome::DistantPass),
        (1.25 * rt, Outcome::GrazingPass),
        (rt, Outcome::GrazingPass),
        (0.9999 * rt, Outcome::Disrupted),
        (2.01, Outcome::Disrupted),
        (CRITICAL_RADIUS, Outcome::Critical),
        (1.99, Outcome::DirectCapture),
    ] {
        let experiment = custom(Input {
            beta: rt / target,
            ..input
        });
        let pass = &experiment.passes[0];
        assert_eq!(pass.outcome, outcome, "target {target}");
        if outcome == Outcome::Critical {
            assert!(pass.nominal_target());
            assert!(pass.events.after.is_none());
            assert!(pass.events.disruption.is_none());
            assert!((pass.events.encounter.unwrap().radius - 2.01).abs() < 1e-8);
        }
    }
}

#[test]
fn massive_hole_captures_a_solar_star_even_when_the_target_is_outside_the_horizon() {
    let experiment = custom(Input {
        black_hole_mass: 8e7,
        beta: 1.0,
        ..Input::default()
    });
    let pass = &experiment.passes[0];
    assert!((HORIZON..CRITICAL_RADIUS).contains(&pass.target_radius));
    assert_eq!(pass.outcome, Outcome::DirectCapture);
    assert!(pass.nominal_target());
    assert!(pass.events.disruption.is_none());
    assert_eq!(pass.fragments_at(experiment.duration).count(), 0);
}

#[test]
fn debris_is_released_at_the_tidal_crossing_and_stretches_after_periapsis() {
    let experiment = custom(Input::default());
    let pass = &experiment.passes[0];
    assert_eq!(pass.outcome, Outcome::Disrupted);
    assert!(!pass.debris_unresolved());
    let crossing = pass.events.disruption.unwrap();
    let closest = pass.events.encounter.unwrap();
    assert!((crossing.radius - experiment.input.tidal_radius()).abs() < 1e-8);
    assert!(crossing.coordinate_time < closest.coordinate_time);
    assert_eq!(
        pass.fragments_at(crossing.coordinate_time - 1e-5).count(),
        0
    );
    let initial: Vec<_> = pass.fragments_at(crossing.coordinate_time).collect();
    let later: Vec<_> = pass.fragments_at(pass.events.after.unwrap()).collect();
    assert_eq!(initial.len(), DEBRIS_COUNT);
    assert_eq!(later.len(), DEBRIS_COUNT);
    assert!(initial.windows(2).all(|w| w[0].radius < w[1].radius));
    assert!(
        initial
            .iter()
            .all(|sample| (sample.azimuth - crossing.azimuth).abs() < 1e-12)
    );
    let length = |samples: &[singularity_rs::physics::stellar::Sample]| {
        let [ax, ay] = samples.first().unwrap().xy();
        let [bx, by] = samples.last().unwrap().xy();
        (ax - bx).hypot(ay - by)
    };
    assert!(length(&later) > 5.0 * length(&initial));
    assert!((later[0].azimuth - later[DEBRIS_COUNT - 1].azimuth).abs() > 0.02);
}

#[test]
fn custom_launch_expands_with_both_tide_and_shallow_target() {
    let large = Input {
        black_hole_mass: 1e5,
        star_radius: 50.0,
        star_mass: 0.1,
        ..Input::default()
    };
    for (beta, outcome) in [(0.1, Outcome::DistantPass), (1.3, Outcome::Disrupted)] {
        let experiment = custom(Input { beta, ..large });
        let pass = &experiment.passes[0];
        assert_eq!(pass.outcome, outcome);
        assert!(experiment.launch_radius > large.tidal_radius().max(pass.target_radius));
        assert!(!pass.debris_unresolved());
    }
    assert!(
        Experiment::custom(Input {
            beta: f64::NAN,
            ..large
        })
        .is_none()
    );
}
