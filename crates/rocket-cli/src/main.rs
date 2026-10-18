//! # aether
//!
//! The Aether command-line interface. Provides headless access to the
//! simulation engine, motor management, and file conversion — for batch
//! workflows, CI pipelines, and scripting.
//!
//! Phase-0 Task 4: `motors search` and `motors info` subcommands.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aether",
    version,
    about = "A professional rocket design and 6-DOF flight simulator",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the saved flight case and write a self-contained run record.
    Simulate {
        project: std::path::PathBuf,
        #[arg(long)]
        output: std::path::PathBuf,
        #[arg(long)]
        csv: Option<std::path::PathBuf>,
        #[arg(long)]
        aerodynamic_dataset: Option<std::path::PathBuf>,
        #[arg(long)]
        outside_domain_fallback: bool,
    },
    /// Export a fresh reproducible `OpenFOAM` study directory.
    CfdPrepare {
        project: std::path::PathBuf,
        directory: std::path::PathBuf,
        #[arg(long, default_value_t = 0.2)]
        mach: f64,
        #[arg(long, default_value_t = 0.0)]
        alpha: f64,
        #[arg(long)]
        mesh_size: Option<f64>,
        #[arg(long, default_value_t = 600)]
        iterations: u32,
    },
    /// Execute an exported CFD study using installed Gmsh/OpenFOAM tools.
    CfdRun {
        directory: std::path::PathBuf,
        #[arg(long, default_value = "python3")]
        python: std::path::PathBuf,
        #[arg(long)]
        foam_bashrc: std::path::PathBuf,
    },
    /// Show the current status of the Aether installation.
    Status,

    /// Search and manage rocket motors (via ThrustCurve.org).
    Motors {
        #[command(subcommand)]
        action: MotorsAction,
    },
}

#[derive(Subcommand)]
enum MotorsAction {
    /// Search for motors on ThrustCurve.org.
    Search {
        /// Common name to search for (e.g., "A8", "H128").
        name: String,

        /// Show only motors with this impulse class (e.g., "H", "J").
        #[arg(long)]
        class: Option<String>,

        /// Show only motors with this diameter (mm).
        #[arg(long)]
        diameter: Option<f64>,
    },

    /// Download and show details for a specific motor.
    Info {
        /// The 24-character ThrustCurve.org motor ID.
        motor_id: String,

        /// Force re-download even if cached.
        #[arg(long)]
        refresh: bool,

        /// Show the full thrust curve data points.
        #[arg(long)]
        full: bool,
    },

