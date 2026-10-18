//! ThrustCurve.org API client.
//!
//! Provides search and download access to the ThrustCurve.org motor
//! database via their v1 JSON API. Motor data is fetched on demand and
//! cached locally (ADR-0006) — no motor files are bundled with the app.
//!
//! # Throttling
//!
//! The client throttles requests to at most 1 per second, to be polite
//! to a free community service. Retries with exponential backoff on
//! 429/5xx responses.
//!
//! # API reference
//!
//! - <https://www.thrustcurve.org/info/api.html>
//! - Search: `POST /api/v1/search.json` with `{"commonName": "A8"}`
//! - Download: `POST /api/v1/download.json` with `{"motorId": "<id>", "format": "RASP"}`

use std::time::Duration;

use crate::{rasp, Motor};
use base64::Engine;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://www.thrustcurve.org/api/v1";
const THROTTLE_INTERVAL: Duration = Duration::from_secs(1);
const MAX_RETRIES: u32 = 3;

/// Errors from the API client.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    #[error("API returned error: {0}")]
    Api(String),

    #[error("motor not found: {0}")]
    NotFound(String),

    #[error("no data files available for motor {0}")]
    NoData(String),

    #[error("base64 decode error: {0}")]
    Base64(base64::DecodeError),

    #[error("parse error: {0}")]
    Parse(#[from] rasp::ParseError),

    #[error("cache I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A motor summary returned by the search endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotorSummary {
    pub motor_id: String,
    pub manufacturer: String,
    pub manufacturer_abbrev: String,
    pub designation: String,
    pub common_name: String,
    pub impulse_class: String,
    pub diameter: f64, // mm
    pub length: f64,   // mm
    #[serde(default)]
    pub motor_type: String, // SU, reload, hybrid
    #[serde(default, rename = "certOrg")]
    pub cert_org: String,
    #[serde(default)]
    pub avg_thrust_n: f64,
    #[serde(default)]
    pub max_thrust_n: f64,
    #[serde(default)]
    pub tot_impulse_ns: f64,
    #[serde(default)]
    pub burn_time_s: f64,
    #[serde(default)]
    pub data_files: u32,
    #[serde(default)]
    pub total_weight_g: f64,
    #[serde(default)]
    pub prop_weight_g: f64,
    #[serde(default)]
    pub delays: String,
    #[serde(default)]
    pub availability: String,
    #[serde(default)]
    pub updated_on: String,
    #[serde(default, rename = "source_url")]
    pub source_url: String,
}

/// A download result from the download endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub motor_id: String,
    pub simfile_id: String,
    pub format: String,
    pub source: String, // "cert", "mfr", or "user"
    pub data: String,   // base64-encoded file content
    #[serde(default)]
    pub info_url: String,
    #[serde(default)]
    pub data_url: String,
    #[serde(default, rename = "source_url")]
    pub source_url: String,
}

/// Metadata stored alongside each cached motor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MotorMetadata {
    pub motor_id: String,
    pub simfile_id: String,
    pub designation: String,
    pub manufacturer: String,
    pub source: String,
    pub format: String,
    pub downloaded_at: String, // ISO 8601
    pub info_url: String,
    /// License tag from ThrustCurve.org ("PD", "free", "restricted", "unknown").
    /// Not currently returned by the download API; may require a separate
    /// call to the simfile info page. Stored as "unknown" until populated.
    pub license: String,
}

/// Response from the search endpoint.
#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    #[allow(dead_code)]
    matches: u32,
    #[serde(default)]
    results: Vec<MotorSummary>,
    #[serde(default)]
    error: Option<String>,
}

/// Response from the download endpoint.
#[derive(Deserialize)]
struct DownloadResponse {
    #[serde(default)]
    results: Vec<DownloadResult>,
    #[serde(default)]
    error: Option<String>,
}

/// Search query builder.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub common_name: Option<String>,
    pub designation: Option<String>,
    pub manufacturer: Option<String>,
    pub impulse_class: Option<String>,
    pub diameter: Option<f64>,
    pub min_tot_impulse: Option<f64>,
    pub max_tot_impulse: Option<f64>,
    pub availability: Option<String>,
}

impl SearchQuery {
    /// Search by common name (e.g., "A8", "H128").
    #[must_use]
    pub fn common_name(mut self, name: impl Into<String>) -> Self {
        self.common_name = Some(name.into());
        self
    }

    /// Search by impulse class (e.g., "H", "J").
    #[must_use]
    pub fn impulse_class(mut self, class: impl Into<String>) -> Self {
        self.impulse_class = Some(class.into());
        self
    }

    /// Search by diameter in mm.
    #[must_use]
    pub fn diameter(mut self, mm: f64) -> Self {
        self.diameter = Some(mm);
        self
    }

