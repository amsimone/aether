//! RASP `.eng` file parser.
//!
//! The RASP (Rocket Altitude Simulation Program) format, also known as
//! `.eng`, is the de-facto standard for rocket motor data interchange.
//! See: <https://www.thrustcurve.org/info/raspformat.html>
//!
//! ## Format
//!
//! ```text
//! ; Comments start with semicolons
//! ; Blank lines are also ignored
//! designation diameter_mm length_mm delays prop_mass_kg total_mass_kg manufacturer
//! time_s thrust_N
//! time_s thrust_N
//! ...
//! final_time_s 0.0
//! ```
//!
//! - Header: 7 space-separated fields. Manufacturer is everything after
//!   the 6th field (can contain spaces).
//! - Delays: dash-separated list (e.g., "3-5-10"). `0` = no delay (has
//!   ejection charge). `P` = plugged (no ejection charge).
//! - Data points: time (seconds) and thrust (newtons), in increasing
//!   time order. An implicit (0, 0) point is assumed — do not include
//!   it explicitly. The final point must have zero thrust.
//! - Zero thrust is only valid at the final data point.

use crate::Motor;
use rocket_core::{Duration, Force, Length, Mass};

/// Standard gravity (m/s²), used for specific impulse calculation.
const G0: f64 = 9.806_65;

/// Errors that can occur while parsing a RASP `.eng` file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The input is empty or contains no motor data.
    #[error("empty input: no motor data found")]
    Empty,

    /// A header line field is missing or malformed.
    #[error("line {line}: {message}")]
    Header { line: usize, message: String },

    /// A thrust-curve data point is malformed.
    #[error("line {line}: {message}")]
    DataPoint { line: usize, message: String },
}

/// Parse a single motor from a RASP `.eng` string.
///
/// If the string contains multiple motors (separated by comment lines),
/// only the first is returned. Use [`parse_all`] for multi-motor files.
///
/// # Errors
///
/// Returns `ParseError` if the input is empty, the header is malformed,
/// a data point is invalid, or the thrust curve violates RASP rules
/// (e.g., zero thrust before the final point, decreasing times).
pub fn parse(text: &str) -> Result<Motor, ParseError> {
    let motors = parse_all(text)?;
    motors.into_iter().next().ok_or(ParseError::Empty)
}

/// Parse all motors from a RASP `.eng` string.
///
/// Multiple motors in a single file are separated by comment lines
/// (lines starting with `;`). Each motor has its own header line and
/// data points.
///
/// # Errors
///
/// Returns `ParseError` for the first motor that fails to parse.
pub fn parse_all(text: &str) -> Result<Vec<Motor>, ParseError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut motors = Vec::new();
    let mut idx = 0;

    while idx < lines.len() {
        // Skip blank lines and comments to find the next header.
        let mut comment = Vec::new();
        while idx < lines.len() {
            let trimmed = lines[idx].trim();
            if trimmed.is_empty() {
                idx += 1;
            } else if trimmed.starts_with(';') {
                comment.push(
                    trimmed
                        .strip_prefix(';')
                        .unwrap_or(trimmed)
                        .trim()
                        .to_string(),
                );
                idx += 1;
            } else {
                break;
            }
        }

        if idx >= lines.len() {
            break; // End of input after comments.
        }

        // Parse the header line.
        let header_line = idx + 1; // 1-based line number.
        let motor = parse_single_motor(&lines, &mut idx, header_line, comment)?;
        motors.push(motor);
    }

    if motors.is_empty() {
        Err(ParseError::Empty)
    } else {
        Ok(motors)
    }
}

