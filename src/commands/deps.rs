//! Logic for managing dependencies between systemd services using drop-in overrides.

use crate::core::Context;
use anyhow::Result;
use clap::{Args, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Command-line arguments for the `deps` subcommand.
#[derive(Args)]
pub struct DepsArgs {
    #[command(subcommand)]
    pub action: DepsAction,
}

#[derive(Subcommand)]
pub enum DepsAction {
    /// List dependencies for a service, or all services if none specified.
    List {
        /// Service name (omit to list all).
        service: Option<String>,
    },
    /// Add one or more dependencies to a service.
    Add {
        /// The service to modify.
        service: String,
        /// Dependencies to add.
        #[arg(required = true)]
        deps: Vec<String>,
        /// Use Requires instead of Wants.
        #[arg(long)]
        requires: bool,
    },
    /// Remove one or more dependencies from a service.
    Remove {
        /// The service to modify.
        service: String,
        /// Dependencies to remove.
        #[arg(required = true)]
        deps: Vec<String>,
    },
    /// Clear all dependencies for a service.
    Clear {
        /// The service to clear.
        service: String,
    },
}

/// Entry point for the `deps` command.
pub fn run(ctx: &Context, args: DepsArgs) -> Result<()> {
    match args.action {
        DepsAction::List { service } => match service {
            Some(service) => list_deps(ctx, &service),
            None => list_all_deps(ctx),
        },
        DepsAction::Add {
            service,
            deps,
            requires,
        } => add_deps(ctx, &service, &deps, requires),
        DepsAction::Remove { service, deps } => remove_deps(ctx, &service, &deps),
        DepsAction::Clear { service } => clear_deps(ctx, &service),
    }
}

fn clear_deps(ctx: &Context, service: &str) -> Result<()> {
    let override_file = get_override_file(ctx, service);
    let json = crate::core::is_json();

    if override_file.exists() {
        if !json {
            println!("Clearing dependencies for {}...", service);
        }
        fs::remove_file(&override_file)?;
        crate::systemd::manager::daemon_reload(ctx)?;

        if json {
            crate::core::print_json(&serde_json::json!({
                "command": "deps", "action": "clear", "service": service, "status": "cleared",
            }))?;
        }
    } else if json {
        crate::core::print_json(&serde_json::json!({
            "command": "deps", "action": "clear", "service": service, "status": "noop",
        }))?;
    } else {
        println!("No dependencies found for {}", service);
    }
    Ok(())
}

fn list_all_deps(ctx: &Context) -> Result<()> {
    if crate::core::is_json() {
        let graph = build_dependency_graph(ctx, "docker.service")?;
        crate::core::print_json(&serde_json::json!({
            "command": "deps",
            "action": "list",
            "edges": graph.edges,
            "states": graph.states,
        }))?;
        return Ok(());
    }
    crate::systemd::manager::list_dependencies(ctx, None)
}

/// A flat reverse-dependency graph rooted at some unit (see
/// [`build_dependency_graph`]).
struct DependencyGraph {
    /// unit -> its direct reverse-dependents (units that require/want it).
    edges: std::collections::BTreeMap<String, Vec<String>>,
    /// unit -> its current `ActiveState`.
    states: std::collections::BTreeMap<String, String>,
}

/// Returns true if `unit` is one of this tool's own `compose@*.service` units.
fn is_compose_unit(unit: &str) -> bool {
    unit.starts_with("compose@") && unit.ends_with(".service")
}

/// Builds the reverse-dependency graph rooted at `root`, following the same
/// edges `systemctl list-dependencies --reverse` would (`RequiredBy=`,
/// `WantedBy=`, `UpheldBy=`, `PartOf=`, `BoundBy=`), fetched via `systemctl
/// show` -- systemd's documented machine-parsable interface -- rather than
/// by parsing that command's human-oriented tree/bullet rendering.
///
/// Two departures from a literal port of that tree:
/// - The result is a flat `unit -> [dependents]` map, not a nested tree,
///   since the underlying relationship is a DAG: a unit like a shared
///   database can legitimately have more than one dependent, and forcing
///   that into a tree means duplicating it under every parent.
/// - Traversal only follows into `compose@*.service` units. Every unit here
///   is also implicitly `WantedBy=default.target` (that's just what "enabled"
///   means) and `Requires=`/`BindsTo=docker.service`, so leaving those in
///   would add a `default.target` (and `docker.service`) leaf under nearly
///   every node without conveying any information beyond "this is enabled".
///   Only edges between the compose services this tool actually manages are
///   kept.
fn build_dependency_graph(ctx: &Context, root: &str) -> Result<DependencyGraph> {
    let mut edges = std::collections::BTreeMap::new();
    let mut states = std::collections::BTreeMap::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    visited.insert(root.to_string());
    queue.push_back(root.to_string());

    while let Some(unit) = queue.pop_front() {
        let props = crate::systemd::manager::get_unit_properties(ctx, &unit)?;
        states.insert(
            unit.clone(),
            props.get("ActiveState").cloned().unwrap_or_else(|| "unknown".to_string()),
        );

        let mut dependents: Vec<String> = crate::systemd::manager::get_reverse_dependents(ctx, &unit)?
            .into_iter()
            .filter(|u| is_compose_unit(u))
            .collect();
        dependents.sort();

        for dep in &dependents {
            if visited.insert(dep.clone()) {
                queue.push_back(dep.clone());
            }
        }

        edges.insert(unit, dependents);
    }

    Ok(DependencyGraph { edges, states })
}

/// Returns the path to the systemd drop-in override directory for a service.
fn get_override_dir(ctx: &Context, service: &str) -> PathBuf {
    let service_name = crate::systemd::service::normalize_unit_name(ctx, service);
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
    deps.insert("BindsTo".to_string(), Vec::new());
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

    for key in ["Requires", "Wants", "BindsTo", "After"] {
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
        let dep_name = crate::systemd::service::normalize_unit_name(ctx, dep);

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

    if crate::core::is_json() {
        crate::core::print_json(&serde_json::json!({
            "command": "deps",
            "action": "add",
            "service": service,
            "added": deps_to_add,
            "type": dep_type,
        }))?;
    } else {
        println!("Added dependencies to {}", service);
    }

    crate::systemd::manager::daemon_reload(ctx)?;

    Ok(())
}

/// Applies the dependency configuration to a service.
///
/// This updates the systemd override file to match the provided configuration.
/// Existing dependencies not specified in the config are preserved, unless
/// there is a conflict (which currently shouldn't happen as we just append).
pub fn apply_dependencies(
    ctx: &Context,
    service: &str,
    config: &crate::compose::ServiceConfig,
) -> Result<()> {
    let override_dir = get_override_dir(ctx, service);
    let override_file = get_override_file(ctx, service);

    fs::create_dir_all(&override_dir)?;

    // Start with a clean slate to avoid stale dependencies
    let mut current_deps: SystemdDeps = HashMap::new();
    current_deps.insert("Requires".to_string(), Vec::new());
    current_deps.insert("Wants".to_string(), Vec::new());
    current_deps.insert("BindsTo".to_string(), Vec::new());
    current_deps.insert("After".to_string(), Vec::new());

    // Standard dependencies that always exist for every compose service
    let standard_deps = vec!["docker.service".to_string()];
    update_deps_list(ctx, &mut current_deps, "Requires", &standard_deps);
    update_deps_list(ctx, &mut current_deps, "BindsTo", &standard_deps);

    if let Some(requires) = &config.requires {
        update_deps_list(ctx, &mut current_deps, "Requires", requires);

        // If BindsTo is not explicitly provided, default it to the same as Requires
        if config.binds_to.is_none() {
            update_deps_list(ctx, &mut current_deps, "BindsTo", requires);
        }
    }

    if let Some(wants) = &config.wants {
        update_deps_list(ctx, &mut current_deps, "Wants", wants);
    }

    if let Some(binds) = &config.binds_to {
        update_deps_list(ctx, &mut current_deps, "BindsTo", binds);
    }

    // Process explicit After.
    if let Some(after) = &config.after {
        for item in after {
            let name = crate::systemd::service::normalize_unit_name(ctx, item);
            let list = current_deps.entry("After".to_string()).or_default();
            if !list.contains(&name) {
                list.push(name);
            }
        }
    }

    write_override_file(&override_file, &current_deps)?;
    Ok(())
}

fn update_deps_list(ctx: &Context, deps: &mut SystemdDeps, key: &str, items: &[String]) {
    for item in items {
        let name = crate::systemd::service::normalize_unit_name(ctx, item);

        let list = deps.entry(key.to_string()).or_default();
        if !list.contains(&name) {
            list.push(name.clone());
        }

        let after_list = deps.entry("After".to_string()).or_default();
        if !after_list.contains(&name) {
            after_list.push(name);
        }
    }
}

/// Logic for the `remove` action.
fn remove_deps(ctx: &Context, service: &str, deps_to_remove: &[String]) -> Result<()> {
    let override_file = get_override_file(ctx, service);
    let json = crate::core::is_json();

    if !override_file.exists() {
        if json {
            crate::core::print_json(&serde_json::json!({
                "command": "deps", "action": "remove", "service": service, "status": "noop",
            }))?;
        } else {
            println!("No dependencies to remove");
        }
        return Ok(());
    }

    let mut current_deps = parse_override_file(&override_file)?;

    for dep in deps_to_remove {
        let dep_name = crate::systemd::service::normalize_unit_name(ctx, dep);

        for key in ["Requires", "Wants", "After"] {
            if let Some(list) = current_deps.get_mut(key) {
                if let Some(pos) = list.iter().position(|x| x == &dep_name) {
                    list.remove(pos);
                }
            }
        }
    }

    write_override_file(&override_file, &current_deps)?;

    if json {
        crate::core::print_json(&serde_json::json!({
            "command": "deps",
            "action": "remove",
            "service": service,
            "status": "removed",
            "removed": deps_to_remove,
        }))?;
    } else {
        println!("Removed dependencies from {}", service);
    }

    crate::systemd::manager::daemon_reload(ctx)?;

    Ok(())
}

/// Logic for the `list` action.
fn list_deps(ctx: &Context, service: &str) -> Result<()> {
    let service_name = crate::systemd::service::normalize_unit_name(ctx, service);
    let json = crate::core::is_json();

    let graph = if json {
        Some(build_dependency_graph(ctx, &service_name)?)
    } else {
        println!("Dependency tree for {}:", service_name);
        if let Err(e) = crate::systemd::manager::list_dependencies(ctx, Some(&service_name)) {
            println!("  Failed to retrieve dependency tree: {}", e);
        }
        None
    };

    // Also show explicit overrides if they exist
    let override_file = get_override_file(ctx, service);
    let overrides = if override_file.exists() {
        Some(parse_override_file(&override_file)?)
    } else {
        None
    };

    if json {
        let graph = graph.expect("graph is always built in JSON mode");
        crate::core::print_json(&serde_json::json!({
            "command": "deps",
            "action": "list",
            "service": service_name,
            "edges": graph.edges,
            "states": graph.states,
            "overrides": overrides,
        }))?;
        return Ok(());
    }

    if let Some(deps) = overrides {
        println!("\nExplicit overrides in {}:", override_file.display());
        for key in ["Requires", "Wants", "After"] {
            if let Some(v) = deps.get(key) {
                if !v.is_empty() {
                    println!("  {}:", key);
                    for item in v {
                        println!("    - {}", item);
                    }
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
    fn test_normalize_unit_name() {
        use crate::systemd::service::normalize_unit_name;
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("myapp")).unwrap();

        assert_eq!(normalize_unit_name(&ctx, "myapp"), "compose@myapp.service");
        assert_eq!(
            normalize_unit_name(&ctx, "compose@myapp"),
            "compose@myapp.service"
        );
        assert_eq!(
            normalize_unit_name(&ctx, "compose@myapp.service"),
            "compose@myapp.service"
        );
        // Standard services should not be prefixed
        assert_eq!(
            normalize_unit_name(&ctx, "docker.service"),
            "docker.service"
        );
        assert_eq!(
            normalize_unit_name(&ctx, "network.target"),
            "network.target"
        );
        assert_eq!(normalize_unit_name(&ctx, "dbus.socket"), "dbus.socket");
        assert_eq!(
            normalize_unit_name(&ctx, "user@1000.service"),
            "user@1000.service"
        );
        // Fallback for bare "docker"
        assert_eq!(normalize_unit_name(&ctx, "docker"), "docker.service");
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

    #[test]
    fn test_apply_dependencies_creates_file() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };

        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("pgvector")).unwrap();
        fs::create_dir_all(ctx.compose_base.join("ollama")).unwrap();

        let config = crate::compose::ServiceConfig {
            requires: Some(vec!["pgvector".to_string(), "docker.service".to_string()]),
            wants: Some(vec!["ollama".to_string()]),
            binds_to: None,
            after: None,
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        assert!(override_file.exists());

        let content = fs::read_to_string(&override_file).unwrap();
        assert!(content.contains("Requires="));
        assert!(content.contains("compose@pgvector.service"));
        assert!(content.contains("docker.service"));
        assert!(content.contains("Wants="));
        assert!(content.contains("compose@ollama.service"));

        assert!(content.contains("After="));
        assert!(content.contains("compose@pgvector.service"));
        assert!(content.contains("docker.service"));
        assert!(content.contains("compose@ollama.service"));
    }

    #[test]
    fn test_apply_dependencies_overwrites() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("app1")).unwrap();
        fs::create_dir_all(ctx.compose_base.join("app2")).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        fs::create_dir_all(override_file.parent().unwrap()).unwrap();
        fs::write(
            &override_file,
            "[Unit]\nRequires=stale.service\nAfter=stale.service\n",
        )
        .unwrap();

        let config = crate::compose::ServiceConfig {
            requires: Some(vec!["app1".to_string()]),
            wants: None,
            binds_to: None,
            after: Some(vec!["app2".to_string()]),
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let content = fs::read_to_string(&override_file).unwrap();
        assert!(content.contains("Requires="));
        assert!(content.contains("compose@app1.service"));
        assert!(content.contains("After="));
        assert!(content.contains("compose@app2.service"));
        assert!(!content.contains("stale.service"));
    }

    #[test]
    fn test_parse_override_file_ignores_non_unit_section() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content = "[Service]\nRestart=on-failure\n\n[Unit]\nWants=compose@db.service\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        assert_eq!(deps.get("Wants").unwrap(), &vec!["compose@db.service"]);
        // Restart is not a Unit key and is in [Service], should be ignored
        assert!(!deps.contains_key("Restart"));
    }

    #[test]
    fn test_parse_override_file_skips_comments() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content = "[Unit]\n# This is a comment\nWants=compose@db.service\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        assert_eq!(deps.get("Wants").unwrap(), &vec!["compose@db.service"]);
    }

    #[test]
    fn test_parse_override_file_empty_values() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content = "[Unit]\nWants=\nAfter=compose@db.service\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        // Empty value should be skipped
        assert!(deps.get("Wants").unwrap().is_empty());
        assert_eq!(deps.get("After").unwrap(), &vec!["compose@db.service"]);
    }

    #[test]
    fn test_parse_override_file_multiple_values_same_key() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content = "[Unit]\nWants=compose@db.service\nWants=compose@cache.service\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        let wants = deps.get("Wants").unwrap();
        assert_eq!(wants.len(), 2);
        assert!(wants.contains(&"compose@db.service".to_string()));
        assert!(wants.contains(&"compose@cache.service".to_string()));
    }

    #[test]
    fn test_write_then_parse_roundtrip() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");

        let mut deps: SystemdDeps = HashMap::new();
        deps.insert(
            "Requires".to_string(),
            vec!["compose@db.service".to_string()],
        );
        deps.insert(
            "Wants".to_string(),
            vec![
                "compose@cache.service".to_string(),
                "compose@queue.service".to_string(),
            ],
        );
        deps.insert("BindsTo".to_string(), vec!["docker.service".to_string()]);
        deps.insert(
            "After".to_string(),
            vec![
                "compose@db.service".to_string(),
                "docker.service".to_string(),
            ],
        );

        write_override_file(&file, &deps).unwrap();
        let parsed = parse_override_file(&file).unwrap();

        assert_eq!(parsed.get("Requires").unwrap(), &vec!["compose@db.service"]);
        assert_eq!(parsed.get("Wants").unwrap().len(), 2);
        assert_eq!(parsed.get("BindsTo").unwrap(), &vec!["docker.service"]);
        assert_eq!(parsed.get("After").unwrap().len(), 2);
    }

    #[test]
    fn test_write_override_file_empty_deps() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let deps: SystemdDeps = HashMap::new();

        write_override_file(&file, &deps).unwrap();
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("[Unit]"));
        // No key=value lines besides [Unit]
        assert_eq!(content.trim(), "[Unit]");
    }

    #[test]
    fn test_apply_dependencies_all_none() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();

        let config = crate::compose::ServiceConfig {
            requires: None,
            wants: None,
            binds_to: None,
            after: None,
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        assert!(override_file.exists());

        let content = fs::read_to_string(&override_file).unwrap();
        // Should still have docker.service as a standard dependency
        assert!(content.contains("docker.service"));
    }

    #[test]
    fn test_apply_dependencies_binds_to_explicit() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("custom")).unwrap();

        let config = crate::compose::ServiceConfig {
            requires: Some(vec!["docker.service".to_string()]),
            wants: None,
            binds_to: Some(vec!["custom".to_string()]),
            after: None,
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        let content = fs::read_to_string(&override_file).unwrap();
        // When binds_to is explicitly set, it should use that instead of defaulting to requires
        assert!(content.contains("BindsTo="));
        assert!(content.contains("compose@custom.service"));
    }

    #[test]
    fn test_get_override_dir_path() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("myapp")).unwrap();

        let override_dir = get_override_dir(&ctx, "myapp");
        assert!(
            override_dir
                .to_string_lossy()
                .contains("compose@myapp.service.d")
        );
    }

    #[test]
    fn test_parse_override_file_no_equals_line() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content = "[Unit]\nSomeGarbageLine\nWants=compose@db.service\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        // The garbage line should be ignored, valid line should parse
        assert_eq!(deps.get("Wants").unwrap(), &vec!["compose@db.service"]);
    }

    // --- apply_dependencies corner cases ---

    #[test]
    fn test_apply_dependencies_all_fields_populated() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("db")).unwrap();
        fs::create_dir_all(ctx.compose_base.join("cache")).unwrap();
        fs::create_dir_all(ctx.compose_base.join("queue")).unwrap();
        fs::create_dir_all(ctx.compose_base.join("monitor")).unwrap();

        let config = crate::compose::ServiceConfig {
            requires: Some(vec!["db".to_string()]),
            wants: Some(vec!["cache".to_string()]),
            binds_to: Some(vec!["queue".to_string()]),
            after: Some(vec!["monitor".to_string()]),
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        let content = fs::read_to_string(&override_file).unwrap();

        assert!(content.contains("Requires=compose@db.service"));
        assert!(content.contains("Requires=docker.service"));
        assert!(content.contains("Wants=compose@cache.service"));
        assert!(content.contains("BindsTo=compose@queue.service"));
        assert!(content.contains("After=compose@monitor.service"));
        // When binds_to is explicitly set, requires should NOT be copied to binds_to
        assert!(!content.contains("BindsTo=compose@db.service"));
    }

    #[test]
    fn test_apply_dependencies_requires_duplicates_standard() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();

        // docker.service is both a standard dep and explicitly required
        let config = crate::compose::ServiceConfig {
            requires: Some(vec!["docker.service".to_string()]),
            wants: None,
            binds_to: None,
            after: None,
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        let content = fs::read_to_string(&override_file).unwrap();

        // docker.service should appear exactly once in Requires
        let requires_count = content.matches("Requires=docker.service").count();
        assert_eq!(requires_count, 1, "docker.service should not be duplicated");
    }

    #[test]
    fn test_apply_dependencies_defaults_binds_to_from_requires() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("db")).unwrap();

        let config = crate::compose::ServiceConfig {
            requires: Some(vec!["db".to_string()]),
            wants: None,
            binds_to: None, // Not set, should default from requires
            after: None,
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        let content = fs::read_to_string(&override_file).unwrap();

        // BindsTo should include db since binds_to was None
        assert!(content.contains("BindsTo=compose@db.service"));
    }

    // --- parse_override_file corner cases ---

    #[test]
    fn test_parse_override_file_unknown_keys_in_unit() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content =
            "[Unit]\nDescription=My Service\nWants=compose@db.service\nConditionPathExists=/tmp\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        assert_eq!(deps.get("Wants").unwrap(), &vec!["compose@db.service"]);
        // Unknown keys should be stored too (entry().or_default())
        assert_eq!(
            deps.get("Description").unwrap(),
            &vec!["My Service".to_string()]
        );
        assert_eq!(
            deps.get("ConditionPathExists").unwrap(),
            &vec!["/tmp".to_string()]
        );
    }

    #[test]
    fn test_parse_override_file_all_four_keys() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let content = "[Unit]\n\
                        Requires=docker.service\n\
                        Wants=compose@cache.service\n\
                        BindsTo=compose@db.service\n\
                        After=docker.service\n\
                        After=compose@db.service\n";
        fs::write(&file, content).unwrap();

        let deps = parse_override_file(&file).unwrap();
        assert_eq!(
            deps.get("Requires").unwrap(),
            &vec!["docker.service".to_string()]
        );
        assert_eq!(
            deps.get("Wants").unwrap(),
            &vec!["compose@cache.service".to_string()]
        );
        assert_eq!(
            deps.get("BindsTo").unwrap(),
            &vec!["compose@db.service".to_string()]
        );
        assert_eq!(deps.get("After").unwrap().len(), 2);
    }

    // --- write_override_file corner cases ---

    #[test]
    fn test_write_override_file_all_four_keys() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let mut deps: SystemdDeps = HashMap::new();
        deps.insert(
            "Requires".to_string(),
            vec!["docker.service".to_string()],
        );
        deps.insert(
            "Wants".to_string(),
            vec!["compose@cache.service".to_string()],
        );
        deps.insert(
            "BindsTo".to_string(),
            vec!["compose@db.service".to_string()],
        );
        deps.insert(
            "After".to_string(),
            vec![
                "docker.service".to_string(),
                "compose@db.service".to_string(),
            ],
        );

        write_override_file(&file, &deps).unwrap();
        let content = fs::read_to_string(&file).unwrap();

        // Verify order: Requires, Wants, BindsTo, After
        let req_pos = content.find("Requires=").unwrap();
        let wants_pos = content.find("Wants=").unwrap();
        let binds_pos = content.find("BindsTo=").unwrap();
        let after_pos = content.find("After=").unwrap();
        assert!(req_pos < wants_pos);
        assert!(wants_pos < binds_pos);
        assert!(binds_pos < after_pos);
    }

    #[test]
    fn test_write_override_file_only_after() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.conf");
        let mut deps: SystemdDeps = HashMap::new();
        deps.insert("Requires".to_string(), Vec::new());
        deps.insert("Wants".to_string(), Vec::new());
        deps.insert("BindsTo".to_string(), Vec::new());
        deps.insert(
            "After".to_string(),
            vec!["docker.service".to_string()],
        );

        write_override_file(&file, &deps).unwrap();
        let content = fs::read_to_string(&file).unwrap();

        assert!(content.contains("[Unit]"));
        assert!(content.contains("After=docker.service"));
        assert!(!content.contains("Requires="));
        assert!(!content.contains("Wants="));
        assert!(!content.contains("BindsTo="));
    }

    // --- get_override_file path tests ---

    #[test]
    fn test_get_override_file_path() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("myapp")).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        assert!(
            override_file
                .to_string_lossy()
                .ends_with("compose@myapp.service.d/dependencies.conf")
        );
    }

    #[test]
    fn test_apply_dependencies_only_wants() {
        let dir = tempdir().unwrap();
        let ctx = Context {
            is_root: false,
            systemd_dir: dir.path().to_path_buf(),
            compose_base: dir.path().join("compose"),
            env_file: dir.path().join("env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        };
        fs::create_dir_all(&ctx.compose_base).unwrap();
        fs::create_dir_all(ctx.compose_base.join("optional-svc")).unwrap();

        let config = crate::compose::ServiceConfig {
            requires: None,
            wants: Some(vec!["optional-svc".to_string()]),
            binds_to: None,
            after: None,
        };

        apply_dependencies(&ctx, "myapp", &config).unwrap();

        let override_file = get_override_file(&ctx, "myapp");
        let content = fs::read_to_string(&override_file).unwrap();

        assert!(content.contains("Wants=compose@optional-svc.service"));
        // Standard deps still present
        assert!(content.contains("Requires=docker.service"));
        assert!(content.contains("BindsTo=docker.service"));
    }
}
