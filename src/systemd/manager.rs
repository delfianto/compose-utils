use super::service::{get_bare_name, name_to_dir_path};
use crate::core::Context;
use anyhow::{Context as _, Result, bail};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

/// Get the symlink path for a service (used when directory is nested).
/// Returns the path where a symlink should be created to map the flat name
/// to the nested directory structure.
pub fn get_symlink_path(ctx: &Context, name: &str) -> Option<PathBuf> {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);

    // If the directory path contains a slash, it's nested
    // and we need a symlink from the flat name to the nested path
    if dir_path.contains('/') {
        let flat_name = bare.replace('/', "-");
        Some(ctx.compose_base.join(flat_name))
    } else {
        None
    }
}

/// Create a symlink for nested directories so systemd can find them.
pub fn ensure_symlink(ctx: &Context, name: &str) -> Result<()> {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);

    // Only needed for nested directories
    if !dir_path.contains('/') {
        return Ok(());
    }

    let flat_name = bare.replace('/', "-");
    let symlink_path = ctx.compose_base.join(&flat_name);
    let target_path = ctx.compose_base.join(&dir_path);

    // If symlink already exists and points to the right place, we're done
    if symlink_path.is_symlink() {
        let current_target = fs::read_link(&symlink_path)?;
        if current_target == target_path
            || current_target.as_path() == std::path::Path::new(&dir_path)
        {
            return Ok(());
        }
        // Wrong target, remove and recreate
        fs::remove_file(&symlink_path)?;
    } else if symlink_path.exists() {
        // Something else exists at this path
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

/// Remove symlink for a service if it exists.
pub fn remove_symlink(ctx: &Context, name: &str) -> Result<()> {
    if let Some(symlink_path) = get_symlink_path(ctx, name)
        && symlink_path.is_symlink()
    {
        println!("Removing symlink: {}", symlink_path.display());
        fs::remove_file(&symlink_path)?;
    }
    Ok(())
}
