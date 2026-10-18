//! Integration test: a `Rocket` survives a save → load round trip.
//!
//! Phase-1 Task 1.2: updated for the full data model (stages, materials).

use rocket_core::{
    Component, ComponentId, Length, MassObject, MassObjectKind, NoseCone, NoseShape, Rocket,
};

#[test]
fn save_load_round_trip_empty() {
    let rocket = Rocket::new("Empty Rocket");
    let json = serde_json::to_string_pretty(&rocket).expect("serialize should succeed");
    let loaded: Rocket = serde_json::from_str(&json).expect("deserialize should succeed");
    assert_eq!(rocket, loaded);
}

#[test]
fn save_load_round_trip_with_components() {
    let mut rocket = Rocket::new("Test Rocket");

    // Add a nose cone using the balsa material from the built-in library.
    let balsa_id = rocket
        .materials
        .iter()
        .find(|m| m.name == "Balsa")
        .expect("balsa should be in the built-in library")
        .id;
    rocket.root.push(Component::NoseCone(NoseCone {
        id: ComponentId::new(1),
        position: Length::ZERO,
        material: balsa_id,
        shape: NoseShape::TangentOgive,
        length: Length::millimeters(70.0),
        base_diameter: Length::millimeters(24.8),
        wall_thickness: Length::millimeters(1.0),
    }));

    // Add a mass object (ballast).
    rocket.root.push(Component::MassObject(MassObject {
        id: ComponentId::new(2),
        position: Length::millimeters(150.0),
        mass: rocket_core::Mass::grams(10.0),
        kind: MassObjectKind::Ballast,
    }));

    let json = serde_json::to_string_pretty(&rocket).expect("serialize should succeed");
    let loaded: Rocket = serde_json::from_str(&json).expect("deserialize should succeed");
    assert_eq!(rocket, loaded);
}

#[test]
fn rocket_new_sets_version_1() {
    let rocket = Rocket::new("Test");
    assert_eq!(rocket.name, "Test");
    assert_eq!(rocket.version, 1);
}

#[test]
fn rocket_has_builtin_materials() {
    let rocket = Rocket::new("Test");
    assert!(
        rocket.materials.len() >= 20,
        "expected at least 20 built-in materials, got {}",
        rocket.materials.len()
    );
}

#[test]
fn rocket_default_has_empty_stage() {
    let rocket = Rocket::default();
    assert_eq!(rocket.root.components.len(), 0);
}
