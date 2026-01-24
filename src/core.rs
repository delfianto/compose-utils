//! Core application logic, context management, and environment validation.

use anyhow::{bail, Context as _, Result};
use directories::BaseDirs;
use nix::unistd::{geteuid, getuid};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::constants;

/// Represents the runtime environment and configuration for the application.
///
/// This context determines whether the application is running in root (system-wide)
/// or rootless (user-specific) mode and stores associated paths and settings.
#[derive(Debug, Clone)]
pub struct Context {
    /// Indicates if the application is running with root privileges.
    pub is_root: bool,
    /// Path to the directory where systemd unit files are stored.
    pub systemd_dir: PathBuf,
    /// Base directory where docker-compose projects are located.
    pub compose_base: PathBuf,
    /// Path to the environment configuration file (compose.env).
    pub env_file: PathBuf,
    /// Optional Docker host URI (e.g., unix:///var/run/docker.sock).
    pub docker_host: Option<String>,
}

/// Initializes and returns the application [`Context`].
///
/// This function performs the following steps:
/// 1. Detects if running as root or a normal user.
/// 2. Validates the environment (e.g., presence of docker sockets).
/// 3. Reads the global or user-specific environment file.
/// 4. Constructs the [`Context`] with appropriate paths.
///
/// # Errors
///
/// Returns an error if:
/// - Environment validation fails.
/// - Required directories cannot be determined.
/// - The environment file cannot be read (if it exists).
pub fn get_context() -> Result<Context> {
    let is_root = geteuid().is_root();

    if is_root {
        let _ = detect_and_validate_mode(
            true,
            &HashMap::new(),
            Path::new(constants::ROOT_DOCKER_SOCKET),
        )?;

        let env_file = PathBuf::from(constants::ROOT_ENV_DIR).join(constants::ENV_FILE_NAME);
        let config = read_env_file(&env_file)?;

        let compose_base = config
            .get("COMPOSE_BASE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(constants::ROOT_COMPOSE_BASE));

        let docker_host = config.get("DOCKER_HOST").cloned();

        return Ok(Context {
            is_root: true,
            systemd_dir: PathBuf::from(constants::ROOT_SYSTEMD_DIR),
            compose_base,
            env_file,
            docker_host,
        });
    }

    let base_dirs = BaseDirs::new().context("Could not determine user home directory")?;
    let xdg_config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| base_dirs.home_dir().join(".config"));

    let uid = getuid();
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", uid));

    let mut env_vars = HashMap::new();
    env_vars.insert("XDG_RUNTIME_DIR".to_string(), runtime_dir.clone());

    let _ = detect_and_validate_mode(false, &env_vars, Path::new(constants::ROOT_DOCKER_SOCKET))?;

    let env_file = xdg_config
        .join(constants::USER_ENV_DIR_REL)
        .join(constants::ENV_FILE_NAME);
    let config = read_env_file(&env_file)?;

    let compose_base = config
        .get("COMPOSE_BASE")
        .map(PathBuf::from)
        .unwrap_or_else(|| base_dirs.home_dir().join(constants::USER_COMPOSE_BASE_NAME));

    let docker_host = config.get("DOCKER_HOST").cloned();

    return Ok(Context {
        is_root: false,
        systemd_dir: xdg_config.join(constants::USER_SYSTEMD_DIR_REL),
        compose_base,
        env_file,
        docker_host,
    });
}

