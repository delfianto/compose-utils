//! Direct Docker Compose operations (no systemd indirection).

use crate::core::Context;
use crate::systemd::discovery::resolve_services;
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::{Context as _, Result};
use std::process::Command;

/// Run `docker compose up -d` directly in the project directory.
pub fn compose_up(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        let dir = get_compose_dir(ctx, bare);

        println!("Starting {}...", bare);

        let mut cmd = Command::new("docker");
        cmd.args(["compose", "up", "-d"]).current_dir(&dir);

        if let Some(ref host) = ctx.docker_host {
            cmd.env("DOCKER_HOST", host);
        }

        let status = cmd
            .status()
            .with_context(|| format!("Failed to run docker compose up in {}", dir.display()))?;

        if !status.success() {
            anyhow::bail!("docker compose up failed for {}", bare);
        }

        println!("Started {}", bare);
    }

    Ok(())
}

/// Run `docker compose down --remove-orphans` directly in the project directory.
pub fn compose_down(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        let dir = get_compose_dir(ctx, bare);

        println!("Stopping {}...", bare);

        let mut cmd = Command::new("docker");
        cmd.args(["compose", "down", "--remove-orphans"])
            .current_dir(&dir);

        if let Some(ref host) = ctx.docker_host {
            cmd.env("DOCKER_HOST", host);
        }

        let status = cmd
            .status()
            .with_context(|| format!("Failed to run docker compose down in {}", dir.display()))?;

        if !status.success() {
            anyhow::bail!("docker compose down failed for {}", bare);
        }

        println!("Stopped {}", bare);
    }

    Ok(())
}

/// Run `docker compose down` then `docker compose up -d`.
pub fn compose_restart(ctx: &Context, names: &[String]) -> Result<()> {
    compose_down(ctx, names)?;
    compose_up(ctx, names)
}
