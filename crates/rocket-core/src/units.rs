//! Dimensional newtypes for the Aether rocket simulator.
//!
//! All quantities are stored internally in SI base units (meters, kilograms,
//! seconds, newtons, pascals, meters per second). The newtypes prevent
//! accidental mixing of units (e.g., adding a `Length` to a `Mass`) and
//! provide unit-aware construction and conversion methods for the UI.
//!
//! `serde` serializes each quantity as a plain `f64` in SI units
//! (`#[serde(transparent)]`), keeping the `.rocket` file format compact.

use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

use serde::{Deserialize, Serialize};

/// A macro to generate the common boilerplate for a dimensional newtype.
/// The resulting struct wraps an `f64` in SI base units.
macro_rules! quantity {
    (
        $(#[$meta:meta])*
        $name:ident,
        $unit_symbol:literal,
        $unit_name:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub f64);

        impl $name {
            /// Create from a value already in SI base units.
            #[must_use]
            pub const fn from_si(value: f64) -> Self {
                Self(value)
            }

            /// Return the value in SI base units.
            #[must_use]
            pub const fn as_si(self) -> f64 {
                self.0
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            fn mul(self, rhs: f64) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Div<f64> for $name {
            type Output = Self;
            fn div(self, rhs: f64) -> Self {
                Self(self.0 / rhs)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                // Use the formatter's precision if set, otherwise default.
                write!(f, "{} {}", self.0, $unit_symbol)
            }
        }

        impl $name {
            /// Zero value (identity for addition).
            pub const ZERO: Self = Self(0.0);
        }
    };
}

quantity!(
    /// A length in meters. Display: `m`.
    Length,
    "m",
    "meter"
);

quantity!(
    /// A mass in kilograms. Display: `kg`.
    Mass,
    "kg",
    "kilogram"
);

quantity!(
    /// A velocity in meters per second. Display: `m/s`.
    Velocity,
    "m/s",
    "meter per second"
);

quantity!(
    /// A force in newtons. Display: `N`.
    Force,
    "N",
    "newton"
);

quantity!(
    /// A pressure in pascals. Display: `Pa`.
    Pressure,
    "Pa",
    "pascal"
);

quantity!(
    /// A duration in seconds. Display: `s`.
    Duration,
    "s",
    "second"
);

// ---- Unit-specific construction & conversion methods ----

impl Length {
    /// Create from millimeters.
    #[must_use]
    pub fn millimeters(mm: f64) -> Self {
        Self(mm * 1.0e-3)
    }

    /// Create from meters.
    #[must_use]
    pub const fn meters(m: f64) -> Self {
        Self(m)
    }

    /// Return the value in millimeters.
    #[must_use]
    pub fn as_millimeters(self) -> f64 {
        self.0 * 1.0e3
    }

    /// Return the value in meters.
    #[must_use]
    pub const fn as_meters(self) -> f64 {
        self.0
    }
}

impl Mass {
    /// Create from grams.
    #[must_use]
    pub fn grams(g: f64) -> Self {
        Self(g * 1.0e-3)
    }

    /// Create from kilograms.
    #[must_use]
    pub const fn kilograms(kg: f64) -> Self {
        Self(kg)
    }

    /// Return the value in grams.
    #[must_use]
    pub fn as_grams(self) -> f64 {
        self.0 * 1.0e3
    }

    /// Return the value in kilograms.
    #[must_use]
    pub const fn as_kilograms(self) -> f64 {
        self.0
    }
}

impl Force {
    /// Create from newtons.
    #[must_use]
    pub const fn newtons(n: f64) -> Self {
        Self(n)
    }

    /// Return the value in newtons.
    #[must_use]
    pub const fn as_newtons(self) -> f64 {
        self.0
    }
}

impl Pressure {
    /// Create from pascals.
    #[must_use]
    pub const fn pascals(pa: f64) -> Self {
        Self(pa)
    }

    /// Create from hectopascals (hPa, used in meteorology).
    #[must_use]
    pub fn hectopascals(hpa: f64) -> Self {
        Self(hpa * 100.0)
    }

    /// Return the value in pascals.
    #[must_use]
    pub const fn as_pascals(self) -> f64 {
        self.0
    }
}

impl Duration {
    /// Create from seconds.
    #[must_use]
    pub const fn seconds(s: f64) -> Self {
        Self(s)
    }

    /// Create from milliseconds.
    #[must_use]
    pub fn milliseconds(ms: f64) -> Self {
        Self(ms * 1.0e-3)
    }

    /// Return the value in seconds.
    #[must_use]
    pub const fn as_seconds(self) -> f64 {
        self.0
    }
}

impl Velocity {
    /// Create from meters per second.
    #[must_use]
    pub const fn meters_per_second(mps: f64) -> Self {
        Self(mps)
    }

    /// Return the value in meters per second.
    #[must_use]
    pub const fn as_meters_per_second(self) -> f64 {
        self.0
    }
}

// ---- Cross-quantity operations ----

impl Div<Duration> for Velocity {
    type Output = f64;
    /// Velocity / Duration → acceleration (m/s², returned as plain f64 for now;
    /// an `Acceleration` newtype will be added when the physics core needs it).
    fn div(self, rhs: Duration) -> f64 {
        self.0 / rhs.0
    }
}

impl Mul<Duration> for Force {
    type Output = f64;
    /// Force × Duration → impulse (N·s, returned as plain f64 for now).
    fn mul(self, rhs: Duration) -> f64 {
        self.0 * rhs.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_conversions() {
        let l = Length::millimeters(250.0);
        assert!((l.as_meters() - 0.25).abs() < 1e-12);
        assert!((l.as_millimeters() - 250.0).abs() < 1e-9);

        let l2 = Length::meters(1.5);
        assert!((l2.as_millimeters() - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn mass_conversions() {
        let m = Mass::grams(50.0);
        assert!((m.as_kilograms() - 0.05).abs() < 1e-12);
        assert!((m.as_grams() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn arithmetic() {
        let a = Length::meters(1.0);
        let b = Length::meters(0.5);
        assert!(((a + b).as_meters() - 1.5).abs() < 1e-12);
        assert!(((a - b).as_meters() - 0.5).abs() < 1e-12);
        assert!(((a * 2.0).as_meters() - 2.0).abs() < 1e-12);
        assert!(((a / 2.0).as_meters() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn display() {
        let l = Length::meters(1.5);
        assert_eq!(format!("{l}"), "1.5 m");

        let f = Force::newtons(9.81);
        assert_eq!(format!("{f}"), "9.81 N");
    }

    #[test]
    fn serde_round_trip() {
        let l = Length::millimeters(250.0);
        let json = serde_json::to_string(&l).expect("serialize");
        let back: Length = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(l, back);
    }
}
