//! Direct Docker Compose operations (no systemd indirection).

use crate::core::{Report, should_use_infisical, Context};
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

/// Builds a docker compose command, optionally wrapped with `infisical run`
/// for non-bootstrap services when Infisical is configured.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `bare` - The bare service name (e.g., "db-mariadb").
/// * `compose_args` - Arguments to pass to `docker compose` (e.g., ["up", "-d"]).
/// * `infisical_available` - Whether infisical is available (pre-checked by caller).
pub(crate) fn build_compose_command(
    ctx: &Context,
    bare: &str,
    compose_args: &[&str],
    infisical_available: bool,
) -> Command {
    let dir = get_compose_dir(ctx, bare);

    if let Some(ref project_id) = ctx.infisical_project_id {
        if infisical_available && !ctx.is_bootstrap_service(bare) {
            let env_name = ctx.infisical_env.as_deref().unwrap_or("production");
            let secret_path = format!("/{}", bare);

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
            ]);
            cmd.args(compose_args);
            cmd.current_dir(&dir);

            if let Some(ref addr) = ctx.infisical_address {
                cmd.env("INFISICAL_API_URL", addr);
            }
            if let Some(ref host) = ctx.docker_host {
                cmd.env("DOCKER_HOST", host);
            }

            return cmd;
        }
    }

    // Fallback: plain docker compose
    let mut cmd = Command::new("docker");
    cmd.arg("compose");
    cmd.args(compose_args);
    cmd.current_dir(&dir);

    if let Some(ref host) = ctx.docker_host {
        cmd.env("DOCKER_HOST", host);
    }

    cmd
}

/// Executes a compose command with graceful fallback.
///
/// If the infisical-wrapped command fails, falls back to a plain docker compose
/// command and logs a warning.
fn run_compose_command(
    ctx: &Context,
    bare: &str,
    compose_args: &[&str],
    infisical_available: bool,
) -> Result<()> {
    let is_infisical = infisical_available
        && ctx.infisical_project_id.is_some()
        && !ctx.is_bootstrap_service(bare);

    let mut cmd = build_compose_command(ctx, bare, compose_args, infisical_available);
    let dir = get_compose_dir(ctx, bare);

    let status = cmd
        .status()
        .with_context(|| format!("Failed to run compose command in {}", dir.display()))?;

    if !status.success() && is_infisical {
        eprintln!(
            "Warning: infisical run failed for {}, falling back to plain docker compose",
            bare
        );
        let mut fallback = Command::new("docker");
        fallback.arg("compose");
        fallback.args(compose_args);
        fallback.current_dir(&dir);
        if let Some(ref host) = ctx.docker_host {
            fallback.env("DOCKER_HOST", host);
        }
        let fallback_status = fallback
            .status()
            .with_context(|| format!("Failed to run docker compose in {}", dir.display()))?;
        if !fallback_status.success() {
            anyhow::bail!("docker compose {} failed for {}", compose_args[0], bare);
        }
    } else if !status.success() {
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
    let infisical_available = should_use_infisical(ctx);
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for name in services {
        let bare = get_bare_name(&name);
        if !json {
            println!("{} {}...", verb_ing, bare);
        }
        run_compose_command(ctx, bare, compose_args, infisical_available)?;
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
        crate::core::print_json(&Report { command: "up", results })?;
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
        crate::core::print_json(&Report { command: "down", results })?;
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
        crate::core::print_json(&Report { command: "restart", results })?;
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

    fn ctx_with_infisical() -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            compose_base: PathBuf::from("/tmp/test-compose"),
            env_file: PathBuf::from("/tmp/test.env"),
            docker_host: None,
            infisical_project_id: Some("proj-123".to_string()),
            infisical_env: Some("production".to_string()),
            infisical_address: Some("https://infisical.example.com".to_string()),
            infisical_bootstrap: vec![
                "db/postgres".to_string(),
                "db/valkey".to_string(),
                "infra/infisical".to_string(),
            ],
        }
    }

    fn ctx_without_infisical() -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            compose_base: PathBuf::from("/tmp/test-compose"),
            env_file: PathBuf::from("/tmp/test.env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        }
    }

    #[test]
    fn test_build_command_no_infisical_configured() {
        let ctx = ctx_without_infisical();
        let cmd = build_compose_command(&ctx, "db-mariadb", &["up", "-d"], false);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("docker"));
        assert!(!debug.contains("infisical"));
    }

    #[test]
    fn test_build_command_infisical_normal_service() {
        let ctx = ctx_with_infisical();
        let cmd = build_compose_command(&ctx, "db-mariadb", &["up", "-d"], true);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("infisical"));
        assert!(debug.contains("proj-123"));
        assert!(debug.contains("/db-mariadb"));
    }

    #[test]
    fn test_build_command_bootstrap_service_skips_infisical() {
        let ctx = ctx_with_infisical();
        let cmd = build_compose_command(&ctx, "db-postgres", &["up", "-d"], true);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("docker"));
        assert!(!debug.contains("infisical"));
    }

    #[test]
    fn test_build_command_infisical_binary_not_available() {
        let ctx = ctx_with_infisical();
        let cmd = build_compose_command(&ctx, "db-mariadb", &["up", "-d"], false);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("docker"));
        assert!(!debug.contains("infisical"));
    }

    #[test]
    fn test_build_command_down_with_infisical() {
        let ctx = ctx_with_infisical();
        let cmd =
            build_compose_command(&ctx, "ai-ollama", &["down", "--remove-orphans"], true);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("infisical"));
        assert!(debug.contains("/ai-ollama"));
    }

    #[test]
    fn test_build_command_infisical_env_default() {
        let mut ctx = ctx_with_infisical();
        ctx.infisical_env = None; // Unset env
        let cmd = build_compose_command(&ctx, "db-mariadb", &["up", "-d"], true);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("infisical"));
        assert!(debug.contains("production")); // Default
    }

    #[test]
    fn test_build_command_with_docker_host() {
        let mut ctx = ctx_without_infisical();
        ctx.docker_host = Some("tcp://localhost:2375".to_string());
        let cmd = build_compose_command(&ctx, "myapp", &["up", "-d"], false);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("DOCKER_HOST"));
    }

    #[test]
    fn test_build_command_infisical_sets_api_url() {
        let ctx = ctx_with_infisical();
        let cmd = build_compose_command(&ctx, "db-mariadb", &["up", "-d"], true);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("INFISICAL_API_URL"));
        assert!(debug.contains("infisical.example.com"));
    }

    #[test]
    fn test_build_command_pull_with_infisical() {
        let ctx = ctx_with_infisical();
        let cmd = build_compose_command(&ctx, "ai-ollama", &["pull"], true);
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("infisical"));
        assert!(debug.contains("pull"));
    }
}
