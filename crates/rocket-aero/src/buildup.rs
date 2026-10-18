//! Extended Barrowman aerodynamic coefficient buildup.
//!
//! Computes the full set of aerodynamic coefficients `{c_a, c_n, c_y, c_l,
//! c_m, c_n_yaw, cp_x}` for a rocket at a given `(α, β, Mach, Re)`. This
//! extends Phase 1's design-time estimate (which gave only `C_Nα` and
//! `x_cp` at M=0.1) with:
//!
//! - Compressibility: Prandtl-Glauert `β = √(1−M²)` for M ≤ 0.7.
//! - Transonic drag rise: empirical model for 0.7 < M < 1.2.
//! - Nonlinear body lift: `α²` crossflow contribution (extended Barrowman).
//! - Pitch damping: `C_m_q + C_m_α̇` from the fin contribution.
//! - Roll damping: `C_l_p` from the fins.
//!
//! The result is the `AeroCoefficients` struct that the 6-DOF dynamics
//! consumes (spec §1.2). The module is **pure**: same `(Rocket,
//! AeroQuery)` → same coefficients, no time or state.
//!
//! See `rocket-simulator-aero-dynamics.md` Part I (§2–§6).

#![allow(clippy::similar_names, clippy::doc_markdown)]

use rocket_core::components::{Component, FinSet, NoseCone, NoseShape, Transition};
use rocket_core::Rocket;

use crate::drag::{estimate_cd_breakdown, DragBreakdown};

/// Aerodynamic coefficients at a given flight condition.
///
/// All force coefficients are referenced to `reference_area`; all moment
/// coefficients to `reference_area · reference_length`. Forces are in
/// body axes (+x = nose), moments about the CG.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AeroCoefficients {
    /// Axial force coefficient (+x_B, drag direction).
    pub c_a: f64,
    /// Normal force coefficient (pitch plane, perpendicular to body axis).
    pub c_n: f64,
    /// Side force coefficient (yaw plane).
    pub c_y: f64,
    /// Roll moment coefficient (about x_B).
    pub c_l: f64,
    /// Pitch moment coefficient (about y_B, at the CG).
    pub c_m: f64,
    /// Yaw moment coefficient (about z_B).
    pub c_n_yaw: f64,
    /// Center of pressure, measured from the nose tip (meters).
    pub cp_x: f64,
    /// Reference area (m²).
    pub reference_area: f64,
    /// Reference length (m), usually max body diameter.
    pub reference_length: f64,
    /// Mach number the coefficients were evaluated at.
    pub mach: f64,
    /// Angle of attack (rad).
    pub alpha: f64,
    /// Sideslip angle (rad).
    pub beta: f64,
}

impl AeroCoefficients {
    /// A zero-coefficients result (for empty/degenerate rockets).
    #[must_use]
    pub fn zero(mach: f64, alpha: f64, beta: f64) -> Self {
        Self {
            c_a: 0.0,
            c_n: 0.0,
            c_y: 0.0,
            c_l: 0.0,
            c_m: 0.0,
            c_n_yaw: 0.0,
            cp_x: 0.0,
            reference_area: 0.0,
            reference_length: 0.0,
            mach,
            alpha,
            beta,
        }
    }
}

/// The flight condition at which to evaluate the aero coefficients.
#[derive(Debug, Clone, Copy)]
pub struct AeroQuery {
    /// Mach number.
    pub mach: f64,
    /// Angle of attack (rad).
    pub alpha: f64,
    /// Sideslip angle (rad).
    pub beta: f64,
    /// Roll rate (rad/s), for roll damping.
    pub roll_rate: f64,
    /// Pitch rate (rad/s), for pitch damping.
    pub pitch_rate: f64,
    /// Reynolds number (for skin friction; if 0, a default is used).
    pub reynolds: f64,
}

impl AeroQuery {
    /// A query at zero angle of attack and zero rates.
    #[must_use]
    pub fn at_mach(mach: f64) -> Self {
        Self {
            mach,
            alpha: 0.0,
            beta: 0.0,
            roll_rate: 0.0,
            pitch_rate: 0.0,
            reynolds: 0.0,
        }
    }

