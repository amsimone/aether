//! The small, reproducible starter design shipped with the editor.

/// Load the same editable design available in `examples/aether-one.rocket`.
pub fn reference_rocket() -> rocket_core::Rocket {
    rocket_io::load_from_json(include_str!("../../../examples/aether-one.rocket"))
        .expect("the bundled reference design must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_design_is_complete_and_round_trips() {
        let rocket = reference_rocket();
        assert_eq!(rocket.root.components.len(), 5);
        assert!((rocket.root.length().as_millimeters() - 250.0).abs() < 1e-9);
        assert!(rocket.mass_properties().mass.as_grams() > 8.0);
        assert!(rocket_aero::static_margin(&rocket) >= 1.0);
        let json = rocket_io::save_to_json(&rocket).expect("serialize reference");
        assert_eq!(rocket, rocket_io::load_from_json(&json).expect("reload"));
        let rocket_core::Component::FinSet(fins) = &rocket.root.components[2] else {
            panic!("reference must include fins");
        };
        assert!(rocket.root.find(fins.mount).is_some());
    }
}
