//! D-Bus type mappings for systemd states and results.

use zbus::zvariant::OwnedObjectPath;

/// Represents the high-level active state of a systemd unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    Active,
    Activating,
    Deactivating,
    Inactive,
    Failed,
    Reloading,
    Unknown,
}

impl UnitState {
    /// Converts a systemd D-Bus state string to a [`UnitState`].
    pub fn from_dbus(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "activating" => Self::Activating,
            "deactivating" => Self::Deactivating,
            "inactive" => Self::Inactive,
            "failed" => Self::Failed,
            "reloading" => Self::Reloading,
            _ => Self::Unknown,
        }
    }
}

/// Represents the low-level sub-state of a systemd unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubState {
    Running,
    Dead,
    Exited,
    Failed,
    AutoRestart,
    Unknown,
}

impl SubState {
    /// Converts a systemd D-Bus sub-state string to a [`SubState`].
    pub fn from_dbus(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "dead" => Self::Dead,
            "exited" => Self::Exited,
            "failed" => Self::Failed,
            "auto-restart" => Self::AutoRestart,
            _ => Self::Unknown,
        }
    }
}

/// Represents the result of a systemd job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobResult {
    Done,
    Canceled,
    Timeout,
    Failed,
    Dependency,
    Skipped,
    Unknown,
}

impl JobResult {
    /// Converts a systemd D-Bus job result string to a [`JobResult`].
    pub fn from_dbus(s: &str) -> Self {
        match s {
            "done" => Self::Done,
            "canceled" => Self::Canceled,
            "timeout" => Self::Timeout,
            "failed" => Self::Failed,
            "dependency" => Self::Dependency,
            "skipped" => Self::Skipped,
            _ => Self::Unknown,
        }
    }
}

/// A wrapper around a systemd job ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u32);

impl JobId {
    /// Extracts a job ID from a D-Bus object path (e.g., `/org/freedesktop/systemd1/job/123`).
    pub fn from_path(path: &OwnedObjectPath) -> Self {
        let s = path.as_str();
        let id = s
            .split('/')
            .next_back()
            .and_then(|id| id.parse().ok())
            .unwrap_or(0);
        Self(id)
    }
}
