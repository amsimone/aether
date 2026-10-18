//! Version 3 project envelope with retained run records; version 1 designs import without inventing a motor.

use super::{load_from_json, write_atomic, DesignFileError, DESIGN_SCHEMA_VERSION};
use rocket_core::{
    validation::{validate_rocket, ValidationError},
    Rocket,
};
use rocket_sim::{RunRecord, SimulationCase};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{io::Write, path::Path};

/// Project envelope schema, distinct from the nested vehicle schema.
pub const PROJECT_SCHEMA_VERSION: u32 = 3;

/// A vehicle and one complete named flight-case configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Project envelope schema.
    pub version: u32,
    /// Vehicle engineering model (schema 1).
    pub rocket: Rocket,
    /// The analysis case; motor may be unresolved in an editing document.
    pub case: SimulationCase,
    #[serde(default)]
    pub runs: Vec<Arc<RunRecord>>,
}
impl Project {
    /// Import a vehicle with a named default case and an explicitly unresolved motor.
    #[must_use]
    pub fn from_rocket(rocket: Rocket) -> Self {
        Self {
            version: PROJECT_SCHEMA_VERSION,
            rocket,
            case: SimulationCase::default(),
            runs: Vec::new(),
        }
    }

    /// Validate both schema identifiers and engineering inputs.
    ///
    /// # Errors
    /// Rejects unsupported schema versions or invalid values/references.
    pub fn validate(&self) -> Result<(), DesignFileError> {
        if self.version != PROJECT_SCHEMA_VERSION {
            return Err(DesignFileError::UnsupportedVersion(self.version));
        }
        if self.rocket.version != DESIGN_SCHEMA_VERSION {
            return Err(DesignFileError::UnsupportedVersion(self.rocket.version));
        }
        validate_rocket(&self.rocket)?;
        self.case.validate()?;
        let mut ids = std::collections::HashSet::new();
        for run in &self.runs {
            run.validate()?;
            if !ids.insert(&run.id) {
                return Err(ValidationError("Duplicate retained-run ID".into()).into());
            }
        }
        Ok(())
    }

    /// Engineering validation suitable for commands after schema dispatch.
    ///
    /// # Errors
    /// Rejects invalid vehicle or case data.
    pub fn validate_inputs(&self) -> Result<(), ValidationError> {
        validate_rocket(&self.rocket)?;
        self.case.validate()
    }
}

/// Load a project, migrating a legacy design to an unresolved default case.
///
/// # Errors
/// Rejects malformed, unsupported, or semantically invalid files.
pub fn load_project_from_json(json: &str) -> Result<Project, DesignFileError> {
    #[derive(Deserialize)]
    struct Header {
        version: u32,
    }
    let header: Header = serde_json::from_str(json)?;
    let project = match header.version {
        DESIGN_SCHEMA_VERSION => Project::from_rocket(load_from_json(json)?),
        2 => {
            let mut legacy: Project = serde_json::from_str(json)?;
            legacy.version = PROJECT_SCHEMA_VERSION;
            legacy
        }
        PROJECT_SCHEMA_VERSION => serde_json::from_str(json)?,
        other => return Err(DesignFileError::UnsupportedVersion(other)),
    };
    project.validate()?;
    Ok(project)
}

/// Serialize all accepted project inputs, including the resolved motor curve.
///
/// # Errors
/// Rejects invalid projects or serialization failures.
pub fn save_project_to_json(project: &Project) -> Result<String, DesignFileError> {
    project.validate()?;
    Ok(serde_json::to_string_pretty(project)?)
}

/// Read a validated project or legacy design from disk.
///
/// # Errors
/// Returns filesystem, schema, parsing, or engineering-validation errors.
pub fn load_project_from_path(path: impl AsRef<Path>) -> Result<Project, DesignFileError> {
    load_project_from_json(&std::fs::read_to_string(path)?)
}

