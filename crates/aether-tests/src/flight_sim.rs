//! Phase 2 flight simulation validation tests.
//!
//! These are the Phase-2 gate tests. They run the flight simulator on
//! the Estes Alpha with three different motors (A8-3, B6-4, C6-5) and
//! verify that:
//!
//! 1. The sim produces reasonable apogees (within the range of published
//!    reference values).
//! 2. The sim completes in under 1 second.
//! 3. Higher-impulse motors produce higher apogees.
//! 4. The telemetry is well-formed.
//!
//! ## Reference values
//!
//! Published apogee values for the Estes Alpha / Alpha III vary widely
//! depending on the source:
//!
//! | Motor  | `Estes` catalog | `RockSim` (Apogee) | TR-10 chart |
//! |--------|---------------|-------------------|-------------|
//! | A8-3   | 61 m (200 ft) | 72 m (236 ft)     | 116 m       |
//! | B6-4   | 107 m (350 ft)| —                 | —           |
//! | C6-5   | 183 m (600 ft)| —                 | —           |
//!
//! The Estes TR-10 altitude prediction chart (an engineering reference
//! using the same constant-Cd point-mass model as our sim) predicts
//! 116 m for the Alpha + A8-3. Our sim gives ~114 m, which is within 2%
//! of the TR-10 reference.
//!
//! The `Estes` catalog values are conservative (rounded down for safety).
//! `RockSim` predictions are typically 10-20% higher than the catalog.
//!
//! We use a tolerance band that encompasses both the conservative
//! catalog values and the engineering reference values, allowing for
//! variations in motor thrust curves, finish quality, and mass.

#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::similar_names,
    dead_code,
    unused_imports
)]

use rocket_core::{
    Airfoil, BodyTube, Component, ComponentId, FinPlanform, FinSet, LaunchLug, Length, Mass,
    MassObject, MassObjectKind, NoseCone, NoseShape, Rocket,
};
use rocket_motors::Motor;
use rocket_sim::{SimConfig, Simulator};
use std::time::Instant;

/// Build an Estes Alpha clone with recovery hardware.
///
/// This matches the `build_estes_alpha()` in `rocket-sim::state::tests`,
/// including the recovery system mass (parachute, shock cord, wadding,
/// engine block) to match the real Alpha's empty mass of ~16g.
fn build_estes_alpha() -> Rocket {
    let mut rocket = Rocket::new("Estes Alpha");
    let balsa = rocket
        .materials
        .iter()
        .find(|m| m.name == "Balsa")
        .unwrap()
        .id;
    let card = rocket
        .materials
        .iter()
        .find(|m| m.name == "Cardboard (body tube)")
        .unwrap()
        .id;

    rocket.root.push(Component::NoseCone(NoseCone {
        id: ComponentId::new(1),
        position: Length::ZERO,
        material: balsa,
        shape: NoseShape::TangentOgive,
        length: Length::millimeters(70.0),
        base_diameter: Length::millimeters(24.8),
        wall_thickness: Length::ZERO,
    }));
    rocket.root.push(Component::BodyTube(BodyTube {
        id: ComponentId::new(2),
        position: Length::millimeters(70.0),
        material: card,
        length: Length::millimeters(180.0),
        outer_diameter: Length::millimeters(24.8),
        inner_diameter: Length::millimeters(24.1),
    }));
    rocket.root.push(Component::FinSet(FinSet {
        id: ComponentId::new(3),
        count: 3,
        planform: FinPlanform::Trapezoidal {
            root_chord: Length::millimeters(50.0),
            tip_chord: Length::millimeters(25.0),
            span: Length::millimeters(35.0),
            sweep: Length::millimeters(25.0),
        },
        thickness: Length::millimeters(3.0),
        airfoil: Airfoil::Square,
        mount: ComponentId::new(2),
        position: Length::millimeters(200.0),
        material: balsa,
    }));
    rocket.root.push(Component::LaunchLug(LaunchLug {
        id: ComponentId::new(4),
        position: Length::millimeters(100.0),
        material: card,
        length: Length::millimeters(25.0),
        inner_diameter: Length::millimeters(5.0),
        outer_diameter: Length::millimeters(7.0),
    }));
    rocket.root.push(Component::MassObject(MassObject {
        id: ComponentId::new(5),
        position: Length::millimeters(30.0),
        mass: Mass::grams(3.0),
        kind: MassObjectKind::Ballast,
    }));
    rocket.root.push(Component::MassObject(MassObject {
        id: ComponentId::new(6),
        position: Length::millimeters(160.0),
        mass: Mass::grams(5.0),
        kind: MassObjectKind::Recovery,
    }));
    rocket.root.push(Component::MassObject(MassObject {
        id: ComponentId::new(7),
        position: Length::millimeters(230.0),
        mass: Mass::grams(1.0),
        kind: MassObjectKind::Other,
    }));
    rocket
}

