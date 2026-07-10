//! Main entry point for the `compose` / `composectl` multi-call binary.
//!
//! Behavior is determined by argv[0]:
//! - `compose`    — Direct Docker Compose project operations
//! - `composectl` — Systemd service controller for Docker Compose projects

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod commands;
mod compose;
mod core;
mod systemd;

use crate::commands::{config, deps, secret};
use crate::core::{enable_json, enable_verbose, get_context};

// ---------------------------------------------------------------------------
// compose persona — direct Docker Compose operations
// ---------------------------------------------------------------------------

/// Direct Docker Compose project utilities.
#[derive(Parser)]
#[command(name = "compose")]
#[command(about = "Docker Compose project utilities", long_about = None)]
struct ComposeCli {
    /// Enable verbose/debug output.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output results as JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: ComposeCommands,
}

#[derive(Subcommand)]
enum ComposeCommands {
    /// Start containers (docker compose up -d).
    Up {
        /// List of service names to start.
        services: Vec<String>,
    },
    /// Stop containers (docker compose down).
    Down {
        /// List of service names to stop.
        services: Vec<String>,
    },
    /// Restart containers (docker compose down + up).
    #[command(visible_alias = "reup")]
    Restart {
        /// List of service names to restart.
        services: Vec<String>,
    },
    /// Pull images for services without restarting.
    Pull {
        /// List of service names to pull images for.
        services: Vec<String>,
    },
    /// List Docker containers and their statuses.
    Ps {
        /// List of service names to filter by (optional).
        services: Vec<String>,
    },
    /// Manage Infisical secrets for a service.
    Secret(secret::SecretArgs),
    /// View or update global configuration.
    Config(Box<config::ConfigArgs>),
}

// ---------------------------------------------------------------------------
// composectl persona — systemd service controller
// ---------------------------------------------------------------------------

/// Systemd service controller for Docker Compose projects.
#[derive(Parser)]
#[command(name = "composectl")]
#[command(about = "Systemd service controller for Docker Compose projects", long_about = None)]
struct CtlCli {
    /// Enable verbose/debug output.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output results as JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: CtlCommands,
}

#[derive(Subcommand)]
enum CtlCommands {
    /// Start services (systemctl start).
    Start {
        /// List of service names to start.
        services: Vec<String>,

        /// Path to dependency configuration file.
        #[arg(long, help = "Path to dependency configuration file")]
        deps: Option<String>,
    },
    /// Stop services (systemctl stop).
    Stop {
        /// List of service names to stop.
        services: Vec<String>,
    },
    /// Restart services (systemctl restart).
    Restart {
        /// List of service names to restart.
        services: Vec<String>,
    },
    /// Pull new images and restart services via systemd.
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
    /// Reconcile systemd's tracked state against actual container state.
    Sync {
        /// List of service names to sync.
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
    /// Manage service dependencies.
    Deps(deps::DepsArgs),
    /// Manage Infisical secrets for a service.
    Secret(secret::SecretArgs),
    /// View or update global configuration.
    Config(Box<config::ConfigArgs>),

    /// Run docker compose up (called by systemd unit).
    #[command(hide = true)]
    RunService { service: String },
    /// Run docker compose down (called by systemd unit).
    #[command(hide = true)]
    StopService { service: String },
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    let binary_name = std::env::args()
        .next()
        .and_then(|s| {
            PathBuf::from(s)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "compose".to_string());

    let result = match binary_name.as_str() {
        "composectl" => run_composectl(),
        _ => run_compose(),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if core::is_json() {
                let _ = core::print_json(&serde_json::json!({
                    "status": "error",
                    "error": format!("{:?}", e),
                }));
            } else {
                eprintln!("Error: {:?}", e);
            }
            ExitCode::FAILURE
        }
    }
}

fn run_compose() -> Result<()> {
    let cli = ComposeCli::parse();
    if cli.verbose {
        enable_verbose();
    }
    if cli.json {
        enable_json();
    }
    let ctx = get_context()?;

    match cli.command {
        ComposeCommands::Up { services } => commands::compose_up(&ctx, &services),
        ComposeCommands::Down { services } => commands::compose_down(&ctx, &services),
        ComposeCommands::Restart { services } => commands::compose_restart(&ctx, &services),
        ComposeCommands::Pull { services } => commands::run_pull(&ctx, &services),
        ComposeCommands::Ps { services } => commands::ps::run_ps(&ctx, &services),
        ComposeCommands::Secret(args) => secret::run(&ctx, args),
        ComposeCommands::Config(args) => config::run(&ctx, *args),
    }
}

fn run_composectl() -> Result<()> {
    let cli = CtlCli::parse();
    if cli.verbose {
        enable_verbose();
    }
    if cli.json {
        enable_json();
    }
    let ctx = get_context()?;

    match cli.command {
        CtlCommands::Start { services, deps } => commands::run_start(&ctx, &services, deps),
        CtlCommands::Stop { services } => commands::run_stop(&ctx, &services),
        CtlCommands::Restart { services } => commands::run_restart(&ctx, &services),
        CtlCommands::Update { services } => commands::run_update(&ctx, &services),
        CtlCommands::Pull { services } => commands::run_pull(&ctx, &services),
        CtlCommands::Status { services } => commands::run_status(&ctx, &services),
        CtlCommands::Sync { services } => commands::run_sync(&ctx, &services),
        CtlCommands::Enable { services, deps } => commands::run_enable(&ctx, &services, deps),
        CtlCommands::Disable { services } => commands::run_disable(&ctx, &services),
        CtlCommands::Deps(args) => deps::run(&ctx, args),
        CtlCommands::Secret(args) => secret::run(&ctx, args),
        CtlCommands::Config(args) => config::run(&ctx, *args),

        CtlCommands::RunService { service } => commands::internal::run_service(&ctx, &service),
        CtlCommands::StopService { service } => commands::internal::stop_service(&ctx, &service),
    }
}
