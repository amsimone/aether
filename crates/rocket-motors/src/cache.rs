//! Local motor cache.
//!
//! Stores downloaded motor data files (`.eng`) and their metadata
//! (`.meta.json`) in a per-user cache directory. This avoids re-fetching
//! from ThrustCurve.org on every use and enables offline operation
//! (ADR-0006).
//!
//! # Cache layout
//!
//! ```text
//! ~/.cache/aether/motors/             (Linux)
//! ~/Library/Caches/aether/motors/     (macOS)
//! %LOCALAPPDATA%\Aether\motors\       (Windows)
//! ├── <motor_id>.eng                  RASP file
//! ├── <motor_id>.meta.json            Metadata sidecar
//! └── ...
//! ```

use std::path::{Path, PathBuf};

use crate::api::MotorMetadata;
use crate::rasp;
use crate::Motor;

/// Errors from the cache.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("parse error: {0}")]
    Parse(#[from] rasp::ParseError),

    #[error("motor not in cache: {0}")]
    NotCached(String),
}

/// The on-disk motor cache.
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// Create a `Cache` pointing at the default per-OS cache directory.
    ///
    /// Returns `None` if the cache directory cannot be determined (rare;
    /// happens on systems without a standard config path).
    #[must_use]
    pub fn default_dir() -> Option<PathBuf> {
        let proj = directories::ProjectDirs::from("dev", "aether", "Aether")?;
        Some(proj.cache_dir().join("motors"))
    }

    /// Create a `Cache` at the default per-OS location.
    ///
    /// # Errors
    ///
    /// Returns `CacheError` if the cache directory cannot be created.
    pub fn new() -> Result<Self, CacheError> {
        let dir = Self::default_dir().ok_or_else(|| {
            CacheError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot determine cache directory",
            ))
        })?;
        Self::at(dir)
    }

    /// Create a `Cache` at a specific directory (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns `CacheError` if the directory cannot be created.
    pub fn at(dir: impl AsRef<Path>) -> Result<Self, CacheError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Return the cache directory path.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.dir
    }

    /// Store a motor and its metadata in the cache.
    ///
    /// Writes `<motor_id>.eng` (the original RASP text) and
    /// `<motor_id>.meta.json` (the metadata sidecar).
    ///
    /// # Errors
    ///
    /// Returns `CacheError` on I/O or serialization failure.
    pub fn store(&self, motor: &Motor, metadata: &MotorMetadata) -> Result<(), CacheError> {
        let eng_path = self.eng_path(&metadata.motor_id);
        let meta_path = self.meta_path(&metadata.motor_id);

        // Serialize the motor back to RASP format for storage.
        // (For now, we store the parsed Motor as JSON for round-trip safety.
        //  A future version will write canonical RASP text.)
        let motor_json = serde_json::to_string_pretty(motor)?;
        std::fs::write(&eng_path, motor_json)?;

        let meta_json = serde_json::to_string_pretty(metadata)?;
        std::fs::write(&meta_path, meta_json)?;

        Ok(())
    }

    /// Retrieve a motor from the cache by motor ID.
    ///
    /// # Errors
    ///
    /// Returns `CacheError::NotCached` if the motor is not in the cache.
    /// Returns other `CacheError` variants on I/O or parse failure.
    pub fn get(&self, motor_id: &str) -> Result<(Motor, MotorMetadata), CacheError> {
        let eng_path = self.eng_path(motor_id);
        let meta_path = self.meta_path(motor_id);

        if !eng_path.exists() {
            return Err(CacheError::NotCached(motor_id.to_string()));
        }

        let motor_json = std::fs::read_to_string(&eng_path)?;
        let motor: Motor = serde_json::from_str(&motor_json)?;

        let meta_json = std::fs::read_to_string(&meta_path)?;
        let metadata: MotorMetadata = serde_json::from_str(&meta_json)?;

        Ok((motor, metadata))
    }

    /// Check if a motor is cached.
    #[must_use]
    pub fn contains(&self, motor_id: &str) -> bool {
        self.eng_path(motor_id).exists()
    }

    /// List all cached motors' metadata.
    ///
    /// # Errors
    ///
    /// Returns `CacheError` on directory read failure. Individual
    /// metadata parse failures are skipped (not propagated).
    pub fn list(&self) -> Result<Vec<MotorMetadata>, CacheError> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only process `*.meta.json` files (the motor data files
            // are `<id>.json`, metadata sidecars are `<id>.meta.json`).
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !name.ends_with(".meta.json") {
                continue;
            }

            let json = std::fs::read_to_string(&path)?;
            if let Ok(meta) = serde_json::from_str::<MotorMetadata>(&json) {
                entries.push(meta);
            }
        }
        Ok(entries)
    }

    fn eng_path(&self, motor_id: &str) -> PathBuf {
        self.dir.join(format!("{motor_id}.json"))
    }

    fn meta_path(&self, motor_id: &str) -> PathBuf {
        self.dir.join(format!("{motor_id}.meta.json"))
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new().expect("cache directory is creatable")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::api::MotorMetadata;

    fn test_motor() -> Motor {
        rasp::parse("A8-3 18 70 3 0.0033 0.0167 Estes\n0.026 9.57\n0.506 0.0\n").unwrap()
    }

    fn test_metadata() -> MotorMetadata {
        MotorMetadata {
            motor_id: "test123".to_string(),
            simfile_id: "sim456".to_string(),
            designation: "A8-3".to_string(),
            manufacturer: "Estes".to_string(),
            source: "cert".to_string(),
            format: "RASP".to_string(),
            downloaded_at: "2026-01-01T00:00:00Z".to_string(),
            info_url: "/simfiles/sim456/".to_string(),
            license: "unknown".to_string(),
        }
    }

    #[test]
    fn store_and_retrieve_motor() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = Cache::at(tmp.path()).unwrap();

        let motor = test_motor();
        let meta = test_metadata();

        cache.store(&motor, &meta).unwrap();
        assert!(cache.contains("test123"));

        let (retrieved_motor, retrieved_meta) = cache.get("test123").unwrap();
        assert_eq!(motor, retrieved_motor);
        assert_eq!(meta, retrieved_meta);
    }

    #[test]
    fn get_uncached_motor_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = Cache::at(tmp.path()).unwrap();

        let result = cache.get("nonexistent");
        assert!(matches!(result, Err(CacheError::NotCached(_))));
    }

    #[test]
    fn contains_returns_false_for_uncached() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = Cache::at(tmp.path()).unwrap();
        assert!(!cache.contains("nope"));
    }
}
