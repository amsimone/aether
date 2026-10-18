//! Drag coefficient estimation.
//!
//! A simple, constant-Cd estimate for Phase 2's flight simulator. Phase 3
//! will replace this with the full drag buildup (skin friction with
//! Reynolds dependence, base drag with Mach correction, form/pressure
//! drag, fin interference, and compressibility).
//!
//! ## Model
//!
//! The total drag coefficient (referenced to the body cross-sectional
//! area) is:
//!
//! ```text
//! Cd = Cf · (S_wet / S_ref) · K_form + Cd_base + Cd_proto
//! ```
//!
//! where:
//! - `Cf` is a turbulent skin-friction coefficient at a representative
//!   Reynolds number (≈ 0.006 for model rockets at Re ~ 1×10⁵).
//! - `S_wet` is the total wetted area (body + nose + both sides of all
//!   fins).
//! - `S_ref = π · (d_ref/2)²` is the reference area.
//! - `K_form ≈ 1.1` is a body form factor (accounts for the fact that
//!   pressure on a curved body is higher than on a flat plate).
//! - `Cd_base = 0.029 · (d_base / d_ref)³` is Barrowman's base drag
//!   formula (for a flat base; boattails reduce it).
//! - `Cd_proto ≈ 0.05` lumps protuberances (launch lug, fin edges,
//!   coarse finish) into a constant. Phase 3 will itemize these.
//!
//! This gives a Cd in the 0.3–0.6 range for typical LPR/MPR rockets,
//! matching `OpenRocket`'s defaults well enough for the 5% apogee gate.
//!
//! ## Mach dependence
//!
//! For Phase 2 (T0/LPR), Cd is constant with Mach. The `mach` argument
//! is accepted so the same call site works when Phase 3 adds a Mach
//! ramp. Below M=0.6 the change is negligible anyway.

use rocket_core::components::{Component, FinSet, NoseCone, NoseShape};
use rocket_core::Rocket;

/// A representative turbulent skin-friction coefficient for model rockets.
///
/// Model rockets operate at Reynolds numbers of 1×10⁴–1×10⁵, which is
/// the laminar-to-turbulent transition regime. The boundary layer is
/// tripped by body-tube seams, fin edges, and the launch lug, making it
/// effectively turbulent. At Re ≈ 5×10⁴, the Schlichting formula gives
/// Cf ≈ 0.0084; we use 0.010 to account for surface roughness and
/// transitional effects, which matches `OpenRocket`'s default skin
/// friction for typical LPR rockets.
const CF_TURBULENT: f64 = 0.010;

/// Body form factor. The wetted-area skin drag is multiplied by this to
/// account for the pressure distribution on a curved body.
const K_FORM_BODY: f64 = 1.1;

/// Fin form factor. Fins are flat plates, so their form factor is 1.0,
/// but a small factor accounts for the fin-body junction and edges.
const K_FORM_FIN: f64 = 1.2;

/// Base drag coefficient per Barrowman: `Cd_base = 0.029 · (d_base/d_ref)³`.
const K_BASE: f64 = 0.029;

/// A lumped constant for protuberances (launch lug, fin edges, finish).
const CD_PROTO: f64 = 0.05;

/// The result of a drag coefficient estimate, with the breakdown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragBreakdown {
    /// Total drag coefficient `Cd` (referenced to body cross-section).
    pub cd: f64,
    /// Skin-friction contribution.
    pub cd_skin: f64,
    /// Base drag contribution.
    pub cd_base: f64,
    /// Protuberance / form lumped contribution.
    pub cd_proto: f64,
    /// Reference area used (m²).
    pub reference_area: f64,
    /// Total wetted area (m²).
    pub wetted_area: f64,
}

impl DragBreakdown {
    /// A zero breakdown (for rockets with no body).
    pub const ZERO: Self = Self {
        cd: 0.0,
        cd_skin: 0.0,
        cd_base: 0.0,
        cd_proto: 0.0,
        reference_area: 0.0,
        wetted_area: 0.0,
    };
}

/// Estimate the drag coefficient of a rocket at the given Mach number.
///
/// Returns the total `Cd` referenced to the body cross-sectional area.
/// For Phase 2, `Cd` is constant with Mach (the argument is accepted for
/// forward compatibility with Phase 3's compressibility model).
///
/// # Panics
///
/// Does not panic; returns `DragBreakdown::ZERO` for an empty rocket.
#[must_use]
pub fn estimate_cd(rocket: &Rocket, _mach: f64) -> f64 {
    estimate_cd_breakdown(rocket).cd
}

