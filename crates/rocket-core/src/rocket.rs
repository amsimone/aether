//! The rocket design tree.
//!
//! The root of the data model: a `Rocket` is a named, versioned
//! collection of stages, components, materials, and overrides.
//!
//! See `rocket-simulator-plan.md` §8 (Design Data Model) and
//! `docs/adr/0008-local-only-files-v1.md` (sync-friendly format).

use serde::{Deserialize, Serialize};

use crate::materials::MaterialLibrary;
use crate::stage::Stage;
use crate::units::{Length, Mass};

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

    /// The root (bottom) stage of the rocket. Upper stages chain via
    /// [`Stage::upper_stage`].
    pub root: Stage,

    /// The material library for this rocket.
    #[serde(default = "MaterialLibrary::with_builtins")]
    pub materials: MaterialLibrary,
}

impl Rocket {
    /// Create a new rocket with the given name, an empty root stage,
    /// and the built-in material library.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: 1,
            root: Stage::new(),
            materials: MaterialLibrary::with_builtins(),
        }
    }
}

impl Default for Rocket {
    fn default() -> Self {
        Self::new("Untitled Rocket")
    }
}

/// Computed mass properties for a rocket or component.
///
/// All quantities are in SI units. The CG is measured from the nose tip
/// (positive = aft).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProperties {
    /// Total mass.
    pub mass: Mass,
    /// Center of gravity, measured from the nose tip.
    pub cg_x: Length,
    /// Axial (roll) moment of inertia, about the CG.
    pub i_axial: f64,
    /// Transverse (pitch/yaw) moment of inertia, about the CG.
    pub i_transverse: f64,
}

impl Rocket {
    /// Compute the mass properties of the rocket.
    ///
    /// Sums each component's mass (density × volume for shaped
    /// components; direct mass for `MassObject`s) and CG, then applies
    /// the parallel-axis theorem for inertia.
    ///
    /// # Panics
    ///
    /// Does not panic; returns zero mass for an empty rocket.
    #[must_use]
    pub fn mass_properties(&self) -> MassProperties {
        let mut total_mass = 0.0_f64;
        let mut weighted_cg = 0.0_f64;
        let mut i_axial_sum = 0.0_f64;
        let mut i_transverse_sum = 0.0_f64;

        for component in self.root.iter() {
            let (m, cg, i_axial_local, i_trans_local) =
                component_mass_properties(component, &self.materials);
            if m <= 0.0 {
                continue;
            }
            total_mass += m;
            weighted_cg += m * cg;

            // Parallel-axis theorem for transverse inertia:
            // I_total = I_local + m * (cg - cg_overall)²
            // We sum I_local + m*cg² here, then subtract m*cg_overall² at the end.
            i_axial_sum += i_axial_local;
            i_transverse_sum += i_trans_local + m * cg * cg;
        }

        if total_mass <= 0.0 {
            return MassProperties {
                mass: Mass::ZERO,
                cg_x: Length::ZERO,
                i_axial: 0.0,
                i_transverse: 0.0,
            };
        }

        let cg_overall = weighted_cg / total_mass;
        let i_transverse = i_transverse_sum - total_mass * cg_overall * cg_overall;

        MassProperties {
            mass: Mass::kilograms(total_mass),
            cg_x: Length::meters(cg_overall),
            i_axial: i_axial_sum,
            i_transverse,
        }
    }
}

