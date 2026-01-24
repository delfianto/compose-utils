//! Logic for managing systemd units via the systemctl CLI.

use crate::core::Context;
use anyhow::{bail, Result};
use std::process::Command;

/// Lists dependencies for a given unit or the default target.
pub fn list_dependencies(ctx: &Context, unit: Option<&str>) -> Result<()> {
    let mut cmd = systemctl_cmd(ctx);
    cmd.arg("list-dependencies").arg("--after").arg("--reverse");

    if let Some(u) = unit {
        cmd.arg(u);
    } else {
        cmd.arg("docker.service");
    }

    let status = cmd.status()?;

    if !status.success() {
        bail!("Failed to list dependencies via systemctl");
    }

    Ok(())
}

/// Shows the status of a specific unit.
pub fn show_status(ctx: &Context, unit: &str) -> Result<()> {
    let mut cmd = systemctl_cmd(ctx);
    cmd.arg("status").arg(unit).arg("--lines=0");

    let _ = cmd.status()?;
    Ok(())
}

/// Enables a systemd unit.
pub fn enable_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "enable", Some(unit))
}

/// Disables a systemd unit.
pub fn disable_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "disable", Some(unit))
}

/// Reloads the systemd daemon.
pub fn daemon_reload(ctx: &Context) -> Result<()> {
    run_systemctl(ctx, "daemon-reload", None)
}

/// Returns the active state of a unit.
pub fn get_unit_state(ctx: &Context, unit: &str) -> Result<String> {
    let mut cmd = systemctl_cmd(ctx);
    let output = cmd
        .arg("show")
        .arg("--property=ActiveState")
        .arg("--value")
        .arg(unit)
        .output()?;

    if !output.status.success() {
        bail!("Failed to get state for {}", unit);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Starts a systemd unit.
pub fn start_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "start", Some(unit))
}

/// Stops a systemd unit.
pub fn stop_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "stop", Some(unit))
}

/// Restarts a systemd unit.
pub fn restart_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "restart", Some(unit))
}

fn run_systemctl(ctx: &Context, action: &str, unit: Option<&str>) -> Result<()> {
    let mut cmd = systemctl_cmd(ctx);
    cmd.arg(action);

    if let Some(u) = unit {
        cmd.arg(u);
    }

    let status = cmd.status()?;

    if !status.success() {
        if let Some(u) = unit {
            bail!("Failed to {} {}: systemctl exited with error", action, u);
        } else {
            bail!("Failed to {}: systemctl exited with error", action);
        }
    }

    Ok(())
}

/// Returns a Command pre-configured for systemctl (either root or user).
fn systemctl_cmd(ctx: &Context) -> std::process::Command {
    let mut cmd = Command::new("systemctl");
    if !ctx.is_root {
        cmd.arg("--user");
    }

    cmd
}
