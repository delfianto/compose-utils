use serde::{Deserialize, Serialize};

/// Represents basic information about a Docker container.
///
/// This is a domain-specific representation decoupled from the underlying
/// Docker API library types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    /// The unique identifier of the container (ID).
    pub id: String,
    /// List of names assigned to the container.
    pub names: Vec<String>,
    /// The name of the image the container is running.
    pub image: Option<String>,
    /// Creation timestamp of the container (Unix epoch).
    pub created: Option<i64>,
    /// Current status of the container (e.g., "Up 2 hours", "Exited (0)").
    pub status: Option<String>,
    /// Current state of the container (e.g., "running", "exited").
    pub state: Option<String>,
    /// List of port mappings for the container.
    pub ports: Vec<PortInfo>,
}

/// Represents a port mapping for a Docker container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortInfo {
    /// IP address on the host (if mapped).
    pub ip: Option<String>,
    /// The internal port used by the container.
    pub private_port: u16,
    /// The external port mapped on the host (if mapped).
    pub public_port: Option<u16>,
    /// The type of protocol (e.g., "tcp", "udp").
    pub type_: Option<String>,
}