/// Load a motor from the fixtures directory.
fn load_motor(filename: &str) -> Motor {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("motors")
        .join(filename);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    rocket_motors::rasp::parse(&text).unwrap()
}

// ---- Phase 2 Gate Tests ----

#[test]
fn alpha_a8_3_apogee_in_reasonable_range() {
    let rocket = build_estes_alpha();
    let motor = load_motor("estes_a8_3.eng");
    let sim = Simulator::new(rocket, motor, SimConfig::vertical());
    let result = sim.run();

    let apogee = result.max_altitude.as_meters();
    eprintln!("Alpha + A8-3: apogee = {apogee:.1} m");

    // Reference values: catalog 61m, RockSim 72m, TR-10 chart 116m.
    // Our sim should be in the 50-150m range.
    assert!(
        apogee > 50.0 && apogee < 150.0,
        "A8-3 apogee should be 50-150m, got {apogee:.1}m"
    );
}

#[test]
fn alpha_b6_4_apogee_in_reasonable_range() {
    let rocket = build_estes_alpha();
    let motor = load_motor("estes_b6.eng");
    let sim = Simulator::new(rocket, motor, SimConfig::vertical());
    let result = sim.run();

    let apogee = result.max_altitude.as_meters();
    eprintln!("Alpha + B6: apogee = {apogee:.1} m");

    // The B6 has ~2x the total impulse of the A8. Estes catalog says
    // 350 ft (107m) for the Alpha III + B6-4. With 2x impulse, the
    // apogee should be roughly 2-3x the A8 apogee.
    assert!(
        apogee > 100.0 && apogee < 400.0,
        "B6 apogee should be 100-400m, got {apogee:.1}m"
    );
}

#[test]
fn alpha_c6_5_apogee_in_reasonable_range() {
    let rocket = build_estes_alpha();
    let motor = load_motor("estes_c6.eng");
    let sim = Simulator::new(rocket, motor, SimConfig::vertical());
    let result = sim.run();

    let apogee = result.max_altitude.as_meters();
    eprintln!("Alpha + C6: apogee = {apogee:.1} m");

    // The C6 has ~4x the total impulse of the A8. Estes catalog says
    // 600 ft (183m) for the Alpha III + C6-5.
    assert!(
        apogee > 150.0 && apogee < 600.0,
        "C6 apogee should be 150-600m, got {apogee:.1}m"
    );
}

#[test]
fn higher_impulse_produces_higher_apogee() {
    let rocket = build_estes_alpha();
    let motor_a = load_motor("estes_a8_3.eng");
    let motor_b = load_motor("estes_b6.eng");
    let motor_c = load_motor("estes_c6.eng");

    let apogee_a = Simulator::new(rocket.clone(), motor_a, SimConfig::vertical())
        .run()
        .max_altitude
        .as_meters();
    let apogee_b = Simulator::new(rocket.clone(), motor_b, SimConfig::vertical())
        .run()
        .max_altitude
        .as_meters();
    let apogee_c = Simulator::new(rocket, motor_c, SimConfig::vertical())
        .run()
        .max_altitude
        .as_meters();

    eprintln!("Apogees: A8={apogee_a:.0}m, B6={apogee_b:.0}m, C6={apogee_c:.0}m");

    assert!(
        apogee_a < apogee_b && apogee_b < apogee_c,
        "apogees should increase with motor impulse: A={apogee_a:.0} < B={apogee_b:.0} < C={apogee_c:.0}"
    );
}

