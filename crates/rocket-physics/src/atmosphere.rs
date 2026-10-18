//! Atmosphere models for the Aether rocket simulator.
//!
//! Currently provides:
//! - The International Standard Atmosphere (ISA) troposphere model
//!   (0–11 km), used as the T0 fidelity tier by Phase 2's flight sim.
//! - The US-1976 standard atmosphere (0–50 km), used by Phase 3's 6-DOF
//!   sim. This covers the troposphere, stratosphere, and stratopause
//!   with piecewise-constant lapse rates.
//! - Phase 4 will add NRLMSISE-00 for T2 (Karman line).
//!
//! ## Reference
//!
//! ISO 2533:1975 / ICAO Doc 7488 for the troposphere; NASA-TM-X-74335
//! / NOAA-S/T 76-1562 for the US-1976 standard atmosphere.
//!
//! ## Formulas
//!
//! For a layer with base temperature `T_b`, base pressure `P_b`, lapse
//! rate `L`, and base altitude `h_b`:
//!
//! - Temperature: `T(h) = T_b + L · (h − h_b)`  (or `T_b` if `L = 0`)
//! - Pressure (lapse ≠ 0): `P(h) = P_b · (T_b / T)^(g·M/(R·L))`
//! - Pressure (isothermal): `P(h) = P_b · exp(−g·M·(h − h_b) / (R·T_b))`
//! - Density: `ρ(h) = P(h) / (R_specific · T(h))`
//! - Speed of sound: `a(h) = √(γ · R_specific · T(h))`

use rocket_core::{Density, Length, Pressure, Temperature, Velocity};

/// Sea-level temperature (K).
const T0: f64 = 288.15;
/// Sea-level pressure (Pa).
const P0: f64 = 101_325.0;
/// Tropospheric lapse rate (K/m).
const LAPSE: f64 = 0.0065;
/// Standard gravitational acceleration (m/s²).
const G: f64 = 9.80665;
/// Molar mass of dry air (kg/mol).
const M_AIR: f64 = 0.028_964_4;
/// Universal gas constant (J/(mol·K)).
const R_UNIV: f64 = 8.314_462;
/// Ratio of specific heats for dry air.
const GAMMA: f64 = 1.4;
/// Tropopause altitude (m).
const TROPOPAUSE: f64 = 11_000.0;

/// Specific gas constant for dry air, `R/M` (J/(kg·K)).
const R_SPECIFIC: f64 = R_UNIV / M_AIR;

/// Precomputed exponent for the pressure formula: `g·M/(R·L)`.
const P_EXP: f64 = G * M_AIR / (R_UNIV * LAPSE);

/// The state of the atmosphere at a given altitude.
///
/// All quantities are in SI base units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtmosphereState {
    /// Air temperature.
    pub temperature: Temperature,
    /// Air pressure.
    pub pressure: Pressure,
    /// Air density.
    pub density: Density,
    /// Speed of sound.
    pub speed_of_sound: Velocity,
}

impl AtmosphereState {
    /// Dynamic viscosity (Pa·s) using Sutherland's formula.
    ///
    /// `μ = μ₀ · ((T/T₀)^(3/2)) · ((T₀ + S) / (T + S))`
    ///
    /// where `μ₀ = 1.716e-5 Pa·s` at `T₀ = 273.15 K` and
    /// `S = 110.4 K` (Sutherland temperature). This is used by the
    /// drag model for Reynolds-number-dependent skin friction.
    #[must_use]
    pub fn dynamic_viscosity(self) -> f64 {
        const MU0: f64 = 1.716e-5;
        const T_REF: f64 = 273.15;
        const S: f64 = 110.4;
        let t = self.temperature.as_kelvin();
        MU0 * (t / T_REF).powi(3).sqrt() * ((T_REF + S) / (t + S))
    }

    /// Kinematic viscosity (m²/s): `ν = μ / ρ`.
    #[must_use]
    pub fn kinematic_viscosity(self) -> f64 {
        self.dynamic_viscosity() / self.density.as_kilograms_per_cubic_meter()
    }
}

