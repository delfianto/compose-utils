use crate::core::Context;
use anyhow::{Context as _, Result, bail};
use std::process::Command;

fn get_service_name(project: &str) -> String {
    let project = if project.ends_with(".service") {
        if project.starts_with("compose@") {
            return project.to_string();
        }
        &project[..project.len() - 8]
    } else {
        project
    };

    if !project.starts_with("compose@") {
        format!("compose@{}.service", project)
    } else {
        if !project.ends_with(".service") {
            format!("{}.service", project)
        } else {
            project.to_string()
        }
    }
}

/// Extract the bare service name (without compose@ prefix and .service suffix)
fn get_bare_name(service: &str) -> &str {
    let s = service.strip_suffix(".service").unwrap_or(service);
    s.strip_prefix("compose@").unwrap_or(s)
}

/// Validate that compose directories exist for all given services
fn validate_compose_dirs(ctx: &Context, services: &[String]) -> Result<()> {
    let mut missing = Vec::new();

    for service in services {
        let name = get_bare_name(service);
        let dir = ctx.compose_base.join(name);
        if !dir.exists() {
            missing.push((name.to_string(), dir));
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
        cmd.arg(get_service_name(service));
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
        let name = get_bare_name(service);
        let dir = ctx.compose_base.join(name);

        println!("Pulling images for '{}'...", name);
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
                name,
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

pub fn run_logs(
    ctx: &Context,
    services: &[String],
    follow: bool,
    lines: Option<usize>,
) -> Result<()> {
    let mut cmd = Command::new("journalctl");
    if !ctx.is_root {
        cmd.arg("--user");
    }

    for service in services {
        cmd.arg("-u").arg(get_service_name(service));
    }

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
