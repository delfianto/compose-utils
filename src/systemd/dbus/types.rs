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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_state_from_dbus() {
        assert_eq!(UnitState::from_dbus("active"), UnitState::Active);
        assert_eq!(UnitState::from_dbus("activating"), UnitState::Activating);
        assert_eq!(UnitState::from_dbus("unknown_state"), UnitState::Unknown);
    }

    #[test]
    fn test_sub_state_from_dbus() {
        assert_eq!(SubState::from_dbus("running"), SubState::Running);
        assert_eq!(SubState::from_dbus("dead"), SubState::Dead);
        assert_eq!(SubState::from_dbus("something_else"), SubState::Unknown);
    }

    #[test]
    fn test_job_result_from_dbus() {
        assert_eq!(JobResult::from_dbus("done"), JobResult::Done);
        assert_eq!(JobResult::from_dbus("failed"), JobResult::Failed);
        assert_eq!(JobResult::from_dbus("what"), JobResult::Unknown);
    }

    #[test]
    fn test_job_id_from_path() {
        use zbus::zvariant::ObjectPath;
        let p = ObjectPath::try_from("/org/freedesktop/systemd1/job/123").unwrap();
        let owned = OwnedObjectPath::from(p);
        assert_eq!(JobId::from_path(&owned).0, 123);

        let p_invalid = ObjectPath::try_from("/invalid/path").unwrap();
        let owned_invalid = OwnedObjectPath::from(p_invalid);
        assert_eq!(JobId::from_path(&owned_invalid).0, 0);
    }
}
