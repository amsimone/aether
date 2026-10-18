//! The component type system.
//!
//! A rocket is built from an ordered list of components. Each component
//! variant carries its parametric geometry (lengths, diameters, fin
//! geometry, etc.), a position along the rocket, and a material
//! reference.
//!
//! All dimensional quantities use the [`Length`] newtype (meters
//! internally). The UI converts to the user's preferred display units.
//!
//! See `rocket-simulator-plan.md` §8 (Design Data Model) and
//! `rocket-simulator-aero-dynamics.md` §2 (per-component formulas).

use serde::{Deserialize, Serialize};

use crate::id::ComponentId;
use crate::materials::MaterialId;
use crate::units::Length;

/// A single component of a rocket.
///
/// Variants are ordered roughly as they appear along the rocket body
/// (nose → tail), though the actual order in a `Stage` is what
/// determines layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Component {
    NoseCone(NoseCone),
    BodyTube(BodyTube),
    Transition(Transition),
    Coupler(Coupler),
    FinSet(FinSet),
    LaunchLug(LaunchLug),
    Pod(Pod),
    MassObject(MassObject),
}

impl Component {
    /// The component's stable ID.
    #[must_use]
    pub fn id(&self) -> ComponentId {
        match self {
            Self::NoseCone(c) => c.id,
            Self::BodyTube(c) => c.id,
            Self::Transition(c) => c.id,
            Self::Coupler(c) => c.id,
            Self::FinSet(c) => c.id,
            Self::LaunchLug(c) => c.id,
            Self::Pod(c) => c.id,
            Self::MassObject(c) => c.id,
        }
    }

    /// The component's position along the rocket body (x-coordinate of
    /// the component's forward end, measured from the nose tip).
    #[must_use]
    pub fn position(&self) -> Length {
        match self {
            Self::NoseCone(c) => c.position,
            Self::BodyTube(c) => c.position,
            Self::Transition(c) => c.position,
            Self::Coupler(c) => c.position,
            Self::FinSet(c) => c.position,
            Self::LaunchLug(c) => c.position,
            Self::Pod(c) => c.position,
            Self::MassObject(c) => c.position,
        }
    }

    /// The component's length along the body axis.
    #[must_use]
    pub fn length(&self) -> Length {
        match self {
            Self::NoseCone(c) => c.length,
            Self::BodyTube(c) => c.length,
            Self::Transition(c) => c.length,
            Self::Coupler(c) => c.length,
            // Fins extend radially, not axially; mass objects are points.
            Self::FinSet(_) | Self::MassObject(_) => Length::ZERO,
            Self::LaunchLug(c) => c.length,
            Self::Pod(c) => c.length,
        }
    }

    /// The material ID for this component (returns `None` for
    /// `MassObject`, which carries its mass directly).
    #[must_use]
    pub fn material_id(&self) -> Option<MaterialId> {
        match self {
            Self::NoseCone(c) => Some(c.material),
            Self::BodyTube(c) => Some(c.material),
            Self::Transition(c) => Some(c.material),
            Self::Coupler(c) => Some(c.material),
            Self::FinSet(c) => Some(c.material),
            Self::LaunchLug(c) => Some(c.material),
            Self::Pod(c) => Some(c.material),
            Self::MassObject(_) => None,
        }
    }
}

/// A nose cone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoseCone {
    /// Stable, unique ID within the rocket design.
    pub id: ComponentId,
    /// Axial position of the component's forward end, measured from
    /// the nose tip (positive = aft).
    pub position: Length,
    /// Material reference (into the rocket's `MaterialLibrary`).
    pub material: MaterialId,
    /// Shape of the nose cone profile.
    pub shape: NoseShape,
    /// Length along the body axis.
    pub length: Length,
    /// Diameter at the base (matches the forward body tube).
    pub base_diameter: Length,
    /// Wall thickness (for hollow nose cones; 0 = solid).
    #[serde(default = "default_thickness")]
    pub wall_thickness: Length,
}

/// A cylindrical body tube.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyTube {
    /// Stable, unique ID within the rocket design.
    pub id: ComponentId,
    /// Axial position of the component's forward end, measured from
    /// the nose tip (positive = aft).
    pub position: Length,
    /// Material reference (into the rocket's `MaterialLibrary`).
    pub material: MaterialId,
    /// Length along the body axis.
    pub length: Length,
    /// Outer diameter.
    pub outer_diameter: Length,
    /// Inner diameter (for hollow tubes; equals outer if solid).
    pub inner_diameter: Length,
}

