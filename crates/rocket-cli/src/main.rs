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
        Some(Commands::Status) => {
            println!("aether {}", env!("CARGO_PKG_VERSION"));
            println!("Status: Phase 0 — scaffolding and de-risking");
            println!("UI:     implemented (egui + wgpu spike, Phase-0 Task 2)");
            println!("Motors: implemented (ThrustCurve API + cache, Phase-0 Task 4)");
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