/// Compute mass, CG (from nose tip), and inertias for a single component.
///
/// Returns `(mass_kg, cg_m, i_axial, i_transverse_local)`.
/// `i_transverse_local` is about the component's own CG (not the rocket CG).
fn component_mass_properties(
    component: &crate::components::Component,
    library: &MaterialLibrary,
) -> (f64, f64, f64, f64) {
    use crate::components::Component;

    match component {
        Component::MassObject(mo) => {
            let m = mo.mass.as_kilograms();
            let cg = mo.position.as_meters();
            // Point mass: zero local inertia.
            (m, cg, 0.0, 0.0)
        }
        Component::NoseCone(nc) => {
            let density = material_density(nc.material, library);
            let length = nc.length.as_meters();
            let r = nc.base_diameter.as_meters() / 2.0;
            // Approximate as a solid cone (ignoring wall thickness for now).
            let volume = (1.0 / 3.0) * std::f64::consts::PI * r * r * length;
            let mass = density * volume;
            // CG of a cone is 3/4 of the way from the tip to the base.
            let cg = nc.position.as_meters() + 0.75 * length;
            // Axial inertia of a cone: I = (3/10) * m * r²
            let i_axial = 0.3 * mass * r * r;
            // Transverse inertia of a cone about its CG: I = (3/80) * m * (4r² + h²)
            // (about the CG, which is 3/4 from the tip)
            let i_transverse = (3.0 / 80.0) * mass * (4.0 * r * r + length * length);
            (mass, cg, i_axial, i_transverse)
        }
        Component::BodyTube(bt) => {
            let density = material_density(bt.material, library);
            let length = bt.length.as_meters();
            let r_out = bt.outer_diameter.as_meters() / 2.0;
            let r_in = bt.inner_diameter.as_meters() / 2.0;
            // Hollow cylinder volume: π * (r_out² - r_in²) * h
            let volume = std::f64::consts::PI * (r_out * r_out - r_in * r_in) * length;
            let mass = density * volume;
            // CG is at the midpoint.
            let cg = bt.position.as_meters() + length / 2.0;
            // Axial inertia of a hollow cylinder: I = (1/2) * m * (r_out² + r_in²)
            let i_axial = 0.5 * mass * (r_out * r_out + r_in * r_in);
            // Transverse inertia of a hollow cylinder about its CG:
            // I = (1/12) * m * (3*(r_out² + r_in²) + h²)
            let i_transverse =
                (1.0 / 12.0) * mass * (3.0 * (r_out * r_out + r_in * r_in) + length * length);
            (mass, cg, i_axial, i_transverse)
        }
        Component::Transition(t) => {
            let density = material_density(t.material, library);
            let length = t.length.as_meters();
            let r_fore = t.fore_diameter.as_meters() / 2.0;
            let r_aft = t.aft_diameter.as_meters() / 2.0;
            // Conical frustrum volume: (π/3) * h * (r1² + r1*r2 + r2²)
            let volume = (std::f64::consts::PI / 3.0)
                * length
                * (r_fore * r_fore + r_fore * r_aft + r_aft * r_aft);
            let mass = density * volume;
            // CG of a frustrum (measured from the fore end):
            // x_cg = h * (r1² + 2*r1*r2 + 3*r2²) / (4 * (r1² + r1*r2 + r2²))
            let cg_from_fore = length
                * (r_fore * r_fore + 2.0 * r_fore * r_aft + 3.0 * r_aft * r_aft)
                / (4.0 * (r_fore * r_fore + r_fore * r_aft + r_aft * r_aft));
            let cg = t.position.as_meters() + cg_from_fore;
            // Approximate inertias (treat as an "average" cylinder for now;
            // exact frustrum inertias are complex and Phase 3 will refine).
            let r_avg = r_fore.midpoint(r_aft);
            let i_axial = 0.5 * mass * r_avg * r_avg;
            let i_transverse = (1.0 / 12.0) * mass * (3.0 * r_avg * r_avg + length * length);
            (mass, cg, i_axial, i_transverse)
        }
        Component::FinSet(fs) => {
            let density = material_density(fs.material, library);
            let area = fs.planform.area(); // m²
            let thickness = fs.thickness.as_meters();
            let volume = area * thickness;
            let mass = density * volume * f64::from(fs.count);
            // CG: approximate at the fin's MAC quarter-chord position.
            // This is rough; Phase 3 will refine with exact planform CG.
            let mac = fs.planform.mac();
            let cg_from_leading = mac / 4.0;
            let cg = fs.position.as_meters() + cg_from_leading;
            // Approximate inertias.
            let i_axial = mass * (0.05_f64).max(thickness * thickness / 12.0);
            let i_transverse = (1.0 / 12.0) * mass * (mac * mac + thickness * thickness);
            (mass, cg, i_axial, i_transverse)
        }
        Component::LaunchLug(ll) => {
            let density = material_density(ll.material, library);
            let length = ll.length.as_meters();
            let r_out = ll.outer_diameter.as_meters() / 2.0;
            let r_in = ll.inner_diameter.as_meters() / 2.0;
            let volume = std::f64::consts::PI * (r_out * r_out - r_in * r_in) * length;
            let mass = density * volume;
            let cg = ll.position.as_meters() + length / 2.0;
            let i_axial = 0.5 * mass * (r_out * r_out + r_in * r_in);
            let i_transverse =
                (1.0 / 12.0) * mass * (3.0 * (r_out * r_out + r_in * r_in) + length * length);
            (mass, cg, i_axial, i_transverse)
        }
        Component::Coupler(c) => {
            let density = material_density(c.material, library);
            let length = c.length.as_meters();
            let r = c.outer_diameter.as_meters() / 2.0;
            // Treat as a solid cylinder for simplicity (couplers are
            // usually short and dense).
            let volume = std::f64::consts::PI * r * r * length;
            let mass = density * volume;
            let cg = c.position.as_meters() + length / 2.0;
            let i_axial = 0.5 * mass * r * r;
            let i_transverse = (1.0 / 12.0) * mass * (3.0 * r * r + length * length);
            (mass, cg, i_axial, i_transverse)
        }
        Component::Pod(p) => {
            let density = material_density(p.material, library);
            let length = p.length.as_meters();
            let r = p.outer_diameter.as_meters() / 2.0;
            let volume = std::f64::consts::PI * r * r * length;
            let mass = density * volume;
            let cg = p.position.as_meters() + length / 2.0;
            let i_axial = 0.5 * mass * r * r;
            let i_transverse = (1.0 / 12.0) * mass * (3.0 * r * r + length * length);
            (mass, cg, i_axial, i_transverse)
        }
    }
}

