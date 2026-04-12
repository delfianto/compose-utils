//! Internal commands called by systemd unit templates.

use crate::core::{should_use_infisical, Context};
use crate::systemd::service::get_compose_dir;
use anyhow::Result;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn run_service(ctx: &Context, name: &str) -> Result<()> {
    let project_dir = get_compose_dir(ctx, name);
    let infisical_available = should_use_infisical(ctx);

    if infisical_available && !ctx.is_bootstrap_service(name) {
        let env_name = ctx.infisical_env.as_deref().unwrap_or("production");
        let secret_path = format!("/{}", name);
        let project_id = ctx.infisical_project_id.as_ref().unwrap();

        eprintln!("Using infisical for service: {}", name);

        let mut cmd = Command::new("infisical");
        cmd.args([
            "run",
            "--projectId",
            project_id,
            "--env",
            env_name,
            "--path",
            &secret_path,
            "--",
            "docker",
            "compose",
            "up",
            "-d",
        ]);
        cmd.current_dir(&project_dir);
        if let Some(ref addr) = ctx.infisical_address {
            cmd.env("INFISICAL_API_URL", addr);
        }

        let err = cmd.exec();
        return Err(anyhow::anyhow!("Failed to exec infisical run: {}", err));
    }

    // Fallback: plain docker compose
    if ctx.infisical_project_id.is_some() && !infisical_available {
        eprintln!(
            "Infisical configured but not available (missing token or binary), \
             using plain docker compose"
        );
    }

    let err = Command::new("docker")
        .arg("compose")
        .arg("up")
        .arg("-d")
        .current_dir(project_dir)
        .exec();

    Err(anyhow::anyhow!("Failed to exec docker compose: {}", err))
}

pub fn stop_service(ctx: &Context, service: &str) -> Result<()> {
    let dir = get_compose_dir(ctx, service);
    let infisical_available = should_use_infisical(ctx);

    if infisical_available && !ctx.is_bootstrap_service(service) {
        let env_name = ctx.infisical_env.as_deref().unwrap_or("production");
        let secret_path = format!("/{}", service);
        let project_id = ctx.infisical_project_id.as_ref().unwrap();

        eprintln!("Using infisical for service stop: {}", service);

        let mut cmd = Command::new("infisical");
        cmd.args([
            "run",
            "--projectId",
            project_id,
            "--env",
            env_name,
            "--path",
            &secret_path,
            "--",
            "docker",
            "compose",
            "down",
            "--remove-orphans",
        ]);
        cmd.current_dir(&dir);
        if let Some(ref addr) = ctx.infisical_address {
            cmd.env("INFISICAL_API_URL", addr);
        }

        let err = cmd.exec();
        return Err(anyhow::anyhow!("Failed to exec infisical run: {}", err));
    }

    if ctx.infisical_project_id.is_some() && !infisical_available {
        eprintln!(
            "Infisical configured but not available (missing token or binary), \
             using plain docker compose"
        );
    }

    let err = Command::new("docker")
        .args(["compose", "down", "--remove-orphans"])
        .current_dir(&dir)
        .exec();

    Err(anyhow::anyhow!("Failed to execute docker compose: {}", err))
}
