//! The rocket design tree.
//!
//! This is the root of the data model: a `Rocket` is a named, versioned
//! collection of stages, components, materials, and overrides. The full
//! component type system (`NoseCone`, `BodyTube`, `FinSet`, …) will be
//! added in Phase 1; for now, this module establishes the root struct and
//! its serialization contract.
//!
//! See: `rocket-simulator-plan.md` §8 (Design Data Model).

use serde::{Deserialize, Serialize};

/// The root of the rocket design tree.
///
/// Serialized as JSON with a `version` field for forward-compatible
/// migrations (ADR-0008).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rocket {
    /// Human-readable name for this design (e.g., "Estes Alpha clone").
    pub name: String,

    /// Schema version of the `.rocket` file format. Incremented on every
    /// breaking format change; used to drive migration logic.
    pub version: u32,
}

impl Rocket {
    /// Create a new rocket with the given name and schema version 1.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: 1,
        }
    }
}

impl Default for Rocket {
    fn default() -> Self {
        Self::new("Untitled Rocket")
    }
}
