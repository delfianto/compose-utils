//! High-level command implementations for managing systemd services via D-Bus.

use crate::compose::project::get_required_images;
use crate::core::Context;
use crate::docker::connect_docker;
use crate::docker::images::pull_image_with_progress;
use crate::systemd::client::SystemdClient;
use crate::systemd::discovery::{resolve_service, resolve_services};
use crate::systemd::journal::{JournalReader, LogEntry};
use crate::systemd::manager::ensure_symlink;
use crate::systemd::service::{get_bare_name, get_compose_dir, name_to_service};
use crate::systemd::types::JobResult;
use anyhow::{bail, Result};

/// Executes the `start` (or `up`) command with smart image pulling.
pub async fn run_start(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let systemd = SystemdClient::new(!ctx.is_root).await?;
    let docker = connect_docker(ctx)?;
    let services = resolve_services(ctx, names)?;

    if let Some(path) = deps_path {
        let path = std::path::Path::new(&path);
        println!("Loading dependencies from {}...", path.display());
        let config = crate::compose::dependencies::load_dependencies(path)?;

        let mut updated = false;
        // Apply dependencies for all services defined in the file
        for (service_name, service_config) in &config.services {
            if crate::systemd::discovery::resolve_service(ctx, service_name).is_ok() {
                crate::commands::deps::apply_dependencies(ctx, service_name, service_config)?;
                updated = true;
            } else {
                println!(
                    "Warning: Service '{}' defined in dependency file not found in projects.",
                    service_name
                );
            }
        }

        if updated {
            systemd.reload_daemon().await?;
        }
    }

    for name in services {
        let bare = get_bare_name(&name);
        ensure_symlink(ctx, bare)?;

        let compose_dir = get_compose_dir(ctx, bare);
        let unit_name = name_to_service(bare);
        let images = get_required_images(&compose_dir)?;

        if !images.is_empty() {
            println!("Checking images for {}...", bare);
            let mut missing = Vec::new();
            for image in &images {
                match docker.inspect_image(image).await {
                    Ok(_) => println!("  {} - present", image),
                    Err(_) => {
                        println!("  {} - missing", image);
                        missing.push(image.clone());
                    }
                }
            }

            if !missing.is_empty() {
                println!();
                for image in &missing {
                    pull_image_with_progress(&docker, image).await?;
                }
                println!();
            }
        }

        println!("Starting {}...", unit_name);
        let job_id = systemd.start_unit(&unit_name).await?;

        let result = systemd.wait_for_job(job_id).await?;

        match result {
            JobResult::Done => {
                let state = systemd.get_unit_state(&unit_name).await?;
                println!("Started {} ({:?})", bare, state);
            }
            JobResult::Failed => {
                let log_cmd = if ctx.is_root {
                    format!("journalctl -u {}", unit_name)
                } else {
                    format!("journalctl --user -u {}", unit_name)
                };
                bail!("Failed to start {} - check: {}", bare, log_cmd);
            }
            other => {
                bail!("Start job for {} ended with: {:?}", bare, other);
            }
        }
    }

    Ok(())
}

/// Executes the `stop` (or `down`) command.
pub async fn run_stop(ctx: &Context, names: &[String]) -> Result<()> {
    let systemd = SystemdClient::new(!ctx.is_root).await?;
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = name_to_service(bare);

        println!("Stopping {}...", unit_name);
        let job_id = systemd.stop_unit(&unit_name).await?;
        let result = systemd.wait_for_job(job_id).await?;

        match result {
            JobResult::Done => println!("Stopped {}", bare),
            other => bail!("Stop job for {} ended with: {:?}", bare, other),
        }
    }

    Ok(())
}

/// Executes the `restart` (or `reup`) command.
pub async fn run_restart(ctx: &Context, names: &[String]) -> Result<()> {
    let systemd = SystemdClient::new(!ctx.is_root).await?;
    let docker = connect_docker(ctx)?;
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        ensure_symlink(ctx, bare)?;

        let compose_dir = get_compose_dir(ctx, bare);
        let unit_name = name_to_service(bare);

        // Check and pull images before restart
        let images = get_required_images(&compose_dir)?;
        if !images.is_empty() {
            let mut missing = Vec::new();
            for image in &images {
                if docker.inspect_image(image).await.is_err() {
                    missing.push(image.clone());
                }
            }
            for image in &missing {
                pull_image_with_progress(&docker, image).await?;
            }
        }

        println!("Restarting {}...", unit_name);
        let job_id = systemd.restart_unit(&unit_name).await?;
        let result = systemd.wait_for_job(job_id).await?;

        match result {
            JobResult::Done => {
                let state = systemd.get_unit_state(&unit_name).await?;
                println!("Restarted {} ({:?})", bare, state);
            }
            other => bail!("Restart job for {} ended with: {:?}", bare, other),
        }
    }

    Ok(())
}

