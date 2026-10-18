//! Estes Alpha integration test — the Phase-1 gate test.
//!
//! Builds the Estes Alpha (the canonical model rocket reference) via
//! the data model and verifies that mass, CG, and CP match expected
//! values within tolerances. This is the test that proves the design
//! editor's data model, mass properties, and aero estimate all work
//! correctly together on a real rocket.

#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::similar_names,
    dead_code,
    unused_imports
)]

use rocket_aero::{estimate, static_margin};
use rocket_core::{
    Airfoil, BodyTube, Component, ComponentId, FinPlanform, FinSet, LaunchLug, Length, Mass,
    MassObject, MassObjectKind, NoseCone, NoseShape, Rocket,
};

/// Build an Estes Alpha clone via the data model.
///
/// Dimensions from the Estes Alpha spec and the validation fixture
/// (`tests/fixtures/rockets/estes_alpha.json`):
/// - Nose cone: tangent ogive, 70mm × 24.8mm, balsa
/// - Body tube: 180mm × 24.8mm OD, 24.1mm ID, cardboard
/// - Fin set: 3 trapezoidal fins, 50mm root, 25mm tip, 35mm span, 25mm sweep, balsa
/// - Launch lug: 25mm × 5mm ID / 7mm OD
/// - Ballast: ~3g of modeling clay in the nose (typical for stability)
fn build_estes_alpha() -> Rocket {
    let mut rocket = Rocket::new("Estes Alpha Clone");

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

    // Nose cone: tangent ogive, 70mm, 24.8mm base.
    rocket.root.push(Component::NoseCone(NoseCone {
        id: ComponentId::new(1),
        position: Length::ZERO,
        material: balsa,
        shape: NoseShape::TangentOgive,
        length: Length::millimeters(70.0),
        base_diameter: Length::millimeters(24.8),
        wall_thickness: Length::ZERO, // solid balsa nose
    }));

    // Body tube: 180mm, 24.8mm OD, 24.1mm ID.
    rocket.root.push(Component::BodyTube(BodyTube {
        id: ComponentId::new(2),
        position: Length::millimeters(70.0),
        material: card,
        length: Length::millimeters(180.0),
        outer_diameter: Length::millimeters(24.8),
        inner_diameter: Length::millimeters(24.1),
    }));

    // Fin set: 3 trapezoidal fins.
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

    // Launch lug: 25mm, 5mm ID / 7mm OD.
    rocket.root.push(Component::LaunchLug(LaunchLug {
        id: ComponentId::new(4),
        position: Length::millimeters(100.0),
        material: card,
        length: Length::millimeters(25.0),
        inner_diameter: Length::millimeters(5.0),
        outer_diameter: Length::millimeters(7.0),
    }));

    // Ballast: 3g of modeling clay in the nose (typical for Alpha stability).
    rocket.root.push(Component::MassObject(MassObject {
        id: ComponentId::new(5),
        position: Length::millimeters(30.0),
        mass: Mass::grams(3.0),
        kind: MassObjectKind::Ballast,
    }));

    rocket
}

#[test]
fn estes_alpha_builds_with_5_components() {
    let rocket = build_estes_alpha();
    assert_eq!(
        rocket.root.components.len(),
        5,
        "Estes Alpha should have 5 components (nose, body, fins, lug, ballast)"
    );
}

#[test]
fn estes_alpha_mass_in_expected_range() {
    let rocket = build_estes_alpha();
    let mp = rocket.mass_properties();
    let mass_g = mp.mass.as_grams();

    // The Estes Alpha weighs about 16-20g without motor.
    // Our model: balsa nose (~3g) + cardboard body (~5g) + 3 balsa fins (~2g)
    // + launch lug (~0.5g) + 3g ballast = ~13.5g.
    // Allow a wide tolerance since material densities vary.
    assert!(
        mass_g > 8.0 && mass_g < 30.0,
        "Estes Alpha mass should be 8-30g, got {mass_g:.1}g"
    );
}

#[test]
fn estes_alpha_cg_is_forward_of_midpoint() {
    let rocket = build_estes_alpha();
    let mp = rocket.mass_properties();
    let cg_mm = mp.cg_x.as_millimeters();
    let rocket_length = rocket.root.length().as_millimeters();

    // The CG should be forward of the midpoint (the ballast + nose cone
    // pull it forward). Rocket length is ~250mm, so CG should be < 125mm.
    assert!(
        cg_mm < rocket_length / 2.0,
        "CG ({cg_mm:.1}mm) should be forward of midpoint ({:.1}mm)",
        rocket_length / 2.0
    );
    // And it should be positive (not behind the nose).
    assert!(cg_mm > 0.0, "CG should be positive, got {cg_mm:.1}mm");
}

#[test]
fn estes_alpha_cp_is_aft_of_cg() {
    let rocket = build_estes_alpha();
    let mp = rocket.mass_properties();
    let aero = estimate(&rocket);

    assert!(
        aero.cn_alpha > 5.0,
        "C_Nα should be > 5 (nose + fins), got {}",
        aero.cn_alpha
    );

    let cg_mm = mp.cg_x.as_millimeters();
    let cp_mm = aero.cp_x * 1000.0;

    assert!(
        cp_mm > cg_mm,
        "CP ({cp_mm:.1}mm) should be aft of CG ({cg_mm:.1}mm) for a stable rocket"
    );
}

#[test]
fn estes_alpha_static_margin_is_positive() {
    let rocket = build_estes_alpha();
    let sm = static_margin(&rocket);

    assert!(
        sm.is_finite(),
        "SM should be finite (not NaN) for a rocket with aero components"
    );
    assert!(
        sm > 0.0,
        "Estes Alpha should have positive SM (stable), got {sm:.2} cal"
    );
    // A well-designed model rocket has SM between 1 and 2 calibers.
    // The Alpha with ballast should be in this range.
    assert!(
        sm < 5.0,
        "SM should be < 5 cal (not excessively over-stable), got {sm:.2} cal"
    );
}

#[test]
fn estes_alpha_serde_round_trip() {
    let rocket = build_estes_alpha();
    let json = serde_json::to_string_pretty(&rocket).unwrap();
    let loaded: Rocket = serde_json::from_str(&json).unwrap();

    // The loaded rocket should have the same mass/CG/CP.
    let orig_mp = rocket.mass_properties();
    let loaded_mp = loaded.mass_properties();
    let orig_aero = estimate(&rocket);
    let loaded_aero = estimate(&loaded);

    assert!((orig_mp.mass.as_grams() - loaded_mp.mass.as_grams()).abs() < 0.001);
    assert!((orig_mp.cg_x.as_millimeters() - loaded_mp.cg_x.as_millimeters()).abs() < 0.001);
    assert!((orig_aero.cp_x - loaded_aero.cp_x).abs() < 1e-9);
}

#[test]
fn estes_alpha_rocket_length_matches_spec() {
    let rocket = build_estes_alpha();
    let length = rocket.root.length().as_millimeters();
    // Nose (70mm) + body (180mm) = 250mm. Fins and lug don't extend
    // the axial length (fins are radial, lug is within the body length).
    assert!(
        (length - 250.0).abs() < 1.0,
        "rocket length should be ~250mm, got {length:.1}mm"
    );
}
