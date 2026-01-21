use crate::core::Context;
use anyhow::{Context as _, Result};
use clap::{Args, Subcommand};
use std::process::Command;

#[derive(Args)]
pub struct ManageArgs {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
pub enum Action {
    Start {
        services: Vec<String>,
    },
    Stop {
        services: Vec<String>,
    },
    Restart {
        services: Vec<String>,
    },
    Status {
        services: Vec<String>,
    },
    Enable {
        services: Vec<String>,
    },
    Disable {
        services: Vec<String>,
    },
    List,
    Logs {
        services: Vec<String>,
        #[arg(short, long)]
        follow: bool,
        #[arg(short = 'n', long)]
        lines: Option<usize>,
    },
}

pub fn run(ctx: &Context, args: ManageArgs) -> Result<()> {
    match args.action {
        Action::Start { services } => run_systemctl(ctx, "start", &services),
        Action::Stop { services } => run_systemctl(ctx, "stop", &services),
        Action::Restart { services } => run_systemctl(ctx, "restart", &services),
        Action::Status { services } => run_systemctl(ctx, "status", &services),
        Action::Enable { services } => run_systemctl(ctx, "enable", &services),
        Action::Disable { services } => run_systemctl(ctx, "disable", &services),
        Action::List => run_list(ctx),
        Action::Logs {
            services,
            follow,
            lines,
        } => run_logs(ctx, &services, follow, lines),
    }
}

fn get_service_name(project: &str) -> String {
    let project = if project.ends_with(".service") {
        if project.starts_with("docker-compose@") {
            return project.to_string();
        }
        &project[..project.len() - 8]
    } else {
        project
    };

    if !project.starts_with("docker-compose@") {
        format!("docker-compose@{}.service", project)
    } else {
        // Should already be handled but safety net
        if !project.ends_with(".service") {
            format!("{}.service", project)
        } else {
            project.to_string()
        }
    }
}

fn run_systemctl(ctx: &Context, action: &str, services: &[String]) -> Result<()> {
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

fn run_list(ctx: &Context) -> Result<()> {
    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.args(["list-units", "docker-compose@*.service", "--all"]);

    cmd.status().context("Failed to list units")?;
    Ok(())
}

fn run_logs(ctx: &Context, services: &[String], follow: bool, lines: Option<usize>) -> Result<()> {
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
