use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod compose;
mod constants;
mod core;
mod display;
mod docker;
mod systemd;

use crate::commands::{config, deps};
use crate::core::get_context;

#[derive(Parser)]
#[command(name = "compose")]
#[command(about = "Utilities for managing Docker Compose services with Systemd", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start services
    #[command(visible_alias = "up")]
    Start {
        /// Service names to start
        services: Vec<String>,
    },
    /// Stop services
    #[command(visible_alias = "down")]
    Stop {
        /// Service names to stop
        services: Vec<String>,
    },
    /// Restart services
    #[command(visible_alias = "reup")]
    Restart {
        /// Service names to restart
        services: Vec<String>,
    },
    /// Update services (pull images, restart only if changed)
    Update {
        /// Service names to update
        services: Vec<String>,
    },
    /// Pull images for services without restarting
    Pull {
        /// Service names to pull images for
        services: Vec<String>,
    },
    /// Show service status
    Status {
        /// Service names to check
        services: Vec<String>,
    },
    /// Enable services to start on boot
    Enable {
        /// Service names to enable
        services: Vec<String>,
    },
    /// Disable services from starting on boot
    Disable {
        /// Service names to disable
        services: Vec<String>,
    },
    /// List all managed docker-compose services
    #[command(visible_alias = "ls")]
    List,
    /// List containers for services
    Ps {
        /// Service names to check. Optional if in a compose project directory.
        services: Vec<String>,
    },
    /// View service logs (journalctl wrapper)
    Logs {
        /// Service name (e.g., myapp or genai-ollama or genai/ollama). Optional if in a compose project directory.
        service: Option<String>,
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        /// Number of lines to show
        #[arg(short = 'n', long)]
        lines: Option<usize>,
    },
    /// Manage service dependencies
    Deps(deps::DepsArgs),
    /// View or update configuration
    Config(config::ConfigArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let ctx = get_context()?;

    match cli.command {
        Commands::Start { services } => commands::run_start(&ctx, &services),
        Commands::Stop { services } => commands::run_stop(&ctx, &services),
        Commands::Restart { services } => commands::run_restart(&ctx, &services),
        Commands::Update { services } => commands::run_update(&ctx, &services).await,
        Commands::Pull { services } => commands::run_pull(&ctx, &services).await,
        Commands::Status { services } => commands::run_systemctl(&ctx, "status", &services, false),
        Commands::Enable { services } => commands::run_enable(&ctx, &services),
        Commands::Disable { services } => commands::run_disable(&ctx, &services),
        Commands::List => commands::run_list(&ctx),
        Commands::Ps { services } => commands::ps::run_ps(&ctx, &services).await,
        Commands::Logs {
            service,
            follow,
            lines,
        } => commands::run_logs(&ctx, service.as_deref().unwrap_or(""), follow, lines),
        Commands::Deps(args) => deps::run(&ctx, args),
        Commands::Config(args) => config::run(&ctx, args),
    }
}