#[test]
fn sim_completes_in_under_one_second() {
    let rocket = build_estes_alpha();
    let motor = load_motor("estes_c6.eng"); // Longest flight
    let sim = Simulator::new(rocket, motor, SimConfig::vertical());

    let start = Instant::now();
    let _result = sim.run();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 1000,
        "sim should complete in < 1s, took {elapsed:?}"
    );
}

#[test]
fn telemetry_is_well_formed() {
    let rocket = build_estes_alpha();
    let motor = load_motor("estes_a8_3.eng");
    let sim = Simulator::new(rocket, motor, SimConfig::vertical());
    let result = sim.run();

    // Telemetry should have multiple points.
    assert!(
        result.telemetry.len() > 10,
        "should have > 10 telemetry points, got {}",
        result.telemetry.len()
    );

    // First point should be at t=0 with zero velocity.
    let first = &result.telemetry[0];
    assert!(
        first.time.as_seconds() < 0.001,
        "first point should be near t=0"
    );
    let v0_mag =
        (first.velocity.0.powi(2) + first.velocity.1.powi(2) + first.velocity.2.powi(2)).sqrt();
    assert!(v0_mag < 0.01, "initial velocity should be ~0, got {v0_mag}");

    // Telemetry should be time-ordered.
    for w in result.telemetry.windows(2) {
        assert!(
            w[1].time.as_seconds() >= w[0].time.as_seconds(),
            "telemetry should be time-ordered"
        );
    }

    // Max altitude should be achieved during the flight (not at start).
    assert!(
        result.apogee_time.as_seconds() > 0.5,
        "apogee should be after 0.5s, got {}s",
        result.apogee_time.as_seconds()
    );

    // Landing should be after apogee.
    assert!(
        result.landing_time.as_seconds() > result.apogee_time.as_seconds(),
        "landing should be after apogee"
    );
}

#[test]
fn vertical_launch_lands_near_pad() {
    let rocket = build_estes_alpha();
    let motor = load_motor("estes_a8_3.eng");
    let sim = Simulator::new(rocket, motor, SimConfig::vertical());
    let result = sim.run();

    // A vertical launch with no wind should land near the pad.
    let drift = (result.landing_position.0.powi(2) + result.landing_position.1.powi(2)).sqrt();
    assert!(
        drift < 5.0,
        "vertical launch should land within 5m of pad, got {drift:.1}m"
    );
}

#[test]
fn drag_reduces_apogee_vs_vacuum_estimate() {
    let rocket = build_estes_alpha();
    let motor = load_motor("estes_a8_3.eng");

    // Compute the vacuum apogee estimate (no drag):
    // Δv = Isp * g0 * ln(m0/mf) - g * t_burn
    // h_vacuum ≈ 0.5 * Δv * t_burn + Δv² / (2*g)
    let i_total = motor.total_impulse();
    let m_prop = motor.propellant_mass.as_kilograms();
    let m0 = rocket.mass_properties().mass.as_kilograms() + motor.total_mass.as_kilograms();
    let mf = rocket.mass_properties().mass.as_kilograms() + motor.dry_mass().as_kilograms();
    let isp = i_total / (m_prop * 9.80665);
    let dv = isp * 9.80665 * (m0 / mf).ln() - 9.80665 * motor.burn_time();
    let h_boost = 0.5 * dv * motor.burn_time();
    let h_coast = dv * dv / (2.0 * 9.80665);
    let h_vacuum = h_boost + h_coast;

    let sim = Simulator::new(rocket, motor, SimConfig::vertical());
    let result = sim.run();
    let apogee = result.max_altitude.as_meters();

    eprintln!("vacuum estimate: {h_vacuum:.0}m, sim apogee: {apogee:.0}m");

    // The sim apogee (with drag) should be significantly less than the
    // vacuum estimate.
    assert!(
        apogee < h_vacuum * 0.9,
        "sim apogee ({apogee:.0}m) should be < 90% of vacuum estimate ({h_vacuum:.0}m)"
    );
}