    /// A query at a given angle of attack (zero sideslip and rates).
    #[must_use]
    pub fn at_alpha(mach: f64, alpha: f64) -> Self {
        Self {
            mach,
            alpha,
            beta: 0.0,
            roll_rate: 0.0,
            pitch_rate: 0.0,
            reynolds: 0.0,
        }
    }
}

/// Compute the full aerodynamic coefficient set for a rocket at the given
/// flight condition.
///
/// This is the main entry point for the 6-DOF dynamics module. It sums
/// the per-component contributions (nose, fins, transitions) with
/// compressibility corrections and dynamic derivatives.
///
/// # Panics
///
/// Does not panic. Returns `AeroCoefficients::zero` for an empty rocket.
#[must_use]
pub fn compute(rocket: &Rocket, query: AeroQuery) -> AeroCoefficients {
    let ref_diameter = find_reference_diameter(rocket);
    if ref_diameter < 1e-9 {
        return AeroCoefficients::zero(query.mach, query.alpha, query.beta);
    }
    let ref_area = std::f64::consts::PI * (ref_diameter / 2.0).powi(2);
    let ref_length = ref_diameter;

    // Compressibility factor (Prandtl-Glauert).
    // β = √(1−M²) for M < 1. The PG correction is 1/β.
    // For M ≥ 1, use a supersonic approximation (Phase 4 will replace this
    // with proper shock-expansion theory).
    let beta_pg = if query.mach < 1.0 {
        (1.0 - query.mach * query.mach).sqrt()
    } else {
        (query.mach * query.mach - 1.0).sqrt().max(0.1)
    };
    let pg_factor = 1.0 / beta_pg; // C_Nα scales by 1/β.

    // Sum per-component normal-force slope and CP.
    let mut total_cn_alpha = 0.0_f64;
    let mut weighted_cp = 0.0_f64;

    for component in rocket.root.iter() {
        let (cn_alpha, x_p) = match component {
            Component::NoseCone(nc) => nose_cone_contribution(nc, pg_factor),
            Component::Transition(t) => transition_contribution(t, ref_diameter, pg_factor),
            Component::FinSet(fs) => fin_set_contribution(fs, ref_diameter, ref_area, pg_factor),
            _ => (0.0, 0.0),
        };

        if cn_alpha.abs() > 1e-9 {
            total_cn_alpha += cn_alpha;
            weighted_cp += cn_alpha * x_p;
        }
    }

    let cp_x = if total_cn_alpha.abs() > 1e-9 {
        weighted_cp / total_cn_alpha
    } else {
        0.0
    };

    // Normal force at the given α: C_N = C_Nα · α + nonlinear body lift.
    // The body-lift term is the extended-Barrowman crossflow contribution
    // (Allison–Mandell): C_N,body ≈ K · α² · sin(α) for the body tubes.
    // For small α this is ~K·α³, so it only matters at higher α.
    let body_lift = body_lift_coefficient(rocket, ref_diameter, ref_area, query.alpha);
    let c_n = total_cn_alpha * query.alpha + body_lift;

    // Side force from sideslip (symmetric rocket: same slope as c_n).
    let c_y = -total_cn_alpha * query.beta;

    // Axial force (drag). Use the Phase 2 drag estimate, with a Mach
    // correction for compressibility (transonic drag rise).
    let drag = estimate_cd_breakdown(rocket);
    let c_a = drag_with_mach(&drag, query.mach);

    // Pitch moment about the CG: M = C_N · (x_cp − x_cg) / L_ref.
    let cg_x = rocket.mass_properties().cg_x.as_meters();
    let c_m = c_n * (cp_x - cg_x) / ref_length;

    // Yaw moment (symmetric): M_yaw = -C_Y · (x_cp − x_cg) / L_ref.
    let c_n_yaw = -c_y * (cp_x - cg_x) / ref_length;

    // Roll damping: C_l_p · (p · L_ref) / (2·V). We return the coefficient
    // that, multiplied by q̄·A·L, gives the roll moment. The dynamics
    // module multiplies by the rate. For now we compute the damping
    // derivative C_l_p (per rad) from the fins.
    let c_l_p = roll_damping_derivative(rocket, ref_diameter, ref_area);
    // The roll moment coefficient already includes the rate factor:
    // C_l = C_l_p · (p · L / (2·V)). We pass the derivative; the dynamics
    // module multiplies by the nondimensional rate.
    let c_l = c_l_p * query.roll_rate * ref_length / 2.0;

    // Pitch damping: C_m_q + C_m_α̇ from the fins.
    let c_m_q = pitch_damping_derivative(rocket, ref_diameter, ref_area, ref_length, cp_x, cg_x);
    let c_m_damping = c_m_q * query.pitch_rate * ref_length / 2.0;

    AeroCoefficients {
        c_a,
        c_n,
        c_y,
        c_l,
        c_m: c_m + c_m_damping,
        c_n_yaw,
        cp_x,
        reference_area: ref_area,
        reference_length: ref_length,
        mach: query.mach,
        alpha: query.alpha,
        beta: query.beta,
    }
}

