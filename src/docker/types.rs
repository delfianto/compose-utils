use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub names: Vec<String>,
    pub image: Option<String>,
    pub created: Option<i64>,
    pub status: Option<String>,
    pub state: Option<String>,
    pub ports: Vec<PortInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortInfo {
    pub ip: Option<String>,
    pub private_port: u16,
    pub public_port: Option<u16>,
    pub type_: Option<String>,
}
