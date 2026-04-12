//! Logic for managing secrets via the Infisical CLI.
//!
//! Wraps `infisical secrets` subcommands, mapping compose service names
//! to Infisical secret paths. All operations are NO-OP with a warning
//! if Infisical is not configured or not available.

use crate::core::{should_use_infisical, Context};
use crate::systemd::discovery::resolve_services;
use crate::systemd::service::get_bare_name;
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use std::process::Command;

/// Command-line arguments for the `secret` subcommand.
#[derive(Args)]
pub struct SecretArgs {
    #[command(subcommand)]
    pub action: SecretAction,
}

#[derive(Subcommand)]
pub enum SecretAction {
    /// List all secrets for a service.
    List {
        /// Service name (auto-detected from CWD if omitted).
        service: Option<String>,
    },
    /// Get the value of a specific secret.
    Get {
        /// The secret key name.
        key: String,
        /// Service name (auto-detected from CWD if omitted).
        #[arg(long)]
        service: Option<String>,
    },
    /// Set or update a secret (creates if it doesn't exist).
    Set {
        /// The secret key name.
        key: String,
        /// The secret value.
        value: String,
        /// Service name (auto-detected from CWD if omitted).
        #[arg(long)]
        service: Option<String>,
    },
    /// Delete one or more secrets.
    Delete {
        /// Secret key name(s) to delete.
        #[arg(required = true)]
        keys: Vec<String>,
        /// Service name (auto-detected from CWD if omitted).
        #[arg(long)]
        service: Option<String>,
    },
}

/// Entry point for the `secret` command.
pub fn run(ctx: &Context, args: SecretArgs) -> Result<()> {
    if !check_infisical_available(ctx) {
        return Ok(());
    }

    match args.action {
        SecretAction::List { service } => {
            let bare = resolve_single_service(ctx, service.as_deref())?;
            list_secrets(ctx, &bare)
        }
        SecretAction::Get { key, service } => {
            let bare = resolve_single_service(ctx, service.as_deref())?;
            get_secret(ctx, &bare, &key)
        }
        SecretAction::Set { key, value, service } => {
            let bare = resolve_single_service(ctx, service.as_deref())?;
            set_secret(ctx, &bare, &key, &value)
        }
        SecretAction::Delete { keys, service } => {
            let bare = resolve_single_service(ctx, service.as_deref())?;
            delete_secrets(ctx, &bare, &keys)
        }
    }
}

/// Checks if Infisical is configured and available. Prints a warning and
/// returns false if not, making the command a NO-OP.
fn check_infisical_available(ctx: &Context) -> bool {
    if ctx.infisical_project_id.is_none() {
        eprintln!("Infisical is not configured. Set INFISICAL_PROJECT_ID in compose.env.");
        return false;
    }
    if !should_use_infisical(ctx) {
        eprintln!(
            "Infisical is not available. Ensure INFISICAL_TOKEN is set \
             and the infisical binary is in PATH."
        );
        return false;
    }
    true
}

/// Resolves a single service name from an explicit argument or CWD detection.
fn resolve_single_service(ctx: &Context, service: Option<&str>) -> Result<String> {
    match service {
        Some(name) => {
            let bare = get_bare_name(name);
            Ok(bare.to_string())
        }
        None => {
            let services = resolve_services(ctx, &[])?;
            if services.len() != 1 {
                bail!("Expected exactly one service, got {}", services.len());
            }
            let bare = get_bare_name(&services[0]);
            Ok(bare.to_string())
        }
    }
}

/// Builds the base infisical command with project/env/path flags.
fn infisical_base_cmd(ctx: &Context, bare: &str) -> Command {
    let project_id = ctx.infisical_project_id.as_ref().unwrap();
    let env_name = ctx.infisical_env.as_deref().unwrap_or("production");
    let secret_path = format!("/{}", bare);

    let mut cmd = Command::new("infisical");
    cmd.arg("secrets");

    if let Some(ref addr) = ctx.infisical_address {
        cmd.env("INFISICAL_API_URL", addr);
    }

    // Store these to append after subcommand
    cmd.args(["--projectId", project_id]);
    cmd.args(["--env", env_name]);
    cmd.args(["--path", &secret_path]);

    cmd
}

fn list_secrets(ctx: &Context, bare: &str) -> Result<()> {
    let mut cmd = infisical_base_cmd(ctx, bare);

    let status = cmd.status()?;
    if !status.success() {
        bail!("Failed to list secrets for {}", bare);
    }

    Ok(())
}