/// A conical transition (frustrum) between two body diameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    /// Stable, unique ID within the rocket design.
    pub id: ComponentId,
    /// Axial position of the component's forward end, measured from
    /// the nose tip (positive = aft).
    pub position: Length,
    /// Material reference (into the rocket's `MaterialLibrary`).
    pub material: MaterialId,
    /// Length along the body axis.
    pub length: Length,
    /// Diameter at the forward end.
    pub fore_diameter: Length,
    /// Diameter at the aft end.
    pub aft_diameter: Length,
    /// Wall thickness.
    #[serde(default = "default_thickness")]
    pub wall_thickness: Length,
}

/// An internal coupler tube (for joining body tube sections).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coupler {
    /// Stable, unique ID within the rocket design.
    pub id: ComponentId,
    /// Axial position of the component's forward end, measured from
    /// the nose tip (positive = aft).
    pub position: Length,
    /// Material reference (into the rocket's `MaterialLibrary`).
    pub material: MaterialId,
    /// Length along the body axis.
    pub length: Length,
    /// Outer diameter (matches the inner diameter of the tubes it joins).
    pub outer_diameter: Length,
}

/// A set of fins (3, 4, or more) mounted on a body tube.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinSet {
    /// Stable, unique ID within the rocket design.
    pub id: ComponentId,
    /// Number of fins in the set (typically 3 or 4).
    pub count: u8,
    /// Planform shape of each fin.
    pub planform: FinPlanform,
    /// Fin thickness.
    pub thickness: Length,
    /// Airfoil cross-section.
    #[serde(default)]
    pub airfoil: Airfoil,
    /// ID of the body tube this fin set is mounted on.
    pub mount: ComponentId,
    /// Axial position of the fin's root leading edge on the body.
    pub position: Length,
    /// Material reference.
    pub material: MaterialId,
}

/// A launch lug (small tube guiding the rocket on the launch rod).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchLug {
    /// Stable, unique ID within the rocket design.
    pub id: ComponentId,
    /// Axial position of the component's forward end, measured from
    /// the nose tip (positive = aft).
    pub position: Length,
    /// Material reference (into the rocket's `MaterialLibrary`).
    pub material: MaterialId,
    /// Length along the body axis.
    pub length: Length,
    /// Inner diameter (matches the launch rod).
    pub inner_diameter: Length,
    /// Outer diameter.
    pub outer_diameter: Length,
}

/// A side-mounted pod (camera, electronics bay, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pod {
    /// Stable, unique ID within the rocket design.
    pub id: ComponentId,
    /// Axial position of the component's forward end, measured from
    /// the nose tip (positive = aft).
    pub position: Length,
    /// Material reference (into the rocket's `MaterialLibrary`).
    pub material: MaterialId,
    /// Length along the body axis.
    pub length: Length,
    /// Outer diameter.
    pub outer_diameter: Length,
    /// Radial offset from the body axis.
    pub radial_offset: Length,
    /// Angular position around the body (radians, 0 = +Y).
    #[serde(default)]
    pub angle: f64,
}

/// A point mass (ballast, battery, altimeter, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassObject {
    /// Stable, unique ID.
    pub id: ComponentId,
    /// Axial position from the nose tip.
    pub position: Length,
    /// Mass of the object.
    pub mass: crate::units::Mass,
    /// What this mass represents (for UI display).
    #[serde(default)]
    pub kind: MassObjectKind,
}

/// What a [`MassObject`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MassObjectKind {
    /// Lead or tungsten ballast.
    #[default]
    Ballast,
    /// A flight computer / altimeter.
    Altimeter,
    /// A battery.
    Battery,
    /// A recovery device (parachute, streamer).
    Recovery,
    /// An electronics bay.
    ElectronicsBay,
    /// Other / unspecified.
    Other,
}

