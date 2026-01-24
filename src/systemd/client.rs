//! High-level client for interacting with systemd via D-Bus.

use super::dbus::{connect, manager::ManagerProxy, types::*};
use anyhow::Result;

/// High-level client for systemd operations.
pub struct SystemdClient {
    connection: zbus::Connection,
}

/// Information about a systemd unit.
#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub name: String,
    pub description: String,
    pub active_state: UnitState,
    pub sub_state: SubState,
}

impl SystemdClient {
    /// Creates a new [`SystemdClient`].
    ///
    /// # Arguments
    ///
    /// * `user_mode` - If true, connects to the session bus; otherwise, connects to the system bus.
    pub async fn new(user_mode: bool) -> Result<Self> {
        let connection = connect(user_mode).await?;
        Ok(Self { connection })
    }

    /// Lists units matching an optional pattern.
    pub async fn list_units(&self, pattern: Option<&str>) -> Result<Vec<UnitInfo>> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        let units = proxy.list_units().await?;

        let result: Vec<UnitInfo> = units
            .into_iter()
            .filter(|(name, ..)| pattern.is_none_or(|p| name.contains(p)))
            .map(
                |(name, description, _load_state, active_state, sub_state, ..)| UnitInfo {
                    name,
                    description,
                    active_state: UnitState::from_dbus(&active_state),
                    sub_state: SubState::from_dbus(&sub_state),
                },
            )
            .collect();

        Ok(result)
    }
}
