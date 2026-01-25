//! Main entry point for the `compose` utility.
//!
//! This tool provides a systemd-integrated wrapper for managing Docker Compose projects,
//! supporting both root and rootless Docker environments.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod compose;
mod core;
mod systemd;

use crate::commands::{config, deps};
use crate::core::{enable_verbose, get_context};

/// Command-line interface for managing Docker Compose services with systemd.
#[derive(Parser)]
#[command(name = "compose")]
#[command(about = "Utilities for managing Docker Compose services with Systemd", long_about = None)]
struct Cli {
    /// Enable verbose/debug output.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// The subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Available subcommands for the application.
#[derive(Subcommand)]
enum Commands {
    /// Start services (systemctl start).
    #[command(visible_alias = "up")]
    Start {
        /// List of service names to start.
        services: Vec<String>,

        /// Path to dependency configuration file.
        #[arg(long, help = "Path to dependency configuration file")]
        deps: Option<String>,
    },
    /// Stop services (systemctl stop).
    #[command(visible_alias = "down")]
    Stop {
        /// List of service names to stop.
        services: Vec<String>,
    },
    /// Restart services (systemctl restart).
    #[command(visible_alias = "reup")]
    Restart {
        /// List of service names to restart.
        services: Vec<String>,
    },
    /// Pull new images and restart services if updated.
    Update {
        /// List of service names to update.
        services: Vec<String>,
    },
    /// Pull images for services without restarting.
    Pull {
        /// List of service names to pull images for.
        services: Vec<String>,
    },
    /// Show service status (systemctl status).
    Status {
        /// List of service names to check.
        services: Vec<String>,
    },
    /// Enable services to start on boot (systemctl enable).
    Enable {
        /// List of service names to enable.
        services: Vec<String>,

        /// Path to dependency configuration file.
        #[arg(long, help = "Path to dependency configuration file")]
        deps: Option<String>,
    },
    /// Disable services from starting on boot (systemctl disable).
    Disable {
        /// List of service names to disable.
        services: Vec<String>,
    },
    /// List all managed Docker Compose services.
    #[command(visible_alias = "ls")]
    List,
    /// List Docker containers and their statuses.
    Ps {
        /// List of service names to filter by (optional).
        services: Vec<String>,
    },
    /// View service logs via journalctl.
    Logs {
        /// Name of the service to show logs for.
        service: Option<String>,
        /// Follow log output in real-time.
        #[arg(short, long)]
        follow: bool,
        /// Number of recent log lines to display.
        #[arg(short = 'n', long)]
        lines: Option<usize>,
    },
    /// Manage service dependencies.
    Deps(deps::DepsArgs),
    /// View or update global configuration.
    Config(config::ConfigArgs),
    
    // Internal Commands (used by systemd)
    #[command(hide = true)]
    RunService {
        service: String,
    },
    #[command(hide = true)]
    StopService {
        service: String,
    },
}

/// Entry point of the application.
///
/// Parses command-line arguments, initializes the execution context,
/// and dispatches the command to the appropriate handler.
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        enable_verbose();
    }

    let ctx = get_context()?;

    match cli.command {
        Commands::Start { services, deps } => commands::run_start(&ctx, &services, deps).await,
        Commands::Stop { services } => commands::run_stop(&ctx, &services).await,
        Commands::Restart { services } => commands::run_restart(&ctx, &services).await,
        Commands::Update { services } => commands::run_update(&ctx, &services).await,
        Commands::Pull { services } => commands::run_pull(&ctx, &services).await,
        Commands::Status { services } => commands::run_status(&ctx, &services).await,
        Commands::Enable { services, deps } => commands::run_enable(&ctx, &services, deps).await,
        Commands::Disable { services } => commands::run_disable(&ctx, &services).await,
        Commands::List => commands::run_list(&ctx).await,
        Commands::Ps { services } => commands::ps::run_ps(&ctx, &services).await,
        Commands::Logs {
            service,
            follow,
            lines,
        } => commands::run_logs(&ctx, service.as_deref().unwrap_or(""), follow, lines).await,
        Commands::Deps(args) => deps::run(&ctx, args).await,
        Commands::Config(args) => config::run(&ctx, args),
        
        Commands::RunService { service } => commands::internal::run_service(&ctx, &service),
        Commands::StopService { service } => commands::internal::stop_service(&ctx, &service),
    }
}