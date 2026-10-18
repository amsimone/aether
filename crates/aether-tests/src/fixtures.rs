//! Fixture loading helpers.
//!
//! Fixtures live at the workspace `tests/fixtures/` directory, accessed
//! via a relative path from this crate's manifest dir.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

fn workspace_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
}

pub fn load_motor_eng(filename: &str) -> String {
    let path = workspace_fixtures_dir().join("motors").join(filename);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read motor fixture {filename}: {e}"))
}

pub fn load_rocket_json(filename: &str) -> serde_json::Value {
    let path = workspace_fixtures_dir().join("rockets").join(filename);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read rocket fixture {filename}: {e}"));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse rocket fixture {filename}: {e}"))
}

pub fn load_reference_data_json(filename: &str) -> serde_json::Value {
    let path = workspace_fixtures_dir()
        .join("reference_data")
        .join(filename);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read reference data {filename}: {e}"));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse reference data {filename}: {e}"))
}