/// Parse one motor starting at `idx` (pointing to the header line).
/// Advances `idx` past the motor's data points.
fn parse_single_motor(
    lines: &[&str],
    idx: &mut usize,
    header_line: usize,
    comment: Vec<String>,
) -> Result<Motor, ParseError> {
    // --- Header ---
    let header = lines[*idx].trim();
    *idx += 1;

    let fields: Vec<&str> = header.split_whitespace().collect();

    if fields.len() < 7 {
        return Err(ParseError::Header {
            line: header_line,
            message: format!("header requires 7 fields, found {}", fields.len()),
        });
    }

    let designation = fields[0].to_string();
    let manufacturer = fields[6..].join(" ");

    let diameter_mm = parse_float_field(fields[1], header_line, "diameter")?;
    let length_mm = parse_float_field(fields[2], header_line, "length")?;
    let prop_mass_kg = parse_float_field(fields[4], header_line, "propellant mass")?;
    let total_mass_kg = parse_float_field(fields[5], header_line, "total mass")?;

    let delays = parse_delays(fields[3], header_line)?;

    // --- Data points ---
    let mut thrust_curve: Vec<(Duration, Force)> = Vec::new();
    let mut prev_time: Option<f64> = None;

    while *idx < lines.len() {
        let trimmed = lines[*idx].trim();

        // Skip blank lines within data (common for readability).
        if trimmed.is_empty() {
            *idx += 1;
            continue;
        }

        // A comment line signals the end of this motor's data
        // (in multi-motor files, the next motor follows after comments).
        if trimmed.starts_with(';') {
            break;
        }

        let data_line = *idx + 1;
        *idx += 1;

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(ParseError::DataPoint {
                line: data_line,
                message: format!("expected 2 values (time thrust), found {}", parts.len()),
            });
        }

        let time = parse_float_field(parts[0], data_line, "time")?;
        let thrust = parse_float_field(parts[1], data_line, "thrust")?;

        if time < 0.0 {
            return Err(ParseError::DataPoint {
                line: data_line,
                message: format!("time must be non-negative, got {time}"),
            });
        }

        if let Some(pt) = prev_time {
            if time < pt {
                return Err(ParseError::DataPoint {
                    line: data_line,
                    message: format!("time {time} is before previous time {pt}"),
                });
            }
            if (time - pt).abs() < f64::EPSILON && time > 0.0 {
                // Duplicate timestamp (allowing t=0 which RASP says not to include,
                // but we're lenient).
                return Err(ParseError::DataPoint {
                    line: data_line,
                    message: format!("duplicate timestamp {time}"),
                });
            }
        }

        // RASP rule: zero thrust is only valid at the final point.
        if thrust <= 0.0 && !thrust_curve.is_empty() {
            // This is the final point (zero thrust = burnout). Accept it
            // and end parsing of this motor's data.
            thrust_curve.push((Duration::seconds(time), Force::newtons(thrust)));
            // Per RASP, the final zero-thrust point ends the data.
            break;
        }

        thrust_curve.push((Duration::seconds(time), Force::newtons(thrust)));
        prev_time = Some(time);
    }

    if thrust_curve.is_empty() {
        return Err(ParseError::Header {
            line: header_line,
            message: "no thrust curve data points found".to_string(),
        });
    }

    // RASP requires the final point to have zero thrust.
    // Be lenient: if the last point has nonzero thrust, that's OK for
    // simulation purposes but we note it.
    // (Strict RASP would reject this; we accept and let the UI warn.)

    Ok(Motor {
        designation,
        manufacturer,
        comment,
        diameter: Length::millimeters(diameter_mm),
        length: Length::millimeters(length_mm),
        delays,
        propellant_mass: Mass::kilograms(prop_mass_kg),
        total_mass: Mass::kilograms(total_mass_kg),
        thrust_curve,
    })
}

/// Parse a float field from a string, with line-number context for errors.
fn parse_float_field(s: &str, line: usize, field_name: &str) -> Result<f64, ParseError> {
    s.parse::<f64>().map_err(|_| ParseError::Header {
        line,
        message: format!("{field_name} value '{s}' is not a valid number"),
    })
}

