use crate::commands::pull::pull_images;
use crate::core::{Report, Context};
use crate::systemd::discovery::resolve_services;
use crate::systemd::manager::restart_unit;
use crate::systemd::service::{get_bare_name, normalize_unit_name};
use anyhow::Result;
use serde::Serialize;

/// Per-service result of an `update` (pull + restart).
#[derive(Serialize)]
struct UpdateResult {
    service: String,
    pulled: String,
    restarted: bool,
}

pub fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
    let services = resolve_services(ctx, services)?;
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for service in services {
        let pull_result = pull_images(ctx, &[service.to_string()])?
            .into_iter()
            .next()
            .expect("pull_images returns exactly one result per requested service");

        let bare = get_bare_name(&service);
        let unit_name = normalize_unit_name(ctx, bare);
        restart_unit(ctx, &unit_name)?;

        if json {
            results.push(UpdateResult {
                service: pull_result.service,
                pulled: pull_result.status,
                restarted: true,
            });
        }
    }

    if json {
        crate::core::print_json(&Report { command: "update", results })?;
    }

    Ok(())
}
