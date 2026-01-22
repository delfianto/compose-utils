use anyhow::{Context as _, Result, bail};
use directories::BaseDirs;
use nix::unistd::{geteuid, getuid};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

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
        Ok(Context {
            is_root: true,
            systemd_dir: PathBuf::from("/etc/systemd/system"),
            systemctl_cmd: vec!["systemctl".to_string()],
            compose_base: PathBuf::from("/srv/compose"),
            env_file: PathBuf::from("/etc/compose.env"),
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

        Ok(Context {
            is_root: false,
            systemd_dir: xdg_config.join("systemd/user"),
            systemctl_cmd: vec!["systemctl".to_string(), "--user".to_string()],
            compose_base: base_dirs.home_dir().join("compose-projects"),
            env_file: xdg_config.join("compose.env"),
        })
    }
}

fn detect_and_validate_mode(is_root: bool, env_vars: &HashMap<String, String>) -> Result<PathBuf> {
    let system_socket = Path::new("/var/run/docker.sock");

    if is_root {
        if !system_socket.exists() {
            bail!(
                r#"Root privileges detected, but system-wide docker socket
                (/var/run/docker.sock) was not found.
                If you are using rootless docker, please run WITHOUT sudo."#
            );
        }
        Ok(system_socket.to_path_buf())
    } else {
        if system_socket.exists() {
            bail!(
                r#"System-wide Docker socket detected (/var/run/docker.sock).
                Please run WITH sudo to manage system-wide docker."#
            );
        }

        let runtime_dir = env_vars
            .get("XDG_RUNTIME_DIR")
            .context("Could not determine XDG_RUNTIME_DIR for rootless socket check.")?;

        let rootless_socket = Path::new(runtime_dir).join("docker.sock");
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
