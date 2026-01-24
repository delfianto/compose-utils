//! Logic for managing filesystem artifacts (like symlinks) required for systemd integration.

use super::service::{get_bare_name, name_to_dir_path};
use crate::core::Context;
use anyhow::{bail, Context as _, Result};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

/// Determines the path where a symlink should be created for a nested service.
///
/// If a service directory is nested (e.g., `genai/ollama`), systemd's template
/// service (which doesn't support slashes in the instance name easily without
/// complex escaping) requires a flat symlink (e.g., `genai-ollama` -> `genai/ollama`)
/// in the `compose_base` directory.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `name` - The project name.
///
/// Returns [`Some(PathBuf)`] if a symlink is required, otherwise [`None`].
pub fn get_symlink_path(ctx: &Context, name: &str) -> Option<PathBuf> {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);

    if dir_path.contains('/') {
        let flat_name = bare.replace('/', "-");
        Some(ctx.compose_base.join(flat_name))
    } else {
        None
    }
}

/// Ensures that a symlink exists for nested service directories.
///
/// This allows systemd template units to find the project directory using a flat name.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `name` - The project name.
///
/// # Errors
///
/// Returns an error if:
/// - A non-symlink file already exists at the symlink path.
/// - The symlink cannot be created or updated.
pub fn ensure_symlink(ctx: &Context, name: &str) -> Result<()> {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);

    if !dir_path.contains('/') {
        return Ok(());
    }

    let flat_name = bare.replace('/', "-");
    let symlink_path = ctx.compose_base.join(&flat_name);
    let target_path = ctx.compose_base.join(&dir_path);

    if symlink_path.is_symlink() {
        let current_target = fs::read_link(&symlink_path)?;
        if current_target == target_path
            || current_target.as_path() == std::path::Path::new(&dir_path)
        {
            return Ok(());
        }
        fs::remove_file(&symlink_path)?;
    } else if symlink_path.exists() {
        bail!(
            "Cannot create symlink at {}: path already exists and is not a symlink",
            symlink_path.display()
        );
    }

    println!(
        "Creating symlink: {} -> {}",
        symlink_path.display(),
        dir_path
    );
    symlink(&dir_path, &symlink_path).with_context(|| {
        format!(
            "Failed to create symlink {} -> {}",
            symlink_path.display(),
            dir_path
        )
    })?;

    Ok(())
}

/// Removes the symlink associated with a service if it exists.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `name` - The project name.
///
/// # Errors
///
/// Returns an error if the symlink exists but cannot be removed.
pub fn remove_symlink(ctx: &Context, name: &str) -> Result<()> {
    if let Some(symlink_path) = get_symlink_path(ctx, name) {
        if symlink_path.is_symlink() {
            println!("Removing symlink: {}", symlink_path.display());
            fs::remove_file(&symlink_path)?;
        }
    }
    Ok(())
}

/// Lists dependencies for a given unit or the default target.
pub fn list_dependencies(ctx: &Context, unit: Option<&str>) -> Result<()> {
    use std::process::Command;

    let mut cmd = if ctx.is_root {
        Command::new("systemctl")
    } else {
        let mut c = Command::new("systemctl");
        c.arg("--user");
        c
    };

    cmd.arg("list-dependencies").arg("--after").arg("--reverse");

    if let Some(u) = unit {
        cmd.arg(u);
    } else {
        cmd.arg("docker.service");
    }

    let status = cmd.status()?;

    if !status.success() {
        bail!("Failed to list dependencies via systemctl");
    }

    Ok(())
}

/// Shows the status of a specific unit.
pub fn show_status(ctx: &Context, unit: &str) -> Result<()> {
    use std::process::Command;

    let mut cmd = if ctx.is_root {
        Command::new("systemctl")
    } else {
        let mut c = Command::new("systemctl");
        c.arg("--user");
        c
    };

    cmd.arg("status").arg(unit).arg("--lines=0");

    let _ = cmd.status()?;
    Ok(())
}

/// Enables a systemd unit.
pub fn enable_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "enable", Some(unit))
}

/// Disables a systemd unit.
pub fn disable_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "disable", Some(unit))
}

/// Reloads the systemd daemon.
pub fn daemon_reload(ctx: &Context) -> Result<()> {
    run_systemctl(ctx, "daemon-reload", None)
}

/// Returns the active state of a unit.
pub fn get_unit_state(ctx: &Context, unit: &str) -> Result<String> {
    use std::process::Command;

    let mut cmd = if ctx.is_root {
        Command::new("systemctl")
    } else {
        let mut c = Command::new("systemctl");
        c.arg("--user");
        c
    };

    let output = cmd
        .arg("show")
        .arg("--property=ActiveState")
        .arg("--value")
        .arg(unit)
        .output()?;

    if !output.status.success() {
        bail!("Failed to get state for {}", unit);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Starts a systemd unit.
pub fn start_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "start", Some(unit))
}

/// Stops a systemd unit.
pub fn stop_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "stop", Some(unit))
}

/// Restarts a systemd unit.
pub fn restart_unit(ctx: &Context, unit: &str) -> Result<()> {
    run_systemctl(ctx, "restart", Some(unit))
}

fn run_systemctl(ctx: &Context, action: &str, unit: Option<&str>) -> Result<()> {
    use std::process::Command;

    let mut cmd = if ctx.is_root {
        Command::new("systemctl")
    } else {
        let mut c = Command::new("systemctl");
        c.arg("--user");
        c
    };

    cmd.arg(action);
    if let Some(u) = unit {
        cmd.arg(u);
    }

    let status = cmd.status()?;

    if !status.success() {
        if let Some(u) = unit {
            bail!("Failed to {} {}: systemctl exited with error", action, u);
        } else {
            bail!("Failed to {}: systemctl exited with error", action);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Context;
    use std::path::Path;
    use tempfile::tempdir;

    fn test_context(compose_base: &Path) -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            compose_base: compose_base.to_path_buf(),
            env_file: PathBuf::from("/tmp/test.env"),
            docker_host: None,
        }
    }

    #[test]
    fn test_get_symlink_path() {
        let dir = tempdir().unwrap();
        let ctx = test_context(dir.path());

        // Flat project - no symlink needed
        assert_eq!(get_symlink_path(&ctx, "myapp"), None);

        // Nested project - symlink needed
        fs::create_dir_all(dir.path().join("genai/ollama")).unwrap();
        let path = get_symlink_path(&ctx, "genai-ollama");
        assert!(path.is_some());
        assert_eq!(path.unwrap(), dir.path().join("genai-ollama"));
    }

    #[test]
    fn test_ensure_and_remove_symlink() {
        let dir = tempdir().unwrap();
        let ctx = test_context(dir.path());

        // Ensure the nested directory exists
        let project_rel_path = "nested/project";
        let project_path = dir.path().join(project_rel_path);
        fs::create_dir_all(&project_path).unwrap();

        // Use the path name directly to ensure detection
        let service_name = "nested/project";
        let symlink_path = dir.path().join("nested-project");

        // Create symlink
        ensure_symlink(&ctx, service_name).unwrap();
        assert!(symlink_path.is_symlink());

        // Canonicalize both paths to ensure we are comparing absolute paths correctly
        let target = fs::read_link(&symlink_path).unwrap();
        let target_absolute = if target.is_absolute() {
            target
        } else {
            ctx.compose_base.join(target)
        };

        assert_eq!(
            target_absolute.canonicalize().unwrap(),
            project_path.canonicalize().unwrap()
        );

        // Remove symlink
        remove_symlink(&ctx, service_name).unwrap();
        assert!(!symlink_path.exists());
    }
}
