//! Logic for managing systemd units via the systemctl CLI.

use crate::core::Context;
use crate::verbose;
use anyhow::{Result, bail};
use std::collections::HashMap;
use std::process::Command;

/// Lists dependencies for a given unit or the default target.
pub fn list_dependencies(ctx: &Context, unit: Option<&str>) -> Result<()> {
    verbose!("Listing dependencies for: {:?}", unit);
    let mut cmd = systemctl_cmd(ctx);
    cmd.arg("list-dependencies").arg("--reverse").arg("--all");

    if let Some(u) = unit {
        cmd.arg(u);
    } else {
        cmd.arg("docker.service");
    }

    let status = cmd.status()?;

    if !status.success() {
        bail!("Failed to list dependencies: systemctl exited with error");
    }

    Ok(())
}

/// Returns the units that declare a reverse dependency on `unit` -- i.e. the
/// same edges `systemctl list-dependencies --reverse` would draw (following
/// `RequiredBy=`, `WantedBy=`, `UpheldBy=`, `PartOf=`, `BoundBy=`), but
/// fetched via `systemctl show --property=... --value` rather than parsed
/// out of the tree/bullet rendering meant for human eyes. Per `man
/// systemctl`, `show` "is intended to be used whenever computer-parsable
/// output is required" -- there is no `--output=json` for dependency
/// listings, so this is the documented machine-readable path.
pub fn get_reverse_dependents(ctx: &Context, unit: &str) -> Result<Vec<String>> {
    verbose!("Getting reverse dependents for unit: {}", unit);
    let mut cmd = systemctl_cmd(ctx);
    cmd.arg("show")
        .arg(unit)
        .arg("--property=RequiredBy,WantedBy,UpheldBy,PartOf,BoundBy")
        .arg("--value");

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to get reverse dependents for {}: {}", unit, stderr.trim());
    }

    let mut seen = std::collections::HashSet::new();
    let mut dependents = Vec::new();
    for name in String::from_utf8_lossy(&output.stdout).split_whitespace() {
        if seen.insert(name.to_string()) {
            dependents.push(name.to_string());
        }
    }

    Ok(dependents)
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

/// Returns a Command pre-configured for systemctl (either root or user).
fn systemctl_cmd(ctx: &Context) -> std::process::Command {
    let mut cmd = Command::new("systemctl");
    // Ensure shell uses UTF-8 for pretty text output.
    cmd.env("LC_ALL", "C.UTF-8");
    if !ctx.is_root {
        cmd.arg("--user");
    }
    cmd
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

/// Returns a curated set of unit properties (for structured/JSON status
/// reporting), fetched via `systemctl show` rather than the interactive
/// `systemctl status`, which can't be reliably parsed.
pub fn get_unit_properties(ctx: &Context, unit: &str) -> Result<HashMap<String, String>> {
    verbose!("Getting properties for unit: {}", unit);
    let mut cmd = systemctl_cmd(ctx);
    let output = cmd
        .arg("show")
        .arg("--property=ActiveState,SubState,LoadState,UnitFileState,Description")
        .arg(unit)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to get properties for {}: {}", unit, stderr.trim());
    }

    let mut props = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            props.insert(key.to_string(), value.to_string());
        }
    }

    Ok(props)
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
