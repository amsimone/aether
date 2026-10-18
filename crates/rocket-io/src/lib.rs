//! # rocket-io
//!
//! Persistence and export:
//! - `.rocket` file format (JSON, versioned, serde-based, ADR-0008).
//! - CSV telemetry export.
//! - KML trajectory export (for Google Earth).
//! - STL geometry export (for 3D printing / manufacturing).
//! - PDF flight reports (via Typst, future).
//!
//! See:
//! - `docs/adr/0008-local-only-files-v1.md`
//! - `rocket-simulator-plan.md` §13

#![deny(unsafe_code)]
#![warn(clippy::pedantic)]

use std::fmt;
use std::io::{self, Write};
use std::path::Path;

use rocket_core::Rocket;
use serde::Deserialize;
mod analysis;
mod project;
pub use analysis::{
    load_aero_dataset, load_run_from_path, save_run_to_path, save_telemetry_csv, telemetry_csv,
};
pub use project::{
    load_project_from_json, load_project_from_path, save_project_to_json, save_project_to_path,
    Project, PROJECT_SCHEMA_VERSION,
};

/// The design schema understood by this release.
pub const DESIGN_SCHEMA_VERSION: u32 = 1;

/// A design could not be decoded or persisted.
#[derive(Debug)]
pub enum DesignFileError {
    /// Decodable values violate engineering constraints.
    Validation(rocket_core::validation::ValidationError),
    /// Malformed JSON or a value incompatible with the supported schema.
    Json(serde_json::Error),
    /// This release cannot interpret the declared schema version.
    UnsupportedVersion(u32),
    /// A filesystem operation failed.
    Io(io::Error),
}

impl fmt::Display for DesignFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => write!(f, "Invalid project: {error}"),
            Self::Json(error) => write!(f, "Invalid design: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "Unsupported file schema version {version}")
            }
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DesignFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::UnsupportedVersion(_) => None,
        }
    }
}

impl From<rocket_core::validation::ValidationError> for DesignFileError {
    fn from(error: rocket_core::validation::ValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<serde_json::Error> for DesignFileError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<io::Error> for DesignFileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Serialize a design in the current schema to readable JSON.
///
/// # Errors
/// Returns an error for unsupported schema versions or serialization failures.
pub fn save_to_json(rocket: &Rocket) -> Result<String, DesignFileError> {
    check_version(rocket.version)?;
    Ok(serde_json::to_string_pretty(rocket)?)
}

/// Decode a design only after dispatching on its declared schema version.
///
/// # Errors
/// Returns an error for malformed input or unsupported schema versions.
/// Unknown versions are rejected before trying to interpret their contents.
pub fn load_from_json(json: &str) -> Result<Rocket, DesignFileError> {
    #[derive(Deserialize)]
    struct Header {
        version: u32,
    }
    let header: Header = serde_json::from_str(json)?;
    check_version(header.version)?;
    Ok(serde_json::from_str(json)?)
}

fn check_version(version: u32) -> Result<(), DesignFileError> {
    if version == DESIGN_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(DesignFileError::UnsupportedVersion(version))
    }
}

/// Read a design from disk with the same schema checks as JSON import.
///
/// # Errors
/// Returns filesystem, schema, or decoding errors without modifying the file.
pub fn load_from_path(path: impl AsRef<Path>) -> Result<Rocket, DesignFileError> {
    load_from_json(&std::fs::read_to_string(path)?)
}

/// Save by syncing a temporary file beside the destination, then atomically replacing it.
///
/// Existing file permissions are preserved. A failure before replacement leaves
/// the previous design intact and removes the temporary file. This guarantees
/// atomic replacement, not durability of directory metadata across a power loss.
///
/// # Errors
/// Returns serialization or filesystem errors. The parent directory must exist.
pub fn save_to_path(rocket: &Rocket, path: impl AsRef<Path>) -> Result<(), DesignFileError> {
    let json = save_to_json(rocket)?;
    write_atomic(path.as_ref(), |file| file.write_all(json.as_bytes()))?;
    Ok(())
}

fn write_atomic(
    path: &Path,
    write: impl FnOnce(&mut std::fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    match std::fs::metadata(path) {
        Ok(metadata) => temporary
            .as_file()
            .set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    write(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let rocket = Rocket::new("Round Trip Test");
        let json = save_to_json(&rocket).expect("save should succeed");
        assert_eq!(rocket, load_from_json(&json).expect("load should succeed"));
    }

    #[test]
    fn unknown_schema_is_rejected_before_decoding_the_body() {
        for version in [0, 2, u32::MAX] {
            let json = format!("{{\"version\":{version},\"future_payload\":[]}}");
            assert!(
                matches!(load_from_json(&json), Err(DesignFileError::UnsupportedVersion(v)) if v==version)
            );
        }
        assert!(matches!(
            load_from_json("{\"name\":\"missing version\"}"),
            Err(DesignFileError::Json(_))
        ));
    }

    #[test]
    fn atomic_save_creates_and_replaces_an_existing_design() {
        let dir = tempfile::tempdir().expect("temp directory");
        let path = dir.path().join("design.rocket");
        save_to_path(&Rocket::new("First"), &path).expect("create");
        let replacement = Rocket::new("Second");
        save_to_path(&replacement, &path).expect("replace");
        assert_eq!(replacement, load_from_path(&path).expect("load"));
        assert_eq!(std::fs::read_dir(dir.path()).expect("entries").count(), 1);
    }

    #[test]
    fn partial_write_failure_preserves_the_previous_file_and_cleans_up() {
        let dir = tempfile::tempdir().expect("temp directory");
        let path = dir.path().join("design.rocket");
        save_to_path(&Rocket::new("Previous"), &path).expect("initial save");
        let previous = std::fs::read(&path).expect("bytes");
        let result = write_atomic(&path, |file| {
            file.write_all(b"partial invalid replacement")?;
            Err(io::Error::other("injected write failure"))
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).expect("preserved file"), previous);
        assert_eq!(std::fs::read_dir(dir.path()).expect("entries").count(), 1);
    }

    #[test]
    fn unsupported_save_preserves_existing_bytes() {
        let dir = tempfile::tempdir().expect("temp directory");
        let path = dir.path().join("design.rocket");
        let mut rocket = Rocket::new("Previous");
        save_to_path(&rocket, &path).expect("save");
        let previous = std::fs::read(&path).expect("bytes");
        rocket.version = 2;
        assert!(matches!(
            save_to_path(&rocket, &path),
            Err(DesignFileError::UnsupportedVersion(2))
        ));
        assert_eq!(std::fs::read(&path).expect("preserved file"), previous);
    }

    #[test]
    fn replacement_failure_preserves_the_destination_and_cleans_up() {
        let dir = tempfile::tempdir().expect("temp directory");
        let path = dir.path().join("existing-directory");
        std::fs::create_dir(&path).expect("destination directory");
        assert!(save_to_path(&Rocket::new("Test"), &path).is_err());
        assert!(path.is_dir());
        assert_eq!(std::fs::read_dir(dir.path()).expect("entries").count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn replacing_a_design_preserves_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp directory");
        let path = dir.path().join("design.rocket");
        save_to_path(&Rocket::new("Test"), &path).expect("save");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("permissions");
        save_to_path(&Rocket::new("Replaced"), &path).expect("replace");
        assert_eq!(
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}