/// Atomically save a version 2 project using the design writer's guarantees.
///
/// # Errors
/// Rejects invalid projects before touching the destination; reports I/O failures.
pub fn save_project_to_path(
    project: &Project,
    path: impl AsRef<Path>,
) -> Result<(), DesignFileError> {
    let json = save_project_to_json(project)?;
    write_atomic(path.as_ref(), |file| file.write_all(json.as_bytes()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        let rocket =
            load_from_json(include_str!("../../../examples/aether-one.rocket")).expect("reference");
        let mut project = Project::from_rocket(rocket);
        project.case.name = "Wind sensitivity".into();
        project.case.motor = Some(
            rocket_motors::rasp::parse(include_str!("../../../tests/fixtures/motors/estes_b6.eng"))
                .expect("motor"),
        );
        project.case.config.launch_angle = 0.08;
        project.case.config.wind = (3.0, 1.0, 0.0);
        project.case.config.pad_altitude = rocket_core::Length::meters(120.0);
        project.case.config.time_step = 0.002;
        project.case.config.telemetry_interval = 0.02;
        project.case.recovery.main_altitude_m = 75.0;
        project.case.recovery.main_cd_a = 0.4;
        project.case.recovery.drogue_cd_a = 0.03;
        project.case.maximum_duration_s = 5.0;
        project
    }

    #[test]
    fn case_roundtrip_preserves_exact_motor_settings_and_repeatable_execution() {
        let project = project();
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("project.rocket");
        save_project_to_path(&project, &path).expect("save");
        let loaded = load_project_from_path(&path).expect("load");
        assert_eq!(project, loaded);
        let first = project.case.run(&project.rocket).expect("original run");
        let second = loaded.case.run(&loaded.rocket).expect("reloaded run");
        assert_eq!(first.max_altitude, second.max_altitude);
        assert_eq!(first.max_velocity, second.max_velocity);
        assert_eq!(first.telemetry.len(), second.telemetry.len());
        for (before, after) in first.telemetry.iter().zip(&second.telemetry) {
            assert_eq!(before.position, after.position);
            assert_eq!(before.velocity, after.velocity);
            assert_eq!(before.acceleration, after.acceleration);
            assert_eq!(before.mass, after.mass);
        }
        assert!(first.telemetry.last().expect("telemetry").time.as_seconds() >= 5.0);
    }

    #[test]
    fn project_numeric_inputs_roundtrip_bit_for_bit() {
        let mut project = project();
        let mut seed = 0x1234_5678_90ab_cdef_u64;
        for _ in 0..128 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let value =
                f64::from_bits(0x3ff0_0000_0000_0000 | (seed & 0x000f_ffff_ffff_ffff)) - 1.0;
            project.case.config.wind.0 = value;
            let loaded = load_project_from_json(&save_project_to_json(&project).expect("save"))
                .expect("load");
            assert_eq!(
                value.to_bits(),
                loaded.case.config.wind.0.to_bits(),
                "input {value}"
            );
        }
    }

    #[test]
    fn legacy_import_keeps_vehicle_and_has_an_explicit_unresolved_case() {
        let json = include_str!("../../../examples/aether-one.rocket");
        let legacy = load_from_json(json).expect("legacy");
        let migrated = load_project_from_json(json).expect("migration");
        assert_eq!(migrated.rocket, legacy);
        assert_eq!(migrated.case, SimulationCase::default());
        assert!(migrated.case.run(&migrated.rocket).is_err());
        assert_eq!(
            migrated,
            load_project_from_json(&save_project_to_json(&migrated).expect("save"))
                .expect("load migrated")
        );
    }

    #[test]
    fn unsupported_schemas_models_and_invalid_references_are_rejected() {
        assert!(matches!(
            load_project_from_json("{\"version\":99}"),
            Err(DesignFileError::UnsupportedVersion(99))
        ));
        let original = project();
        let mut json = serde_json::to_value(&original).expect("JSON");
        json["case"]["models"] = "unknown_future_model".into();
        assert!(load_project_from_json(&json.to_string()).is_err());
        let mut invalid = original.clone();
        invalid.rocket.version = 9;
        assert!(matches!(
            load_project_from_json(&serde_json::to_string(&invalid).expect("JSON")),
            Err(DesignFileError::UnsupportedVersion(9))
        ));
        let mut invalid = original;
        invalid.rocket.root.components.remove(1); // dangling fin mount
        assert!(matches!(
            load_project_from_json(&serde_json::to_string(&invalid).expect("JSON")),
            Err(DesignFileError::Validation(_))
        ));
    }

    #[test]
    fn invalid_case_save_never_replaces_the_previous_project() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("project.rocket");
        let mut project = project();
        save_project_to_path(&project, &path).expect("save");
        let previous = std::fs::read(&path).expect("bytes");
        project.case.config.time_step = f64::NAN;
        assert!(save_project_to_path(&project, &path).is_err());
        assert_eq!(std::fs::read(&path).expect("preserved"), previous);
        assert_eq!(std::fs::read_dir(dir.path()).expect("entries").count(), 1);
    }
}
