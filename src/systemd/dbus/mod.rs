pub mod manager;
pub mod types;

use anyhow::{Context, Result};
use zbus::Connection;

/// Establishes a connection to the D-Bus.
///
/// # Arguments
///
/// * `user_mode` - If true, connects to the session bus; otherwise, connects to the system bus.
///
/// # Errors
///
/// Returns an error if the connection to the specified bus fails.
pub async fn connect(user_mode: bool) -> Result<Connection> {
    if user_mode {
        Connection::session().await
    } else {
        Connection::system().await
    }
    .context("Failed to connect to D-Bus")
}
