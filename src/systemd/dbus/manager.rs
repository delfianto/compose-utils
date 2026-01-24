//! D-Bus proxy for the systemd Manager interface.

use zbus::zvariant::OwnedObjectPath;

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Manager {
    /// Starts a unit.
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    /// Stops a unit.
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    /// Restarts a unit.
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;

    /// Reloads the systemd daemon.
    fn reload(&self) -> zbus::Result<()>;

    /// Gets the object path for a unit.
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;

    /// Lists all units.
    #[allow(clippy::type_complexity)]
    fn list_units(
        &self,
    ) -> zbus::Result<
        Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            OwnedObjectPath,
            u32,
            String,
            OwnedObjectPath,
        )>,
    >;

    /// Subscribes to signals.
    fn subscribe(&self) -> zbus::Result<()>;

    /// Enables unit files.
    fn enable_unit_files(
        &self,
        files: Vec<&str>,
        runtime: bool,
        force: bool,
    ) -> zbus::Result<(bool, Vec<(String, String, String)>)>;

    /// Disables unit files.
    fn disable_unit_files(
        &self,
        files: Vec<&str>,
        runtime: bool,
    ) -> zbus::Result<Vec<(String, String, String)>>;

    /// Signal emitted when a job is removed.
    #[zbus(signal)]
    fn job_removed(&self, id: u32, job: OwnedObjectPath, unit: &str, result: &str);
}
