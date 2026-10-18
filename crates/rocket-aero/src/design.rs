//! Design-time aerodynamic approximation.
//!
//! A fast, simplified Barrowman calculation for live UI feedback.
//! Computes `C_Nα` (normal-force slope) and `x_cp` (center of pressure)
//! at M=0.1 (subsonic, near-zero compressibility) for the components
//! that contribute to static stability: nose cones, fin sets, and
//! transitions.
//!
//! This is **not** the full aero module (Phase 3). It omits:
//! - Compressibility corrections (Prandtl-Glauert, transonic, supersonic)
//! - Skin friction and drag
//! - Dynamic derivatives (pitch/roll damping)
//! - Nonlinear/high-α effects
//!
//! It runs in <1 ms for a typical 10-component rocket, making it
//! suitable for per-frame UI updates as the user edits the design.
//!
//! See `rocket-simulator-aero-dynamics.md` §2 (subsonic buildup).

use rocket_core::components::{Component, FinPlanform, FinSet, NoseCone, NoseShape, Transition};
use rocket_core::Rocket;

/// The Mach number used for the design estimate.
const DESIGN_MACH: f64 = 0.1;

/// The result of a design-time aero estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignAero {
    /// Total normal-force slope `C_Nα` (per radian).
    pub cn_alpha: f64,
    /// Center of pressure, measured from the nose tip (meters).
    pub cp_x: f64,
    /// The Mach number this estimate was computed at.
    pub mach: f64,
}

impl DesignAero {
    /// A zero/empty estimate (for rockets with no aero components).
    pub const ZERO: Self = Self {
        cn_alpha: 0.0,
        cp_x: 0.0,
        mach: DESIGN_MACH,
    };
}

/// Compute a design-time aero estimate for a rocket.
///
/// Returns `(C_Nα_total, x_cp)` at M=0.1. The CP is the area-weighted
/// average of each contributing component's CP:
/// ```text
/// x_cp = Σ(C_Nα,i · x_p,i) / Σ(C_Nα,i)
/// ```
///
/// Only nose cones, fin sets, and transitions contribute (body tubes
/// contribute zero `C_Nα` in linear Barrowman theory).
///
/// # Panics
///
/// Does not panic; returns `DesignAero::ZERO` for an empty rocket.
#[must_use]
pub fn estimate(rocket: &Rocket) -> DesignAero {
    let mut total_cn_alpha = 0.0_f64;
    let mut weighted_cp = 0.0_f64;

    // Find the reference diameter (max body diameter, or nose base if
    // no body tube exists).
    let ref_diameter = find_reference_diameter(rocket);
    let ref_area = std::f64::consts::PI * (ref_diameter / 2.0).powi(2);

    for component in rocket.root.iter() {
        let (cn_alpha, x_p) = match component {
            Component::NoseCone(nc) => nose_cone_contribution(nc),
            Component::Transition(t) => transition_contribution(t, ref_diameter),
            Component::FinSet(fs) => fin_set_contribution(fs, ref_diameter, ref_area),
            // Body tubes, couplers, launch lugs, pods, mass objects
            // contribute zero C_Nα in linear Barrowman.
            _ => (0.0, 0.0),
        };

        if cn_alpha.abs() > 1e-9 {
            total_cn_alpha += cn_alpha;
            weighted_cp += cn_alpha * x_p;
        }
    }

    if total_cn_alpha.abs() < 1e-9 {
        return DesignAero::ZERO;
    }

    DesignAero {
        cn_alpha: total_cn_alpha,
        cp_x: weighted_cp / total_cn_alpha,
        mach: DESIGN_MACH,
    }
}

/// Find the reference diameter (max body tube outer diameter, or nose
/// cone base diameter if no body tube exists).
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

