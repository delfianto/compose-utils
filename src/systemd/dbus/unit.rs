//! D-Bus proxy for the systemd Unit interface.

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
pub trait Unit {
    /// The high-level active state of the unit.
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;

    /// The low-level sub-state of the unit.
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;

    /// The description of the unit.
    #[zbus(property)]
    fn description(&self) -> zbus::Result<String>;

    /// Units that this unit requires.
    #[zbus(property)]
    fn requires(&self) -> zbus::Result<Vec<String>>;

    /// Units that this unit wants.
    #[zbus(property)]
    fn wants(&self) -> zbus::Result<Vec<String>>;

    /// Units that this unit must start after.
    #[zbus(property)]
    fn after(&self) -> zbus::Result<Vec<String>>;

    /// Units that this unit must start before.
    #[zbus(property)]
    fn before(&self) -> zbus::Result<Vec<String>>;
}