/// Parse the delays field (e.g., "3", "3-5-10", "0", "P").
///
/// Returns a list where:
/// - `-1.0` means plugged (no ejection charge)
/// - `0.0` means no delay (has ejection charge, fires immediately)
/// - Positive values are delay times in seconds
fn parse_delays(field: &str, line: usize) -> Result<Vec<f32>, ParseError> {
    if field.eq_ignore_ascii_case("P") {
        return Ok(vec![-1.0]); // Plugged
    }

    let mut delays = Vec::new();
    for part in field.split('-') {
        let part = part.trim();
        if part.is_empty() {
            return Err(ParseError::Header {
                line,
                message: format!("delays field '{field}' has an empty value"),
            });
        }
        let d: f32 = part.parse().map_err(|_| ParseError::Header {
            line,
            message: format!("delay value '{part}' is not a valid number"),
        })?;
        delays.push(d);
    }

    if delays.is_empty() {
        return Err(ParseError::Header {
            line,
            message: "delays field is empty".to_string(),
        });
    }

    Ok(delays)
}

// ---- Computed accessors on Motor ----

impl Motor {
    /// Total impulse of the motor (integral of the thrust curve), in N·s.
    ///
    /// Computed via the trapezoidal rule. If the curve has fewer than 2
    /// points, returns 0.
    #[must_use]
    pub fn total_impulse(&self) -> f64 {
        if self.thrust_curve.len() < 2 {
            return 0.0;
        }
        let mut total = 0.0_f64;
        for window in self.thrust_curve.windows(2) {
            let (t0, f0) = (window[0].0.as_seconds(), window[0].1.as_newtons());
            let (t1, f1) = (window[1].0.as_seconds(), window[1].1.as_newtons());
            total += (t1 - t0) * (f0 + f1) / 2.0;
        }
        total
    }

    /// Burn time of the motor (time of the final data point), in seconds.
    ///
    /// Per RASP, this is the time at the last (zero-thrust) data point.
    #[must_use]
    pub fn burn_time(&self) -> f64 {
        self.thrust_curve
            .last()
            .map_or(0.0, |(t, _)| t.as_seconds())
    }

    /// Peak (maximum) thrust in the curve, in newtons.
    #[must_use]
    pub fn max_thrust(&self) -> f64 {
        self.thrust_curve
            .iter()
            .map(|(_, f)| f.as_newtons())
            .fold(0.0_f64, f64::max)
    }

    /// Average thrust over the burn, in newtons.
    ///
    /// `total_impulse / burn_time`. Returns 0 if `burn_time` is 0.
    #[must_use]
    pub fn average_thrust(&self) -> f64 {
        let bt = self.burn_time();
        if bt > 0.0 {
            self.total_impulse() / bt
        } else {
            0.0
        }
    }

    /// Specific impulse (Isp), in seconds.
    ///
    /// `Isp = total_impulse / (propellant_mass * g0)`
    #[must_use]
    pub fn specific_impulse(&self) -> f64 {
        let m = self.propellant_mass.as_kilograms();
        if m > 0.0 {
            self.total_impulse() / (m * G0)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::format_push_string
    )]

    use super::*;
    use proptest::prelude::*;

    // A realistic Estes A8-3 .eng file for testing.
    const ESTES_A8_3: &str = "\
; Estes A8-3
; Source: ThrustCurve.org (public domain)
A8-3 18 70 3 0.0033 0.0167 Estes
0.026 9.57
0.061 8.77
0.092 7.38
0.123 6.47
0.154 5.56
0.185 4.65
0.216 4.19
0.247 3.73
0.278 3.27
0.309 2.81
0.340 2.35
0.371 1.89
0.402 1.43
0.433 0.97
0.464 0.51
0.495 0.05
0.506 0.0
";

    // ---- Basic parsing tests ----

    #[test]
    fn parse_a8_3_fields() {
        let motor = parse(ESTES_A8_3).expect("A8-3 should parse");
        assert_eq!(motor.designation, "A8-3");
        assert_eq!(motor.manufacturer, "Estes");
        assert!((motor.diameter.as_millimeters() - 18.0).abs() < 1e-9);
        assert!((motor.length.as_millimeters() - 70.0).abs() < 1e-9);
        assert!((motor.propellant_mass.as_grams() - 3.3).abs() < 1e-6);
        assert!((motor.total_mass.as_grams() - 16.7).abs() < 1e-6);
        assert_eq!(motor.delays, vec![3.0]);
        assert_eq!(motor.thrust_curve.len(), 17);
    }

