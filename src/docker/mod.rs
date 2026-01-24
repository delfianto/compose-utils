pub mod containers;
pub mod images;
pub mod types;

use crate::core::Context;
use anyhow::{Context as _, Result, bail};
use bollard::Docker;

/// Connect to Docker using the configured DOCKER_HOST from compose.env
pub fn connect_docker(ctx: &Context) -> Result<Docker> {
    match &ctx.docker_host {
        Some(host) => {
            if let Some(socket_path) = host.strip_prefix("unix://") {
                Docker::connect_with_unix(socket_path, 120, bollard::API_DEFAULT_VERSION)
                    .with_context(|| format!("Failed to connect to Docker socket: {}", socket_path))
            } else if host.starts_with("tcp://") {
                Docker::connect_with_http(host, 120, bollard::API_DEFAULT_VERSION)
                    .with_context(|| format!("Failed to connect to Docker via TCP: {}", host))
            } else {
                bail!(
                    "Unsupported DOCKER_HOST format: {}. Use unix:// or tcp://",
                    host
                )
            }
        }
        None => Docker::connect_with_socket_defaults()
            .context("Failed to connect to Docker API (no DOCKER_HOST configured)"),
    }
}
