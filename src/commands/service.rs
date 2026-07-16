//! High-level command implementations for managing systemd compose services.

use crate::core::{Context, Report};
use crate::systemd::discovery::resolve_services;
use crate::systemd::service::{get_bare_name, get_compose_dir, normalize_unit_name};
use anyhow::Result;
use serde::Serialize;

/// Per-service result carrying the unit's `ActiveState` after an action.
#[derive(Serialize)]
struct ServiceStateResult {
    service: String,
    unit: String,
    state: String,
}

/// Per-service result for a simple action with no follow-up state check.
#[derive(Serialize)]
struct ServiceActionResult {
    service: String,
    unit: String,
    status: String,
}

/// Per-service result for the `sync` command.
#[derive(Serialize)]
struct SyncResult {
    service: String,
    unit: String,
    systemd_active: bool,
    containers_running: bool,
    action: String,
}

/// Per-service result for the `status` command.
#[derive(Serialize)]
struct StatusResult {
    service: String,
    unit: String,
    active_state: String,
    sub_state: String,
    load_state: String,
    unit_file_state: String,
    description: String,
}

/// Load and apply dependencies from a TOML file, reloading systemd if any were applied.
fn apply_deps_from_file(ctx: &Context, deps_path: &str) -> Result<()> {
    let path = std::path::Path::new(deps_path);
    let json = crate::core::is_json();

    if !json {
        println!("Loading dependencies from {}...", path.display());
    }

    let config = crate::compose::load_dependencies(path)?;
    let mut updated = false;

    for (service_name, service_config) in &config.services {
        let bare = get_bare_name(service_name);
        let dir = get_compose_dir(ctx, bare);

        if dir.exists() {
            crate::commands::deps::apply_dependencies(ctx, service_name, service_config)?;
            updated = true;
        } else {
            eprintln!(
                "Warning: Service '{}' defined in dependency file not found in projects (checked at {}).",
                service_name,
                dir.display()
            );
        }
    }

    if updated {
        crate::systemd::manager::daemon_reload(ctx)?;
    }

    Ok(())
}

/// Executes the `start` (or `up`) command with smart image pulling.
pub fn run_start(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    if let Some(path) = deps_path {
        apply_deps_from_file(ctx, &path)?;
    }

    let mut results = Vec::new();

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        if !json {
            println!("Starting {}...", unit_name);
        }
        crate::systemd::manager::start_unit(ctx, &unit_name)?;

        let state = crate::systemd::manager::get_unit_state(ctx, &unit_name)?;
        if json {
            results.push(ServiceStateResult {
                service: bare.to_string(),
                unit: unit_name,
                state,
            });
        } else {
            println!("Started {} ({})", bare, state);
        }
    }

    if json {
        crate::core::print_json(&Report {
            command: "start",
            results,
        })?;
    }

    Ok(())
}

/// Executes the `stop` (or `down`) command.
pub fn run_stop(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        if !json {
            println!("Stopping {}...", unit_name);
        }
        crate::systemd::manager::stop_unit(ctx, &unit_name)?;

        if json {
            results.push(ServiceActionResult {
                service: bare.to_string(),
                unit: unit_name,
                status: "stopped".to_string(),
            });
        } else {
            println!("Stopped {}", bare);
        }
    }

    if json {
        crate::core::print_json(&Report {
            command: "stop",
            results,
        })?;
    }

    Ok(())
}

/// Executes the `restart` (or `reup`) command.
pub fn run_restart(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        if !json {
            println!("Restarting {}...", unit_name);
        }
        crate::systemd::manager::restart_unit(ctx, &unit_name)?;

        let state = crate::systemd::manager::get_unit_state(ctx, &unit_name)?;
        if json {
            results.push(ServiceStateResult {
                service: bare.to_string(),
                unit: unit_name,
                state,
            });
        } else {
            println!("Restarted {} ({})", bare, state);
        }
    }

    if json {
        crate::core::print_json(&Report {
            command: "restart",
            results,
        })?;
    }

    Ok(())
}

