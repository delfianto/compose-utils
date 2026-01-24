//! Logic for managing filesystem artifacts (like symlinks) required for systemd integration.

use super::service::{get_bare_name, name_to_dir_path};
use crate::core::Context;
use anyhow::{Context as _, Result, bail};
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
            systemctl_cmd: vec!["systemctl".to_string()],
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