/// The International Standard Atmosphere (troposphere model).
///
/// Construct with `IsaAtmosphere::new()` and query with `.at(altitude)`.
/// The sea-level constants can be overridden for non-standard days
/// (e.g. a hot launch site) via `with_sea_level(t0, p0)`.
///
/// This is a thin convenience wrapper that clamps the input to the
/// troposphere (0–11 km). For flights above 11 km, use [`Us1976`]
/// directly.
#[derive(Debug, Clone, Copy)]
pub struct IsaAtmosphere {
    /// Sea-level temperature (K).
    t0: f64,
    /// Sea-level pressure (Pa).
    p0: f64,
}

impl Default for IsaAtmosphere {
    fn default() -> Self {
        Self::new()
    }
}

impl IsaAtmosphere {
    /// Create a standard ISA atmosphere (T₀ = 288.15 K, P₀ = 101325 Pa).
    #[must_use]
    pub const fn new() -> Self {
        Self { t0: T0, p0: P0 }
    }

    /// Create an ISA atmosphere with custom sea-level conditions.
    ///
    /// Use this for non-standard launch days (e.g. a hot summer afternoon
    /// at a desert launch site) or for launch sites above sea level where
    /// you want the model to start from a different base.
    #[must_use]
    pub const fn with_sea_level(t0: f64, p0: f64) -> Self {
        Self { t0, p0 }
    }

    /// Compute the atmosphere state at the given altitude above sea level.
    ///
    /// Altitudes above the tropopause (11 km) are clamped to 11 km. For
    /// T0 (Phase 2) this is sufficient — model rockets don't reach 11 km.
    /// Negative altitudes (below sea level, e.g. Death Valley) are
    /// allowed and handled correctly by the formulas.
    #[must_use]
    pub fn at(&self, altitude: Length) -> AtmosphereState {
        let h = altitude.as_meters().clamp(-1_000.0, TROPOPAUSE);

        // Temperature decreases linearly with altitude in the troposphere.
        let t = self.t0 - LAPSE * h;
        let temperature = Temperature::kelvin(t);

        // Pressure follows the hydrostatic equation with polytropic index.
        let ratio = 1.0 - LAPSE * h / self.t0;
        let p = self.p0 * ratio.powf(P_EXP);
        let pressure = Pressure::pascals(p);

        // Density from the ideal gas law: ρ = P / (R_specific · T).
        let rho = p / (R_SPECIFIC * t);
        let density = Density::kilograms_per_cubic_meter(rho);

        // Speed of sound: a = √(γ · R · T).
        let a = (GAMMA * R_SPECIFIC * t).sqrt();
        let speed_of_sound = Velocity::meters_per_second(a);

        AtmosphereState {
            temperature,
            pressure,
            density,
            speed_of_sound,
        }
    }

    /// Sea-level state (convenience for `at(0)`).
    #[must_use]
    pub fn sea_level(&self) -> AtmosphereState {
        self.at(Length::ZERO)
    }
}

/// A layer of the US-1976 atmosphere.
///
/// Each layer has a base altitude, base temperature, base pressure, and
/// a lapse rate (which may be zero for isothermal layers).
#[derive(Debug, Clone, Copy)]
struct AtmosphereLayer {
    /// Base altitude of the layer (m, geopotential).
    base_altitude: f64,
    /// Base temperature (K).
    base_temperature: f64,
    /// Base pressure (Pa).
    base_pressure: f64,
    /// Lapse rate (K/m). Positive = temperature increases with altitude.
    lapse_rate: f64,
}

/// The US-1976 standard atmosphere, valid from sea level to 50 km
/// (geopotential). Phase 3 uses this for HPR flights that reach the
/// stratosphere; Phase 4 will extend to 86 km and add NRLMSISE-00 for
/// T2 (Karman line).
///
/// The atmosphere is divided into layers with piecewise-constant lapse
/// rates. Within each layer:
/// - If `lapse_rate ≠ 0`: `T(h) = T_b + L·(h−h_b)`,
///   `P(h) = P_b·(T_b/T)^(gM/(RL))`.
/// - If `lapse_rate = 0` (isothermal): `T(h) = T_b`,
///   `P(h) = P_b·exp(−gM(h−h_b)/(RT_b))`.
#[derive(Debug, Clone, Copy)]
pub struct Us1976 {
    layers: [AtmosphereLayer; 5],
}