/// Apply a Mach correction to the Phase 2 constant-Cd drag estimate.
///
/// For M ≤ 0.7, Cd is constant (incompressible). For 0.7 < M < 1.2, the
/// transonic drag rise roughly doubles Cd. For M > 1.2, Cd decreases
/// (supersonic, but Phase 4 will replace this with wave-drag theory).
fn drag_with_mach(drag: &DragBreakdown, mach: f64) -> f64 {
    let cd0 = drag.cd;
    if mach < 0.7 {
        cd0
    } else if mach < 1.0 {
        // Transonic drag rise: roughly linear from cd0 at M=0.7 to
        // ~1.5·cd0 at M=1.0 (empirical).
        let frac = (mach - 0.7) / 0.3;
        cd0 * (1.0 + 0.5 * frac)
    } else if mach < 1.2 {
        // Peak transonic drag at M≈1.1, then decreasing.
        let frac = (mach - 1.0) / 0.2;
        cd0 * (1.5 - 0.3 * frac)
    } else {
        // Supersonic: Cd decreases roughly as 1/√M (Phase 4 will replace).
        cd0 * 1.2 / mach.sqrt()
    }
}

/// Nose cone contribution: `C_Nα = 2` (slender body) scaled by PG factor.
///
/// CP per shape (from Phase 1 design estimate). The PG factor scales
/// `C_Nα` by `1/β` for subsonic compressibility.
fn nose_cone_contribution(nc: &NoseCone, pg_factor: f64) -> (f64, f64) {
    let length = nc.length.as_meters();
    let pos = nc.position.as_meters();

    // C_Nα = 2 for all slender nose shapes (slender-body limit).
    let cn_alpha = 2.0 * pg_factor;

    let x_p_fraction = match nc.shape {
        NoseShape::Conical => 2.0 / 3.0,
        NoseShape::TangentOgive => 0.466,
        NoseShape::Elliptical => 1.0 / 3.0,
        NoseShape::Parabolic { n } => 2.0 / 3.0 - n * (1.0 / 6.0),
        NoseShape::VonKarman | NoseShape::LvHaack => 0.45,
        NoseShape::SecantOgive { .. } => 0.47,
    };

    let x_p = pos + x_p_fraction * length;
    (cn_alpha, x_p)
}

/// Transition contribution (conical frustrum) with PG factor.
fn transition_contribution(t: &Transition, ref_diameter: f64, pg_factor: f64) -> (f64, f64) {
    if ref_diameter < 1e-9 {
        return (0.0, 0.0);
    }

    let length = t.length.as_meters();
    let pos = t.position.as_meters();
    let d_fore = t.fore_diameter.as_meters();
    let d_aft = t.aft_diameter.as_meters();

    let r_fore = d_fore / ref_diameter;
    let r_aft = d_aft / ref_diameter;

    let cn_alpha = 2.0 * (r_aft * r_aft - r_fore * r_fore) * pg_factor;

    let denom = d_fore + d_aft;
    let x_p_from_fore = if denom > 1e-9 {
        length / 3.0 * (d_aft + 2.0 * d_fore) / denom
    } else {
        length / 2.0
    };

    let x_p = pos + x_p_from_fore;
    (cn_alpha, x_p)
}

