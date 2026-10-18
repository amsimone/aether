//! Portable run/data files and SI telemetry export.
use crate::{write_atomic, DesignFileError};
use rocket_sim::RunRecord;
use std::{fmt::Write as _, io::Write, path::Path};

/// Persist a self-contained run record atomically.
/// # Errors
/// Rejects invalid records and reports filesystem/serialization errors.
pub fn save_run_to_path(run: &RunRecord, path: impl AsRef<Path>) -> Result<(), DesignFileError> {
    run.validate()?;
    let json = serde_json::to_vec_pretty(run)?;
    write_atomic(path.as_ref(), |f| f.write_all(&json))?;
    Ok(())
}
/// Import a validated retained run.
/// # Errors
/// Reports malformed records or I/O failures.
pub fn load_run_from_path(path: impl AsRef<Path>) -> Result<RunRecord, DesignFileError> {
    let record: RunRecord = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    record.validate()?;
    Ok(record)
}
/// Import the explicit aerodynamic JSON contract; never infer axes from CSV column names.
/// # Errors
/// Rejects invalid metadata, coefficients or grids.
pub fn load_aero_dataset(
    path: impl AsRef<Path>,
) -> Result<rocket_aero::AeroDataset, DesignFileError> {
    let data: rocket_aero::AeroDataset = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    data.validate()?;
    Ok(data)
}
/// Exact decimal round-trippable telemetry, with explicit SI column units.
#[must_use]
pub fn telemetry_csv(run: &RunRecord) -> String {
    let mut out=String::from("time_s,height_agl_m,altitude_msl_m,x_m,y_m,vx_m_s,vy_m_s,vz_m_s,speed_m_s,mass_kg,thrust_n,drag_n,acceleration_m_s2,airspeed_m_s,mach,dynamic_pressure_pa,reynolds,alpha_rad,beta_rad,qw,qx,qy,qz,roll_rate_rad_s,pitch_rate_rad_s,yaw_rate_rad_s,ax_m_s2,ay_m_s2,az_m_s2,density_kg_m3,temperature_k,drogue_deployed,main_deployed,aero_source\n");
    for p in &run.result.telemetry {
        let speed = (p.velocity.0.powi(2) + p.velocity.1.powi(2) + p.velocity.2.powi(2)).sqrt();
        let values = [
            p.time.as_seconds(),
            p.position.2 - run.case.config.pad_altitude.as_meters(),
            p.position.2,
            p.position.0,
            p.position.1,
            p.velocity.0,
            p.velocity.1,
            p.velocity.2,
            speed,
            p.mass.as_kilograms(),
            p.thrust.as_newtons(),
            p.drag.as_newtons(),
            p.acceleration,
        ];
        let mut row: Vec<String> = values.iter().map(ToString::to_string).collect();
        if let Some(f) = p.flight {
            row.extend(
                [
                    f.airspeed_m_s,
                    f.mach,
                    f.dynamic_pressure_pa,
                    f.reynolds,
                    f.alpha_rad,
                    f.beta_rad,
                ]
                .iter()
                .map(ToString::to_string),
            );
            row.extend(f.quaternion_wxyz.iter().map(ToString::to_string));
            row.extend(f.angular_rate_rad_s.iter().map(ToString::to_string));
            row.extend(f.acceleration_m_s2.iter().map(ToString::to_string));
            row.extend(
                [f.density_kg_m3, f.temperature_k]
                    .iter()
                    .map(ToString::to_string),
            );
            row.push(u8::from(f.drogue_deployed).to_string());
            row.push(u8::from(f.main_deployed).to_string());
            row.push(
                match f.aero_source {
                    rocket_aero::AeroSource::Builtin => "builtin",
                    rocket_aero::AeroSource::Dataset => "dataset",
                    rocket_aero::AeroSource::BuiltinOutsideDataset => "builtin_outside_dataset",
                    rocket_aero::AeroSource::ZeroAirspeed => "zero_airspeed",
                }
                .into(),
            );
        } else {
            row.extend(std::iter::repeat_n(String::new(), 21));
        }
        writeln!(&mut out, "{}", row.join(",")).expect("string write");
    }
    out
}
/// Atomically export one retained run's telemetry.
/// # Errors
/// Reports invalid records or write failures.
pub fn save_telemetry_csv(run: &RunRecord, path: impl AsRef<Path>) -> Result<(), DesignFileError> {
    run.validate()?;
    let csv = telemetry_csv(run);
    write_atomic(path.as_ref(), |f| f.write_all(csv.as_bytes()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn record() -> RunRecord {
        let mut project = crate::load_project_from_json(include_str!(
            "../../../examples/aether-one-project.rocket"
        ))
        .expect("project");
        project.case.maximum_duration_s = 0.2;
        project.case.config.pad_altitude = rocket_core::Length::meters(1500.0);
        let result = project.case.run(&project.rocket).expect("run");
        RunRecord::capture(project.rocket, project.case, result, 7)
    }
    #[test]
    fn full_run_and_project_roundtrip_keep_exact_inputs_outputs_and_outcome() {
        let record = record();
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("run.json");
        save_run_to_path(&record, &path).expect("save run");
        let loaded = load_run_from_path(&path).expect("load run");
        assert_eq!(record, loaded);
        let mut project = crate::Project::from_rocket(record.rocket.clone());
        project.case = record.case.clone();
        project.runs.push(Arc::new(record));
        let path = directory.path().join("project.rocket");
        crate::save_project_to_path(&project, &path).expect("save project");
        assert_eq!(
            project,
            crate::load_project_from_path(path).expect("load project")
        );
    }
    #[test]
    fn csv_has_consistent_columns_correct_agl_and_exact_numerical_values() {
        let record = record();
        let csv = telemetry_csv(&record);
        let lines: Vec<_> = csv.lines().collect();
        let headers: Vec<_> = lines[0].split(',').collect();
        assert_eq!(headers.len(), 34);
        assert_eq!(lines.len(), record.result.telemetry.len() + 1);
        for (line, sample) in lines.iter().skip(1).zip(&record.result.telemetry) {
            let row: Vec<_> = line.split(',').collect();
            assert_eq!(row.len(), headers.len());
            assert_eq!(
                row[1].parse::<f64>().expect("AGL"),
                sample.position.2 - 1500.0
            );
            assert_eq!(
                row[14].parse::<f64>().expect("Mach").to_bits(),
                sample.flight.expect("6DOF").mach.to_bits()
            );
        }
        assert_eq!(lines[1].split(',').nth(1), Some("0"));
    }
    #[test]
    fn legacy_schema_two_imports_as_current_project_without_inventing_results() {
        let source = include_str!("../../../examples/aether-one-project.rocket");
        let project = crate::load_project_from_json(source).expect("schema 2 migration");
        assert_eq!(project.version, crate::PROJECT_SCHEMA_VERSION);
        assert_eq!(project.runs.len(), 0);
        assert_eq!(project.case.aerodynamics, rocket_aero::AeroModel::Builtin);
    }
}
