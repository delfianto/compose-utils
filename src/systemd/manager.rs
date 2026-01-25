//! Logic for managing systemd units via the systemctl CLI.

use crate::core::Context;
use crate::verbose;
use anyhow::{bail, Result};
use std::process::Command;

/// Lists dependencies for a given unit or the default target.
pub fn list_dependencies(ctx: &Context, unit: Option<&str>) -> Result<()> {
    verbose!("Listing dependencies for: {:?}", unit);
    let mut cmd = systemctl_cmd(ctx);
    cmd.arg("list-dependencies").arg("--after").arg("--reverse");

    if let Some(u) = unit {
        cmd.arg(u);
    } else {
        cmd.arg("docker.service");
    }

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to list dependencies: {}", stderr.trim());
    }

    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

/// Shows the status of a specific unit.
pub fn show_status(ctx: &Context, unit: &str) -> Result<()> {
    verbose!("Showing status for unit: {}", unit);
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
    verbose!("Getting state for unit: {}", unit);
    let mut cmd = systemctl_cmd(ctx);
    let output = cmd
        .arg("show")
        .arg("--property=ActiveState")
        .arg("--value")
        .arg(unit)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to get state for {}: {}", unit, stderr.trim());
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

/// Information about a systemd unit.
#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub name: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

/// Lists units matching an optional pattern.
///
/// Uses structured `systemctl show` output instead of parsing formatted text,
/// making this robust against output format changes.
pub fn list_units(ctx: &Context, pattern: Option<&str>) -> Result<Vec<UnitInfo>> {
    verbose!("Listing units matching pattern: {:?}", pattern);
    // First, get the list of unit names using plain output
    let mut cmd = systemctl_cmd(ctx);
    cmd.args(["list-units", "--no-pager", "--no-legend", "--plain"]);

    if let Some(p) = pattern {
        cmd.arg(format!("{}*", p));
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to list units: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let unit_names: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();

    if unit_names.is_empty() {
        verbose!("No units found matching pattern");
        return Ok(Vec::new());
    }

    verbose!("Found {} units, fetching details...", unit_names.len());

    // Get structured properties for all units at once
    let mut cmd = systemctl_cmd(ctx);
    cmd.args(["show", "--property=Id,ActiveState,SubState,Description"]);
    for name in &unit_names {
        cmd.arg(name);
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to get unit properties: {}", stderr.trim());
    }

    // Parse the structured output (property blocks separated by blank lines)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let units = stdout
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .filter_map(parse_unit_properties)
        .collect();

    Ok(units)
}

/// Parses a block of key=value properties into a UnitInfo.
fn parse_unit_properties(block: &str) -> Option<UnitInfo> {
    let mut name = String::new();
    let mut active = String::new();
    let mut sub = String::new();
    let mut description = String::new();

    for line in block.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "Id" => name = value.to_string(),
                "ActiveState" => active = value.to_string(),
                "SubState" => sub = value.to_string(),
                "Description" => description = value.to_string(),
                _ => {}
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(UnitInfo {
        name,
        active,
        sub,
        description,
    })
}

fn run_systemctl(ctx: &Context, action: &str, unit: Option<&str>) -> Result<()> {
    verbose!("Running systemctl {} {:?}", action, unit);
    let mut cmd = systemctl_cmd(ctx);
    cmd.arg(action);

    if let Some(u) = unit {
        cmd.arg(u);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();

        if let Some(u) = unit {
            if stderr.is_empty() {
                bail!("Failed to {} {}: systemctl exited with error", action, u);
            } else {
                bail!("Failed to {} {}: {}", action, u, stderr);
            }
        } else if stderr.is_empty() {
            bail!("Failed to {}: systemctl exited with error", action);
        } else {
            bail!("Failed to {}: {}", action, stderr);
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
