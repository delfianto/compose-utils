//! Logic for pulling Docker Compose images.

use crate::commands::compose_direct::build_compose_command;
use crate::core::{should_use_infisical, Context};
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::{Context as _, Result};

pub fn run_pull(ctx: &Context, services: &[String]) -> Result<()> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;
    let infisical_available = should_use_infisical(ctx);

    for service in services {
        let bare = get_bare_name(&service);

        println!(">> Pulling images for '{}'...", bare);

        let mut cmd = build_compose_command(ctx, bare, &["pull"], infisical_available);
        let status = cmd.status().with_context(|| {
            format!(
                "Failed to execute docker compose pull in {:?}",
                get_compose_dir(ctx, bare)
            )
        })?;

        if !status.success() {
            eprintln!("Warning: Failed to pull images for {}", bare);
        }
    }
    Ok(())
}