/// Nose cone contribution: `C_Nα = 2` (slender body limit), `x_p` depends
/// on the shape.
///
/// For a conical nose: `x_p = 2/3 · L` (exact).
/// For other shapes: approximate `x_p` based on the profile.
fn nose_cone_contribution(nc: &NoseCone) -> (f64, f64) {
    let length = nc.length.as_meters();
    let pos = nc.position.as_meters();

    // C_Nα = 2 for all slender nose shapes (the slender-body limit).
    let cn_alpha = 2.0;

    // x_p depends on the shape. The exact values require integrating
    // the pressure distribution; here we use the known closed-form values.
    let x_p_fraction = match nc.shape {
        NoseShape::Conical => 2.0 / 3.0,    // Exact: 2/3 from the tip.
        NoseShape::TangentOgive => 0.466,   // Approximate for tangent ogive.
        NoseShape::Elliptical => 1.0 / 3.0, // CG of a half-ellipsoid.
        NoseShape::Parabolic { n } => {
            // For a parabolic series, the CP moves based on n.
            // n=0 (cone): 2/3, n=1 (full parabolic): ~1/2
            2.0 / 3.0 - n * (1.0 / 6.0)
        }
        NoseShape::VonKarman | NoseShape::LvHaack => 0.45, // Haack series: ~0.45
        NoseShape::SecantOgive { .. } => 0.47,             // Close to tangent ogive.
    };

    let x_p = pos + x_p_fraction * length;
    (cn_alpha, x_p)
}

/// Transition contribution (conical frustrum).
///
/// ```text
/// C_Nα,trans = 2 · ((d_aft/d_ref)² − (d_fore/d_ref)²)    (per rad)
/// x_p,trans = x_fore + L/3 · (d_aft + 2·d_fore) / (d_fore + d_aft)
/// ```
///
/// Sign convention: an expanding transition (`d_aft > d_fore`) adds
/// positive `C_Nα` with CP aft (stabilizing). A boattail (`d_aft < d_fore`)
/// adds negative `C_Nα` with CP aft (destabilizing — moves overall CP
/// forward).
fn transition_contribution(t: &Transition, ref_diameter: f64) -> (f64, f64) {
    if ref_diameter < 1e-9 {
        return (0.0, 0.0);
    }

    let length = t.length.as_meters();
    let pos = t.position.as_meters();
    let d_fore = t.fore_diameter.as_meters();
    let d_aft = t.aft_diameter.as_meters();

    let r_fore = d_fore / ref_diameter;
    let r_aft = d_aft / ref_diameter;

    let cn_alpha = 2.0 * (r_aft * r_aft - r_fore * r_fore);

    // x_p measured from the fore end of the transition.
    let denom = d_fore + d_aft;
    let x_p_from_fore = if denom > 1e-9 {
        length / 3.0 * (d_aft + 2.0 * d_fore) / denom
    } else {
        length / 2.0
    };

    let x_p = pos + x_p_from_fore;
    (cn_alpha, x_p)
}