/// Estimate the drag coefficient with a full breakdown.
///
/// See the module docs for the model.
#[must_use]
pub fn estimate_cd_breakdown(rocket: &Rocket) -> DragBreakdown {
    let ref_diameter = find_reference_diameter(rocket);
    if ref_diameter < 1e-9 {
        return DragBreakdown::ZERO;
    }
    let ref_area = std::f64::consts::PI * (ref_diameter / 2.0).powi(2);
    if ref_area < 1e-12 {
        return DragBreakdown::ZERO;
    }

    let mut wet_body = 0.0_f64;
    let mut wet_fin = 0.0_f64;
    // The base diameter is the aft diameter of the rearmost body
    // component (body tube or transition). Track the largest axial
    // extent and the corresponding aft diameter.
    let mut max_aft_x = f64::NEG_INFINITY;
    let mut base_diameter = ref_diameter;

    for c in rocket.root.iter() {
        match c {
            Component::NoseCone(nc) => wet_body += nose_cone_wetted_area(nc),
            Component::BodyTube(bt) => {
                wet_body +=
                    std::f64::consts::PI * bt.outer_diameter.as_meters() * bt.length.as_meters();
                let aft_x = bt.position.as_meters() + bt.length.as_meters();
                if aft_x > max_aft_x {
                    max_aft_x = aft_x;
                    base_diameter = bt.outer_diameter.as_meters();
                }
            }
            Component::Transition(t) => {
                let r1 = t.fore_diameter.as_meters() / 2.0;
                let r2 = t.aft_diameter.as_meters() / 2.0;
                let h = t.length.as_meters();
                let slant = ((r2 - r1).powi(2) + h * h).sqrt();
                wet_body += std::f64::consts::PI * (r1 + r2) * slant;
                let aft_x = t.position.as_meters() + t.length.as_meters();
                if aft_x > max_aft_x {
                    max_aft_x = aft_x;
                    base_diameter = t.aft_diameter.as_meters();
                }
            }
            Component::FinSet(fs) => {
                wet_fin += fin_set_wetted_area(fs);
            }
            // Couplers, pods, launch lugs, mass objects don't contribute
            // significantly to wetted area for a Phase 2 estimate.
            _ => {}
        }
    }

    let wet_total = wet_body + wet_fin;

    // If nothing contributes wetted area, there's no body to compute
    // drag on (e.g. an empty rocket or one with only mass objects).
    if wet_total < 1e-12 {
        return DragBreakdown::ZERO;
    }

    // Skin friction: body and fins get different form factors.
    let cd_skin_body = CF_TURBULENT * (wet_body / ref_area) * K_FORM_BODY;
    let cd_skin_fin = CF_TURBULENT * (wet_fin / ref_area) * K_FORM_FIN;
    let cd_skin = cd_skin_body + cd_skin_fin;

    // Base drag (Barrowman). Reduced if the aft diameter is smaller than
    // the reference (a boattail).
    let cd_base = K_BASE * (base_diameter / ref_diameter).powi(3);

    let cd_proto = CD_PROTO;

    let cd = cd_skin + cd_base + cd_proto;

    DragBreakdown {
        cd,
        cd_skin,
        cd_base,
        cd_proto,
        reference_area: ref_area,
        wetted_area: wet_total,
    }
}

/// Wetted (lateral) area of a nose cone.
///
/// For a cone: `π · r · √(r² + h²)`.
/// For a tangent ogive / Haack: approximate with the cone formula using
/// the slant height of the equivalent cone — accurate to within a few
/// percent, which is fine for a Phase 2 Cd estimate.
fn nose_cone_wetted_area(nc: &NoseCone) -> f64 {
    let r = nc.base_diameter.as_meters() / 2.0;
    let h = nc.length.as_meters();
    let slant = match nc.shape {
        // Conical: exact slant height.
        NoseShape::Conical => (r * r + h * h).sqrt(),
        // For ogive/Haack/elliptical, the surface area is slightly less
        // than the cone. Use 0.97 of the cone slant as an approximation.
        NoseShape::TangentOgive
        | NoseShape::SecantOgive { .. }
        | NoseShape::VonKarman
        | NoseShape::LvHaack => 0.97 * (r * r + h * h).sqrt(),
        // Elliptical is even shorter; use 0.92.
        NoseShape::Elliptical => 0.92 * (r * r + h * h).sqrt(),
        // Parabolic: between cone and elliptical.
        NoseShape::Parabolic { n } => {
            let cone_slant = (r * r + h * h).sqrt();
            cone_slant * (1.0 - 0.05 * n)
        }
    };
    std::f64::consts::PI * r * slant
}