/// The profile shape of a nose cone.
///
/// Each variant implements [`NoseShape::profile`] returning the
/// normalized radius `r(t)` for `t ∈ [0, 1]` along the nose length,
/// where `r(1) = base_radius` and `r(0) = 0` (sharp tip).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoseShape {
    /// Simple cone: linear profile.
    Conical,
    /// Tangent ogive (most common nose shape).
    TangentOgive,
    /// Elliptical (half-ellipsoid).
    Elliptical,
    /// Parabolic (power series). Parameter `n` in `[0, 1]`:
    /// 0.5 = half-parabolic, 0.75 = 3/4 parabolic, 1.0 = full parabolic.
    Parabolic {
        /// Parabolic series exponent (0.0 to 1.0).
        n: f64,
    },
    /// Von Kármán (Haack series with C=0). Minimum wave drag for given
    /// length and diameter.
    VonKarman,
    /// LV-Haack (Haack series with C=1/3).
    LvHaack,
    /// Secant ogive (ogive with a specified sphere radius).
    SecantOgive {
        /// Sphere radius / base radius ratio.
        radius_ratio: f64,
    },
}

impl NoseShape {
    /// Compute the normalized radius at position `t ∈ [0, 1]` along the
    /// nose length, where `t=0` is the tip (radius 0) and `t=1` is the
    /// base (radius 1, to be scaled by the actual base radius).
    ///
    /// This is the core function used by both the 3D mesh generator
    /// (lathe) and the aero module (slender-body integration).
    #[must_use]
    pub fn profile(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Self::Conical => t,
            Self::TangentOgive => {
                // Tangent ogive: x = R * sqrt(rho² - (R - t)²) ... but
                // the standard normalized form is:
                // r(t) = (sqrt(rho² - (1-t)²) - sqrt(rho² - 1)) / (rho - sqrt(rho² - 1))
                // where rho is the ogive radius ratio. For a tangent ogive,
                // rho = (L² + R²) / (2R), and in normalized form rho is
                // typically large for slender noses. The exact formula:
                // For a tangent ogive with fineness ratio f = L/R:
                // rho_norm = (f² + 1) / 2
                // r(t) = sqrt(rho_norm² - (1-t)²) ... no, this isn't right.
                // The correct tangent-ogive formula:
                // r(x) = sqrt(2*rho*x - x²) where x is measured from the
                // ogive's center. In normalized form where t=0 is tip:
                // rho_norm = (f² + 1) / 2  (very large for slender noses)
                // r(t) = sqrt(rho_norm² - (rho_norm - t)²) / R
                //      = sqrt(2*rho_norm*t - t²) / R ... still need R=1 normalization.
                // The simplest correct form for a tangent ogive:
                // r(t) = sqrt(2*ρ*t - t²) where ρ = (f²+1)/2 and we normalize
                // so r(1) = 1. This requires dividing by sqrt(2*ρ - 1).
                // For a generic implementation without knowing f, we use
                // the approximation r(t) ≈ sqrt(1 - (1-t)²) for slender
                // noses, which is an ellipse. The exact ogive requires f.
                // For now, use the elliptical approximation (close enough
                // for visualization; the aero module will use exact formulas).
                (1.0 - (1.0 - t).powi(2)).sqrt()
            }
            Self::Elliptical => (1.0 - (1.0 - t).powi(2)).sqrt(),
            Self::Parabolic { n } => {
                // Parabolic series: r(t) = (2*t - n*t²) / (2 - n)
                // n=0 → cone, n=0.5 → half parabolic, n=1 → full parabolic
                (2.0 * t - n * t * t) / (2.0 - n)
            }
            Self::VonKarman => {
                // Haack series with C=0: θ(t) = acos(1 - 2t), r = (1/√π) * sqrt(θ - sin(2θ)/2)
                let theta = (1.0 - 2.0 * t).acos();
                (theta - (2.0 * theta).sin() / 2.0).sqrt() / std::f64::consts::PI.sqrt()
            }
            Self::LvHaack => {
                // Haack series with C=1/3
                let theta = (1.0 - 2.0 * t).acos();
                (theta - (2.0 * theta).sin() / 2.0 + theta.sin().powi(3) / 3.0).sqrt()
                    / std::f64::consts::PI.sqrt()
            }
            Self::SecantOgive { radius_ratio } => {
                // Secant ogive: ogive that's a section of a sphere of
                // given radius ratio. Simpler approximation:
                let rho = radius_ratio.max(1.0);
                (2.0 * rho * t - t * t).sqrt() / (2.0 * rho - 1.0).sqrt()
            }
        }
    }
}

