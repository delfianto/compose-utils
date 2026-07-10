//! Logic for pulling Docker Compose images.

use crate::commands::compose_direct::build_compose_command;
use crate::core::{Report, should_use_infisical, Context};
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::{Context as _, Result};
use serde::Serialize;

/// Per-service result of a `docker compose pull`.
#[derive(Serialize)]
pub struct PullResult {
    pub service: String,
    pub status: String,
}

/// Pulls images for the given services, returning a per-service result.
///
/// A failed pull is reported as `"failed"` rather than aborting the whole
/// batch, matching the existing "continue on failure" behavior.
pub fn pull_images(ctx: &Context, services: &[String]) -> Result<Vec<PullResult>> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;
    let infisical_available = should_use_infisical(ctx);
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for service in services {
        let bare = get_bare_name(&service);

        if !json {
            println!(">> Pulling images for '{}'...", bare);
        }

        let mut cmd = build_compose_command(ctx, bare, &["pull"], infisical_available);
        let status = cmd.status().with_context(|| {
            format!(
                "Failed to execute docker compose pull in {:?}",
                get_compose_dir(ctx, bare)
            )
        })?;

        let ok = status.success();
        if !ok {
            eprintln!("Warning: Failed to pull images for {}", bare);
        }

        results.push(PullResult {
            service: bare.to_string(),
            status: if ok { "ok" } else { "failed" }.to_string(),
        });
    }

    Ok(results)
}

pub fn run_pull(ctx: &Context, services: &[String]) -> Result<()> {
    let results = pull_images(ctx, services)?;

    if crate::core::is_json() {
        crate::core::print_json(&Report { command: "pull", results })?;
    }

    Ok(())
}