    /// List all motors in the local cache.
    List,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(
            command @ (Commands::Simulate { .. }
            | Commands::CfdPrepare { .. }
            | Commands::CfdRun { .. }),
        ) => {
            if let Err(error) = run_analysis(command) {
                eprintln!("Error: {error}");
                std::process::exit(1);
            }
        }
        Some(Commands::Status) => {
            println!("aether {}", env!("CARGO_PKG_VERSION"));
            println!("Status: Experimental engineering workbench");
            println!("UI:     Design, Flight case, Results, Aerodynamics");
            println!("Analysis: simulate, cfd-prepare, cfd-run; see --help");
        }
        Some(Commands::Motors { action }) => {
            if let Err(e) = run_motors_command(action).await {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        None => {
            println!("aether {}", env!("CARGO_PKG_VERSION"));
            println!();
            println!("Use --help to see available commands.");
        }
    }
}

fn run_analysis(command: Commands) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match command {
        Commands::Simulate {
            project,
            output,
            csv,
            aerodynamic_dataset,
            outside_domain_fallback,
        } => {
            let mut project = rocket_io::load_project_from_path(project)?;
            if let Some(path) = aerodynamic_dataset {
                project.case.aerodynamics = rocket_aero::AeroModel::Dataset {
                    data: std::sync::Arc::new(rocket_io::load_aero_dataset(path)?),
                    outside_domain: if outside_domain_fallback {
                        rocket_aero::DomainPolicy::BuiltinFallback
                    } else {
                        rocket_aero::DomainPolicy::Stop
                    },
                };
            }
            let result = project.case.run(&project.rocket)?;
            let record = rocket_sim::RunRecord::capture(project.rocket, project.case, result, 0);
            rocket_io::save_run_to_path(&record, output)?;
            if let Some(path) = csv {
                rocket_io::save_telemetry_csv(&record, path)?;
            }
            println!(
                "Run {}: {:?}; apogee {:.3} m AGL; {} samples",
                record.id,
                record.result.termination,
                record.result.max_altitude.as_meters(),
                record.result.telemetry.len()
            );
            if !matches!(
                record.result.termination,
                rocket_sim::Termination::GroundImpact | rocket_sim::Termination::DurationLimit
            ) {
                return Err(std::io::Error::other(
                    "Propagation stopped early; retained record contains diagnostics",
                )
                .into());
            }
        }
        Commands::CfdPrepare {
            project,
            directory,
            mach,
            alpha,
            mesh_size,
            iterations,
        } => {
            let project = rocket_io::load_project_from_path(project)?;
            let mut settings = rocket_cfd::CfdSettings::for_vehicle(&project.rocket);
            settings.mach = mach;
            settings.alpha_deg = alpha;
            settings.iterations = iterations;
            if let Some(size) = mesh_size {
                settings.mesh_size_m = size;
            }
            let path = rocket_cfd::prepare_case(&project.rocket, &settings, directory)?;
            println!("Exported {}", path.display());
        }
        Commands::CfdRun {
            directory,
            python,
            foam_bashrc,
        } => {
            let result = rocket_cfd::run_case(
                &directory,
                &python,
                &foam_bashrc,
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            println!(
                "{} cells; {} iterations; converged: {}; CA: {:.6}; CN: {:.6}; Cm: {:.6}",
                result.mesh_cells,
                result.iterations,
                result.converged,
                result.coefficients.ca,
                result.coefficients.cn,
                result.coefficients.cm
            );
            if !result.converged {
                return Err(std::io::Error::other("CFD did not meet convergence criteria; raw outputs retained, no activation dataset created").into());
            }
        }
        _ => unreachable!("analysis command"),
    }
    Ok(())
}

async fn run_motors_command(action: MotorsAction) -> Result<(), Box<dyn std::error::Error>> {
    use rocket_motors::{Cache, SearchQuery, ThrustCurveClient};

    match action {
        MotorsAction::Search {
            name,
            class,
            diameter,
        } => {
            let client = ThrustCurveClient::new();
            let mut query = SearchQuery::default().common_name(&name);
            if let Some(c) = class {
                query = query.impulse_class(c);
            }
            if let Some(d) = diameter {
                query = query.diameter(d);
            }

            println!("Searching ThrustCurve.org for '{name}'…");
            let results = client.search(&query).await?;

            if results.is_empty() {
                println!("No motors found.");
                return Ok(());
            }

            println!("Found {} motor(s):", results.len());
            println!();
            println!(
                "{:<24} {:<6} {:<5} {:<6} {:<8} {:<5} {:<7} {:<10}",
                "MOTOR_ID", "CLASS", "DIA", "IMPULS", "AVG_THR", "BURN", "AVAIL", "DESIGNATION"
            );
            println!("{}", "-".repeat(80));

            for m in &results {
                println!(
                    "{:<24} {:<6} {:<5.0} {:<6.1} {:<8.2} {:<5.2} {:<7} {:<10}",
                    m.motor_id,
                    m.impulse_class,
                    m.diameter,
                    m.tot_impulse_ns,
                    m.avg_thrust_n,
                    m.burn_time_s,
                    m.availability,
                    m.designation,
                );
            }
        }

        MotorsAction::Info {
            motor_id,
            refresh,
            full,
        } => {
            // Check cache first (unless --refresh).
            let cache = Cache::new()?;

            if !refresh && cache.contains(&motor_id) {
                println!("(from cache)");
                let (motor, meta) = cache.get(&motor_id)?;
                print_motor_info(&motor, &meta, full);
                return Ok(());
            }

            // Fetch from ThrustCurve.org.
            println!("Downloading from ThrustCurve.org…");
            let client = ThrustCurveClient::new();
            let (motor, meta) = client.fetch_motor(&motor_id).await?;

            // Cache it.
            cache.store(&motor, &meta)?;

            print_motor_info(&motor, &meta, full);
        }

        MotorsAction::List => {
            let cache = Cache::new()?;
            let entries = cache.list()?;

            if entries.is_empty() {
                println!("Cache is empty. Use 'aether motors search <name>' to find motors.");
                return Ok(());
            }

            println!("Cached motors ({}):", entries.len());
            println!();
            for m in entries {
                println!(
                    "{:<24} {:<10} {:<10} {}",
                    m.motor_id, m.designation, m.manufacturer, m.source
                );
            }
        }
    }

    Ok(())
}

fn print_motor_info(motor: &rocket_motors::Motor, meta: &rocket_motors::MotorMetadata, full: bool) {
    println!();
    println!("Designation:    {}", motor.designation);
    println!("Manufacturer:   {}", motor.manufacturer);
    println!("Motor ID:       {}", meta.motor_id);
    println!("Source:         {}", meta.source);
    println!("License:        {}", meta.license);
    println!("Downloaded:     {}", meta.downloaded_at);
    println!();
    println!("Diameter:       {:.1} mm", motor.diameter.as_millimeters());
    println!("Length:         {:.1} mm", motor.length.as_millimeters());
    println!("Propellant:     {:.2} g", motor.propellant_mass.as_grams());
    println!("Total mass:     {:.2} g", motor.total_mass.as_grams());

    let delays_str = motor
        .delays
        .iter()
        .map(|d| {
            if *d < 0.0 {
                "P".to_string()
            } else {
                format!("{d:.0}")
            }
        })
        .collect::<Vec<_>>()
        .join("-");
    println!("Delays:         {delays_str}");

    println!();
    println!("Total impulse:  {:.2} N·s", motor.total_impulse());
    println!("Burn time:      {:.3} s", motor.burn_time());
    println!("Average thrust: {:.2} N", motor.average_thrust());
    println!("Peak thrust:    {:.2} N", motor.max_thrust());
    println!("Specific imp.:  {:.1} s", motor.specific_impulse());

    if full {
        println!();
        println!("Thrust curve ({} points):", motor.thrust_curve.len());
        println!("  {:>8}  {:>8}", "time (s)", "thrust (N)");
        println!("  {}", "-".repeat(20));
        for (t, f) in &motor.thrust_curve {
            println!("  {:>8.3}  {:>8.2}", t.as_seconds(), f.as_newtons());
        }
    }
}

/// Helper trait for formatting units (placeholder; will be expanded).
mod units_fmt {
    // This module is intentionally empty; it's a placeholder for future
    // unit-aware formatting helpers.
}