/// Look up a material's density; returns 0 if not found (shouldn't happen
/// for valid rockets).
fn material_density(id: crate::materials::MaterialId, library: &MaterialLibrary) -> f64 {
    library
        .get(id)
        .map_or(0.0, |m| m.density.as_kilograms_per_cubic_meter())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::components::{BodyTube, Component, NoseCone, NoseShape};
    use crate::id::ComponentId;
    use crate::units::Length;

    #[test]
    fn empty_rocket_has_zero_mass() {
        let rocket = Rocket::new("Empty");
        let mp = rocket.mass_properties();
        assert_eq!(mp.mass, Mass::ZERO);
    }

    #[test]
    fn cylinder_mass_matches_formula() {
        // A solid cardboard cylinder: π·r²·h·ρ
        // r = 0.0125 m, h = 0.180 m, ρ = 690 kg/m³
        let r = 0.0125_f64;
        let h = 0.180_f64;
        let rho = 690.0_f64;
        let expected_mass = std::f64::consts::PI * r * r * h * rho;

        let mut rocket = Rocket::new("Test");
        let balsa_id = rocket.materials.iter().next().unwrap().id;
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa_id,
            length: Length::meters(h),
            outer_diameter: Length::meters(2.0 * r),
            inner_diameter: Length::ZERO, // solid
        }));

        // Need to set the density to cardboard. Find the cardboard material.
        for m in rocket.materials.iter() {
            if m.name == "Cardboard (body tube)" {
                let card_id = m.id;
                rocket.root.components[0] = Component::BodyTube(BodyTube {
                    id: ComponentId::new(1),
                    position: Length::ZERO,
                    material: card_id,
                    length: Length::meters(h),
                    outer_diameter: Length::meters(2.0 * r),
                    inner_diameter: Length::ZERO,
                });
                break;
            }
        }

        let mp = rocket.mass_properties();
        let actual = mp.mass.as_kilograms();
        assert!(
            (actual - expected_mass).abs() / expected_mass < 0.01,
            "mass {actual} should be close to {expected_mass}"
        );
    }

    #[test]
    fn cg_of_two_component_stack_is_weighted_average() {
        let mut rocket = Rocket::new("Test");

        // Add a nose cone (mass m1, CG at 0.75 * 0.070 = 0.0525 m)
        let balsa_id = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .unwrap()
            .id;
        rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa_id,
            shape: NoseShape::Conical,
            length: Length::meters(0.070),
            base_diameter: Length::meters(0.025),
            wall_thickness: Length::ZERO,
        }));

        // Add a body tube (mass m2, CG at 0.070 + 0.180/2 = 0.160 m)
        let card_id = rocket
            .materials
            .iter()
            .find(|m| m.name == "Cardboard (body tube)")
            .unwrap()
            .id;
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::meters(0.070),
            material: card_id,
            length: Length::meters(0.180),
            outer_diameter: Length::meters(0.025),
            inner_diameter: Length::meters(0.024),
        }));

        let mp = rocket.mass_properties();
        // CG should be between 0.0525 and 0.160.
        let cg = mp.cg_x.as_meters();
        assert!(cg > 0.0525, "CG {cg} should be > nose CG 0.0525");
        assert!(cg < 0.160, "CG {cg} should be < body CG 0.160");
    }

    #[test]
    fn rocket_serde_round_trip() {
        let mut rocket = Rocket::new("Test Rocket");
        let balsa_id = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .unwrap()
            .id;
        rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa_id,
            shape: NoseShape::TangentOgive,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(25.0),
            wall_thickness: Length::millimeters(1.0),
        }));

        let json = serde_json::to_string_pretty(&rocket).unwrap();
        let back: Rocket = serde_json::from_str(&json).unwrap();
        assert_eq!(rocket, back);
    }
}