    /// Serialize to a JSON object for the API request body.
    fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({});
        if let Some(ref v) = self.common_name {
            obj["commonName"] = serde_json::Value::String(v.clone());
        }
        if let Some(ref v) = self.designation {
            obj["designation"] = serde_json::Value::String(v.clone());
        }
        if let Some(ref v) = self.manufacturer {
            obj["manufacturer"] = serde_json::Value::String(v.clone());
        }
        if let Some(ref v) = self.impulse_class {
            obj["impulseClass"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = self.diameter {
            obj["diameter"] = serde_json::json!(v);
        }
        if let Some(v) = self.min_tot_impulse {
            obj["minImpulseNs"] = serde_json::json!(v);
        }
        if let Some(v) = self.max_tot_impulse {
            obj["maxImpulseNs"] = serde_json::json!(v);
        }
        if let Some(ref v) = self.availability {
            obj["availability"] = serde_json::Value::String(v.clone());
        }
        obj
    }
}

/// The ThrustCurve.org API client.
pub struct ThrustCurveClient {
    http: reqwest::Client,
    last_request: tokio::sync::Mutex<Option<std::time::Instant>>,
}

impl ThrustCurveClient {
    /// Create a new client with default settings.
    ///
    /// # Panics
    ///
    /// Panics if the `reqwest::Client` cannot be built, which should only
    /// happen with invalid configuration (the defaults are valid).
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent(concat!("aether/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client builds with valid config"),
            last_request: tokio::sync::Mutex::new(None),
        }
    }

    /// Search for motors matching the given query.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure, HTTP error, or API error.
    pub async fn search(&self, query: &SearchQuery) -> Result<Vec<MotorSummary>, ApiError> {
        let body = query.to_json();
        let resp = self.post("/search.json", &body).await?;

        let parsed: SearchResponse = resp.json().await?;
        if let Some(err) = parsed.error {
            return Err(ApiError::Api(err));
        }
        Ok(parsed.results)
    }

    /// Download all available data files for a motor.
    ///
    /// Returns a list of `DownloadResult`s (a motor may have multiple
    /// simfiles from different sources). The `data` field in each is
    /// base64-encoded RASP (or RSE) content.
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on network failure, HTTP error, or if the motor
    /// has no data files.
    pub async fn download(&self, motor_id: &str) -> Result<Vec<DownloadResult>, ApiError> {
        let body = serde_json::json!({
            "motorId": motor_id,
            "format": "RASP"
        });
        let resp = self.post("/download.json", &body).await?;

        let parsed: DownloadResponse = resp.json().await?;
        if let Some(err) = parsed.error {
            return Err(ApiError::Api(err));
        }
        if parsed.results.is_empty() {
            return Err(ApiError::NoData(motor_id.to_string()));
        }
        Ok(parsed.results)
    }

    /// Download a motor, decode the base64 data, parse it as RASP, and
    /// return the parsed `Motor` along with its metadata.
    ///
    /// If multiple simfiles are available, returns the first one (typically
    /// from a certification source).
    ///
    /// # Errors
    ///
    /// Returns `ApiError` on any failure (network, parse, etc.).
    pub async fn fetch_motor(&self, motor_id: &str) -> Result<(Motor, MotorMetadata), ApiError> {
        let downloads = self.download(motor_id).await?;
        let dl = downloads
            .into_iter()
            .next()
            .ok_or(ApiError::NoData(motor_id.to_string()))?;

        let eng_bytes = base64::engine::general_purpose::STANDARD
            .decode(&dl.data)
            .map_err(ApiError::Base64)?;
        let eng_text = String::from_utf8_lossy(&eng_bytes);
        let motor = rasp::parse(&eng_text)?;

        let metadata = MotorMetadata {
            motor_id: dl.motor_id,
            simfile_id: dl.simfile_id,
            designation: motor.designation.clone(),
            manufacturer: motor.manufacturer.clone(),
            source: dl.source,
            format: dl.format,
            downloaded_at: now_iso8601(),
            info_url: dl.info_url,
            license: "unknown".to_string(), // Not available from download API
        };

        Ok((motor, metadata))
    }

    /// Throttled POST request with retry on 429/5xx.
    async fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, ApiError> {
        let url = format!("{BASE_URL}{path}");

        for attempt in 0..MAX_RETRIES {
            self.throttle().await;

            let resp = self.http.post(&url).json(body).send().await?;
            let status = resp.status();

            if status.is_success() {
                return Ok(resp);
            }

            if (status.as_u16() == 429 || status.is_server_error()) && attempt + 1 < MAX_RETRIES {
                let backoff = Duration::from_secs(1 << attempt);
                tokio::time::sleep(backoff).await;
                continue;
            }

            let body_text = resp.text().await.unwrap_or_default();
            return Err(ApiError::Http {
                status: status.as_u16(),
                body: body_text,
            });
        }

        unreachable!("retry loop exhausted");
    }

    /// Ensure at least `THROTTLE_INTERVAL` has elapsed since the last request.
    async fn throttle(&self) {
        let mut last = self.last_request.lock().await;
        let now = std::time::Instant::now();
        if let Some(prev) = *last {
            let elapsed = now.duration_since(prev);
            if let Some(remaining) = THROTTLE_INTERVAL.checked_sub(elapsed) {
                tokio::time::sleep(remaining).await;
            }
        }
        *last = Some(std::time::Instant::now());
    }
}

impl Default for ThrustCurveClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Current time as an ISO 8601 string.
fn now_iso8601() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn search_query_serializes_common_name() {
        let q = SearchQuery::default().common_name("A8");
        let json = q.to_json();
        assert_eq!(json["commonName"], "A8");
    }

    #[test]
    fn search_query_serializes_multiple_fields() {
        let q = SearchQuery::default()
            .common_name("H128")
            .impulse_class("H")
            .diameter(38.0);
        let json = q.to_json();
        assert_eq!(json["commonName"], "H128");
        assert_eq!(json["impulseClass"], "H");
        assert_eq!(json["diameter"], 38.0);
    }

    #[test]
    fn search_query_empty_produces_empty_object() {
        let q = SearchQuery::default();
        let json = q.to_json();
        assert!(json.as_object().unwrap().is_empty());
    }
}
