//! High-level command implementations for managing systemd compose services.

use crate::core::Context;
use crate::systemd::discovery::{resolve_service, resolve_services};
use crate::systemd::journal::{JournalReader, LogEntry};
use crate::systemd::service::{get_bare_name, get_compose_dir, normalize_unit_name};
use anyhow::Result;
use colored::Colorize;

/// Executes the `start` (or `up`) command with smart image pulling.
pub async fn run_start(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    if let Some(path) = deps_path {
        let path = std::path::Path::new(&path);
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
pub async fn run_stop(ctx: &Context, names: &[String]) -> Result<()> {
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
pub async fn run_restart(ctx: &Context, names: &[String]) -> Result<()> {
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

/// Executes the `list` (or `ls`) command to show all `compose@` units.
pub async fn run_list(ctx: &Context) -> Result<()> {
    let units = crate::systemd::manager::list_units(ctx, Some("compose@"))?;

    if units.is_empty() {
        println!("No compose units found.");
        return Ok(());
    }

    println!("{:<40} {:<15} {:<15} DESCRIPTION", "UNIT", "ACTIVE", "SUB");
    for unit in units {
        println!(
            "{:<40} {:<15} {:<15} {}",
            unit.name, unit.active, unit.sub, unit.description
        );
    }

    Ok(())
}

/// Executes the `logs` command using native journal integration.
pub async fn run_logs(
    ctx: &Context,
    service: &str,
    follow: bool,
    lines: Option<usize>,
) -> Result<()> {
    let resolved = resolve_service(ctx, service)?;
    let bare = get_bare_name(&resolved);
    let unit_name = normalize_unit_name(ctx, bare);

    let mut reader = JournalReader::new()?;
    let n = lines.unwrap_or(100);

    if follow {
        let entries = reader.logs_for_unit(&unit_name, n)?;
        for entry in entries {
            print_entry(&entry);
        }

        reader.follow_unit(&unit_name, |entry| {
            print_entry(entry);
            true
        })?;
    } else {
        let entries = reader.logs_for_unit(&unit_name, n)?;
        for entry in entries {
            print_entry(&entry);
        }
    }

    Ok(())
}

/// Executes the `status` command for a set of services.
pub async fn run_status(ctx: &Context, names: &[String]) -> Result<()> {
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

fn print_entry(entry: &LogEntry) {
    let ts = chrono::DateTime::from_timestamp_micros(entry.timestamp as i64)
        .map(|dt| dt.format("%b %d %H:%M:%S").to_string())
        .unwrap_or_default();

    if let Some(id) = &entry.identifier {
        println!("{} {:>12} | {}", ts.dimmed(), id.cyan(), entry.message);
    } else {
        println!("{} {}", ts.dimmed(), entry.message);
    }
}

/// Executes the `enable` command.
pub async fn run_enable(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    if let Some(path) = deps_path {
        let path = std::path::Path::new(&path);
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
pub async fn run_disable(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in &services {
        let bare = get_bare_name(name);
        let unit_name = normalize_unit_name(ctx, bare);

        println!("Disabling {}...", unit_name);
        crate::systemd::manager::disable_unit(ctx, &unit_name)?;
    }

    Ok(())
}