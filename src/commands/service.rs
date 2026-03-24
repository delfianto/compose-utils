//! High-level command implementations for managing systemd compose services.

use crate::core::Context;
use crate::systemd::discovery::resolve_services;
use crate::systemd::service::{get_bare_name, get_compose_dir, normalize_unit_name};
use anyhow::Result;

/// Load and apply dependencies from a TOML file, reloading systemd if any were applied.
fn apply_deps_from_file(ctx: &Context, deps_path: &str) -> Result<()> {
    let path = std::path::Path::new(deps_path);
    println!("Loading dependencies from {}...", path.display());

    let config = crate::compose::dependencies::load_dependencies(path)?;
    let mut updated = false;

    for (service_name, service_config) in &config.services {
        let bare = get_bare_name(service_name);
        let dir = get_compose_dir(ctx, bare);

        if dir.exists() {
            crate::commands::deps::apply_dependencies(ctx, service_name, service_config)?;
            updated = true;
        } else {
            println!(
                "Warning: Service '{}' defined in dependency file not found in projects (checked at {}).",
                service_name,
                dir.display()
            );
        }
    }

    if updated {
        crate::systemd::manager::daemon_reload(ctx)?;
    }

    Ok(())
}

/// Executes the `start` (or `up`) command with smart image pulling.
pub fn run_start(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    if let Some(path) = deps_path {
        apply_deps_from_file(ctx, &path)?;
    }

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        println!("Starting {}...", unit_name);
        crate::systemd::manager::start_unit(ctx, &unit_name)?;

        let state = crate::systemd::manager::get_unit_state(ctx, &unit_name)?;
        println!("Started {} ({})", bare, state);
    }

    Ok(())
}

/// Executes the `stop` (or `down`) command.
pub fn run_stop(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        println!("Stopping {}...", unit_name);
        crate::systemd::manager::stop_unit(ctx, &unit_name)?;
        println!("Stopped {}", bare);
    }

    Ok(())
}

/// Executes the `restart` (or `reup`) command.
pub fn run_restart(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        println!("Restarting {}...", unit_name);
        crate::systemd::manager::restart_unit(ctx, &unit_name)?;

        let state = crate::systemd::manager::get_unit_state(ctx, &unit_name)?;
        println!("Restarted {} ({})", bare, state);
    }

    Ok(())
}

/// Executes the `status` command for a set of services.
pub fn run_status(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    if services.is_empty() {
        println!("No services found.");
        return Ok(());
    }

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        crate::systemd::manager::show_status(ctx, &unit_name)?;
        println!();
    }

    Ok(())
}

/// Executes the `enable` command.
pub fn run_enable(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    if let Some(path) = deps_path {
        apply_deps_from_file(ctx, &path)?;
    }

    for name in &services {
        let bare = get_bare_name(name);

        let unit_name = normalize_unit_name(ctx, bare);
        println!("Enabling {}...", unit_name);
        crate::systemd::manager::enable_unit(ctx, &unit_name)?;
    }

    Ok(())
}

/// Executes the `disable` command.
pub fn run_disable(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in &services {
        let bare = get_bare_name(name);
        let unit_name = normalize_unit_name(ctx, bare);

        println!("Disabling {}...", unit_name);
        crate::systemd::manager::disable_unit(ctx, &unit_name)?;
    }

    Ok(())
}
