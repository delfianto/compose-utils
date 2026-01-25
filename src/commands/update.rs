use crate::core::Context;
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::Result;
use std::process::Command;

pub async fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;

    for service in services {
        let bare = get_bare_name(&service);
        let dir = get_compose_dir(ctx, bare);

        println!(">> Updating '{}'...", bare);

        // 1. Pull new images
        let pull_status = Command::new("docker")
            .arg("compose")
            .arg("pull")
            .current_dir(&dir)
            .status()?;

        if pull_status.success() {
             // 2. Restart the systemd unit to pick up changes
             // (Systemd will call `docker compose up` which recreates containers if images changed)
             let unit = crate::systemd::service::normalize_unit_name(ctx, bare);
             crate::systemd::manager::restart_unit(ctx, &unit)?;
             println!("Restarted {}", unit);
        }
    }
    Ok(())
}