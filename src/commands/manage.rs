use crate::core::Context;
use anyhow::{Context as _, Result, bail};
use std::path::PathBuf;
use std::process::Command;

/// Convert project name to directory path.
/// Both `genai-ollama` and `genai/ollama` resolve to `genai/ollama`.
fn name_to_dir_path(name: &str) -> String {
    name.replace('-', "/")
}

/// Convert project name to systemd service name.
/// Both `genai-ollama` and `genai/ollama` become `compose@genai-ollama.service`.
fn name_to_service(name: &str) -> String {
    let normalized = name.replace('/', "-");
    format!("compose@{}.service", normalized)
}

/// Extract the bare project name from various input formats.
/// Strips `compose@` prefix and `.service` suffix if present.
fn get_bare_name(service: &str) -> &str {
    let s = service.strip_suffix(".service").unwrap_or(service);
    s.strip_prefix("compose@").unwrap_or(s)
}

/// Get the compose directory for a project.
fn get_compose_dir(ctx: &Context, name: &str) -> PathBuf {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(bare);
    ctx.compose_base.join(dir_path)
}

/// Validate that compose directories exist for all given services.
fn validate_compose_dirs(ctx: &Context, services: &[String]) -> Result<()> {
    let mut missing = Vec::new();

    for service in services {
        let bare = get_bare_name(service);
        let dir = get_compose_dir(ctx, bare);
        if !dir.exists() {
            missing.push((bare.to_string(), dir));
        }
    }

    if !missing.is_empty() {
        let msg = missing
            .iter()
            .map(|(name, path)| format!("  - '{}' (expected at {})", name, path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "Compose directory not found for the following services:\n{}\n\nEnsure the service name matches an existing directory under {}",
            msg,
            ctx.compose_base.display()
        );
    }

    Ok(())
}

pub fn run_systemctl(
    ctx: &Context,
    action: &str,
    services: &[String],
    validate: bool,
) -> Result<()> {
    if validate {
        validate_compose_dirs(ctx, services)?;
    }

    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg(action);

    for service in services {
        let bare = get_bare_name(service);
        cmd.arg(name_to_service(bare));
    }

    println!("Running: {:?}", cmd);
    let status = cmd.status().context("Failed to execute systemctl")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

pub fn run_start(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "start", services, true)
}

pub fn run_stop(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "stop", services, true)
}

pub fn run_restart(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "restart", services, true)
}

pub fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
    validate_compose_dirs(ctx, services)?;

    for service in services {
        let bare = get_bare_name(service);
        let dir = get_compose_dir(ctx, bare);

        println!("Pulling images for '{}'...", bare);
        let mut pull_cmd = Command::new("docker");
        pull_cmd.args(["compose", "pull"]);
        pull_cmd.current_dir(&dir);

        println!("Running: {:?}", pull_cmd);
        let status = pull_cmd
            .status()
            .context("Failed to run docker compose pull")?;

        if !status.success() {
            bail!(
                "Failed to pull images for '{}' (exit code: {})",
                bare,
                status.code().unwrap_or(1)
            );
        }
    }

    println!("\nRestarting services...");
    run_systemctl(ctx, "restart", services, false)
}

pub fn run_list(ctx: &Context) -> Result<()> {
    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.args(["list-units", "compose@*.service", "--all"]);

    cmd.status().context("Failed to list units")?;
    Ok(())
}

pub fn run_logs(ctx: &Context, service: &str, follow: bool, lines: Option<usize>) -> Result<()> {
    let mut cmd = Command::new("journalctl");
    if !ctx.is_root {
        cmd.arg("--user");
    }

    let bare = get_bare_name(service);
    cmd.arg("-u").arg(name_to_service(bare));

    if follow {
        cmd.arg("-f");
    }
    if let Some(n) = lines {
        cmd.arg("-n").arg(n.to_string());
    }

    println!("Running: {:?}", cmd);
    cmd.status().context("Failed to run logs")?;
    Ok(())
}