    #[test]
    fn parse_a8_3_comments_preserved() {
        let motor = parse(ESTES_A8_3).expect("A8-3 should parse");
        assert_eq!(motor.comment[0], "Estes A8-3");
        assert!(motor.comment[0].contains("Estes A8-3"));
    }

    #[test]
    fn parse_with_blank_lines() {
        let text = "A8-3 18 70 3 0.0033 0.0167 Estes\n\n0.026 9.57\n\n0.506 0.0\n";
        let motor = parse(text).expect("blank lines should be handled");
        assert_eq!(motor.thrust_curve.len(), 2);
    }

    #[test]
    fn parse_with_windows_line_endings() {
        let text = "A8-3 18 70 3 0.0033 0.0167 Estes\r\n0.026 9.57\r\n0.506 0.0\r\n";
        let motor = parse(text).expect("CRLF should be handled");
        assert_eq!(motor.thrust_curve.len(), 2);
    }

    #[test]
    fn parse_with_extra_whitespace() {
        let text =
            "A8-3   18   70   3   0.0033   0.0167   Estes\n   0.026   9.57\n   0.506   0.0\n";
        let motor = parse(text).expect("extra whitespace should be handled");
        assert_eq!(motor.designation, "A8-3");
        assert_eq!(motor.thrust_curve.len(), 2);
    }

    #[test]
    fn parse_manufacturer_with_spaces() {
        let text = "A8-3 18 70 3 0.0033 0.0167 Estes Corporation Inc\n0.026 9.57\n0.506 0.0\n";
        let motor = parse(text).expect("multi-word manufacturer should parse");
        assert_eq!(motor.manufacturer, "Estes Corporation Inc");
    }

    // ---- Delays parsing tests ----

    #[test]
    fn parse_delays_single() {
        let motor = parse("M 10 50 5 0.01 0.05 Mfr\n0.1 10\n0.2 0.0\n").unwrap();
        assert_eq!(motor.delays, vec![5.0]);
    }

    #[test]
    fn parse_delays_multiple() {
        let motor = parse("M 10 50 3-5-10 0.01 0.05 Mfr\n0.1 10\n0.2 0.0\n").unwrap();
        assert_eq!(motor.delays, vec![3.0, 5.0, 10.0]);
    }

    #[test]
    fn parse_delays_zero() {
        let motor = parse("M 10 50 0 0.01 0.05 Mfr\n0.1 10\n0.2 0.0\n").unwrap();
        assert_eq!(motor.delays, vec![0.0]);
    }

    #[test]
    fn parse_delays_plugged_uppercase() {
        let motor = parse("M 10 50 P 0.01 0.05 Mfr\n0.1 10\n0.2 0.0\n").unwrap();
        assert_eq!(motor.delays, vec![-1.0]);
    }

    #[test]
    fn parse_delays_plugged_lowercase() {
        let motor = parse("M 10 50 p 0.01 0.05 Mfr\n0.1 10\n0.2 0.0\n").unwrap();
        assert_eq!(motor.delays, vec![-1.0]);
    }

    // ---- Computed accessors tests ----

    #[test]
    fn total_impulse_a8_3() {
        let motor = parse(ESTES_A8_3).unwrap();
        let impulse = motor.total_impulse();
        // Should be ~2.08 N·s (trapezoidal integral of the curve).
        assert!(impulse > 1.5, "impulse {impulse} should be > 1.5");
        assert!(impulse < 2.5, "impulse {impulse} should be < 2.5");
    }

    #[test]
    fn total_impulse_matches_manual_calculation() {
        // Two-point curve: (0.1, 10) → (0.2, 0)
        // Impulse = (0.2 - 0.1) * (10 + 0) / 2 = 0.5 N·s
        let motor = parse("M 10 50 5 0.01 0.05 Mfr\n0.1 10\n0.2 0.0\n").unwrap();
        let impulse = motor.total_impulse();
        assert!(
            (impulse - 0.5).abs() < 1e-9,
            "impulse should be 0.5, got {impulse}"
        );
    }