/// Fin set contribution (Barrowman fin formula with fin-body
/// interference).
///
/// For a single fin:
/// ```text
/// C_Nα,1fin = 2π · (S_fin / A_ref) / (1 + √(1 + (2s/MAC)² · (1 + tan²Λ/β²)))
/// ```
/// where:
/// - `S_fin` = fin planform area (one fin)
/// - `A_ref` = reference area (body cross-section)
/// - `s` = semispan (from body surface to fin tip)
/// - `MAC` = mean aerodynamic chord
/// - `Λ` = leading-edge sweep angle
/// - `β` = √(1−M²) ≈ 0.995 at M=0.1
///
/// With fin-body interference (Barrowman's classic factor):
/// ```text
/// C_Nα,finset = N · C_Nα,1fin · (1 + r/s)
/// ```
/// where `r` is the body radius.
///
/// The CP of the fin set is at the MAC quarter-chord position.
fn fin_set_contribution(fs: &FinSet, ref_diameter: f64, ref_area: f64) -> (f64, f64) {
    if ref_area < 1e-9 || fs.count == 0 {
        return (0.0, 0.0);
    }

    let body_radius = ref_diameter / 2.0;

    // Get fin geometry from the planform.
    let (area, mac, span, sweep) = fin_geometry(fs);
    if area < 1e-9 || mac < 1e-9 || span < 1e-9 {
        return (0.0, 0.0);
    }

    // β = √(1−M²) at M=0.1
    let beta = (1.0 - DESIGN_MACH * DESIGN_MACH).sqrt();

    // Leading-edge sweep angle (radians).
    // For a trapezoidal fin, sweep is the axial offset of the tip LE
    // from the root LE. The sweep angle is atan(sweep / span).
    let sweep_angle = (sweep / span).atan();
    let tan_sweep = sweep_angle.tan();

    // Single-fin C_Nα (Barrowman formula, per radian).
    // C_Nα,1fin = 2π · (S_fin / A_ref) / (1 + √(1 + (2s/MAC)² · (1 + tan²Λ/β²)))
    let aspect_ratio_term = (2.0 * span / mac).powi(2);
    let sweep_term = 1.0 + tan_sweep * tan_sweep / (beta * beta);
    let denominator = 1.0 + (1.0 + aspect_ratio_term * sweep_term).sqrt();
    let cn_alpha_1fin = 2.0 * std::f64::consts::PI * (area / ref_area) / denominator;

    // Fin-body interference factor (Barrowman's classic).
    // K_fb = 1 + r/s (the body "carries" the fin lift).
    let k_fb = 1.0 + body_radius / span;

    // Total fin set C_Nα.
    let cn_alpha = f64::from(fs.count) * cn_alpha_1fin * k_fb;

    // CP of the fin set: at the MAC quarter-chord from the fin's
    // leading edge, plus the fin's axial position.
    // For a trapezoidal fin, the MAC leading edge position from the
    // root leading edge is: x_MAC_LE = sweep * (c_t / (c_r + c_t))
    // (the MAC sits partway between root and tip).
    // For simplicity, we use the fin position + MAC/4.
    let x_p = fs.position.as_meters() + mac / 4.0;

    (cn_alpha, x_p)
}

/// Extract fin geometry (area, MAC, span, sweep) from a planform.
fn fin_geometry(fs: &FinSet) -> (f64, f64, f64, f64) {
    // area() and mac() are already in m and m².
    let area = fs.planform.area();
    let mac = fs.planform.mac();

    let (span, sweep) = match &fs.planform {
        FinPlanform::Trapezoidal { span, sweep, .. } => (span.as_meters(), sweep.as_meters()),
        FinPlanform::Elliptical { span, .. } => (span.as_meters(), 0.0),
        FinPlanform::Custom { .. } => (0.0, 0.0), // TODO: extract from custom points
    };

    (area, mac, span, sweep)
}

