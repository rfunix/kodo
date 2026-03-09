//! # ko — The Kōdo Build Tool
//!
//! A build tool for Kōdo projects that reads `project.ko.toml` configuration
//! and orchestrates compilation, testing, and dependency management.
//!
//! ## Current Status
//!
//! Stub implementation — prints help and version information.

use clap::{Parser, Subcommand};

/// The Kōdo build tool.
#[derive(Parser)]
#[command(name = "ko", version, about = "Build tool for the Kōdo language")]
struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Command,
}

/// Available build tool commands.
#[derive(Subcommand)]
enum Command {
    /// Build the current project.
    Build,
    /// Run the current project.
    Run,
    /// Run tests.
    Test,
    /// Check the project without generating code.
    Check,
    /// Initialize a new Kōdo project.
    Init {
        /// Project name.
        name: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Build => {
            println!("ko build: not yet implemented");
        }
        Command::Run => {
            println!("ko run: not yet implemented");
        }
        Command::Test => {
            println!("ko test: not yet implemented");
        }
        Command::Check => {
            println!("ko check: not yet implemented");
        }
        Command::Init { name } => {
            println!("ko init: would create project `{name}` — not yet implemented");
        }
    }
}
