//! High-level client for interacting with systemd via D-Bus.

use super::dbus::{connect, manager::ManagerProxy, types::*, unit::UnitProxy};
use anyhow::{Context as _, Result};

/// High-level client for systemd operations.
pub struct SystemdClient {
    connection: zbus::Connection,
}

/// Information about a systemd unit.
#[derive(Debug, Clone)]
pub struct UnitInfo {
    pub name: String,
    pub description: String,
    #[allow(dead_code)]
    pub load_state: String,
    pub active_state: UnitState,
    pub sub_state: SubState,
}

/// Detailed properties of a systemd unit.
#[derive(Debug, Clone)]
pub struct UnitProperties {
    pub state: UnitState,
    #[allow(dead_code)]
    pub main_pid: Option<u32>,
    #[allow(dead_code)]
    pub memory_current: Option<u64>,
    pub requires: Vec<String>,
    pub wants: Vec<String>,
    pub after: Vec<String>,
    pub before: Vec<String>,
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

    /// Reloads the systemd daemon (equivalent to `systemctl daemon-reload`).
    pub async fn reload_daemon(&self) -> Result<()> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        proxy.reload().await.context("Failed to reload daemon")
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
            main_pid: None, // Would need Service interface
            memory_current: None,
            requires: unit_proxy.requires().await.unwrap_or_default(),
            wants: unit_proxy.wants().await.unwrap_or_default(),
            after: unit_proxy.after().await.unwrap_or_default(),
            before: unit_proxy.before().await.unwrap_or_default(),
        })
    }

    /// Lists units matching an optional pattern.
    pub async fn list_units(&self, pattern: Option<&str>) -> Result<Vec<UnitInfo>> {
        let proxy = ManagerProxy::new(&self.connection).await?;
        let units = proxy.list_units().await?;

        let result: Vec<UnitInfo> = units
            .into_iter()
            .filter(|(name, ..)| pattern.is_none_or(|p| name.contains(p)))
            .map(
                |(name, description, load_state, active_state, sub_state, ..)| UnitInfo {
                    name,
                    description,
                    load_state,
                    active_state: UnitState::from_dbus(&active_state),
                    sub_state: SubState::from_dbus(&sub_state),
                },
            )
            .collect();

        Ok(result)
    }

    /// Gets the units required by a unit.
    #[allow(dead_code)]
    pub async fn get_unit_requires(&self, name: &str) -> Result<Vec<String>> {
        let props = self.get_unit_properties(name).await?;
        Ok(props.requires)
    }

    /// Gets the units wanted by a unit.
    #[allow(dead_code)]
    pub async fn get_unit_wants(&self, name: &str) -> Result<Vec<String>> {
        let props = self.get_unit_properties(name).await?;
        Ok(props.wants)
    }

    /// Gets the units this unit starts after.
    #[allow(dead_code)]
    pub async fn get_unit_after(&self, name: &str) -> Result<Vec<String>> {
        let props = self.get_unit_properties(name).await?;
        Ok(props.after)
    }

    /// Gets the units this unit starts before.
    #[allow(dead_code)]
    pub async fn get_unit_before(&self, name: &str) -> Result<Vec<String>> {
        let props = self.get_unit_properties(name).await?;
        Ok(props.before)
    }

    /// Gets a dependency tree for a unit.
    pub async fn get_dependency_tree(&self, name: &str) -> Result<DependencyNode> {
        let mut visited = std::collections::HashSet::new();
        self.build_tree(name, &mut visited).await
    }

    #[async_recursion::async_recursion]
    async fn build_tree(
        &self,
        name: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<DependencyNode> {
        if visited.contains(name) {
            return Ok(DependencyNode {
                name: name.to_string(),
                requires: vec![],
                wants: vec![],
                state: None,
            });
        }
        visited.insert(name.to_string());

        let props = self.get_unit_properties(name).await.ok();

        let mut requires_nodes = Vec::new();
        if let Some(ref p) = props {
            for dep in &p.requires {
                if let Ok(node) = self.build_tree(dep, visited).await {
                    requires_nodes.push(node);
                }
            }
        }

        let mut wants_nodes = Vec::new();
        if let Some(ref p) = props {
            for dep in &p.wants {
                if let Ok(node) = self.build_tree(dep, visited).await {
                    wants_nodes.push(node);
                }
            }
        }

        Ok(DependencyNode {
            name: name.to_string(),
            requires: requires_nodes,
            wants: wants_nodes,
            state: props.map(|p| p.state),
        })
    }
}

/// A node in a dependency tree.
#[derive(Debug, Clone)]
pub struct DependencyNode {
    pub name: String,
    pub requires: Vec<DependencyNode>,
    pub wants: Vec<DependencyNode>,
    pub state: Option<UnitState>,
}

impl DependencyNode {
    /// Recursively prints the dependency tree to stdout.
    pub fn print(&self, indent: usize) {
        let prefix = "  ".repeat(indent);
        let state_str = self
            .state
            .as_ref()
            .map(|s| format!(" ({:?})", s))
            .unwrap_or_default();

        println!("{}{}{}", prefix, self.name, state_str);

        if !self.requires.is_empty() {
            println!("{}  Requires:", prefix);
            for req in &self.requires {
                req.print(indent + 2);
            }
        }

        if !self.wants.is_empty() {
            println!("{}  Wants:", prefix);
            for want in &self.wants {
                want.print(indent + 2);
            }
        }
    }
}
