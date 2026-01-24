//! Logic for the `ps` command.

use crate::core::Context;
use anyhow::Result;

/// Executes the `ps` command to list Docker containers.
///
/// This function:
/// 1. Connects to the Docker daemon.
/// 2. Retrieves a list of all containers.
/// 3. Renders the information in a table format.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `_services` - Currently ignored (reserved for future filtering).
///
/// # Errors
///
/// Returns an error if the Docker connection or container retrieval fails.
pub async fn run_ps(ctx: &Context, _services: &[String]) -> Result<()> {
    let docker = crate::docker::connect_docker(ctx)?;
    let containers = crate::docker::containers::list_containers(&docker).await?;
    crate::display::table::render_containers_table(containers);
    Ok(())
}
