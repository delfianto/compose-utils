use crate::core::Context;
use crate::systemd::service::get_compose_dir;
use anyhow::Result;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn run_service(ctx: &Context, name: &str) -> Result<()> {
    let project_dir = get_compose_dir(ctx, name);

    let err = Command::new("docker")
        .arg("compose")
        .arg("up")
        .arg("-d")
        .current_dir(project_dir)
        .exec();

    Err(anyhow::anyhow!("Failed to exec docker compose: {}", err))
}

pub fn stop_service(ctx: &Context, service: &str) -> Result<()> {
    let dir = get_compose_dir(ctx, service);
    
    let err = Command::new("docker")
        .args(["compose", "down", "--remove-orphans"])
        .current_dir(&dir)
        .exec();

    Err(anyhow::anyhow!("Failed to execute docker compose: {}", err))
}
