//! Logic for managing dependencies between systemd services using drop-in overrides.

use crate::core::Context;
use anyhow::{Context as _, Result};
use clap::Args;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Command-line arguments for the `deps` subcommand.
#[derive(Args)]
pub struct DepsArgs {
    /// The name of the service to manage dependencies for.
    #[arg(help = "Service name")]
    pub service: String,

    /// Add one or more services as dependencies.
    #[arg(long, help = "Add dependencies")]
    pub add: Option<Vec<String>>,

    /// Remove one or more services from dependencies.
    #[arg(long, help = "Remove dependencies")]
    pub remove: Option<Vec<String>>,

    /// List currently configured dependencies for the service.
    #[arg(long, help = "List dependencies")]
    pub list: bool,

    /// Use `Requires` instead of `Wants` when adding dependencies.
    #[arg(long, help = "Use Requires instead of Wants")]
    pub requires: bool,
}

/// Entry point for the `deps` command.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `args` - The parsed command arguments.
pub fn run(ctx: &Context, args: DepsArgs) -> Result<()> {
    if let Some(deps) = &args.add {
        add_deps(ctx, &args.service, deps, args.requires)
    } else if let Some(deps) = &args.remove {
        remove_deps(ctx, &args.service, deps)
    } else {
        list_deps(ctx, &args.service)
    }
}

/// Normalizes a project name into a full systemd service name (e.g., `compose@myapp.service`).
fn get_compose_service_name(project: &str) -> String {
    let project = if let Some(stripped) = project.strip_suffix(".service") {
        if project.starts_with("compose@") {
            return project.to_string();
        }
        stripped
    } else {
        project
    };

    if !project.starts_with("compose@") {
        format!("compose@{}.service", project)
    } else if !project.ends_with(".service") {
        format!("{}.service", project)
    } else {
        project.to_string()
    }
}

/// Returns the path to the systemd drop-in override directory for a service.
fn get_override_dir(ctx: &Context, service: &str) -> PathBuf {
    let service_name = get_compose_service_name(service);
    ctx.systemd_dir.join(format!("{}.d", service_name))
}

/// Returns the path to the dependency configuration file within the override directory.
fn get_override_file(ctx: &Context, service: &str) -> PathBuf {
    get_override_dir(ctx, service).join("dependencies.conf")
}

/// Internal type for storing parsed systemd unit dependencies.
type SystemdDeps = HashMap<String, Vec<String>>;

/// Parses a systemd drop-in override file to extract dependency keys.
fn parse_override_file(override_file: &Path) -> Result<SystemdDeps> {
    let mut deps: SystemdDeps = HashMap::new();
    deps.insert("Requires".to_string(), Vec::new());
    deps.insert("Wants".to_string(), Vec::new());
    deps.insert("After".to_string(), Vec::new());

    if !override_file.exists() {
        return Ok(deps);
    }

    let content = fs::read_to_string(override_file)?;
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].to_string();
            continue;
        }

        if current_section != "Unit" {
            continue;
        }

        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim().to_string();
            let val = val.trim().to_string();
            if !val.is_empty() {
                deps.entry(key).or_default().push(val);
            }
        }
    }
    Ok(deps)
}

/// Writes dependency configuration to a systemd drop-in override file.
fn write_override_file(override_file: &Path, deps: &SystemdDeps) -> Result<()> {
    let mut lines = Vec::new();
    lines.push("[Unit]".to_string());

    for key in ["Requires", "Wants", "After"] {
        if let Some(values) = deps.get(key) {
            for v in values {
                lines.push(format!("{}={}", key, v));
            }
        }
    }

    fs::write(override_file, lines.join("\n") + "\n")?;
    Ok(())
}

/// Logic for the `add` action.
fn add_deps(ctx: &Context, service: &str, deps_to_add: &[String], requires: bool) -> Result<()> {
    let override_dir = get_override_dir(ctx, service);
    let override_file = get_override_file(ctx, service);

    fs::create_dir_all(&override_dir)?;

    let mut current_deps = parse_override_file(&override_file)?;
    let dep_type = if requires { "Requires" } else { "Wants" };

    for dep in deps_to_add {
        let dep_name = get_compose_service_name(dep);

        let list = current_deps.entry(dep_type.to_string()).or_default();
        if !list.contains(&dep_name) {
            list.push(dep_name.clone());
        }

        let after_list = current_deps.entry("After".to_string()).or_default();
        if !after_list.contains(&dep_name) {
            after_list.push(dep_name);
        }
    }

    write_override_file(&override_file, &current_deps)?;
    println!("Added dependencies to {}", service);

    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg("daemon-reload");
    cmd.status().context("Failed to reload daemon")?;

    Ok(())
}

/// Logic for the `remove` action.
fn remove_deps(ctx: &Context, service: &str, deps_to_remove: &[String]) -> Result<()> {
    let override_file = get_override_file(ctx, service);
    if !override_file.exists() {
        println!("No dependencies to remove");
        return Ok(());
    }

    let mut current_deps = parse_override_file(&override_file)?;

    for dep in deps_to_remove {
        let dep_name = get_compose_service_name(dep);

        for key in ["Requires", "Wants", "After"] {
            if let Some(list) = current_deps.get_mut(key) {
                if let Some(pos) = list.iter().position(|x| x == &dep_name) {
                    list.remove(pos);
                }
            }
        }
    }

    write_override_file(&override_file, &current_deps)?;
    println!("Removed dependencies from {}", service);

    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg("daemon-reload");
    cmd.status().context("Failed to reload daemon")?;

    Ok(())
}

/// Logic for the `list` action.
fn list_deps(ctx: &Context, service: &str) -> Result<()> {
    let override_file = get_override_file(ctx, service);
    if !override_file.exists() {
        println!("No explicit dependencies for {}", service);
        return Ok(());
    }

    let deps = parse_override_file(&override_file)?;
    for key in ["Requires", "Wants", "After"] {
        if let Some(v) = deps.get(key) {
            if !v.is_empty() {
                println!("{}:", key);
                for item in v {
                    println!("  - {}", item);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_get_compose_service_name() {
        assert_eq!(get_compose_service_name("myapp"), "compose@myapp.service");
        assert_eq!(
            get_compose_service_name("compose@myapp"),
            "compose@myapp.service"
        );
        assert_eq!(
            get_compose_service_name("compose@myapp.service"),
            "compose@myapp.service"
        );
        assert_eq!(
            get_compose_service_name("myapp.service"),
            "compose@myapp.service"
        );
    }

    #[test]
    fn test_parse_override_file_empty() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let deps = parse_override_file(&file).unwrap();
        assert!(deps.get("Wants").unwrap().is_empty());
    }

    #[test]
    fn test_parse_override_file_valid() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content = "[Unit]\nWants=compose@db.service\nAfter=compose@db.service\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        assert_eq!(deps.get("Wants").unwrap(), &vec!["compose@db.service"]);
        assert_eq!(deps.get("After").unwrap(), &vec!["compose@db.service"]);
    }

    #[test]
    fn test_write_override_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let mut deps: SystemdDeps = HashMap::new();
        deps.insert("Wants".to_string(), vec!["compose@db.service".to_string()]);

        write_override_file(&file, &deps).unwrap();
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("[Unit]"));
        assert!(content.contains("Wants=compose@db.service"));
    }
}