/// Wetted area of a fin set (both sides of all fins).
///
/// `S_wet = 2 · N · S_planform` (the factor of 2 accounts for both
/// sides of each fin being wetted).
fn fin_set_wetted_area(fs: &FinSet) -> f64 {
    let planform_area = fs.planform.area();
    2.0 * f64::from(fs.count) * planform_area
}

/// Find the reference diameter (max body diameter).
///
/// This mirrors `rocket-aero::design::find_reference_diameter` but is
/// kept local to avoid a cross-module coupling for a single helper.
fn find_reference_diameter(rocket: &Rocket) -> f64 {
    let mut max_d = 0.0_f64;
    for c in rocket.root.iter() {
        let d = match c {
            Component::BodyTube(bt) => bt.outer_diameter.as_meters(),
            Component::NoseCone(nc) => nc.base_diameter.as_meters(),
            Component::Transition(t) => t.fore_diameter.as_meters().max(t.aft_diameter.as_meters()),
            _ => 0.0,
        };
        if d > max_d {
            max_d = d;
        }
    }
    if max_d > 0.0 {
        max_d
    } else {
        0.025 // Default 25mm if nothing found.
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]

    use super::*;
    use rocket_core::{
        Airfoil, BodyTube, ComponentId, FinPlanform, FinSet, LaunchLug, Length, Mass, MassObject,
        MassObjectKind, NoseCone, NoseShape, Rocket,
    };

    /// Build an Estes Alpha clone (same as the aether-tests fixture).
    fn build_estes_alpha() -> Rocket {
        let mut rocket = Rocket::new("Estes Alpha");
        let balsa = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .unwrap()
            .id;
        let card = rocket
            .materials
            .iter()
            .find(|m| m.name == "Cardboard (body tube)")
            .unwrap()
            .id;

        rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa,
            shape: NoseShape::TangentOgive,
            length: Length::millimeters(70.0),
            base_diameter: Length::millimeters(24.8),
            wall_thickness: Length::ZERO,
        }));
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::millimeters(70.0),
            material: card,
            length: Length::millimeters(180.0),
            outer_diameter: Length::millimeters(24.8),
            inner_diameter: Length::millimeters(24.1),
        }));
        rocket.root.push(Component::FinSet(FinSet {
            id: ComponentId::new(3),
            count: 3,
            planform: FinPlanform::Trapezoidal {
                root_chord: Length::millimeters(50.0),
                tip_chord: Length::millimeters(25.0),
                span: Length::millimeters(35.0),
                sweep: Length::millimeters(25.0),
            },
            thickness: Length::millimeters(3.0),
            airfoil: Airfoil::Square,
            mount: ComponentId::new(2),
            position: Length::millimeters(200.0),
            material: balsa,
        }));
        rocket.root.push(Component::LaunchLug(LaunchLug {
            id: ComponentId::new(4),
            position: Length::millimeters(100.0),
            material: card,
            length: Length::millimeters(25.0),
            inner_diameter: Length::millimeters(5.0),
            outer_diameter: Length::millimeters(7.0),
        }));
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(5),
            position: Length::millimeters(30.0),
            mass: Mass::grams(3.0),
            kind: MassObjectKind::Ballast,
        }));
        rocket
    }

    #[test]
    fn estes_alpha_cd_in_expected_range() {
        let rocket = build_estes_alpha();
        let cd = estimate_cd(&rocket, 0.1);
        // OpenRocket reports ~0.45–0.5 for the Estes Alpha. Our estimate
        // should land in the 0.3–0.7 range.
        assert!(
            (0.30..=0.70).contains(&cd),
            "Estes Alpha Cd should be 0.3–0.7, got {cd:.3}"
        );
    }

    #[test]
    fn cd_breakdown_sums_to_total() {
        let rocket = build_estes_alpha();
        let bd = estimate_cd_breakdown(&rocket);
        let sum = bd.cd_skin + bd.cd_base + bd.cd_proto;
        assert!(
            (bd.cd - sum).abs() < 1e-9,
            "Cd {bd:.3?} should equal skin + base + proto = {sum:.3}"
        );
    }

    #[test]
    fn cd_has_nonzero_skin_and_base() {
        let rocket = build_estes_alpha();
        let bd = estimate_cd_breakdown(&rocket);
        assert!(
            bd.cd_skin > 0.1,
            "skin drag should be > 0.1, got {}",
            bd.cd_skin
        );
        assert!(
            bd.cd_base > 0.01,
            "base drag should be > 0.01, got {}",
            bd.cd_base
        );
    }

    #[test]
    fn cd_is_constant_with_mach_in_phase2() {
        // For Phase 2, Cd does not vary with Mach.
        let rocket = build_estes_alpha();
        let cd_01 = estimate_cd(&rocket, 0.1);
        let cd_05 = estimate_cd(&rocket, 0.5);
        assert!(
            (cd_01 - cd_05).abs() < 1e-9,
            "Cd should be constant with Mach in Phase 2"
        );
    }

    #[test]
    fn longer_rocket_has_higher_cd() {
        // A longer body tube means more wetted area → higher skin drag.
        let mut short = build_estes_alpha();
        let mut long = build_estes_alpha();

        // Make the long rocket's body tube twice as long.
        if let Component::BodyTube(bt) = &mut long.root.components[1] {
            bt.length = Length::millimeters(360.0);
        }

        let cd_short = estimate_cd(&short, 0.1);
        let cd_long = estimate_cd(&long, 0.1);
        assert!(
            cd_long > cd_short,
            "longer rocket should have higher Cd: short={cd_short:.3}, long={cd_long:.3}"
        );

        // Restore for the empty-rocket test below.
        let _ = &mut short;
    }

    #[test]
    fn empty_rocket_has_zero_cd() {
        let rocket = Rocket::new("Empty");
        let bd = estimate_cd_breakdown(&rocket);
        assert_eq!(bd, DragBreakdown::ZERO);
    }

    #[test]
    fn boattail_reduces_base_drag() {
        // A rocket with a boattail (aft diameter < max body diameter)
        // should have lower *base* drag than one without. (The total Cd
        // may not decrease for a very short boattail, since the added
        // wetted area adds skin friction — but the base component drops.)
        use rocket_core::Transition;

        let mut rocket = build_estes_alpha();
        let balsa = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .unwrap()
            .id;
        rocket.root.push(Component::Transition(Transition {
            id: ComponentId::new(6),
            position: Length::millimeters(250.0),
            material: balsa,
            length: Length::millimeters(20.0),
            fore_diameter: Length::millimeters(24.8),
            aft_diameter: Length::millimeters(18.0),
            wall_thickness: Length::ZERO,
        }));

        let bd_no_boattail = estimate_cd_breakdown(&build_estes_alpha());
        let bd_with_boattail = estimate_cd_breakdown(&rocket);
        assert!(
            bd_with_boattail.cd_base < bd_no_boattail.cd_base,
            "boattail should reduce base drag: no={}, with={}",
            bd_no_boattail.cd_base,
            bd_with_boattail.cd_base
        );
    }

    #[test]
    fn more_fins_increase_drag() {
        let three_fins = build_estes_alpha();
        let mut four_fins = build_estes_alpha();

        if let Component::FinSet(fs) = &mut four_fins.root.components[2] {
            fs.count = 4;
        }

        let cd3 = estimate_cd(&three_fins, 0.1);
        let cd4 = estimate_cd(&four_fins, 0.1);
        assert!(
            cd4 > cd3,
            "4 fins should have higher Cd than 3: 3={cd3:.3}, 4={cd4:.3}"
        );
    }

    #[test]
    fn reference_area_is_body_cross_section() {
        let rocket = build_estes_alpha();
        let bd = estimate_cd_breakdown(&rocket);
        let d = 0.0248_f64; // Estes Alpha body diameter.
        let expected = std::f64::consts::PI * (d / 2.0).powi(2);
        assert!(
            (bd.reference_area - expected).abs() < 1e-9,
            "reference area should be {expected:.6e}, got {bd:.3?}"
        );
    }
}
