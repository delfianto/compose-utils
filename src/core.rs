use anyhow::{Context as _, Result, bail};
use directories::BaseDirs;
use nix::unistd::{geteuid, getuid};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::constants;

#[derive(Debug, Clone)]
pub struct Context {
    pub is_root: bool,
    pub systemd_dir: PathBuf,
    pub systemctl_cmd: Vec<String>,
    pub compose_base: PathBuf,
    pub env_file: PathBuf,
}

pub fn get_context() -> Result<Context> {
    let is_root = geteuid().is_root();

    if is_root {
        let _ = detect_and_validate_mode(true, &HashMap::new())?;
        
        let env_file = PathBuf::from(constants::ROOT_ENV_DIR).join(constants::ENV_FILE_NAME);
        let config = read_env_file(&env_file)?;
        
        let compose_base = config.get("COMPOSE_BASE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(constants::ROOT_COMPOSE_BASE));

        Ok(Context {
            is_root: true,
            systemd_dir: PathBuf::from(constants::ROOT_SYSTEMD_DIR),
            systemctl_cmd: vec![constants::SYSTEMCTL_CMD.to_string()],
            compose_base,
            env_file,
        })
    } else {
        let base_dirs = BaseDirs::new().context("Could not determine user home directory")?;
        let xdg_config = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| base_dirs.home_dir().join(".config"));

        let uid = getuid();
        let runtime_dir =
            env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", uid));

        let mut env_vars = HashMap::new();
        env_vars.insert("XDG_RUNTIME_DIR".to_string(), runtime_dir.clone());

        let _ = detect_and_validate_mode(false, &env_vars)?;

        let env_file = xdg_config.join(constants::ENV_FILE_NAME);
        let config = read_env_file(&env_file)?;

        let compose_base = config.get("COMPOSE_BASE")
            .map(PathBuf::from)
            .unwrap_or_else(|| base_dirs.home_dir().join(constants::USER_COMPOSE_BASE_NAME));

        Ok(Context {
            is_root: false,
            systemd_dir: xdg_config.join(constants::USER_SYSTEMD_DIR_REL),
            systemctl_cmd: vec![constants::SYSTEMCTL_CMD.to_string(), "--user".to_string()],
            compose_base,
            env_file,
        })
    }
}

pub fn read_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let mut config = HashMap::new();

    if !path.exists() {
        return Ok(config);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            config.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(config)
}

fn detect_and_validate_mode(is_root: bool, env_vars: &HashMap<String, String>) -> Result<PathBuf> {
    let system_socket = Path::new(constants::ROOT_DOCKER_SOCKET);

    if is_root {
        if !system_socket.exists() {
            bail!(
                r#"Root privileges detected, but system-wide docker socket
                ({:?}) was not found.
                If you are using rootless docker, please run WITHOUT sudo."#,
                system_socket
            );
        }
        Ok(system_socket.to_path_buf())
    } else {
        if system_socket.exists() {
            bail!(
                r#"System-wide Docker socket detected ({:?}).
                Please run WITH sudo to manage system-wide docker."#,
                system_socket
            );
        }

        let runtime_dir = env_vars
            .get("XDG_RUNTIME_DIR")
            .context("Could not determine XDG_RUNTIME_DIR for rootless socket check.")?;

        let rootless_socket = Path::new(runtime_dir).join(constants::USER_DOCKER_SOCKET_NAME);
        if !rootless_socket.exists() {
            bail!(
                r#"Rootless docker socket not found at {:?}.
                Docker daemon is not running or not installed in rootless mode.
                Please start the daemon and try again."#,
                rootless_socket
            );
        }
        Ok(rootless_socket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn test_read_env_file_success() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("compose_test_{}.env", std::process::id()));
        
        // Ensure cleanup
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(file_path.clone());

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "COMPOSE_BASE=/tmp/test_base").unwrap();
        writeln!(file, "ANOTHER_VAR=some_value").unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file, "  SPACED_VAR =  spaced value  ").unwrap();

        let config = read_env_file(&file_path).unwrap();

        assert_eq!(config.get("COMPOSE_BASE"), Some(&"/tmp/test_base".to_string()));
        assert_eq!(config.get("ANOTHER_VAR"), Some(&"some_value".to_string()));
        assert_eq!(config.get("SPACED_VAR"), Some(&"spaced value".to_string()));
    }

    #[test]
    fn test_read_env_file_not_found() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("compose_test_nonexistent_{}.env", std::process::id()));
        // Ensure it doesn't exist
        let _ = std::fs::remove_file(&file_path);
        
        let config = read_env_file(&file_path).unwrap();
        assert!(config.is_empty());
    }
}