/// Executes the `list` (or `ls`) command to show all `compose@` units.
pub async fn run_list(ctx: &Context) -> Result<()> {
    let systemd = SystemdClient::new(!ctx.is_root).await?;
    let units = systemd.list_units(Some("compose@")).await?;

    if units.is_empty() {
        println!("No compose units found.");
        return Ok(());
    }

    println!("{:<40} {:<15} {:<15} DESCRIPTION", "UNIT", "ACTIVE", "SUB");
    for unit in units {
        println!(
            "{:<40} {:<15} {:<15} {}",
            unit.name,
            format!("{:?}", unit.active_state),
            format!("{:?}", unit.sub_state),
            unit.description
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
    let unit_name = name_to_service(bare);

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
    let systemd = SystemdClient::new(!ctx.is_root).await?;
    let services = resolve_services(ctx, names)?;

    if services.is_empty() {
        println!("No services found.");
        return Ok(());
    }

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = name_to_service(bare);

        match systemd.get_unit_properties(&unit_name).await {
            Ok(props) => {
                println!("● {} - {}", bare, props.description);
                println!("   Active: {:?} ({:?})", props.state, props.sub_state);
                if !props.requires.is_empty() {
                    println!("   Requires: {}", props.requires.join(", "));
                }
                if !props.wants.is_empty() {
                    println!("   Wants: {}", props.wants.join(", "));
                }
                if !props.after.is_empty() {
                    println!("   After: {}", props.after.join(", "));
                }
                if !props.before.is_empty() {
                    println!("   Before: {}", props.before.join(", "));
                }
                println!();
            }
            Err(e) => {
                println!("○ {} - Could not retrieve status: {}", bare, e);
                println!();
            }
        }
    }

    Ok(())
}

fn print_entry(entry: &LogEntry) {
    let ts = chrono::DateTime::from_timestamp_micros(entry.timestamp as i64)
        .map(|dt| dt.format("%b %d %H:%M:%S").to_string())
        .unwrap_or_default();

    println!("{} {}", ts, entry.message);
}

/// Executes the `enable` command.
pub async fn run_enable(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let systemd = SystemdClient::new(!ctx.is_root).await?;
    let services = resolve_services(ctx, names)?;

    if let Some(path) = deps_path {
        let path = std::path::Path::new(&path);
        println!("Loading dependencies from {}...", path.display());
        let config = crate::compose::dependencies::load_dependencies(path)?;

        let mut updated = false;
        for (service_name, service_config) in &config.services {
            if crate::systemd::discovery::resolve_service(ctx, service_name).is_ok() {
                crate::commands::deps::apply_dependencies(ctx, service_name, service_config)?;
                updated = true;
            } else {
                println!(
                    "Warning: Service '{}' defined in dependency file not found in projects.",
                    service_name
                );
            }
        }

        if updated {
            systemd.reload_daemon().await?;
        }
    }

    for name in &services {
        let bare = get_bare_name(name);
        ensure_symlink(ctx, bare)?;

        let unit_name = name_to_service(bare);
        println!("Enabling {}...", unit_name);
        systemd.enable_unit(&unit_name).await?;
    }

    Ok(())
}

/// Executes the `disable` command.
pub async fn run_disable(ctx: &Context, names: &[String]) -> Result<()> {
    let systemd = SystemdClient::new(!ctx.is_root).await?;
    let services = resolve_services(ctx, names)?;

    for name in &services {
        let bare = get_bare_name(name);
        let unit_name = name_to_service(bare);

        println!("Disabling {}...", unit_name);
        systemd.disable_unit(&unit_name).await?;
        crate::systemd::manager::remove_symlink(ctx, bare)?;
    }

    Ok(())
}
