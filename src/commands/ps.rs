//! Logic for the `ps` command.

use crate::core::Context;
use anyhow::{Context as _, Result};
use std::process::Command;

/// Executes the `ps` command to list Docker containers.
///
/// This function calls `docker ps -a` directly, delegating the output to the Docker CLI.
///
/// # Arguments
///
/// * `_ctx` - The application context (unused for now).
/// * `_services` - Currently ignored (reserved for future filtering).
pub async fn run_ps(_ctx: &Context, _services: &[String]) -> Result<()> {
    let status = Command::new("docker")
        .arg("ps")
        .arg("-a")
        .status()
        .context("Failed to execute docker ps")?;

    if !status.success() {
        eprintln!("docker ps exited with non-zero status");
    }

    Ok(())
}