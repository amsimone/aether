//! Materials used in rocket construction.
//!
//! Each material has a density (mass per unit volume). Components refer
//! to materials by [`MaterialId`] (a stable handle into a
//! [`MaterialLibrary`]), per ADR-0008's sync-friendly format rules.
//!
//! A built-in library of common rocketry materials is provided; users
//! can add custom materials to their rocket's library.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ComponentId;
use crate::Density;

/// A stable identifier for a material within a [`MaterialLibrary`].
pub type MaterialId = ComponentId;

/// A construction material with a known density.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Unique ID within the library.
    pub id: MaterialId,
    /// Human-readable name (e.g., "Balsa", "Fiberglass").
    pub name: String,
    /// Mass density.
    pub density: Density,
    /// Optional source attribution (e.g., "Estes", "custom").
    #[serde(default)]
    pub source: String,
}

/// A collection of materials, keyed by ID.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MaterialLibrary {
    /// ID → material lookup.
    materials: HashMap<MaterialId, Material>,
    /// Counter for generating new IDs.
    next_id: u64,
}

impl MaterialLibrary {
    /// Create a new, empty library.
    #[must_use]
    pub fn new() -> Self {
        Self {
            materials: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create a library pre-populated with the built-in materials.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut lib = Self::new();
        for &(name, density_kg_m3) in BUILTIN_MATERIALS {
            lib.add(
                (*name).to_string(),
                Density::kilograms_per_cubic_meter(density_kg_m3),
                "built-in",
            );
        }
        lib
    }

    /// Add a material to the library, returning its ID.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        density: Density,
        source: impl Into<String>,
    ) -> MaterialId {
        let id = MaterialId::new(self.next_id);
        self.next_id += 1;
        let material = Material {
            id,
            name: name.into(),
            density,
            source: source.into(),
        };
        self.materials.insert(id, material);
        id
    }

    /// Look up a material by ID.
    #[must_use]
    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(&id)
    }

    /// Iterate over all materials in the library.
    pub fn iter(&self) -> impl Iterator<Item = &Material> {
        self.materials.values()
    }

    /// Number of materials in the library.
    #[must_use]
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// Whether the library is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}

/// Built-in materials with their densities in kg/m³.
///
/// Sources: various rocketry material references; values are typical
/// ranges rounded to reasonable precision.
#[allow(clippy::cast_possible_truncation)]
const BUILTIN_MATERIALS: &[(&str, f64)] = &[
    // Woods
    ("Balsa", 160.0),
    ("Basswood", 440.0),
    ("Plywood (1/8\")", 380.0),
    ("Plywood (1/4\")", 380.0),
    // Papers
    ("Cardboard (body tube)", 690.0),
    ("Phenolic tube", 1_250.0),
    ("Kraft paper", 800.0),
    // Composites
    ("Fiberglass", 1_900.0),
    ("Carbon fiber", 1_600.0),
    ("Kevlar", 1_440.0),
    // Metals
    ("Aluminum (6061)", 2_700.0),
    ("Aluminum (2024)", 2_780.0),
    ("Steel", 7_850.0),
    ("Brass", 8_500.0),
    ("Copper", 8_960.0),
    // Plastics
    ("PLA (3D printed)", 1_240.0),
    ("ABS (3D printed)", 1_040.0),
    ("PETG (3D printed)", 1_270.0),
    ("Nylon", 1_150.0),
    ("Polycarbonate", 1_200.0),
    // Ballast
    ("Lead", 11_340.0),
    ("Tungsten", 19_250.0),
    // Other
    ("Epoxy", 1_100.0),
    ("Foam (insulation)", 35.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_library_is_populated() {
        let lib = MaterialLibrary::with_builtins();
        assert!(lib.len() >= 20, "expected at least 20 built-in materials");
        // Balsa should be the lightest wood.
        let balsa = lib
            .iter()
            .find(|m| m.name == "Balsa")
            .expect("balsa should be in the built-in library");
        assert!((balsa.density.as_kilograms_per_cubic_meter() - 160.0).abs() < 0.1);
    }

    #[test]
    fn add_and_retrieve_material() {
        let mut lib = MaterialLibrary::new();
        let id = lib.add(
            "Unobtanium",
            Density::kilograms_per_cubic_meter(1234.0),
            "test",
        );
        let m = lib.get(id).expect("material should be retrievable");
        assert_eq!(m.name, "Unobtanium");
        assert!((m.density.as_kilograms_per_cubic_meter() - 1234.0).abs() < 0.1);
    }

    #[test]
    fn ids_are_unique() {
        let mut lib = MaterialLibrary::new();
        let id1 = lib.add("A", Density::kilograms_per_cubic_meter(1.0), "test");
        let id2 = lib.add("B", Density::kilograms_per_cubic_meter(2.0), "test");
        assert_ne!(id1, id2);
    }

    #[test]
    fn serde_round_trip() {
        let lib = MaterialLibrary::with_builtins();
        let json = serde_json::to_string(&lib).expect("serialize");
        let back: MaterialLibrary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(lib.len(), back.len());
    }
}
