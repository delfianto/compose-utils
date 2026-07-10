//! Internal commands called by systemd unit templates.

use crate::commands::compose_direct::build_compose_command;
use crate::core::{should_use_infisical, Context};
use crate::verbose;
use anyhow::Result;
use std::os::unix::process::CommandExt;

pub fn run_service(ctx: &Context, name: &str) -> Result<()> {
    let infisical_available = should_use_infisical(ctx);

    if ctx.infisical_project_id.is_some() && !infisical_available {
        verbose!("Infisical configured but not available, using plain docker compose");
    }

    let mut cmd = build_compose_command(ctx, name, &["up"], infisical_available);
    let err = cmd.exec();
    Err(anyhow::anyhow!("Failed to exec compose command: {}", err))
}

pub fn stop_service(ctx: &Context, service: &str) -> Result<()> {
    let infisical_available = should_use_infisical(ctx);

    if ctx.infisical_project_id.is_some() && !infisical_available {
        verbose!("Infisical configured but not available, using plain docker compose");
    }

    let mut cmd =
        build_compose_command(ctx, service, &["down", "--remove-orphans"], infisical_available);
    let err = cmd.exec();
    Err(anyhow::anyhow!("Failed to exec compose command: {}", err))
}