/// Compute the static margin in calibers for a rocket.
///
/// `SM = (x_cp − x_cg) / d_ref`
///
/// Positive = stable (CP behind CG), negative = unstable.
#[must_use]
pub fn static_margin(rocket: &Rocket) -> f64 {
    let aero = estimate(rocket);
    let mp = rocket.mass_properties();

    if aero.cn_alpha.abs() < 1e-9 {
        return f64::NAN; // No aero components.
    }

    let ref_d = find_reference_diameter(rocket);
    if ref_d < 1e-9 {
        return f64::NAN;
    }

    (aero.cp_x - mp.cg_x.as_meters()) / ref_d
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use rocket_core::{
        BodyTube, ComponentId, FinPlanform, Length, MassObject, MassObjectKind, NoseCone, NoseShape,
    };

    fn balsa_rocket() -> Rocket {
        Rocket::new("Test")
    }

    fn add_nose(rocket: &mut Rocket, shape: NoseShape, length_mm: f64, diam_mm: f64) {
        let balsa = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .unwrap()
            .id;
        rocket.root.push(Component::NoseCone(NoseCone {
            id: ComponentId::new(1),
            position: Length::ZERO,
            material: balsa,
            shape,
            length: Length::millimeters(length_mm),
            base_diameter: Length::millimeters(diam_mm),
            wall_thickness: Length::ZERO,
        }));
    }

    fn add_body(rocket: &mut Rocket, pos_mm: f64, length_mm: f64, diam_mm: f64) {
        let card = rocket
            .materials
            .iter()
            .find(|m| m.name == "Cardboard (body tube)")
            .unwrap()
            .id;
        rocket.root.push(Component::BodyTube(BodyTube {
            id: ComponentId::new(2),
            position: Length::millimeters(pos_mm),
            material: card,
            length: Length::millimeters(length_mm),
            outer_diameter: Length::millimeters(diam_mm),
            inner_diameter: Length::millimeters(diam_mm - 1.0),
        }));
    }

    fn add_fins(
        rocket: &mut Rocket,
        pos_mm: f64,
        count: u8,
        root_mm: f64,
        tip_mm: f64,
        span_mm: f64,
        sweep_mm: f64,
    ) {
        let balsa = rocket
            .materials
            .iter()
            .find(|m| m.name == "Balsa")
            .unwrap()
            .id;
        rocket.root.push(Component::FinSet(FinSet {
            id: ComponentId::new(3),
            count,
            planform: FinPlanform::Trapezoidal {
                root_chord: Length::millimeters(root_mm),
                tip_chord: Length::millimeters(tip_mm),
                span: Length::millimeters(span_mm),
                sweep: Length::millimeters(sweep_mm),
            },
            thickness: Length::millimeters(3.0),
            airfoil: rocket_core::Airfoil::Square,
            mount: ComponentId::new(2),
            position: Length::millimeters(pos_mm),
            material: balsa,
        }));
    }

    fn add_ballast(rocket: &mut Rocket, pos_mm: f64, mass_g: f64) {
        rocket.root.push(Component::MassObject(MassObject {
            id: ComponentId::new(4),
            position: Length::millimeters(pos_mm),
            mass: rocket_core::Mass::grams(mass_g),
            kind: MassObjectKind::Ballast,
        }));
    }

    // ---- SP-7224 acceptance tests ----

    #[test]
    fn conical_nose_cn_alpha_is_2() {
        let mut rocket = balsa_rocket();
        add_nose(&mut rocket, NoseShape::Conical, 76.2, 25.4);

        let aero = estimate(&rocket);
        assert!(
            (aero.cn_alpha - 2.0).abs() < 0.001,
            "C_Nα should be 2.0, got {}",
            aero.cn_alpha
        );
    }

    #[test]
    fn conical_nose_cp_is_two_thirds_length() {
        let mut rocket = balsa_rocket();
        let length_mm = 76.2;
        add_nose(&mut rocket, NoseShape::Conical, length_mm, 25.4);

        let aero = estimate(&rocket);
        let expected_cp = length_mm * 2.0 / 3.0;
        let actual_cp = aero.cp_x * 1000.0; // m → mm
        assert!(
            (actual_cp - expected_cp).abs() < 0.1,
            "x_cp should be {expected_cp} mm, got {actual_cp} mm"
        );
    }

    // ---- Fin contribution tests ----

    #[test]
    fn fins_add_positive_cn_alpha() {
        let mut rocket = balsa_rocket();
        add_nose(&mut rocket, NoseShape::Conical, 70.0, 25.0);
        add_body(&mut rocket, 70.0, 180.0, 25.0);
        add_fins(&mut rocket, 200.0, 3, 50.0, 25.0, 35.0, 25.0);

        let aero = estimate(&rocket);
        // With fins, C_Nα should be significantly more than the nose
        // cone alone (which contributes 2.0).
        assert!(
            aero.cn_alpha > 2.5,
            "C_Nα with fins should be > 2.5, got {}",
            aero.cn_alpha
        );
    }

    #[test]
    fn fins_move_cp_aft() {
        // Without fins, CP is at the nose cone's CP (~2/3 of nose length).
        // With fins, CP should move aft (toward the fins).
        let mut rocket_no_fins = balsa_rocket();
        add_nose(&mut rocket_no_fins, NoseShape::Conical, 70.0, 25.0);
        add_body(&mut rocket_no_fins, 70.0, 180.0, 25.0);
        let cp_no_fins = estimate(&rocket_no_fins).cp_x;

        let mut rocket_with_fins = balsa_rocket();
        add_nose(&mut rocket_with_fins, NoseShape::Conical, 70.0, 25.0);
        add_body(&mut rocket_with_fins, 70.0, 180.0, 25.0);
        add_fins(&mut rocket_with_fins, 200.0, 3, 50.0, 25.0, 35.0, 25.0);
        let cp_with_fins = estimate(&rocket_with_fins).cp_x;

        assert!(
            cp_with_fins > cp_no_fins,
            "CP with fins ({:.1} mm) should be aft of CP without fins ({:.1} mm)",
            cp_with_fins * 1000.0,
            cp_no_fins * 1000.0
        );
    }

    // ---- Boattail sign-convention test ----

    #[test]
    fn boattail_moves_cp_forward() {
        // A boattail (reducing tail diameter) adds negative C_Nα with
        // CP aft, moving the overall CP FORWARD (destabilizing).
        //
        // We test this by comparing two identical rockets that differ
        // only in the transition: one expanding (stabilizing) and one
        // contracting (boattail, destabilizing). The boattail rocket's
        // CP should be forward of the expanding-transition rocket's CP.
        use rocket_core::Transition;

        fn make_rocket(transition_kind: &str) -> Rocket {
            let mut rocket = balsa_rocket();
            add_nose(&mut rocket, NoseShape::Conical, 70.0, 30.0);
            add_body(&mut rocket, 70.0, 200.0, 30.0);
            let balsa = rocket
                .materials
                .iter()
                .find(|m| m.name == "Balsa")
                .unwrap()
                .id;
            let (fore, aft) = if transition_kind == "boattail" {
                (30.0, 20.0) // Boattail: contracting
            } else {
                (20.0, 30.0) // Expanding: stabilizing
            };
            rocket.root.push(Component::Transition(Transition {
                id: ComponentId::new(5),
                position: Length::millimeters(270.0),
                material: balsa,
                length: Length::millimeters(30.0),
                fore_diameter: Length::millimeters(fore),
                aft_diameter: Length::millimeters(aft),
                wall_thickness: Length::ZERO,
            }));
            rocket
        }

        let cp_expanding = estimate(&make_rocket("expanding")).cp_x;
        let cp_boattail = estimate(&make_rocket("boattail")).cp_x;

        assert!(
            cp_boattail < cp_expanding,
            "CP with boattail ({:.1} mm) should be FORWARD of CP with expanding transition ({:.1} mm)",
            cp_boattail * 1000.0,
            cp_expanding * 1000.0
        );
    }

    // ---- Static margin tests ----

    #[test]
    fn stable_rocket_has_positive_static_margin() {
        let mut rocket = balsa_rocket();
        add_nose(&mut rocket, NoseShape::Conical, 70.0, 25.0);
        add_body(&mut rocket, 70.0, 180.0, 25.0);
        add_fins(&mut rocket, 200.0, 3, 50.0, 25.0, 35.0, 25.0);
        // Add ballast at the nose to move CG forward.
        add_ballast(&mut rocket, 10.0, 20.0);

        let sm = static_margin(&rocket);
        assert!(
            sm.is_finite() && sm > 0.0,
            "stable rocket should have positive SM, got {sm}"
        );
    }

    #[test]
    fn empty_rocket_returns_zero_aero() {
        let rocket = balsa_rocket();
        let aero = estimate(&rocket);
        assert_eq!(aero, DesignAero::ZERO);
    }

    #[test]
    fn nose_only_rocket_has_cp_at_nose_cp() {
        let mut rocket = balsa_rocket();
        add_nose(&mut rocket, NoseShape::Conical, 60.0, 25.0);
        let aero = estimate(&rocket);
        // CP should be at 2/3 * 60 = 40 mm = 0.040 m
        assert!((aero.cp_x - 0.040).abs() < 0.0001);
    }
}
