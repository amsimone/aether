//! Stable identifiers for entities in the rocket design tree.
//!
//! Per ADR-0008, every component (and material) has a stable ID that
//! survives serialization and partial edits. References between
//! components (e.g., "this fin set is mounted on body tube #abc123")
//! use these IDs, not array positions.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A stable, unique identifier for a component within a rocket design.
///
/// Internally a `u64`; serialized as a string for forward compatibility
/// (if we ever switch to UUIDs, old files with numeric IDs can be
/// migrated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentId(pub u64);

impl ComponentId {
    /// Create a new ID from a raw counter value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The raw ID value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_distinct() {
        assert_ne!(ComponentId::new(1), ComponentId::new(2));
    }

    #[test]
    fn serde_round_trip() {
        let id = ComponentId::new(42);
        let json = serde_json::to_string(&id).expect("serialize");
        let back: ComponentId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }
}
