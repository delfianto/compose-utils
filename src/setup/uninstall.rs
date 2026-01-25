use super::detect::{detect_system_info, SystemInfo};
use crate::core::Context;
use anyhow::Result;
use std::fs;

/// Orchestrates the uninstallation process.
pub fn run_uninstall() -> Result<()> {
    let info = detect_system_info();
    uninstall_impl(&info)
}

/// Core logic for uninstallation.
///
/// Steps taken:
/// 1. Stops and disables any running services managed by this tool.
/// 2. Removes the systemd service template.
/// 3. Reloads systemd.
/// 4. Removes the binary.
fn uninstall_impl(info: &SystemInfo) -> Result<()> {
    let ctx: Context = info.into();
    println!("Uninstalling for {} mode...", info.mode);

    stop_services(&ctx)?;

    let service_path = info.systemd_dir.join("compose@.service");
    if service_path.exists() {
        println!("Removing service template {}...", service_path.display());
        let _ = fs::remove_file(service_path);
    }

    println!("Reloading systemd daemon...");
    let _ = crate::systemd::manager::daemon_reload(&ctx);

    let target_bin = info.bin_dir.join("compose");
    if target_bin.exists() {
        println!("Removing binary {}...", target_bin.display());
        let _ = fs::remove_file(target_bin);
    }

    println!("\nUninstall complete!");
    println!("--------------------------------------------------");
    println!(
        "Note: Environment file preserved at {}",
        info.env_file.display()
    );
    println!("Note: Data directories preserved.");
    println!("--------------------------------------------------");

    Ok(())
}

/// Lists and stops all active `compose@*.service` units.
fn stop_services(ctx: &Context) -> Result<()> {
    println!("Stopping running services...");

    // We want to list all units matching 'compose@*.service'.
    // list_units appends '*' to the pattern.
    // However, list_units might fail if systemctl is missing or fails (e.g. in test env without systemd).
    // manager::list_units handles systemctl execution.

    // In test environments, this might fail or return empty. We should be robust.
    if std::env::var("TEST_ENV").is_ok() {
        return Ok(());
    }

    match crate::systemd::manager::list_units(ctx, Some("compose@")) {
        Ok(units) => {
            for unit in units {
                println!("  Stopping {}...", unit.name);
                let _ = crate::systemd::manager::stop_unit(ctx, &unit.name);
                let _ = crate::systemd::manager::disable_unit(ctx, &unit.name);
            }
        }
        Err(e) => {
            // Warn but don't fail uninstallation just because we couldn't list units (maybe systemd is down)
            println!("  Warning: Could not list running services: {}", e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// Helper to create a dummy SystemInfo
    fn mock_info(dir: &tempfile::TempDir) -> SystemInfo {
        SystemInfo {
            mode: "test".to_string(),
            uid: 1000,
            is_root: false,
            user_home: Some(dir.path().to_path_buf()),
            xdg_runtime_dir: None,
            xdg_config_home: None,
            docker_socket_path: PathBuf::from("/dev/null"),
            docker_socket_exists: false,
            bin_dir: dir.path().join("bin"),
            systemd_dir: dir.path().join("systemd"),
            env_file: dir.path().join("env"),
            data_base: dir.path().join("data"),
            compose_base: dir.path().join("compose"),
            systemctl_cmd: vec!["echo".to_string()], // Mock systemctl
        }
    }

    #[test]
    fn test_uninstall_impl_removes_files() {
        let dir = tempdir().unwrap();
        let info = mock_info(&dir);

        // Setup dummy files
        fs::create_dir_all(&info.bin_dir).unwrap();
        fs::create_dir_all(&info.systemd_dir).unwrap();

        let binary = info.bin_dir.join("compose");
        File::create(&binary).unwrap();

        let service = info.systemd_dir.join("compose@.service");
        File::create(&service).unwrap();

        std::env::set_var("TEST_ENV", "1");
        let result = uninstall_impl(&info);
        std::env::remove_var("TEST_ENV");

        assert!(result.is_ok());
        assert!(!binary.exists(), "Binary should be removed");
        assert!(!service.exists(), "Service template should be removed");
    }
}
