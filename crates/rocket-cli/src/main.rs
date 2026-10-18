//! # aether
//!
//! The Aether command-line interface. Provides headless access to the
//! simulation engine, motor management, and file conversion — for batch
//! workflows, CI pipelines, and scripting.
//!
//! Phase-0 Task 1: minimal skeleton with `--version` and a `status`
//! subcommand. Phase-0 Task 4 will add `motors search` and
//! `motors info` subcommands.

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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => {
            println!("aether {}", env!("CARGO_PKG_VERSION"));
            println!("Status: Phase 0 — scaffolding and de-risking");
            println!("UI:     not yet implemented (Phase-0 Task 2)");
            println!("Motors: not yet implemented (Phase-0 Task 4)");
        }
        None => {
            println!("aether {}", env!("CARGO_PKG_VERSION"));
            println!();
            println!("Use --help to see available commands.");
        }
    }
}