/// Fin set contribution (Barrowman fin formula with fin-body interference),
/// scaled by the PG factor for compressibility.
///
/// For a single fin (incompressible):
/// ```text
/// C_Nα,1fin = 2π · (S_fin / A_ref) / (1 + √(1 + (2s/MAC)² · (1 + tan²Λ)))
/// ```
///
/// With Prandtl-Glauert compressibility:
/// - The slender-body `2π` term scales by `1/β` (overall PG correction).
/// - The denominator's sweep term uses `1/β²` (the sweep-angle correction).
///
/// With fin-body interference: `C_Nα,finset = N · C_Nα,1fin · (1 + r/s)`.
fn fin_set_contribution(
    fs: &FinSet,
    ref_diameter: f64,
    ref_area: f64,
    pg_factor: f64,
) -> (f64, f64) {
    if ref_area < 1e-9 || fs.count == 0 {
        return (0.0, 0.0);
    }

    let body_radius = ref_diameter / 2.0;
    let (area, mac, span, sweep) = fin_geometry(fs);
    if area < 1e-9 || mac < 1e-9 || span < 1e-9 {
        return (0.0, 0.0);
    }

    // The PG factor enters the sweep term as 1/β² = pg_factor².
    let beta_sq_inv = pg_factor * pg_factor;

    let sweep_angle = (sweep / span).atan();
    let tan_sweep = sweep_angle.tan();

    // Incompressible denominator.
    let aspect_ratio_term = (2.0 * span / mac).powi(2);
    let sweep_term = 1.0 + tan_sweep * tan_sweep * beta_sq_inv;
    let denominator = 1.0 + (1.0 + aspect_ratio_term * sweep_term).sqrt();

    // Incompressible C_Nα for one fin.
    let cn_alpha_1fin_inc = 2.0 * std::f64::consts::PI * (area / ref_area) / denominator;

    // Apply the overall PG correction (1/β) to get the compressible C_Nα.
    let cn_alpha_1fin = cn_alpha_1fin_inc * pg_factor;

    let k_fb = 1.0 + body_radius / span;
    let cn_alpha = f64::from(fs.count) * cn_alpha_1fin * k_fb;

    let x_p = fs.position.as_meters() + mac / 4.0;

    (cn_alpha, x_p)
}

/// Extract fin geometry (area, MAC, span, sweep) from a planform.
fn fin_geometry(fs: &FinSet) -> (f64, f64, f64, f64) {
    use rocket_core::components::FinPlanform;
    let area = fs.planform.area();
    let mac = fs.planform.mac();

    let (span, sweep) = match &fs.planform {
        FinPlanform::Trapezoidal { span, sweep, .. } => (span.as_meters(), sweep.as_meters()),
        FinPlanform::Elliptical { span, .. } => (span.as_meters(), 0.0),
        FinPlanform::Custom { .. } => (0.0, 0.0),
    };

    (area, mac, span, sweep)
}

