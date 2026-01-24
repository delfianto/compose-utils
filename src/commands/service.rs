//! High-level command implementations for managing systemd services.

use crate::core::Context;
use anyhow::{Context as _, Result};
use std::process::Command;

/// Orchestrates service-related `systemctl` actions with discovery and validation.
///
/// This is a generic wrapper that handles:
/// 1. Automatic service detection if no services are specified.
/// 2. Validation of compose project directories.
/// 3. Execution of the `systemctl` command via [`crate::systemd::service::run_systemctl`].
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `action` - The systemctl action (e.g., "start", "stop", "status").
/// * `services` - Explicit list of services.
/// * `validate` - Whether to verify directory existence before running.
///
/// # Errors
///
/// Returns an error if resolution or execution fails.
pub fn run_systemctl(
    ctx: &Context,
    action: &str,
    services: &[String],
    validate: bool,
) -> Result<()> {
    let services = if services.is_empty() && action == "status" {
        if let Some(service) = crate::systemd::discovery::detect_service_from_cwd(ctx) {
            vec![service]
        } else {
            crate::systemd::discovery::find_all_services(ctx)?
        }
    } else {
        crate::systemd::discovery::resolve_services(ctx, services)?
    };

    if services.is_empty() && action == "status" {
        println!("No services found.");
        return Ok(());
    }

    if validate {
        crate::systemd::discovery::validate_compose_dirs(ctx, &services)?;
    }

    crate::systemd::service::run_systemctl(ctx, action, &services)
}

/// Executes the `start` (or `up`) command.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `services` - Services to start.
pub fn run_start(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "start", services, true)
}

/// Executes the `stop` (or `down`) command.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `services` - Services to stop.
pub fn run_stop(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "stop", services, true)
}

/// Executes the `restart` (or `reup`) command.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `services` - Services to restart.
pub fn run_restart(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "restart", services, true)
}

/// Executes the `list` (or `ls`) command to show all `compose@` units.
///
/// # Arguments
///
/// * `ctx` - The application context.
pub fn run_list(ctx: &Context) -> Result<()> {
    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.args(["list-units", "compose@*.service", "--all"]);

    cmd.status().context("Failed to list units")?;
    Ok(())
}

/// Executes the `logs` command using `journalctl`.
///
/// Automatically determines whether to use `--user` mode.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `service` - The service name.
/// * `follow` - Whether to tail the logs (`-f`).
/// * `lines` - Number of tail lines to show (`-n`).
///
/// # Errors
///
/// Returns an error if `journalctl` fails.
pub fn run_logs(ctx: &Context, service: &str, follow: bool, lines: Option<usize>) -> Result<()> {
    let service = crate::systemd::discovery::resolve_service(ctx, service)?;

    let mut cmd = Command::new("journalctl");
    if !ctx.is_root {
        cmd.arg("--user");
    }

    let bare = crate::systemd::service::get_bare_name(&service);
    cmd.arg("-u")
        .arg(crate::systemd::service::name_to_service(bare));

    if follow {
        cmd.arg("-f");
    } else {
        cmd.arg("-e");
    }

    let n = lines.unwrap_or(100);
    cmd.arg("-n").arg(n.to_string());

    println!("Running: {:?}", cmd);
    cmd.status().context("Failed to run logs")?;
    Ok(())
}

/// Executes the `enable` command.
///
/// Also ensures that symlinks for nested directories are created.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `services` - Services to enable.
pub fn run_enable(ctx: &Context, services: &[String]) -> Result<()> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;
    crate::systemd::discovery::validate_compose_dirs(ctx, &services)?;

    for service in &services {
        crate::systemd::manager::ensure_symlink(ctx, service)?;
    }

    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg("enable");

    for service in &services {
        let bare = crate::systemd::service::get_bare_name(service);
        cmd.arg(crate::systemd::service::name_to_service(bare));
    }

    println!("Running: {:?}", cmd);
    let status = cmd.status().context("Failed to execute systemctl")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// Executes the `disable` command.
///
/// Also removes associated symlinks for nested directories.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `services` - Services to disable.
pub fn run_disable(ctx: &Context, services: &[String]) -> Result<()> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;

    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg("disable");

    for service in &services {
        let bare = crate::systemd::service::get_bare_name(service);
        cmd.arg(crate::systemd::service::name_to_service(bare));
    }

    println!("Running: {:?}", cmd);
    let status = cmd.status().context("Failed to execute systemctl")?;

    for service in &services {
        crate::systemd::manager::remove_symlink(ctx, service)?;
    }

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
