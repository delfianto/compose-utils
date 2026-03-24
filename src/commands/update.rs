use crate::commands::run_pull;
use crate::core::Context;
use crate::systemd::discovery::resolve_services;
use crate::systemd::manager::restart_unit;
use anyhow::Result;

pub fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
    let services = resolve_services(ctx, services)?;

    for service in services {
        run_pull(ctx, &[service.to_string()])?;
        restart_unit(ctx, &service)?;
    }

    Ok(())
}
