//! # rocket-io
//!
//! Persistence and export:
//! - `.rocket` file format (JSON, versioned, serde-based, ADR-0008).
//! - CSV telemetry export.
//! - KML trajectory export (for Google Earth).
//! - STL geometry export (for 3D printing / manufacturing).
//! - PDF flight reports (via Typst, future).
//!
//! See:
//! - `docs/adr/0008-local-only-files-v1.md`
//! - `rocket-simulator-plan.md` §13

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

use rocket_core::Rocket;

/// Serialize a `Rocket` to a pretty-printed JSON string.
///
/// This is the canonical `.rocket` file format. The `version` field in
/// the JSON drives future migration logic.
///
/// # Errors
///
/// Returns `serde_json::Error` if the `Rocket` cannot be serialized
/// (e.g., if it contains non-string keys in a map — not possible with the
/// current data model, but the error type is preserved for forward
/// compatibility).
pub fn save_to_json(rocket: &Rocket) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(rocket)
}

/// Deserialize a `Rocket` from a JSON string.
///
/// # Errors
///
/// Returns `serde_json::Error` if the input is not valid JSON or does not
/// match the `Rocket` schema (missing fields, wrong types).
pub fn load_from_json(json: &str) -> Result<Rocket, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let rocket = Rocket::new("Round Trip Test");
        let json = save_to_json(&rocket).expect("save should succeed");
        let loaded = load_from_json(&json).expect("load should succeed");
        assert_eq!(rocket, loaded);
    }
}
