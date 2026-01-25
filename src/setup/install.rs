use super::constants::{ENV_TEMPLATE, SERVICE_TEMPLATE};
use super::detect::{detect_system_info, resolve_path_setting};
use crate::core::Context;
use anyhow::{Context as _, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct InstallOptions {
    pub compose_data: Option<PathBuf>,
    pub compose_base: Option<PathBuf>,
    pub acme_domain: Option<String>,
    pub acme_email: Option<String>,
    pub acme_server: Option<String>,
    pub docker_host: Option<String>,
}

/// Orchestrates the installation process.
///
/// Steps taken:
/// 1. Detects system environment (root vs rootless, paths).
/// 2. Installs the binary to the appropriate location.
/// 3. Installs and configures the systemd service template.
/// 4. Reloads systemd to recognize the new unit.
/// 5. Generates or migrates the environment configuration file.
/// 6. Creates necessary base directories.
pub fn run_install(opts: InstallOptions) -> Result<()> {
    let info = detect_system_info();
    let ctx: Context = (&info).into();

    println!("Installing for {} mode...", info.mode);

    if !info.docker_socket_exists {
        eprintln!(
            "Warning: Docker socket not found at {:?}. Installation will proceed, but services may fail.",
            info.docker_socket_path
        );
    }

    let current_exe = std::env::current_exe().context("Failed to get current executable path")?;
    install_binary(&current_exe, &info.bin_dir)?;

    install_service_template(
        &info.systemd_dir,
        &info.env_file,
        &info.compose_base,
        opts.compose_base.as_deref(),
    )?;

    println!("Reloading systemd daemon...");
    crate::systemd::manager::daemon_reload(&ctx)?;

    generate_or_update_env_file(&info, &opts)?;

    // Resolve base path again to ensure we create the configured one
    let final_compose_base = resolve_path_setting(
        opts.compose_base.as_deref(),
        &info.env_file,
        "COMPOSE_BASE",
        &info.compose_base,
    );
    fs::create_dir_all(&final_compose_base)?;

    let current_data_base = resolve_path_setting(
        opts.compose_data.as_deref(),
        &info.env_file,
        "COMPOSE_DATA",
        &info.data_base,
    );
    fs::create_dir_all(&current_data_base)?;

    println!("\nInstallation complete!");
    println!("--------------------------------------------------");
    println!(
        "Binary location: {}",
        info.bin_dir.join("compose").display()
    );
    println!("Environment file: {}", info.env_file.display());
    println!("Compose projects: {}", final_compose_base.display());
    println!("Mode: {}", info.mode);
    println!("--------------------------------------------------");

    Ok(())
}

/// Copies the binary to the install destination and sets permissions.
fn install_binary(source: &Path, bin_dir: &Path) -> Result<PathBuf> {
    let target_bin = bin_dir.join("compose");
    println!("Installing binary to {}...", target_bin.display());
    fs::create_dir_all(bin_dir).context("Failed to create bin dir")?;
    fs::copy(source, &target_bin).context("Failed to copy binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_bin, perms)?;
    }

    Ok(target_bin)
}

/// Writes the systemd service template file, substituting placeholder variables.
fn install_service_template(
    systemd_dir: &Path,
    env_file_path: &Path, // Used for resolution
    default_compose_base: &Path,
    arg_compose_base: Option<&Path>,
) -> Result<PathBuf> {
    let service_path = systemd_dir.join("compose@.service");
    println!(
        "Installing service template to {}...",
        service_path.display()
    );
    fs::create_dir_all(systemd_dir).context("Failed to create systemd dir")?;

    let docker_path = which::which("docker").unwrap_or_else(|_| PathBuf::from("/usr/bin/docker"));
    let compose_bin_path = format!("{} compose", docker_path.display());

    let final_compose_base = resolve_path_setting(
        arg_compose_base,
        env_file_path,
        "COMPOSE_BASE",
        default_compose_base,
    );

    let service_content = SERVICE_TEMPLATE
        .replace("${compose_base}", &final_compose_base.to_string_lossy())
        .replace("${compose_bin_path}", &compose_bin_path);

    fs::write(&service_path, service_content).context("Failed to write service file")?;
    Ok(service_path)
}

/// Creates or migrates the environment configuration file.
fn generate_or_update_env_file(
    info: &super::detect::SystemInfo,
    opts: &InstallOptions,
) -> Result<()> {
    println!("Checking env file at {}...", info.env_file.display());
    if let Some(parent) = info.env_file.parent() {
        fs::create_dir_all(parent)?;
    }

    if !info.is_root {
        if let Some(home) = &info.user_home {
            let old_env = home.join(".config/compose.env");
            if old_env.exists() && !info.env_file.exists() {
                println!(
                    "Migrating existing env file from {} to {}...",
                    old_env.display(),
                    info.env_file.display()
                );
                fs::rename(&old_env, &info.env_file).context("Failed to migrate env file")?;
            }
        }
    }

    if info.env_file.exists() {
        println!("Env file exists, preserving existing configuration.");
        return Ok(());
    }

    println!("Generating new env file...");
    let final_data_base = resolve_path_setting(
        opts.compose_data.as_deref(),
        &info.env_file,
        "COMPOSE_DATA",
        &info.data_base,
    );
    let final_compose_base = resolve_path_setting(
        opts.compose_base.as_deref(),
        &info.env_file,
        "COMPOSE_BASE",
        &info.compose_base,
    );

    let final_docker_host = opts
        .docker_host
        .clone()
        .unwrap_or_else(|| format!("unix://{}", info.docker_socket_path.to_string_lossy()));

    let final_docker_sock = final_docker_host.replace("unix://", "");

    let env_content = ENV_TEMPLATE
        .replace("${{data_base}}", &final_data_base.to_string_lossy())
        .replace("${{compose_base}}", &final_compose_base.to_string_lossy())
        .replace(
            "${{acme_domain}}",
            opts.acme_domain.as_deref().unwrap_or("example.com"),
        )
        .replace(
            "${{acme_email}}",
            opts.acme_email.as_deref().unwrap_or("admin@example.com"),
        )
        .replace(
            "${{acme_server}}",
            opts.acme_server
                .as_deref()
                .unwrap_or("https://acme-v02.api.letsencrypt.org/directory"),
        )
        .replace("${{docker_host}}", &final_docker_host)
        .replace("${{docker_sock}}", &final_docker_sock);

    fs::write(&info.env_file, env_content).context("Failed to write env file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_install_binary() {
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("bin");

        // Create a dummy source binary
        let source = dir.path().join("dummy_compose");
        fs::write(&source, "fake binary content").unwrap();

        let result = install_binary(&source, &bin_dir);
        assert!(result.is_ok());

        let target = bin_dir.join("compose");
        assert!(target.exists());
        let content = fs::read_to_string(target).unwrap();
        assert_eq!(content, "fake binary content");
    }

    #[test]
    fn test_install_service_template() {
        let dir = tempdir().unwrap();
        let systemd_dir = dir.path().join("systemd");
        let env_file = dir.path().join("compose.env");
        let default_base = dir.path().join("default_base");

        let result = install_service_template(&systemd_dir, &env_file, &default_base, None);

        assert!(result.is_ok());
        let service_file = systemd_dir.join("compose@.service");
        assert!(service_file.exists());

        let content = fs::read_to_string(service_file).unwrap();
        assert!(content.contains("EnvironmentFile="));
        // Check default base usage
        assert!(content.contains(&default_base.to_string_lossy().to_string()));
    }

    #[test]
    fn test_install_service_template_with_override() {
        let dir = tempdir().unwrap();
        let systemd_dir = dir.path().join("systemd");
        let env_file = dir.path().join("compose.env");
        let default_base = dir.path().join("default_base");
        let override_base = dir.path().join("override_base");

        let result =
            install_service_template(&systemd_dir, &env_file, &default_base, Some(&override_base));

        assert!(result.is_ok());
        let service_file = systemd_dir.join("compose@.service");
        let content = fs::read_to_string(service_file).unwrap();
        assert!(content.contains(&override_base.to_string_lossy().to_string()));
    }
}
