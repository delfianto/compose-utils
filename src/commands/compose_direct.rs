//! Direct Docker Compose operations (no systemd indirection).

use crate::core::{Context, Report};
use crate::systemd::discovery::resolve_services;
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::{Context as _, Result};
use serde::Serialize;
use std::process::Command;

/// Per-service result for `compose up`/`down`/`restart`.
#[derive(Serialize)]
struct ComposeResult {
    service: String,
    status: String,
}

/// Builds a `docker compose` command rooted in the service's project directory.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `bare` - The bare service name (e.g., "db-mariadb").
/// * `compose_args` - Arguments to pass to `docker compose` (e.g., ["up", "-d"]).
pub(crate) fn build_compose_command(ctx: &Context, bare: &str, compose_args: &[&str]) -> Command {
    let dir = get_compose_dir(ctx, bare);

    let mut cmd = Command::new("docker");
    cmd.arg("compose");
    cmd.args(compose_args);
    cmd.current_dir(&dir);

    if let Some(ref host) = ctx.docker_host {
        cmd.env("DOCKER_HOST", host);
    }

    cmd
}

/// Executes a compose command, failing if it exits non-zero.
fn run_compose_command(ctx: &Context, bare: &str, compose_args: &[&str]) -> Result<()> {
    let mut cmd = build_compose_command(ctx, bare, compose_args);
    let dir = get_compose_dir(ctx, bare);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to run compose command in {}", dir.display()))?;

    if !status.success() {
        anyhow::bail!("docker compose {} failed for {}", compose_args[0], bare);
    }

    Ok(())
}

/// Runs a compose lifecycle action (up/down) across the resolved services,
/// printing human progress lines or collecting JSON results depending on mode.
fn run_lifecycle(
    ctx: &Context,
    names: &[String],
    compose_args: &[&str],
    verb_ing: &str,
    verb_ed: &str,
    status_word: &str,
) -> Result<Vec<ComposeResult>> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for name in services {
        let bare = get_bare_name(&name);
        if !json {
            println!("{} {}...", verb_ing, bare);
        }
        run_compose_command(ctx, bare, compose_args)?;
        if json {
            results.push(ComposeResult {
                service: bare.to_string(),
                status: status_word.to_string(),
            });
        } else {
            println!("{} {}", verb_ed, bare);
        }
    }

    Ok(results)
}

/// Run `docker compose up -d` directly in the project directory.
pub fn compose_up(ctx: &Context, names: &[String]) -> Result<()> {
    let results = run_lifecycle(ctx, names, &["up", "-d"], "Starting", "Started", "started")?;
    if crate::core::is_json() {
        crate::core::print_json(&Report {
            command: "up",
            results,
        })?;
    }
    Ok(())
}

/// Run `docker compose down --remove-orphans` directly in the project directory.
pub fn compose_down(ctx: &Context, names: &[String]) -> Result<()> {
    let results = run_lifecycle(
        ctx,
        names,
        &["down", "--remove-orphans"],
        "Stopping",
        "Stopped",
        "stopped",
    )?;
    if crate::core::is_json() {
        crate::core::print_json(&Report {
            command: "down",
            results,
        })?;
    }
    Ok(())
}

/// Run `docker compose down` then `docker compose up -d`.
pub fn compose_restart(ctx: &Context, names: &[String]) -> Result<()> {
    run_lifecycle(
        ctx,
        names,
        &["down", "--remove-orphans"],
        "Stopping",
        "Stopped",
        "stopped",
    )?;
    let results = run_lifecycle(ctx, names, &["up", "-d"], "Starting", "Started", "started")?;

    if crate::core::is_json() {
        let results: Vec<ComposeResult> = results
            .into_iter()
            .map(|r| ComposeResult {
                service: r.service,
                status: "restarted".to_string(),
            })
            .collect();
        crate::core::print_json(&Report {
            command: "restart",
            results,
        })?;
    }

    Ok(())
}

/// Returns true if the compose project has at least one running container.
///
/// Used to detect drift between systemd's tracked unit state and the actual
/// state of the containers (e.g. after a manual `docker compose up`/`down`
/// that bypassed systemd entirely).
pub(crate) fn project_is_running(ctx: &Context, bare: &str) -> Result<bool> {
    let dir = get_compose_dir(ctx, bare);

    let mut cmd = Command::new("docker");
    cmd.args(["compose", "ps", "--status", "running", "--quiet"]);
    cmd.current_dir(&dir);

    if let Some(ref host) = ctx.docker_host {
        cmd.env("DOCKER_HOST", host);
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to check running state for {}", bare))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose ps failed for {}: {}", bare, stderr.trim());
    }

    Ok(!output.stdout.iter().all(u8::is_ascii_whitespace))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            compose_base: PathBuf::from("/tmp/test-compose"),
            env_file: PathBuf::from("/tmp/test.env"),
            docker_host: None,
        }
    }

    #[test]
    fn test_build_command_up() {
        let ctx = test_ctx();
        let cmd = build_compose_command(&ctx, "db-mariadb", &["up", "-d"]);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("docker"));
        assert!(debug.contains("compose"));
        assert!(debug.contains("-d"));
    }

    #[test]
    fn test_build_command_down() {
        let ctx = test_ctx();
        let cmd = build_compose_command(&ctx, "ai-ollama", &["down", "--remove-orphans"]);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("down"));
        assert!(debug.contains("--remove-orphans"));
    }

    #[test]
    fn test_build_command_pull() {
        let ctx = test_ctx();
        let cmd = build_compose_command(&ctx, "ai-ollama", &["pull"]);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("docker"));
        assert!(debug.contains("pull"));
    }

    #[test]
    fn test_build_command_with_docker_host() {
        let mut ctx = test_ctx();
        ctx.docker_host = Some("tcp://localhost:2375".to_string());
        let cmd = build_compose_command(&ctx, "myapp", &["up", "-d"]);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("DOCKER_HOST"));
    }
}
