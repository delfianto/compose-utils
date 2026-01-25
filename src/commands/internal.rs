use crate::core::Context;
use crate::systemd::service::get_compose_dir;
use anyhow::Result;
use std::process::Command;
use std::os::unix::process::CommandExt;

pub fn run_service(ctx: &Context, service: &str) -> Result<()> {
    let dir = get_compose_dir(ctx, service);
    
    // We replace the current process with docker compose
    // This ensures signals are handled correctly and we don't keep an extra process
    let err = Command::new("docker")
        .args(["compose", "up", "--wait", "--remove-orphans"])
        .current_dir(&dir)
        .exec();

    // exec only returns on error
    Err(anyhow::anyhow!("Failed to execute docker compose: {}", err))
}

pub fn stop_service(ctx: &Context, service: &str) -> Result<()> {
    let dir = get_compose_dir(ctx, service);
    
    let err = Command::new("docker")
        .args(["compose", "down", "--remove-orphans"])
        .current_dir(&dir)
        .exec();

    Err(anyhow::anyhow!("Failed to execute docker compose: {}", err))
}