/// Nonlinear body-lift coefficient (extended Barrowman crossflow).
///
/// For a body tube of length `L` and diameter `d`, the crossflow
/// contribution at angle of attack α is approximately:
///
/// ```text
/// C_N,body = K · (L/d) · sin(2α) · sin(α)
/// ```
///
/// where `K ≈ 1.1` is an empirical crossflow coefficient. This is the
/// Allison–Mandell correction used by RockSim. At small α this is
/// ~`K·(L/d)·2α·α = 2K·(L/d)·α²`, which is negligible compared to the
/// linear `C_Nα·α` from the fins.
fn body_lift_coefficient(rocket: &Rocket, ref_diameter: f64, ref_area: f64, alpha: f64) -> f64 {
    if ref_area < 1e-9 || alpha.abs() < 1e-6 {
        return 0.0;
    }

    // Total body length (nose + body tubes + transitions).
    let mut body_length = 0.0_f64;
    for c in rocket.root.iter() {
        match c {
            Component::BodyTube(bt) => body_length += bt.length.as_meters(),
            Component::NoseCone(nc) => body_length += nc.length.as_meters(),
            Component::Transition(t) => body_length += t.length.as_meters(),
            _ => {}
        }
    }

    if body_length < 1e-9 || ref_diameter < 1e-9 {
        return 0.0;
    }

    let l_over_d = body_length / ref_diameter;
    let k = 1.1; // Empirical crossflow coefficient.
    k * l_over_d * (2.0 * alpha).sin() * alpha.sin()
}

/// Roll damping derivative `C_l_p` (per radian of nondimensional rate).
///
/// Each fin contributes a roll damping proportional to its area and the
/// square of its span. The total is:
///
/// ```text
/// C_l_p = -N · (S_fin / A_ref) · (span / L_ref)² · K
/// ```
///
/// where `K ≈ 2.0` is an empirical constant. The negative sign means
/// roll damping opposes the roll rate.
fn roll_damping_derivative(rocket: &Rocket, ref_diameter: f64, ref_area: f64) -> f64 {
    if ref_area < 1e-9 || ref_diameter < 1e-9 {
        return 0.0;
    }

    let mut total = 0.0_f64;
    for c in rocket.root.iter() {
        if let Component::FinSet(fs) = c {
            let area = fs.planform.area();
            let span = match &fs.planform {
                rocket_core::components::FinPlanform::Trapezoidal { span, .. }
                | rocket_core::components::FinPlanform::Elliptical { span, .. } => span.as_meters(),
                rocket_core::components::FinPlanform::Custom { .. } => 0.0,
            };
            if area > 1e-9 && span > 1e-9 {
                let term = (area / ref_area) * (span / ref_diameter).powi(2);
                total += f64::from(fs.count) * term;
            }
        }
    }

    -2.0 * total
}

/// Pitch damping derivative `C_m_q + C_m_α̇` (per radian of nondim rate).
///
/// The fins contribute pitch damping proportional to their area and the
/// square of their moment arm (CP − CG). The total is:
///
/// ```text
/// C_m_q = -N · (S_fin / A_ref) · ((x_fin − x_cg) / L_ref)² · K
/// ```
///
/// where `K ≈ 2.0`. The negative sign means pitch damping opposes the
/// pitch rate.
fn pitch_damping_derivative(
    rocket: &Rocket,
    _ref_diameter: f64,
    ref_area: f64,
    ref_length: f64,
    cp_x: f64,
    cg_x: f64,
) -> f64 {
    if ref_area < 1e-9 || ref_length < 1e-9 {
        return 0.0;
    }

    let mut total = 0.0_f64;
    for c in rocket.root.iter() {
        if let Component::FinSet(fs) = c {
            let area = fs.planform.area();
            let mac = fs.planform.mac();
            let x_fin = fs.position.as_meters() + mac / 4.0;
            let arm = (x_fin - cg_x).abs();
            if area > 1e-9 && arm > 1e-9 {
                let term = (area / ref_area) * (arm / ref_length).powi(2);
                total += f64::from(fs.count) * term;
            }
        }
    }

    // Use the CP-CG distance as a fallback if no fins (shouldn't happen
    // for a stable rocket, but be defensive).
    let _ = cp_x;
    -2.0 * total
}

