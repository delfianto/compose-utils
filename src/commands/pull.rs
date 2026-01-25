use crate::core::Context;
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::{Context as _, Result};
use std::process::Command;

pub async fn run_pull(ctx: &Context, services: &[String]) -> Result<()> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;

    for service in services {
        let bare = get_bare_name(&service);
        let dir = get_compose_dir(ctx, bare);

        println!(">> Pulling images for '{}'...", bare);

        let status = Command::new("docker")
            .arg("compose")
            .arg("pull")
            .current_dir(&dir)
            .status()
            .with_context(|| format!("Failed to execute docker compose pull in {:?}", dir))?;

        if !status.success() {
            eprintln!("Warning: Failed to pull images for {}", bare);
        }
    }
    Ok(())
}