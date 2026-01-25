//! Logic for the `system` command group.
//! Handles system information, installation, and uninstallation.

use crate::setup;
use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum SystemCommands {
    /// Show system information in JSON format.
    Info(InfoArgs),
    /// Install the application and systemd services.
    Install(InstallArgs),
    /// Uninstall the application and remove services.
    Uninstall(UninstallArgs),
    /// Reinstall the application (requires existing configuration).
    Reinstall(InstallArgs),
}

#[derive(Args)]
pub struct InfoArgs {
    /// Output format (currently only 'json' is supported).
    #[arg(long, default_value = "json")]
    format: String,
}

#[derive(Args)]
pub struct InstallArgs {
    /// Set COMPOSE_DATA directory path.
    #[arg(long)]
    compose_data: Option<PathBuf>,

    /// Set COMPOSE_BASE directory path.
    #[arg(long)]
    compose_base: Option<PathBuf>,

    /// Set ACME domain for Traefik.
    #[arg(long)]
    acme_domain: Option<String>,

    /// Set ACME email for Traefik.
    #[arg(long)]
    acme_email: Option<String>,

    /// Set ACME server URL for Traefik.
    #[arg(long)]
    acme_server: Option<String>,

    /// Set DOCKER_HOST.
    #[arg(long)]
    docker_host: Option<String>,
}

#[derive(Args)]
pub struct UninstallArgs {}

pub fn run_system(command: SystemCommands) -> Result<()> {
    match command {
        SystemCommands::Info(_) => run_info(),
        SystemCommands::Install(args) => {
            let opts = setup::InstallOptions {
                compose_data: args.compose_data,
                compose_base: args.compose_base,
                acme_domain: args.acme_domain,
                acme_email: args.acme_email,
                acme_server: args.acme_server,
                docker_host: args.docker_host,
            };
            setup::run_install(opts)
        }
        SystemCommands::Uninstall(_) => setup::run_uninstall(),
        SystemCommands::Reinstall(args) => {
            let opts = setup::InstallOptions {
                compose_data: args.compose_data,
                compose_base: args.compose_base,
                acme_domain: args.acme_domain,
                acme_email: args.acme_email,
                acme_server: args.acme_server,
                docker_host: args.docker_host,
            };
            setup::run_reinstall(opts)
        }
    }
}

fn run_info() -> Result<()> {
    let info = setup::detect_system_info();
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}
