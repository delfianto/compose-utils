//! High-level client for interacting with systemd via D-Bus.

use super::dbus::{connect, manager::ManagerProxy, types::*, unit::UnitProxy};
use anyhow::{Context as _, Result};
use futures_util::StreamExt;

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

/// Detailed properties of a systemd unit.
#[derive(Debug, Clone)]
pub struct UnitProperties {
    pub state: UnitState,
    pub sub_state: SubState,
    pub description: String,
    pub requires: Vec<String>,
    pub wants: Vec<String>,
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

    /// Starts a systemd unit.
    pub async fn start_unit(&self, name: &str) -> Result<JobId> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        let job_path = proxy
            .start_unit(name, "replace")
            .await
            .with_context(|| format!("Failed to start {}", name))?;
        Ok(JobId::from_path(&job_path))
    }

    /// Stops a systemd unit.
    pub async fn stop_unit(&self, name: &str) -> Result<JobId> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        let job_path = proxy
            .stop_unit(name, "replace")
            .await
            .with_context(|| format!("Failed to stop {}", name))?;
        Ok(JobId::from_path(&job_path))
    }

    /// Restarts a systemd unit.
    pub async fn restart_unit(&self, name: &str) -> Result<JobId> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        let job_path = proxy
            .restart_unit(name, "replace")
            .await
            .with_context(|| format!("Failed to restart {}", name))?;
        Ok(JobId::from_path(&job_path))
    }

    /// Reloads the systemd daemon (equivalent to `systemctl daemon-reload`).
    pub async fn reload_daemon(&self) -> Result<()> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        proxy.reload().await.context("Failed to reload daemon")
    }

    /// Gets the high-level state of a unit.
    pub async fn get_unit_state(&self, name: &str) -> Result<UnitState> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        let unit_path = proxy.get_unit(name).await?;
        let unit_proxy = UnitProxy::builder(&self.connection)
            .path(unit_path)?
            .build()
            .await?;
        let state_str = unit_proxy.active_state().await?;
        Ok(UnitState::from_dbus(&state_str))
    }

    /// Gets detailed properties of a unit.
    pub async fn get_unit_properties(&self, name: &str) -> Result<UnitProperties> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        let unit_path = proxy.get_unit(name).await?;
        let unit_proxy = UnitProxy::builder(&self.connection)
            .path(unit_path)?
            .build()
            .await?;

        Ok(UnitProperties {
            state: UnitState::from_dbus(&unit_proxy.active_state().await?),
            sub_state: SubState::from_dbus(&unit_proxy.sub_state().await?),
            description: unit_proxy.description().await.unwrap_or_default(),
            requires: unit_proxy.requires().await.unwrap_or_default(),
            wants: unit_proxy.wants().await.unwrap_or_default(),
        })
    }

    /// Waits for a systemd job to complete and returns its result.
    pub async fn wait_for_job(&self, job_id: JobId) -> Result<JobResult> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        proxy.subscribe().await?;

        let mut stream = proxy.receive_job_removed().await?;

        while let Some(signal) = stream.next().await {
            let args = signal.args()?;
            if args.id == job_id.0 {
                return Ok(JobResult::from_dbus(args.result));
            }
        }

        anyhow::bail!("Job signal stream ended unexpectedly")
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
