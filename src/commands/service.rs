use crate::core::Context;
use anyhow::{Context as _, Result};
use std::process::Command;

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

pub fn run_start(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "start", services, true)
}

pub fn run_stop(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "stop", services, true)
}

pub fn run_restart(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "restart", services, true)
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

pub fn run_enable(ctx: &Context, services: &[String]) -> Result<()> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;
    crate::systemd::discovery::validate_compose_dirs(ctx, &services)?;

    // Create symlinks for nested directories
    for service in &services {
        crate::systemd::manager::ensure_symlink(ctx, service)?;
    }

    // Run systemctl enable
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

pub fn run_disable(ctx: &Context, services: &[String]) -> Result<()> {
    let services = crate::systemd::discovery::resolve_services(ctx, services)?;

    // Run systemctl disable first
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

    // Remove symlinks
    for service in &services {
        crate::systemd::manager::remove_symlink(ctx, service)?;
    }

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