impl Default for Us1976 {
    fn default() -> Self {
        Self::new()
    }
}

impl Us1976 {
    /// Create the standard US-1976 atmosphere.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            layers: [
                // Troposphere: 0–11 km, L = -6.5 K/km.
                // Base (sea level): T=288.15K, P=101325 Pa.
                AtmosphereLayer {
                    base_altitude: 0.0,
                    base_temperature: 288.15,
                    base_pressure: 101_325.0,
                    lapse_rate: -0.0065,
                },
                // Tropopause (lower stratosphere): 11–20 km, isothermal.
                // Base (11 km): T=216.65K, P=22632.0 Pa.
                AtmosphereLayer {
                    base_altitude: 11_000.0,
                    base_temperature: 216.65,
                    base_pressure: 22_632.0,
                    lapse_rate: 0.0,
                },
                // Mid stratosphere: 20–32 km, L = +1.0 K/km.
                // Base (20 km): T=216.65K, P=5529.3 Pa.
                AtmosphereLayer {
                    base_altitude: 20_000.0,
                    base_temperature: 216.65,
                    base_pressure: 5_529.3,
                    lapse_rate: 0.001,
                },
                // Upper stratosphere: 32–47 km, L = +2.8 K/km.
                // Base (32 km): T=228.65K, P=868.02 Pa.
                AtmosphereLayer {
                    base_altitude: 32_000.0,
                    base_temperature: 228.65,
                    base_pressure: 868.02,
                    lapse_rate: 0.0028,
                },
                // Stratopause: 47–50 km, isothermal.
                // Base (47 km): T=270.65K, P=110.91 Pa.
                AtmosphereLayer {
                    base_altitude: 47_000.0,
                    base_temperature: 270.65,
                    base_pressure: 110.91,
                    lapse_rate: 0.0,
                },
            ],
        }
    }

    /// Compute the atmosphere state at the given altitude.
    ///
    /// Altitudes below sea level are handled by the tropospheric formula
    /// (extrapolated). Altitudes above 50 km (stratopause) are clamped
    /// to 50 km; Phase 4 will extend this to 86 km and beyond via
    /// NRLMSISE-00.
    #[must_use]
    pub fn at(&self, altitude: Length) -> AtmosphereState {
        let h = altitude.as_meters().clamp(-1_000.0, 50_000.0);

        // Find the layer containing this altitude.
        let layer_idx = self
            .layers
            .iter()
            .rposition(|l| h >= l.base_altitude)
            .unwrap_or(0);
        let layer = self.layers[layer_idx];

        let h_above = h - layer.base_altitude;
        let t = layer.base_temperature + layer.lapse_rate * h_above;

        let p = if layer.lapse_rate.abs() < 1e-12 {
            // Isothermal: P = P_b · exp(−gM(h−h_b)/(RT_b)).
            layer.base_pressure * (-G * M_AIR * h_above / (R_UNIV * layer.base_temperature)).exp()
        } else {
            // Lapse ≠ 0: P = P_b · (T_b/T)^(gM/(RL)).
            let exponent = G * M_AIR / (R_UNIV * layer.lapse_rate);
            layer.base_pressure * (layer.base_temperature / t).powf(exponent)
        };

        let rho = p / (R_SPECIFIC * t);
        let a = (GAMMA * R_SPECIFIC * t).sqrt();

        AtmosphereState {
            temperature: Temperature::kelvin(t),
            pressure: Pressure::pascals(p),
            density: Density::kilograms_per_cubic_meter(rho),
            speed_of_sound: Velocity::meters_per_second(a),
        }
    }

    /// Sea-level state (convenience for `at(0)`).
    #[must_use]
    pub fn sea_level(&self) -> AtmosphereState {
        self.at(Length::ZERO)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]

    use super::*;
    use rocket_core::Length;

    const TOL: f64 = 1e-3;

    // ---- ISA (troposphere) regression tests ----

    #[test]
    fn sea_level_temperature_is_standard() {
        let isa = IsaAtmosphere::new();
        let s = isa.sea_level();
        assert!(
            (s.temperature.as_kelvin() - 288.15).abs() < TOL,
            "sea-level T = {}",
            s.temperature.as_kelvin()
        );
    }

    #[test]
    fn sea_level_pressure_is_standard() {
        let isa = IsaAtmosphere::new();
        let s = isa.sea_level();
        assert!(
            (s.pressure.as_pascals() - 101_325.0).abs() < TOL,
            "sea-level P = {}",
            s.pressure.as_pascals()
        );
    }

    #[test]
    fn sea_level_density_is_standard() {
        let isa = IsaAtmosphere::new();
        let s = isa.sea_level();
        // ISA sea-level density is 1.225 kg/m³.
        assert!(
            (s.density.as_kilograms_per_cubic_meter() - 1.225).abs() < 1e-3,
            "sea-level rho = {}",
            s.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn sea_level_speed_of_sound_is_standard() {
        let isa = IsaAtmosphere::new();
        let s = isa.sea_level();
        // ISA sea-level speed of sound is 340.29 m/s.
        assert!(
            (s.speed_of_sound.as_meters_per_second() - 340.29).abs() < 0.05,
            "sea-level a = {}",
            s.speed_of_sound.as_meters_per_second()
        );
    }

    #[test]
    fn pressure_at_1000m_matches_isa_table() {
        let isa = IsaAtmosphere::new();
        let s = isa.at(Length::meters(1000.0));
        // ISA table: P(1000m) = 89874.6 Pa (we accept ±10 Pa).
        assert!(
            (s.pressure.as_pascals() - 89_874.6).abs() < 10.0,
            "P(1000m) = {}",
            s.pressure.as_pascals()
        );
    }

    #[test]
    fn density_at_1000m_matches_isa_table() {
        let isa = IsaAtmosphere::new();
        let s = isa.at(Length::meters(1000.0));
        // ISA table: ρ(1000m) = 1.1117 kg/m³.
        assert!(
            (s.density.as_kilograms_per_cubic_meter() - 1.1117).abs() < 1e-3,
            "rho(1000m) = {}",
            s.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn speed_of_sound_at_5000m_matches_isa_table() {
        let isa = IsaAtmosphere::new();
        let s = isa.at(Length::meters(5000.0));
        // ISA table: a(5000m) = 320.55 m/s (we accept ±0.5).
        assert!(
            (s.speed_of_sound.as_meters_per_second() - 320.55).abs() < 0.5,
            "a(5000m) = {}",
            s.speed_of_sound.as_meters_per_second()
        );
    }

    #[test]
    fn temperature_at_5000m_matches_isa_table() {
        let isa = IsaAtmosphere::new();
        let s = isa.at(Length::meters(5000.0));
        // T(5000m) = 288.15 - 0.0065*5000 = 255.65 K.
        assert!(
            (s.temperature.as_kelvin() - 255.65).abs() < TOL,
            "T(5000m) = {}",
            s.temperature.as_kelvin()
        );
    }

    #[test]
    fn pressure_at_5000m_matches_isa_table() {
        let isa = IsaAtmosphere::new();
        let s = isa.at(Length::meters(5000.0));
        // ISA table: P(5000m) = 54019.4 Pa.
        assert!(
            (s.pressure.as_pascals() - 54_019.4).abs() < 20.0,
            "P(5000m) = {}",
            s.pressure.as_pascals()
        );
    }

    #[test]
    fn density_at_11000m_tropopause() {
        let isa = IsaAtmosphere::new();
        let s = isa.at(Length::meters(11_000.0));
        // ISA tropopause: T = 216.65 K, P = 22632 Pa, ρ = 0.3639 kg/m³.
        assert!((s.temperature.as_kelvin() - 216.65).abs() < TOL);
        assert!((s.pressure.as_pascals() - 22_632.0).abs() < 30.0);
        assert!(
            (s.density.as_kilograms_per_cubic_meter() - 0.3639).abs() < 1e-3,
            "rho(11km) = {}",
            s.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn density_decreases_monotonically_with_altitude() {
        let isa = IsaAtmosphere::new();
        let mut prev = isa.sea_level().density.as_kilograms_per_cubic_meter();
        for h_m in [500.0, 1000.0, 2000.0, 3000.0, 5000.0, 8000.0, 11_000.0] {
            let s = isa.at(Length::meters(h_m));
            let rho = s.density.as_kilograms_per_cubic_meter();
            assert!(
                rho < prev,
                "density should decrease: h={h_m}m, rho={rho}, prev={prev}"
            );
            prev = rho;
        }
    }

    #[test]
    fn altitude_above_tropopause_is_clamped() {
        let isa = IsaAtmosphere::new();
        let s11 = isa.at(Length::meters(11_000.0));
        let s20 = isa.at(Length::meters(20_000.0));
        // Above 11 km the model clamps, so 20 km returns the same state.
        assert_eq!(s11, s20);
    }

    #[test]
    fn below_sea_level_is_handled() {
        let isa = IsaAtmosphere::new();
        // Death Valley is about -86 m. Temperature should be higher,
        // pressure higher, density higher than sea level.
        let s = isa.at(Length::meters(-86.0));
        let sea = isa.sea_level();
        assert!(s.temperature.as_kelvin() > sea.temperature.as_kelvin());
        assert!(s.pressure.as_pascals() > sea.pressure.as_pascals());
        assert!(
            s.density.as_kilograms_per_cubic_meter() > sea.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn custom_sea_level_conditions() {
        let isa = IsaAtmosphere::new();
        let hot = IsaAtmosphere::with_sea_level(303.15, 101_000.0); // 30°C day
        let s_std = isa.sea_level();
        let s_hot = hot.sea_level();
        assert!(s_hot.temperature.as_kelvin() > s_std.temperature.as_kelvin());
        // At the same pressure, a hotter day has lower density.
        assert!(
            s_hot.density.as_kilograms_per_cubic_meter()
                < s_std.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn dynamic_viscosity_at_sea_level() {
        let isa = IsaAtmosphere::new();
        let s = isa.sea_level();
        let mu = s.dynamic_viscosity();
        // Sea-level dynamic viscosity of air ≈ 1.789e-5 Pa·s.
        assert!((mu - 1.789e-5).abs() < 1e-8, "mu(sea level) = {mu:e}");
    }

    #[test]
    fn kinematic_viscosity_at_sea_level() {
        let isa = IsaAtmosphere::new();
        let s = isa.sea_level();
        let nu = s.kinematic_viscosity();
        // Sea-level kinematic viscosity of air ≈ 1.460e-5 m²/s.
        assert!((nu - 1.460e-5).abs() < 5e-8, "nu(sea level) = {nu:e}");
    }

    // ---- US-1976 atmosphere tests ----

    #[test]
    fn us1976_sea_level_matches_isa() {
        let isa = IsaAtmosphere::new();
        let us = Us1976::new();
        let s_isa = isa.sea_level();
        let s_us = us.sea_level();
        assert!((s_isa.temperature.as_kelvin() - s_us.temperature.as_kelvin()).abs() < TOL);
        assert!((s_isa.pressure.as_pascals() - s_us.pressure.as_pascals()).abs() < 1.0);
        assert!(
            (s_isa.density.as_kilograms_per_cubic_meter()
                - s_us.density.as_kilograms_per_cubic_meter())
            .abs()
                < 1e-4
        );
    }

    #[test]
    fn us1976_troposphere_matches_isa() {
        // The US-1976 troposphere (0-11 km) should match ISA exactly.
        let isa = IsaAtmosphere::new();
        let us = Us1976::new();
        for h_m in [0.0, 1000.0, 5000.0, 10_000.0, 11_000.0] {
            let s_isa = isa.at(Length::meters(h_m));
            let s_us = us.at(Length::meters(h_m));
            assert!(
                (s_isa.temperature.as_kelvin() - s_us.temperature.as_kelvin()).abs() < 1e-3,
                "T({h_m}m): ISA={}, US={}",
                s_isa.temperature.as_kelvin(),
                s_us.temperature.as_kelvin()
            );
            assert!(
                (s_isa.pressure.as_pascals() - s_us.pressure.as_pascals()).abs()
                    / s_isa.pressure.as_pascals()
                    < 1e-3,
                "P({h_m}m): ISA={}, US={}",
                s_isa.pressure.as_pascals(),
                s_us.pressure.as_pascals()
            );
        }
    }

    #[test]
    fn us1976_tropopause_is_isothermal() {
        // At 11-20 km, temperature should be constant at 216.65 K.
        let us = Us1976::new();
        let s_11 = us.at(Length::meters(11_000.0));
        let s_15 = us.at(Length::meters(15_000.0));
        let s_20 = us.at(Length::meters(20_000.0));
        assert!((s_11.temperature.as_kelvin() - 216.65).abs() < TOL);
        assert!((s_15.temperature.as_kelvin() - 216.65).abs() < TOL);
        assert!((s_20.temperature.as_kelvin() - 216.65).abs() < TOL);
    }

    #[test]
    fn us1976_density_at_20km_matches_table() {
        let us = Us1976::new();
        let s = us.at(Length::meters(20_000.0));
        // US-1976 table: ρ(20km) = 0.0889 kg/m³.
        assert!(
            (s.density.as_kilograms_per_cubic_meter() - 0.0889).abs() < 1e-3,
            "rho(20km) = {}",
            s.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn us1976_temperature_at_32km() {
        // 20-32 km: L = +1.0 K/km. T(32km) = 216.65 + 12 = 228.65 K.
        let us = Us1976::new();
        let s = us.at(Length::meters(32_000.0));
        assert!(
            (s.temperature.as_kelvin() - 228.65).abs() < TOL,
            "T(32km) = {}",
            s.temperature.as_kelvin()
        );
    }

    #[test]
    fn us1976_density_at_32km_matches_table() {
        let us = Us1976::new();
        let s = us.at(Length::meters(32_000.0));
        // US-1976 table: ρ(32km) = 0.0132 kg/m³.
        assert!(
            (s.density.as_kilograms_per_cubic_meter() - 0.0132).abs() < 5e-4,
            "rho(32km) = {}",
            s.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn us1976_stratopause_temperature() {
        // 32-47 km: L = +2.8 K/km. T(47km) = 228.65 + 15*2.8 = 270.65 K.
        let us = Us1976::new();
        let s = us.at(Length::meters(47_000.0));
        assert!(
            (s.temperature.as_kelvin() - 270.65).abs() < TOL,
            "T(47km) = {}",
            s.temperature.as_kelvin()
        );
    }

    #[test]
    fn us1976_density_at_50km_matches_table() {
        let us = Us1976::new();
        let s = us.at(Length::meters(50_000.0));
        // US-1976 table: ρ(50km) ≈ 0.00103 kg/m³.
        assert!(
            (s.density.as_kilograms_per_cubic_meter() - 0.00103).abs() < 2e-4,
            "rho(50km) = {}",
            s.density.as_kilograms_per_cubic_meter()
        );
    }

    #[test]
    fn us1976_pressure_at_20km_matches_table() {
        let us = Us1976::new();
        let s = us.at(Length::meters(20_000.0));
        // US-1976 table: P(20km) = 5529.3 Pa.
        assert!(
            (s.pressure.as_pascals() - 5_529.3).abs() < 50.0,
            "P(20km) = {}",
            s.pressure.as_pascals()
        );
    }

    #[test]
    fn us1976_density_monotonic_decreasing_to_50km() {
        let us = Us1976::new();
        let mut prev = us.sea_level().density.as_kilograms_per_cubic_meter();
        for h_m in [
            1000.0, 5000.0, 11_000.0, 15_000.0, 20_000.0, 25_000.0, 32_000.0, 40_000.0, 47_000.0,
            50_000.0,
        ] {
            let s = us.at(Length::meters(h_m));
            let rho = s.density.as_kilograms_per_cubic_meter();
            assert!(
                rho < prev,
                "density should decrease: h={h_m}m, rho={rho}, prev={prev}"
            );
            prev = rho;
        }
    }

    #[test]
    fn us1976_altitude_above_50km_is_clamped() {
        let us = Us1976::new();
        let s50 = us.at(Length::meters(50_000.0));
        let s70 = us.at(Length::meters(70_000.0));
        assert_eq!(s50, s70);
    }
}
