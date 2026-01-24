//! Logic for retrieving and processing Docker container information.

use super::types::{ContainerInfo, PortInfo};
use anyhow::{Context, Result};
use bollard::query_parameters::ListContainersOptions;
use bollard::Docker;

/// Retrieves a list of all Docker containers and transforms them into domain types.
///
/// This function queries the Docker API for all containers (including stopped ones)
/// and maps the result into a vector of [`ContainerInfo`].
///
/// # Arguments
///
/// * `docker` - A reference to the initialized [`Docker`] client.
///
/// # Errors
///
/// Returns an error if the Docker API call fails.
pub async fn list_containers(docker: &Docker) -> Result<Vec<ContainerInfo>> {
    let options = ListContainersOptions {
        all: true,
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(options))
        .await
        .context("Failed to list containers via Docker API")?;

    let mut result = Vec::new();

    for c in containers {
        let ports = c
            .ports
            .map(|ports| {
                ports
                    .into_iter()
                    .map(|p| PortInfo {
                        ip: p.ip,
                        private_port: p.private_port,
                        public_port: p.public_port,
                        type_: p.typ.map(|t| format!("{:?}", t).to_lowercase()),
                    })
                    .collect()
            })
            .unwrap_or_default();

        result.push(ContainerInfo {
            id: c.id.unwrap_or_else(|| "<unknown>".to_string()),
            names: c.names.unwrap_or_default(),
            image: c.image,
            created: c.created,
            status: c.status,
            state: c.state.map(|s| format!("{:?}", s).to_lowercase()),
            ports,
        });
    }

    Ok(result)
}