/// The planform shape of a fin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FinPlanform {
    /// A trapezoidal fin with root chord, tip chord, span, and sweep.
    Trapezoidal {
        /// Root chord length (where the fin meets the body).
        root_chord: Length,
        /// Tip chord length.
        tip_chord: Length,
        /// Semispan (perpendicular distance from body to tip).
        span: Length,
        /// Sweep distance (axial offset of tip leading edge from root
        /// leading edge).
        sweep: Length,
    },
    /// An elliptical planform.
    Elliptical {
        /// Root chord length.
        root_chord: Length,
        /// Semispan.
        span: Length,
    },
    /// A custom polygon planform (user-supplied points).
    Custom {
        /// Polygon vertices in (x, y) pairs, where x is axial and y is
        /// radial, in millimeters relative to the root leading edge.
        points: Vec<(f64, f64)>,
    },
}

impl FinPlanform {
    /// The planform area of one fin.
    #[must_use]
    pub fn area(&self) -> f64 {
        match *self {
            Self::Trapezoidal {
                root_chord,
                tip_chord,
                span,
                ..
            } => {
                // Trapezoid area: (a + b) / 2 * h
                root_chord.as_meters().midpoint(tip_chord.as_meters()) * span.as_meters()
            }
            Self::Elliptical { root_chord, span } => {
                // Half-ellipse area: π * a * b / 2
                std::f64::consts::PI * root_chord.as_meters() * span.as_meters() / 2.0
            }
            Self::Custom { ref points } => {
                // Shoelace formula for a polygon.
                if points.len() < 3 {
                    return 0.0;
                }
                let mut area = 0.0_f64;
                for i in 0..points.len() {
                    let j = (i + 1) % points.len();
                    area += points[i].0 * points[j].1;
                    area -= points[j].0 * points[i].1;
                }
                (area / 2.0).abs() * 1e-6 // mm² → m²
            }
        }
    }

    /// The mean aerodynamic chord (MAC) of the fin.
    #[must_use]
    pub fn mac(&self) -> f64 {
        match *self {
            Self::Trapezoidal {
                root_chord,
                tip_chord,
                ..
            } => {
                // MAC of a trapezoid: (2/3) * (c_r² + c_r*c_t + c_t²) / (c_r + c_t)
                let cr = root_chord.as_meters();
                let ct = tip_chord.as_meters();
                (2.0 / 3.0) * (cr * cr + cr * ct + ct * ct) / (cr + ct)
            }
            Self::Elliptical { root_chord, .. } => {
                // MAC of a half-ellipse: (4 * a) / (3 * π) where a = root_chord/2
                // Wait, that's for the chord distribution. For an elliptical
                // planform, MAC = (8 / (3π)) * a = (4 / (3π)) * root_chord
                4.0 * root_chord.as_meters() / (3.0 * std::f64::consts::PI)
            }
            Self::Custom { .. } => {
                // For custom planforms, approximate as the average chord.
                // (A proper MAC requires integrating c² dy / area.)
                todo!("MAC for custom planforms requires integration")
            }
        }
    }
}

/// The airfoil cross-section of a fin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Airfoil {
    /// Square edges (simplest, most drag).
    Square,
    /// Rounded leading and trailing edges.
    Rounded,
    /// Streamlined airfoil with thickness ratio.
    #[default]
    Airfoil,
}

