//! Integration test: a `Rocket` survives a save → load round trip.
//!
//! This is the Phase-0 acceptance test for `rocket-core`'s serialization
//! contract (Phase-0 spike Task 1, acceptance criterion 2).

use rocket_core::Rocket;

#[test]
fn save_load_round_trip() {
    let rocket = Rocket {
        name: "Estes Alpha Clone".to_string(),
        version: 1,
    };

    let json = serde_json::to_string_pretty(&rocket).expect("serialize should succeed");
    let loaded: Rocket = serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(rocket, loaded);
}

#[test]
fn save_load_default() {
    let rocket = Rocket::default();

    let json = serde_json::to_string(&rocket).expect("serialize should succeed");
    let loaded: Rocket = serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(rocket, loaded);
}

#[test]
fn rocket_new_sets_version_1() {
    let rocket = Rocket::new("Test");
    assert_eq!(rocket.name, "Test");
    assert_eq!(rocket.version, 1);
}
