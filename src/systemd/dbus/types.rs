//! D-Bus type mappings for systemd states and results.

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
}