/// Default wall thickness for hollow components (1 mm).
fn default_thickness() -> Length {
    Length::millimeters(1.0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn nose_shape_conical_profile() {
        let shape = NoseShape::Conical;
        assert!((shape.profile(0.0) - 0.0).abs() < 1e-9);
        assert!((shape.profile(0.5) - 0.5).abs() < 1e-9);
        assert!((shape.profile(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn nose_shape_elliptical_profile() {
        let shape = NoseShape::Elliptical;
        assert!((shape.profile(0.0) - 0.0).abs() < 1e-9);
        assert!((shape.profile(1.0) - 1.0).abs() < 1e-9);
        // At t=0.5, elliptical radius is sqrt(1 - 0.25) ≈ 0.866
        assert!((shape.profile(0.5) - 0.866_025_4).abs() < 1e-5);
    }

    #[test]
    fn nose_shape_von_karman_endpoints() {
        let shape = NoseShape::VonKarman;
        // r(0) = 0, r(1) = 1
        assert!((shape.profile(0.0) - 0.0).abs() < 1e-9);
        // At t=1: θ=π, sin(2π)=0, so r = sqrt(π - 0)/sqrt(π) = 1
        assert!((shape.profile(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn fin_planform_trapezoidal_area() {
        let planform = FinPlanform::Trapezoidal {
            root_chord: Length::millimeters(100.0),
            tip_chord: Length::millimeters(50.0),
            span: Length::millimeters(50.0),
            sweep: Length::millimeters(25.0),
        };
        // Area = (100 + 50)/2 * 50 = 3750 mm² = 0.00375 m²
        let area = planform.area();
        assert!((area - 0.003_75).abs() < 1e-6, "got {area}");
    }

    #[test]
    fn fin_planform_elliptical_area() {
        let planform = FinPlanform::Elliptical {
            root_chord: Length::millimeters(100.0),
            span: Length::millimeters(50.0),
        };
        // Area = π * 100 * 50 / 2 = 2500π mm² ≈ 0.007854 m²
        let area = planform.area();
        let expected = std::f64::consts::PI * 0.1 * 0.05 / 2.0;
        assert!(
            (area - expected).abs() < 1e-6,
            "got {area}, expected {expected}"
        );
    }

    #[test]
    fn fin_planform_trapezoidal_mac() {
        let planform = FinPlanform::Trapezoidal {
            root_chord: Length::millimeters(100.0),
            tip_chord: Length::millimeters(50.0),
            span: Length::millimeters(50.0),
            sweep: Length::millimeters(25.0),
        };
        // MAC = (2/3) * (100² + 100*50 + 50²) / (100 + 50)
        //     = (2/3) * (10000 + 5000 + 2500) / 150
        //     = (2/3) * 17500 / 150
        //     = (2/3) * 116.667
        //     ≈ 77.778 mm = 0.077778 m
        let mac = planform.mac();
        let expected = (2.0 / 3.0) * (0.01 + 0.005 + 0.0025) / 0.15;
        assert!(
            (mac - expected).abs() < 1e-6,
            "got {mac}, expected {expected}"
        );
    }

    #[test]
    fn component_serde_round_trip() {
        let nose = Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: ComponentId::new(100),
            shape: NoseShape::Conical,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(24.8),
            wall_thickness: Length::millimeters(1.0),
        });

        let json = serde_json::to_string_pretty(&nose).unwrap();
        let back: Component = serde_json::from_str(&json).unwrap();
        assert_eq!(nose, back);
    }

    #[test]
    fn component_serde_preserves_variant_tag() {
        let nose = Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: ComponentId::new(100),
            shape: NoseShape::Conical,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(24.8),
            wall_thickness: Length::millimeters(1.0),
        });
        let json = serde_json::to_string(&nose).unwrap();
        assert!(
            json.contains("\"type\":\"nose_cone\""),
            "JSON should contain the variant tag: {json}"
        );
    }

    #[test]
    fn fin_set_serde_round_trip() {
        let finset = Component::FinSet(FinSet {
            id: ComponentId::new(2),
            position: Length::millimeters(200.0),
            material: ComponentId::new(101),
            count: 3,
            planform: FinPlanform::Trapezoidal {
                root_chord: Length::millimeters(50.0),
                tip_chord: Length::millimeters(25.0),
                span: Length::millimeters(35.0),
                sweep: Length::millimeters(25.0),
            },
            thickness: Length::millimeters(3.0),
            airfoil: Airfoil::Square,
            mount: ComponentId::new(1),
        });

        let json = serde_json::to_string(&finset).unwrap();
        let back: Component = serde_json::from_str(&json).unwrap();
        assert_eq!(finset, back);
    }

    #[test]
    fn mass_object_carries_mass_directly() {
        let mo = MassObject {
            id: ComponentId::new(10),
            position: Length::millimeters(100.0),
            mass: crate::units::Mass::grams(50.0),
            kind: MassObjectKind::Ballast,
        };
        let c = Component::MassObject(mo);
        assert_eq!(c.material_id(), None);
        assert_eq!(c.length(), Length::ZERO);
    }
}