fn get_secret(ctx: &Context, bare: &str, key: &str) -> Result<()> {
    let project_id = ctx.infisical_project_id.as_ref().unwrap();
    let env_name = ctx.infisical_env.as_deref().unwrap_or("production");
    let secret_path = format!("/{}", bare);

    let mut cmd = Command::new("infisical");
    cmd.args(["secrets", "get", key]);
    cmd.args(["--projectId", project_id]);
    cmd.args(["--env", env_name]);
    cmd.args(["--path", &secret_path]);

    if let Some(ref addr) = ctx.infisical_address {
        cmd.env("INFISICAL_API_URL", addr);
    }

    let status = cmd.status()?;
    if !status.success() {
        bail!("Failed to get secret '{}' for {}", key, bare);
    }

    Ok(())
}

fn set_secret(ctx: &Context, bare: &str, key: &str, value: &str) -> Result<()> {
    let project_id = ctx.infisical_project_id.as_ref().unwrap();
    let env_name = ctx.infisical_env.as_deref().unwrap_or("production");
    let secret_path = format!("/{}", bare);
    let kv = format!("{}={}", key, value);

    let mut cmd = Command::new("infisical");
    cmd.args(["secrets", "set", &kv]);
    cmd.args(["--projectId", project_id]);
    cmd.args(["--env", env_name]);
    cmd.args(["--path", &secret_path]);

    if let Some(ref addr) = ctx.infisical_address {
        cmd.env("INFISICAL_API_URL", addr);
    }

    let status = cmd.status()?;
    if !status.success() {
        bail!("Failed to set secret '{}' for {}", key, bare);
    }

    println!("Secret '{}' set for {}", key, bare);
    Ok(())
}

fn delete_secrets(ctx: &Context, bare: &str, keys: &[String]) -> Result<()> {
    let project_id = ctx.infisical_project_id.as_ref().unwrap();
    let env_name = ctx.infisical_env.as_deref().unwrap_or("production");
    let secret_path = format!("/{}", bare);

    let mut cmd = Command::new("infisical");
    cmd.args(["secrets", "delete"]);
    cmd.args(keys);
    cmd.args(["--projectId", project_id]);
    cmd.args(["--env", env_name]);
    cmd.args(["--path", &secret_path]);

    if let Some(ref addr) = ctx.infisical_address {
        cmd.env("INFISICAL_API_URL", addr);
    }

    let status = cmd.status()?;
    if !status.success() {
        bail!("Failed to delete secret(s) for {}", bare);
    }

    println!("Deleted {} secret(s) from {}", keys.len(), bare);
    Ok(())
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
            infisical_bootstrap: vec![],
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
    fn test_check_infisical_not_configured() {
        let ctx = ctx_without_infisical();
        assert!(!check_infisical_available(&ctx));
    }

    #[test]
    fn test_check_infisical_configured_but_unavailable() {
        let ctx = ctx_with_infisical();
        // INFISICAL_TOKEN is not set and binary not in PATH
        // should_use_infisical returns false
        assert!(!check_infisical_available(&ctx));
    }

    #[test]
    fn test_resolve_single_service_explicit() {
        let ctx = ctx_with_infisical();
        let result = resolve_single_service(&ctx, Some("db-mariadb")).unwrap();
        assert_eq!(result, "db-mariadb");
    }

    #[test]
    fn test_resolve_single_service_strips_prefix() {
        let ctx = ctx_with_infisical();
        let result = resolve_single_service(&ctx, Some("compose@db-mariadb.service")).unwrap();
        assert_eq!(result, "db-mariadb");
    }

    #[test]
    fn test_infisical_base_cmd_structure() {
        let ctx = ctx_with_infisical();
        let cmd = infisical_base_cmd(&ctx, "db-mariadb");
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("infisical"));
        assert!(debug.contains("secrets"));
        assert!(debug.contains("proj-123"));
        assert!(debug.contains("production"));
        assert!(debug.contains("/db-mariadb"));
        assert!(debug.contains("INFISICAL_API_URL"));
    }

    #[test]
    fn test_infisical_base_cmd_default_env() {
        let mut ctx = ctx_with_infisical();
        ctx.infisical_env = None;
        let cmd = infisical_base_cmd(&ctx, "myapp");
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("production")); // default
    }

    #[test]
    fn test_infisical_base_cmd_no_address() {
        let mut ctx = ctx_with_infisical();
        ctx.infisical_address = None;
        let cmd = infisical_base_cmd(&ctx, "myapp");
        let debug = format!("{:?}", cmd);
        assert!(!debug.contains("INFISICAL_API_URL"));
    }
}
