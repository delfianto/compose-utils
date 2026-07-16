//! Internal commands called by systemd unit templates.

use crate::commands::compose_direct::build_compose_command;
use crate::core::Context;
use anyhow::Result;
use std::os::unix::process::CommandExt;

pub fn run_service(ctx: &Context, name: &str) -> Result<()> {
    let mut cmd = build_compose_command(ctx, name, &["up"]);
    let err = cmd.exec();
    Err(anyhow::anyhow!("Failed to exec compose command: {}", err))
}

pub fn stop_service(ctx: &Context, service: &str) -> Result<()> {
    let mut cmd = build_compose_command(ctx, service, &["down", "--remove-orphans"]);
    let err = cmd.exec();
    Err(anyhow::anyhow!("Failed to exec compose command: {}", err))
}