/// Reads a simple KEY=VALUE environment file into a [`HashMap`].
///
/// - Trims whitespace from keys and values.
/// - Ignores empty lines and lines starting with `#`.
/// - Supports values containing `=` characters.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be read.
pub fn read_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let mut config = HashMap::new();

    if !path.exists() {
        return Ok(config);
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

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

/// Validates the current execution mode (root vs. rootless) by checking for Docker sockets.
///
/// This function verifies that the expected Docker socket exists for the given mode.
///
/// # Arguments
///
/// * `is_root` - Boolean flag indicating if currently running as root.
/// * `env_vars` - A map of environment variables (e.g., XDG_RUNTIME_DIR).
/// * `root_socket_path` - Path to the system-wide docker socket.
///
/// # Errors
///
/// - If `is_root` is true but the system-wide docker socket is missing.
/// - If `is_root` is false but the system-wide docker socket is present (suggests sudo should be used).
/// - If `is_root` is false but the rootless docker socket is missing.
fn detect_and_validate_mode(
    is_root: bool,
    env_vars: &HashMap<String, String>,
    root_socket_path: &Path,
) -> Result<PathBuf> {
    if is_root {
        if !root_socket_path.exists() {
            bail!(
                r#"Root privileges detected, but system-wide docker socket
                ({:?}) was not found.
                If you are using rootless docker, please run WITHOUT sudo."#,
                root_socket_path
            );
        }
        Ok(root_socket_path.to_path_buf())
    } else {
        if root_socket_path.exists() {
            bail!(
                r#"System-wide Docker socket detected ({:?}).
                Please run WITH sudo to manage system-wide docker."#,
                root_socket_path
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
    use tempfile::tempdir;

    #[test]
    fn test_read_env_file_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("compose.env");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "COMPOSE_BASE=/tmp/test_base").unwrap();
        writeln!(file, "ANOTHER_VAR=some_value").unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file, "  SPACED_VAR =  spaced value  ").unwrap();
        writeln!(file, "VAR_WITH_EQUALS=key=value").unwrap();
        writeln!(file, "EMPTY_VAL=").unwrap();

        let config = read_env_file(&file_path).unwrap();

        assert_eq!(
            config.get("COMPOSE_BASE"),
            Some(&"/tmp/test_base".to_string())
        );
        assert_eq!(config.get("ANOTHER_VAR"), Some(&"some_value".to_string()));
        assert_eq!(config.get("SPACED_VAR"), Some(&"spaced value".to_string()));
        assert_eq!(
            config.get("VAR_WITH_EQUALS"),
            Some(&"key=value".to_string())
        );
        assert_eq!(config.get("EMPTY_VAL"), Some(&"".to_string()));
    }

    #[test]
    fn test_read_env_file_not_found() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("nonexistent.env");

        let config = read_env_file(&file_path).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn test_read_env_file_malformed_lines() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("malformed.env");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "VALID=key").unwrap();
        writeln!(file, "INVALID_NO_EQUALS").unwrap();
        writeln!(file, "=NO_KEY").unwrap();

        let config = read_env_file(&file_path).unwrap();
        assert_eq!(config.len(), 2);
        assert_eq!(config.get("VALID"), Some(&"key".to_string()));
        assert_eq!(config.get(""), Some(&"NO_KEY".to_string()));
    }

    #[test]
    fn test_read_env_file_whitespace_handling() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("whitespace.env");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "  KEY  =  VALUE  ").unwrap();
        writeln!(file, "\tTAB_KEY\t=\tTAB_VALUE\t").unwrap();

        let config = read_env_file(&file_path).unwrap();
        assert_eq!(config.get("KEY"), Some(&"VALUE".to_string()));
        assert_eq!(config.get("TAB_KEY"), Some(&"TAB_VALUE".to_string()));
    }

    #[test]
    fn test_read_env_file_multiple_equals() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("equals.env");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "CONNECTION_STRING=host=localhost;port=5432").unwrap();

        let config = read_env_file(&file_path).unwrap();
        assert_eq!(
            config.get("CONNECTION_STRING"),
            Some(&"host=localhost;port=5432".to_string())
        );
    }

    #[test]
    fn test_detect_and_validate_mode_root_success() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("docker.sock");
        File::create(&socket_path).unwrap();

        let result = detect_and_validate_mode(true, &HashMap::new(), &socket_path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), socket_path);
    }

    #[test]
    fn test_detect_and_validate_mode_root_missing_socket() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("docker.sock");
        // Do not create socket

        let result = detect_and_validate_mode(true, &HashMap::new(), &socket_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Root privileges detected"));
    }

    #[test]
    fn test_detect_and_validate_mode_rootless_success() {
        let dir = tempdir().unwrap();
        let root_socket = dir.path().join("root_docker.sock");
        // Root socket does not exist

        let runtime_dir = dir.path().join("runtime");
        std::fs::create_dir(&runtime_dir).unwrap();
        let user_socket = runtime_dir.join(constants::USER_DOCKER_SOCKET_NAME);
        File::create(&user_socket).unwrap();

        let mut env_vars = HashMap::new();
        env_vars.insert(
            "XDG_RUNTIME_DIR".to_string(),
            runtime_dir.to_string_lossy().to_string(),
        );

        let result = detect_and_validate_mode(false, &env_vars, &root_socket);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), user_socket);
    }

    #[test]
    fn test_detect_and_validate_mode_rootless_root_socket_exists() {
        let dir = tempdir().unwrap();
        let root_socket = dir.path().join("root_docker.sock");
        File::create(&root_socket).unwrap();

        let result = detect_and_validate_mode(false, &HashMap::new(), &root_socket);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("System-wide Docker socket detected"));
    }

    #[test]
    fn test_detect_and_validate_mode_rootless_missing_socket() {
        let dir = tempdir().unwrap();
        let root_socket = dir.path().join("root_docker.sock");
        // Root socket does not exist

        let runtime_dir = dir.path().join("runtime");
        // User socket does not exist

        let mut env_vars = HashMap::new();
        env_vars.insert(
            "XDG_RUNTIME_DIR".to_string(),
            runtime_dir.to_string_lossy().to_string(),
        );

        let result = detect_and_validate_mode(false, &env_vars, &root_socket);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Rootless docker socket not found"));
    }
}