/// Executes the `sync` command: reconciles systemd's tracked unit state
/// against the actual state of the containers.
///
/// `composectl start`/`stop` are no-ops when systemd already believes a unit
/// is in the target state, even if reality has drifted (e.g. someone ran
/// `docker compose up`/`down` directly, bypassing systemd). This command
/// detects that drift and corrects it:
///
/// - Systemd thinks inactive, containers are actually running -> `start`
///   the unit. `docker compose up` is idempotent, so this adopts the
///   already-running containers under systemd's supervision instead of
///   recreating them.
/// - Systemd thinks active, containers are actually gone -> `stop` the
///   unit to clear the stale state.
pub fn run_sync(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        let systemd_active = crate::systemd::manager::get_unit_state(ctx, &unit_name)? == "active";
        let actually_running = crate::commands::compose_direct::project_is_running(ctx, bare)?;

        let action = match (systemd_active, actually_running) {
            (true, true) | (false, false) => {
                if !json {
                    println!(
                        "{}: in sync ({})",
                        bare,
                        if actually_running { "up" } else { "down" }
                    );
                }
                "none"
            }
            (false, true) => {
                if !json {
                    println!(
                        "{}: drift detected (systemd down, containers up) -- adopting...",
                        bare
                    );
                }
                crate::systemd::manager::start_unit(ctx, &unit_name)?;
                if !json {
                    println!("{}: adopted under systemd supervision", bare);
                }
                "adopted"
            }
            (true, false) => {
                if !json {
                    println!(
                        "{}: drift detected (systemd up, containers down) -- resetting...",
                        bare
                    );
                }
                crate::systemd::manager::stop_unit(ctx, &unit_name)?;
                if !json {
                    println!("{}: reset to inactive", bare);
                }
                "reset"
            }
        };

        if json {
            results.push(SyncResult {
                service: bare.to_string(),
                unit: unit_name,
                systemd_active,
                containers_running: actually_running,
                action: action.to_string(),
            });
        }
    }

    if json {
        crate::core::print_json(&Report {
            command: "sync",
            results,
        })?;
    }

    Ok(())
}

/// Executes the `status` command for a set of services.
pub fn run_status(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    if services.is_empty() {
        if json {
            crate::core::print_json(&Report::<StatusResult> {
                command: "status",
                results: Vec::new(),
            })?;
        } else {
            println!("No services found.");
        }
        return Ok(());
    }

    let mut results = Vec::new();

    for name in services {
        let bare = get_bare_name(&name);
        let unit_name = normalize_unit_name(ctx, bare);

        if json {
            let props = crate::systemd::manager::get_unit_properties(ctx, &unit_name)?;
            results.push(StatusResult {
                service: bare.to_string(),
                active_state: props.get("ActiveState").cloned().unwrap_or_default(),
                sub_state: props.get("SubState").cloned().unwrap_or_default(),
                load_state: props.get("LoadState").cloned().unwrap_or_default(),
                unit_file_state: props.get("UnitFileState").cloned().unwrap_or_default(),
                description: props.get("Description").cloned().unwrap_or_default(),
                unit: unit_name,
            });
        } else {
            crate::systemd::manager::show_status(ctx, &unit_name)?;
            println!();
        }
    }

    if json {
        crate::core::print_json(&Report {
            command: "status",
            results,
        })?;
    }

    Ok(())
}

/// Executes the `enable` command.
pub fn run_enable(ctx: &Context, names: &[String], deps_path: Option<String>) -> Result<()> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    if let Some(path) = deps_path {
        apply_deps_from_file(ctx, &path)?;
    }

    let mut results = Vec::new();

    for name in &services {
        let bare = get_bare_name(name);
        let unit_name = normalize_unit_name(ctx, bare);

        if !json {
            println!("Enabling {}...", unit_name);
        }
        crate::systemd::manager::enable_unit(ctx, &unit_name)?;

        if json {
            results.push(ServiceActionResult {
                service: bare.to_string(),
                unit: unit_name,
                status: "enabled".to_string(),
            });
        }
    }

    if json {
        crate::core::print_json(&Report {
            command: "enable",
            results,
        })?;
    }

    Ok(())
}

/// Executes the `disable` command.
pub fn run_disable(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;
    let json = crate::core::is_json();

    let mut results = Vec::new();

    for name in &services {
        let bare = get_bare_name(name);
        let unit_name = normalize_unit_name(ctx, bare);

        if !json {
            println!("Disabling {}...", unit_name);
        }
        crate::systemd::manager::disable_unit(ctx, &unit_name)?;

        if json {
            results.push(ServiceActionResult {
                service: bare.to_string(),
                unit: unit_name,
                status: "disabled".to_string(),
            });
        }
    }

    if json {
        crate::core::print_json(&Report {
            command: "disable",
            results,
        })?;
    }

    Ok(())
}