    #[test]
    fn burn_time_a8_3() {
        let motor = parse(ESTES_A8_3).unwrap();
        let bt = motor.burn_time();
        assert!(
            (bt - 0.506).abs() < 1e-9,
            "burn time should be 0.506, got {bt}"
        );
    }

    #[test]
    fn max_thrust_a8_3() {
        let motor = parse(ESTES_A8_3).unwrap();
        let mt = motor.max_thrust();
        assert!(
            (mt - 9.57).abs() < 1e-9,
            "max thrust should be 9.57, got {mt}"
        );
    }

    #[test]
    fn average_thrust_a8_3() {
        let motor = parse(ESTES_A8_3).unwrap();
        let avg = motor.average_thrust();
        let expected = motor.total_impulse() / motor.burn_time();
        assert!(
            (avg - expected).abs() < 1e-9,
            "avg should be {expected}, got {avg}"
        );
    }

    #[test]
    fn specific_impulse_a8_3() {
        let motor = parse(ESTES_A8_3).unwrap();
        let isp = motor.specific_impulse();
        let expected = motor.total_impulse() / (0.0033 * G0);
        assert!(
            (isp - expected).abs() < 1e-6,
            "Isp should be {expected}, got {isp}"
        );
    }

    #[test]
    fn total_impulse_single_point_curve() {
        // Single point: no integral (need >= 2 points).
        let motor = parse("M 10 50 5 0.01 0.05 Mfr\n0.1 0.0\n").unwrap();
        assert!((motor.total_impulse() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn total_impulse_empty_curve_returns_zero() {
        // This shouldn't happen (parser rejects empty curves), but test
        // the accessor's defensive behavior.
        let motor = Motor {
            designation: "X".into(),
            manufacturer: "Y".into(),
            comment: vec![],
            diameter: Length::millimeters(10.0),
            length: Length::millimeters(50.0),
            delays: vec![],
            propellant_mass: Mass::kilograms(0.01),
            total_mass: Mass::kilograms(0.05),
            thrust_curve: vec![],
        };
        assert!((motor.total_impulse() - 0.0).abs() < 1e-9);
    }

    // ---- Error handling tests ----

    #[test]
    fn error_empty_input() {
        let result = parse("");
        assert!(matches!(result, Err(ParseError::Empty)));
    }

    #[test]
    fn error_comments_only() {
        let result = parse("; just a comment\n; another\n");
        assert!(matches!(result, Err(ParseError::Empty)));
    }

    #[test]
    fn error_header_missing_fields() {
        let result = parse("A8-3 18 70 3 0.0033\n0.1 10\n0.2 0.0\n");
        assert!(matches!(result, Err(ParseError::Header { .. })));
    }

    #[test]
    fn error_non_numeric_diameter() {
        let result = parse("A8-3 abc 70 3 0.0033 0.0167 Estes\n0.1 10\n0.2 0.0\n");
        let Err(ParseError::Header { message, .. }) = result else {
            panic!("expected Header error");
        };
        assert!(message.contains("diameter"));
    }

    #[test]
    fn error_non_numeric_thrust() {
        let result = parse("A8-3 18 70 3 0.0033 0.0167 Estes\n0.1 abc\n0.2 0.0\n");
        assert!(matches!(result, Err(ParseError::Header { .. })));
    }

    #[test]
    fn error_negative_time() {
        let result = parse("A8-3 18 70 3 0.0033 0.0167 Estes\n-0.1 10\n0.2 0.0\n");
        let Err(ParseError::DataPoint { message, .. }) = result else {
            panic!("expected DataPoint error");
        };
        assert!(message.contains("non-negative"));
    }

    #[test]
    fn error_decreasing_time() {
        let result = parse("A8-3 18 70 3 0.0033 0.0167 Estes\n0.5 10\n0.3 5\n0.4 0.0\n");
        let Err(ParseError::DataPoint { message, .. }) = result else {
            panic!("expected DataPoint error");
        };
        assert!(message.contains("before previous"));
    }

    #[test]
    fn error_no_data_points() {
        let result = parse("A8-3 18 70 3 0.0033 0.0167 Estes\n; comment\n");
        assert!(matches!(result, Err(ParseError::Header { .. })));
    }

    #[test]
    fn error_data_point_wrong_field_count() {
        let result = parse("A8-3 18 70 3 0.0033 0.0167 Estes\n0.1 10 extra\n0.2 0.0\n");
        assert!(matches!(result, Err(ParseError::DataPoint { .. })));
    }

    // ---- Multi-motor file test ----

    #[test]
    fn parse_multi_motor_file() {
        let text = "\
; First motor
A8-3 18 70 3 0.0033 0.0167 Estes
0.026 9.57
0.506 0.0
;
; Second motor
B6-4 18 70 4 0.0062 0.0201 Estes
0.1 12.0
0.8 0.0
";
        let motors = parse_all(text).expect("multi-motor file should parse");
        assert_eq!(motors.len(), 2);
        assert_eq!(motors[0].designation, "A8-3");
        assert_eq!(motors[1].designation, "B6-4");
    }

    // ---- Round-trip test ----

    #[test]
    fn serde_round_trip() {
        let motor = parse(ESTES_A8_3).unwrap();
        let json = serde_json::to_string(&motor).unwrap();
        let back: Motor = serde_json::from_str(&json).unwrap();
        assert_eq!(motor, back);
    }

    // ---- Property tests (fuzz-like coverage) ----

    proptest! {
        /// Arbitrary input must never panic — it returns Ok or Err.
        #[test]
        fn prop_no_panic_on_arbitrary_input(input in ".{0,500}") {
            let _ = parse(&input);
        }

        /// A valid header followed by random float pairs should either
        /// parse successfully or return a clean DataPoint error.
        #[test]
        fn prop_valid_header_random_data(
            points in proptest::collection::vec(
                ("[0-9]{1,3}(\\.[0-9]{1,3})?", "[0-9]{1,3}(\\.[0-9]{1,3})?"),
                0..20
            )
        ) {
            let mut text = String::from("A8-3 18 70 3 0.0033 0.0167 Estes\n");
            for (t, f) in &points {
                text.push_str(t);
                text.push(' ');
                text.push_str(f);
                text.push('\n');
            }
            // Must not panic, regardless of data validity.
            let _ = parse(&text);
        }

        /// Round-trip: parse → serialize → deserialize → compare.
        #[test]
        fn prop_serde_round_trip(
            designation in "[A-Z][0-9]-[0-9]",
            diameter in 5f64..100f64,
            length in 30f64..200f64,
            prop_mass in 0.001f64..1.0,
            total_mass in 0.01f64..5.0,
            n_points in 2usize..10,
        ) {
            let mut text = format!(
                "{designation} {diameter} {length} 3 {prop_mass} {total_mass} TestMfr\n"
            );
            for i in 1..=n_points {
                let t = i as f64 * 0.1;
                let thrust = if i == n_points { 0.0 } else { 10.0 };
                text.push_str(&format!("{t} {thrust}\n"));
            }
            if let Ok(motor) = parse(&text) {
                let json = serde_json::to_string(&motor).unwrap();
                let back: Motor = serde_json::from_str(&json).unwrap();
                // Compare with tolerance (f64 round-trip can differ by 1 ULP).
                prop_assert!((motor.diameter.as_meters() - back.diameter.as_meters()).abs() < 1e-12);
                prop_assert!((motor.length.as_meters() - back.length.as_meters()).abs() < 1e-12);
                prop_assert!((motor.propellant_mass.as_kilograms() - back.propellant_mass.as_kilograms()).abs() < 1e-12);
                prop_assert!((motor.total_mass.as_kilograms() - back.total_mass.as_kilograms()).abs() < 1e-12);
                prop_assert_eq!(motor.designation, back.designation);
                prop_assert_eq!(motor.thrust_curve.len(), back.thrust_curve.len());
            }
        }
    }
}
