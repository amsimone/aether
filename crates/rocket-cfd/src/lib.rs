//! Explicit external CFD studies. No display mesh, shell interpolation, or automatic data activation.
#![allow(clippy::too_many_lines)]
use rocket_aero::{vehicle_fingerprint, AeroDataset, CoefficientRow};
use rocket_core::{
    validation::{validate_rocket, ValidationError},
    Component, FinPlanform, Rocket,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const DRIVER: &str = include_str!("driver.py");
/// One steady compressible RANS operating point. Coefficients require independent validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CfdSettings {
    /// Launch-lug placement is approximate; inclusion requires finer surface resolution.
    pub include_launch_lugs: bool,
    pub mach: f64,
    pub alpha_deg: f64,
    pub beta_deg: f64,
    pub pressure_pa: f64,
    pub temperature_k: f64,
    pub mesh_size_m: f64,
    pub iterations: u32,
    pub reference_area_m2: f64,
    pub reference_length_m: f64,
    pub moment_reference_station_m: f64,
    pub turbulence_intensity: f64,
}
impl CfdSettings {
    #[must_use]
    pub fn for_vehicle(rocket: &Rocket) -> Self {
        let diameter = rocket
            .root
            .iter()
            .filter_map(|c| match c {
                Component::BodyTube(c) => Some(c.outer_diameter.as_meters()),
                Component::NoseCone(c) => Some(c.base_diameter.as_meters()),
                _ => None,
            })
            .fold(0.0, f64::max)
            .max(0.001);
        Self {
            include_launch_lugs: false,
            mach: 0.2,
            alpha_deg: 0.0,
            beta_deg: 0.0,
            pressure_pa: 101_325.0,
            temperature_k: 288.15,
            mesh_size_m: diameter / 6.0,
            iterations: 600,
            reference_area_m2: std::f64::consts::PI * diameter * diameter / 4.0,
            reference_length_m: diameter,
            moment_reference_station_m: rocket.mass_properties().cg_x.as_meters(),
            turbulence_intensity: 0.01,
        }
    }
    /// # Errors
    /// Rejects settings outside the first backend's supported domain or resource bounds.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if [
            self.mach,
            self.alpha_deg,
            self.beta_deg,
            self.pressure_pa,
            self.temperature_k,
            self.mesh_size_m,
            self.reference_area_m2,
            self.reference_length_m,
            self.moment_reference_station_m,
            self.turbulence_intensity,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || !(0.05..=0.7).contains(&self.mach)
            || self.alpha_deg.abs() > 15.0
            || self.beta_deg.abs() > 15.0
            || !(1000.0..=200_000.0).contains(&self.pressure_pa)
            || !(180.0..=350.0).contains(&self.temperature_k)
            || self.mesh_size_m <= 0.0
            || self.reference_area_m2 <= 0.0
            || self.reference_length_m <= 0.0
            || self.moment_reference_station_m < 0.0
            || !(50..=10_000).contains(&self.iterations)
            || !(0.0001..=0.2).contains(&self.turbulence_intensity)
        {
            return Err(ValidationError("CFD supports Mach 0.05–0.7, angles ±15°, positive mesh/reference sizes, pressure 1000–200000 Pa, temperature 180–350 K, turbulence intensity 0.0001–0.2, and 50–10000 iterations".into()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Profile {
    points: Vec<[f64; 2]>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Fin {
    polygon: Vec<[f64; 2]>,
    thickness: f64,
    count: u8,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Lug {
    x: f64,
    length: f64,
    inner: f64,
    outer: f64,
    offset: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Pod {
    x: f64,
    length: f64,
    radius: f64,
    offset: f64,
    angle: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Geometry {
    profiles: Vec<Profile>,
    fins: Vec<Fin>,
    lugs: Vec<Lug>,
    pods: Vec<Pod>,
    length: f64,
    diameter: f64,
    omitted: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Input {
    version: u32,
    rocket: Rocket,
    vehicle_sha256: String,
    settings: CfdSettings,
    geometry: Geometry,
    driver_sha256: String,
}
/// Outcome of the actual solver, not an aerodynamic validation certificate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfdOutcome {
    pub version: u32,
    pub vehicle_sha256: String,
    pub settings: CfdSettings,
    pub coefficients: CoefficientRow,
    pub reynolds: f64,
    pub mesh_cells: u64,
    pub iterations: u32,
    pub converged: bool,
    pub coefficient_relative_change: f64,
    pub solver_version: String,
    pub gmsh_version: String,
    pub dataset: Option<AeroDataset>,
}

fn geometry(rocket: &Rocket, settings: &CfdSettings) -> Result<Geometry, ValidationError> {
    validate_rocket(rocket)?;
    if rocket.root.upper_stage.is_some() {
        return Err(ValidationError(
            "CFD geometry currently supports one stage".into(),
        ));
    }
    let mut g = Geometry {
        profiles: Vec::new(),
        fins: Vec::new(),
        lugs: Vec::new(),
        pods: Vec::new(),
        length: 0.0,
        diameter: rocket
            .root
            .iter()
            .filter_map(|component| match component {
                Component::NoseCone(c) => Some(c.base_diameter.as_meters()),
                Component::BodyTube(c) => Some(c.outer_diameter.as_meters()),
                Component::Transition(c) => {
                    Some(c.fore_diameter.as_meters().max(c.aft_diameter.as_meters()))
                }
                _ => None,
            })
            .fold(0.0, f64::max),
        omitted: Vec::new(),
    };
    for c in rocket.root.iter() {
        let x = c.position().as_meters();
        match c {
            Component::NoseCone(c) => {
                let points = (0..=64)
                    .map(|i| {
                        let t = f64::from(i) / 64.0;
                        [
                            x + t * c.length.as_meters(),
                            c.shape.profile(t) * c.base_diameter.as_meters() / 2.0,
                        ]
                    })
                    .collect();
                g.profiles.push(Profile { points });
            }
            Component::BodyTube(c) => g.profiles.push(Profile {
                points: vec![
                    [x, c.outer_diameter.as_meters() / 2.0],
                    [x + c.length.as_meters(), c.outer_diameter.as_meters() / 2.0],
                ],
            }),
            Component::Transition(c) => g.profiles.push(Profile {
                points: vec![
                    [x, c.fore_diameter.as_meters() / 2.0],
                    [x + c.length.as_meters(), c.aft_diameter.as_meters() / 2.0],
                ],
            }),
            Component::FinSet(c) => {
                let Some(Component::BodyTube(body)) = rocket.root.find(c.mount) else {
                    return Err(ValidationError("Missing fin mount".into()));
                };
                let r = body.outer_diameter.as_meters() / 2.0;
                let overlap = (c.thickness.as_meters() * 0.1).min(r * 0.05);
                let polygon = match &c.planform {
                    FinPlanform::Trapezoidal {
                        root_chord,
                        tip_chord,
                        span,
                        sweep,
                    } => vec![
                        [x, r - overlap],
                        [x + sweep.as_meters(), r + span.as_meters()],
                        [
                            x + sweep.as_meters() + tip_chord.as_meters(),
                            r + span.as_meters(),
                        ],
                        [x + root_chord.as_meters(), r - overlap],
                    ],
                    FinPlanform::Elliptical { root_chord, span } => (0..=32)
                        .map(|i| {
                            let theta = f64::from(i) * std::f64::consts::PI / 32.0;
                            [
                                x + 0.5 * root_chord.as_meters() * (1.0 - theta.cos()),
                                r + span.as_meters() * theta.sin() - overlap,
                            ]
                        })
                        .collect(),
                    FinPlanform::Custom { points } => points
                        .iter()
                        .map(|(a, b)| [x + a * 0.001, r + b * 0.001 - overlap])
                        .collect(),
                };
                g.fins.push(Fin {
                    polygon,
                    thickness: c.thickness.as_meters(),
                    count: c.count,
                });
            }
            Component::LaunchLug(c) if settings.include_launch_lugs => g.lugs.push(Lug {
                x,
                length: c.length.as_meters(),
                inner: c.inner_diameter.as_meters() / 2.0,
                outer: c.outer_diameter.as_meters() / 2.0,
                offset: g.diameter / 2.0 + c.outer_diameter.as_meters() / 2.0 - 0.00005,
            }),
            Component::Pod(c) => g.pods.push(Pod {
                x,
                length: c.length.as_meters(),
                radius: c.outer_diameter.as_meters() / 2.0,
                offset: c.radial_offset.as_meters(),
                angle: c.angle,
            }),
            Component::Coupler(_) | Component::MassObject(_) => g.omitted.push(format!(
                "Internal component {} excluded from external-flow geometry",
                c.id()
            )),
            Component::LaunchLug(c) => g.omitted.push(format!(
                "Launch lug {} excluded by explicit study setting",
                c.id
            )),
        }
    }
    g.length = g
        .profiles
        .iter()
        .flat_map(|p| p.points.iter().map(|p| p[0]))
        .chain(g.fins.iter().flat_map(|f| f.polygon.iter().map(|p| p[0])))
        .chain(g.lugs.iter().map(|l| l.x + l.length))
        .chain(g.pods.iter().map(|p| p.x + p.length))
        .fold(0.0, f64::max);
    if g.length <= 0.0
        || g.profiles.is_empty()
        || g.length / settings.mesh_size_m > 1000.0
        || g.length / settings.mesh_size_m < 5.0
    {
        return Err(ValidationError(
            "CFD needs an airframe and a mesh size between vehicle length/1000 and length/5".into(),
        ));
    }
    Ok(g)
}
/// Export a fresh, self-contained CFD job directory. Existing directories are never overwritten.
/// # Errors
/// Reports invalid geometry/settings or file creation failures.
pub fn prepare_case(
    rocket: &Rocket,
    settings: &CfdSettings,
    directory: impl AsRef<Path>,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    settings.validate()?;
    let geometry = geometry(rocket, settings)?;
    let directory = directory.as_ref();
    std::fs::create_dir(directory)?;
    let input = Input {
        version: 1,
        rocket: rocket.clone(),
        vehicle_sha256: vehicle_fingerprint(rocket),
        settings: settings.clone(),
        geometry,
        driver_sha256: format!("{:x}", Sha256::digest(DRIVER.as_bytes())),
    };
    std::fs::write(
        directory.join("input.json"),
        serde_json::to_vec_pretty(&input)?,
    )?;
    std::fs::write(directory.join("driver.py"), DRIVER)?;
    Ok(directory.to_path_buf())
}
/// Execute only the bundled, checksum-verified driver; all arguments are passed without shell interpolation.
/// The driver sources a user-selected installed `OpenFOAM` environment and owns its solver children.
/// # Errors
/// Reports missing tools, cancelled/failed jobs, and malformed solver outputs.
pub fn run_case(
    directory: &Path,
    python: &Path,
    foam_bashrc: &Path,
    cancelled: &AtomicBool,
) -> Result<CfdOutcome, Box<dyn std::error::Error + Send + Sync>> {
    let directory = directory.canonicalize()?;
    if std::fs::read(directory.join("driver.py"))? != DRIVER.as_bytes() {
        return Err(
            io::Error::other("CFD driver differs from this release; export a fresh study").into(),
        );
    }
    let input: Input = serde_json::from_slice(&std::fs::read(directory.join("input.json"))?)?;
    input.settings.validate()?;
    if input.version != 1
        || input.driver_sha256 != format!("{:x}", Sha256::digest(DRIVER.as_bytes()))
        || input.vehicle_sha256 != vehicle_fingerprint(&input.rocket)
        || input.geometry != geometry(&input.rocket, &input.settings)?
    {
        return Err(io::Error::other(
            "CFD input binding or generated geometry changed; export a fresh study",
        )
        .into());
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err(io::Error::other("CFD cancelled before startup").into());
    }
    let log = std::fs::File::create(directory.join("driver.log"))?;
    let mut child = Command::new(python)
        .arg(directory.join("driver.py"))
        .arg("--case")
        .arg(&directory)
        .arg("--foam-bashrc")
        .arg(foam_bashrc)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            std::fs::write(directory.join("cancel.flag"), b"cancel")?;
        }
        if let Some(status) = child.try_wait()? {
            if cancelled.load(Ordering::Relaxed) {
                return Err(io::Error::other("CFD cancelled; partial artifacts retained").into());
            }
            if !status.success() {
                return Err(io::Error::other(format!(
                    "CFD failed ({status}); inspect {}",
                    directory.join("driver.log").display()
                ))
                .into());
            }
            let result: CfdOutcome =
                serde_json::from_str(&std::fs::read_to_string(directory.join("result.json"))?)?;
            result.validate(&input)?;
            return Ok(result);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl CfdOutcome {
    fn validate(&self, input: &Input) -> Result<(), ValidationError> {
        let row = &self.coefficients;
        if self.version != 1
            || self.vehicle_sha256 != input.vehicle_sha256
            || self.settings != input.settings
            || self.mesh_cells == 0
            || self.iterations == 0
            || self.iterations > self.settings.iterations
            || self.solver_version.is_empty()
            || self.gmsh_version.is_empty()
            || !self.reynolds.is_finite()
            || self.reynolds <= 0.0
            || !self.coefficient_relative_change.is_finite()
            || self.coefficient_relative_change < 0.0
            || [row.ca, row.cy, row.cn, row.cl, row.cm, row.cn_yaw]
                .iter()
                .any(|v| !v.is_finite())
            || (row.mach - self.settings.mach).abs() > 1e-12
            || (row.alpha_rad - self.settings.alpha_deg.to_radians()).abs() > 1e-12
            || (row.beta_rad - self.settings.beta_deg.to_radians()).abs() > 1e-12
            || self.converged != self.dataset.is_some()
        {
            return Err(ValidationError(
                "CFD outcome does not match its study or contains invalid data".into(),
            ));
        }
        if let Some(data) = &self.dataset {
            data.validate()?;
            let m = &data.metadata;
            if m.vehicle_sha256 != input.vehicle_sha256
                || data.rows != [row.clone()]
                || m.reference_area_m2.to_bits() != self.settings.reference_area_m2.to_bits()
                || m.reference_length_m.to_bits() != self.settings.reference_length_m.to_bits()
                || m.moment_reference_station_m.to_bits()
                    != self.settings.moment_reference_station_m.to_bits()
                || self.reynolds < m.reynolds_min
                || self.reynolds > m.reynolds_max
            {
                return Err(ValidationError(
                    "CFD dataset does not match its study references".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Rocket {
        let project: serde_json::Value =
            serde_json::from_str(include_str!("../../../examples/aether-one-project.rocket"))
                .expect("valid test fixture");
        serde_json::from_value(project["rocket"].clone()).expect("valid test fixture")
    }
    #[test]
    fn export_is_bound_and_never_overwrites() {
        let root = tempfile::tempdir().expect("valid test fixture");
        let path = root.path().join("study");
        let rocket = fixture();
        let settings = CfdSettings::for_vehicle(&rocket);
        prepare_case(&rocket, &settings, &path).expect("valid test fixture");
        let input: Input = serde_json::from_slice(
            &std::fs::read(path.join("input.json")).expect("valid test fixture"),
        )
        .expect("valid test fixture");
        assert_eq!(input.vehicle_sha256, vehicle_fingerprint(&rocket));
        assert_eq!(
            input.geometry,
            geometry(&rocket, &settings).expect("valid test fixture")
        );
        assert_eq!(input.geometry.lugs.len(), 0);
        assert!(prepare_case(&rocket, &settings, &path).is_err());
        let mut invalid = settings;
        invalid.mach = 1.1;
        assert!(prepare_case(&rocket, &invalid, root.path().join("invalid")).is_err());
        assert!(!root.path().join("invalid").exists());
    }
    #[test]
    fn altered_input_is_rejected_before_spawning_tools() {
        let root = tempfile::tempdir().expect("valid test fixture");
        let path = root.path().join("study");
        let rocket = fixture();
        prepare_case(&rocket, &CfdSettings::for_vehicle(&rocket), &path)
            .expect("valid test fixture");
        let mut input: Input = serde_json::from_slice(
            &std::fs::read(path.join("input.json")).expect("valid test fixture"),
        )
        .expect("valid test fixture");
        input.geometry.length *= 2.0;
        std::fs::write(
            path.join("input.json"),
            serde_json::to_vec(&input).expect("valid test fixture"),
        )
        .expect("valid test fixture");
        let error = run_case(
            &path,
            Path::new("/missing-python"),
            Path::new("/missing-foam"),
            &AtomicBool::new(false),
        )
        .expect_err("modified geometry must be rejected");
        assert!(error.to_string().contains("binding or generated geometry"));
        assert!(!path.join("driver.log").exists());
    }
}
