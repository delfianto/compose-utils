use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod core;

use crate::commands::{deps, manage};
use crate::core::get_context;

#[derive(Parser)]
#[command(name = "compose-utils")]
#[command(about = "Utilities for managing Docker Compose services with Systemd", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage docker-compose services (systemctl wrapper)
    Manage(manage::ManageArgs),
    /// Manage service dependencies
    Deps(deps::DepsArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let ctx = get_context()?;

    match cli.command {
        Commands::Manage(args) => manage::run(&ctx, args),
        Commands::Deps(args) => deps::run(&ctx, args),
    }
}