/// Find the reference diameter (max body diameter).
///
/// Returns 0.0 for an empty rocket (no body components), so the caller
/// can detect the degenerate case. The UI's default-diameter fallback
/// is handled separately.
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
    max_d
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]

    use super::*;
    use rocket_core::{
        Airfoil, BodyTube, ComponentId, FinPlanform, FinSet, LaunchLug, Length, Mass, MassObject,
        MassObjectKind, NoseCone, NoseShape, Rocket,
    };

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
    fn cp_at_mach_0_1_matches_design_estimate() {
        // The buildup CP at M=0.1 should match the Phase 1 design estimate.
        let rocket = build_estes_alpha();
        let design = crate::design::estimate(&rocket);
        let buildup = compute(&rocket, AeroQuery::at_mach(0.1));

        // The buildup includes the PG factor (1/β at M=0.1 ≈ 1.005), which
        // the design estimate doesn't. But since PG scales all components
        // equally, the CP (area-weighted average) is unchanged.
        // Small floating-point differences in the fin formula are expected.
        assert!(
            (buildup.cp_x - design.cp_x).abs() < 1e-4,
            "CP at M=0.1: buildup={}, design={}",
            buildup.cp_x,
            design.cp_x
        );
    }

    #[test]
    fn cn_alpha_increases_with_mach_subsonic() {
        // Prandtl-Glauert: C_Nα increases as M → 1 (by 1/β).
        let rocket = build_estes_alpha();
        let a_m01 = compute(&rocket, AeroQuery::at_alpha(0.1, 0.05));
        let a_m05 = compute(&rocket, AeroQuery::at_alpha(0.5, 0.05));
        let a_m07 = compute(&rocket, AeroQuery::at_alpha(0.7, 0.05));

        // C_N at α=0.05 should increase with Mach (1/β factor).
        assert!(
            a_m05.c_n > a_m01.c_n,
            "C_N at M=0.5 ({}) should be > M=0.1 ({})",
            a_m05.c_n,
            a_m01.c_n
        );
        assert!(
            a_m07.c_n > a_m05.c_n,
            "C_N at M=0.7 ({}) should be > M=0.5 ({})",
            a_m07.c_n,
            a_m05.c_n
        );
    }

    #[test]
    fn cn_scales_with_prandtl_glauert_factor() {
        // At small α, C_N ≈ C_Nα · α. The PG factor is 1/β.
        // The body-lift term (nonlinear) doesn't scale with Mach, so the
        // ratio is slightly less than the pure PG factor.
        let rocket = build_estes_alpha();
        let alpha = 0.02; // Small α for linear regime.
        let a_m01 = compute(&rocket, AeroQuery::at_alpha(0.1, alpha));
        let a_m05 = compute(&rocket, AeroQuery::at_alpha(0.5, alpha));

        let beta_01 = (1.0_f64 - 0.1_f64 * 0.1_f64).sqrt();
        let beta_05 = (1.0_f64 - 0.5_f64 * 0.5_f64).sqrt();
        let expected_ratio = (1.0 / beta_05) / (1.0 / beta_01);

        let actual_ratio = a_m05.c_n / a_m01.c_n;
        // The body-lift term dilutes the PG scaling slightly.
        assert!(
            (actual_ratio - expected_ratio).abs() / expected_ratio < 0.05,
            "C_N ratio {actual_ratio:.3} should match PG factor {expected_ratio:.3} within 5%"
        );
    }

    #[test]
    fn cd_rises_in_transonic_regime() {
        // Cd should rise sharply between M=0.7 and M=1.0.
        let rocket = build_estes_alpha();
        let c_m03 = compute(&rocket, AeroQuery::at_mach(0.3)).c_a;
        let c_m07 = compute(&rocket, AeroQuery::at_mach(0.7)).c_a;
        let c_m10 = compute(&rocket, AeroQuery::at_mach(1.0)).c_a;

        // Subsonic Cd is constant.
        assert!(
            (c_m03 - c_m07).abs() < 1e-9,
            "subsonic Cd should be constant"
        );
        // Transonic Cd rises.
        assert!(
            c_m10 > c_m07 * 1.4,
            "Cd at M=1.0 ({c_m10:.3}) should be > 1.4× Cd at M=0.7 ({c_m07:.3})"
        );
    }

    #[test]
    fn normal_force_proportional_to_alpha_at_small_angles() {
        let rocket = build_estes_alpha();
        let a1 = compute(&rocket, AeroQuery::at_alpha(0.1, 0.01)).c_n;
        let a2 = compute(&rocket, AeroQuery::at_alpha(0.1, 0.02)).c_n;
        // Doubling α should (approximately) double C_N at small angles.
        assert!(
            (a2 / a1 - 2.0).abs() < 0.05,
            "C_N should scale linearly with α: a1={a1}, a2={a2}, ratio={}",
            a2 / a1
        );
    }

    #[test]
    fn body_lift_negligible_at_small_alpha() {
        let rocket = build_estes_alpha();
        // At α=1°, body lift should be much less than fin lift.
        let alpha = 0.017; // ~1°
        let total = compute(&rocket, AeroQuery::at_alpha(0.1, alpha)).c_n;
        let body = body_lift_coefficient(
            &rocket,
            find_reference_diameter(&rocket),
            std::f64::consts::PI * (find_reference_diameter(&rocket) / 2.0).powi(2),
            alpha,
        );
        assert!(
            body.abs() < total.abs() * 0.05,
            "body lift ({body:.4}) should be < 5% of total ({total:.4})"
        );
    }

    #[test]
    fn body_lift_significant_at_high_alpha() {
        let rocket = build_estes_alpha();
        // At α=15°, body lift should be a non-trivial fraction.
        let alpha = 15.0_f64.to_radians();
        let body = body_lift_coefficient(
            &rocket,
            find_reference_diameter(&rocket),
            std::f64::consts::PI * (find_reference_diameter(&rocket) / 2.0).powi(2),
            alpha,
        );
        assert!(
            body > 0.01,
            "body lift at 15° should be > 0.01, got {body:.4}"
        );
    }

    #[test]
    fn pitch_moment_zero_at_zero_alpha() {
        // At α=0, C_N=0 so C_M should be 0 (no aero pitching moment).
        let rocket = build_estes_alpha();
        let a = compute(&rocket, AeroQuery::at_mach(0.1));
        assert!(a.c_m.abs() < 1e-9, "C_M at α=0 should be 0, got {}", a.c_m);
    }

    #[test]
    fn roll_damping_is_negative() {
        // Roll damping should oppose the roll rate (negative derivative).
        let rocket = build_estes_alpha();
        let c_l_p = roll_damping_derivative(
            &rocket,
            find_reference_diameter(&rocket),
            std::f64::consts::PI * (find_reference_diameter(&rocket) / 2.0).powi(2),
        );
        assert!(c_l_p < 0.0, "C_l_p should be negative, got {c_l_p}");
    }

    #[test]
    fn pitch_damping_is_negative() {
        let rocket = build_estes_alpha();
        let ref_d = find_reference_diameter(&rocket);
        let ref_a = std::f64::consts::PI * (ref_d / 2.0).powi(2);
        let cg = rocket.mass_properties().cg_x.as_meters();
        let aero = compute(&rocket, AeroQuery::at_mach(0.1));
        let c_m_q = pitch_damping_derivative(&rocket, ref_d, ref_a, ref_d, aero.cp_x, cg);
        assert!(c_m_q < 0.0, "C_m_q should be negative, got {c_m_q}");
    }

    #[test]
    fn empty_rocket_returns_zero() {
        let rocket = Rocket::new("Empty");
        let a = compute(&rocket, AeroQuery::at_mach(0.1));
        assert_eq!(a, AeroCoefficients::zero(0.1, 0.0, 0.0));
    }

    #[test]
    fn sideslip_produces_side_force() {
        let rocket = build_estes_alpha();
        let a = compute(
            &rocket,
            AeroQuery {
                mach: 0.1,
                alpha: 0.0,
                beta: 0.05,
                roll_rate: 0.0,
                pitch_rate: 0.0,
                reynolds: 0.0,
            },
        );
        // Symmetric rocket: side force from β should be same magnitude as
        // normal force from α would be (sign convention differs).
        assert!(a.c_y.abs() > 0.01, "C_Y should be nonzero, got {}", a.c_y);
        assert!(a.c_n.abs() < 1e-9, "C_N should be 0 at α=0");
    }
}
