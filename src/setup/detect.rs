use crate::core::constants;
use crate::core::Context;
use directories::BaseDirs;
use nix::unistd::{geteuid, getuid};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};

/// Snapshot of the system environment relevant to installation.
#[derive(Serialize, Debug, Clone)]
pub struct SystemInfo {
    pub mode: String,
    pub uid: u32,
    pub is_root: bool,
    pub user_home: Option<PathBuf>,
    pub xdg_runtime_dir: Option<String>,
    pub xdg_config_home: Option<PathBuf>,

    /// Path to the detected Docker socket.
    pub docker_socket_path: PathBuf,
    /// Whether the Docker socket actually exists.
    pub docker_socket_exists: bool,

    /// Target directory for the binary installation.
    pub bin_dir: PathBuf,
    /// Target directory for systemd units.
    pub systemd_dir: PathBuf,
    /// Target path for the environment file.
    pub env_file: PathBuf,
    /// Base directory for application data.
    pub data_base: PathBuf,
    /// Base directory for compose projects.
    pub compose_base: PathBuf,

    /// Command prefix for running systemctl (e.g., ["systemctl"] or ["systemctl", "--user"]).
    pub systemctl_cmd: Vec<String>,
}

impl From<&SystemInfo> for Context {
    fn from(info: &SystemInfo) -> Self {
        Context {
            is_root: info.is_root,
            systemd_dir: info.systemd_dir.clone(),
            compose_base: info.compose_base.clone(),
            env_file: info.env_file.clone(),
            // We use the detected socket path as the default host if not overridden later.
            // Note: This matches the "unix://..." format.
            docker_host: Some(format!(
                "unix://{}",
                info.docker_socket_path.to_string_lossy()
            )),
        }
    }
}

/// Detects the current system state (root vs rootless, paths, environment).
pub fn detect_system_info() -> SystemInfo {
    let is_root = geteuid().is_root();
    let uid = getuid().as_raw();

    let base_dirs = BaseDirs::new();
    let user_home = base_dirs.as_ref().map(|b| b.home_dir().to_path_buf());

    let xdg_runtime_dir = env::var("XDG_RUNTIME_DIR").ok().or_else(|| {
        if !is_root {
            Some(format!("/run/user/{}", uid))
        } else {
            None
        }
    });

    let xdg_config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| user_home.as_ref().map(|h| h.join(".config")));

    if is_root {
        get_root_info(uid, user_home, xdg_runtime_dir, xdg_config_home)
    } else {
        get_rootless_info(uid, user_home, xdg_runtime_dir, xdg_config_home)
    }
}

fn get_root_info(
    uid: u32,
    user_home: Option<PathBuf>,
    xdg_runtime_dir: Option<String>,
    xdg_config_home: Option<PathBuf>,
) -> SystemInfo {
    let socket_path = PathBuf::from(constants::ROOT_DOCKER_SOCKET);

    SystemInfo {
        mode: "root".to_string(),
        uid,
        is_root: true,
        user_home,
        xdg_runtime_dir,
        xdg_config_home,

        docker_socket_exists: socket_path.exists(),
        docker_socket_path: socket_path,

        bin_dir: PathBuf::from("/usr/local/bin"),
        systemd_dir: PathBuf::from(constants::ROOT_SYSTEMD_DIR),
        env_file: PathBuf::from(constants::ROOT_ENV_DIR).join(constants::ENV_FILE_NAME),
        data_base: PathBuf::from("/srv/appdata"),
        compose_base: PathBuf::from(constants::ROOT_COMPOSE_BASE),

        systemctl_cmd: vec!["systemctl".to_string()],
    }
}

fn get_rootless_info(
    uid: u32,
    user_home: Option<PathBuf>,
    xdg_runtime_dir: Option<String>,
    xdg_config_home: Option<PathBuf>,
) -> SystemInfo {
    let runtime_dir_path = xdg_runtime_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", uid)));

    let socket_path = runtime_dir_path.join(constants::USER_DOCKER_SOCKET_NAME);

    let config_home = xdg_config_home
        .clone()
        .unwrap_or_else(|| PathBuf::from(".config"));

    let home = user_home.clone().unwrap_or_else(|| PathBuf::from("/"));

    SystemInfo {
        mode: "rootless".to_string(),
        uid,
        is_root: false,
        user_home,
        xdg_runtime_dir,
        xdg_config_home,

        docker_socket_exists: socket_path.exists(),
        docker_socket_path: socket_path,

        bin_dir: home.join(".local/bin"),
        systemd_dir: config_home.join(constants::USER_SYSTEMD_DIR_REL),
        env_file: config_home
            .join(constants::USER_ENV_DIR_REL)
            .join(constants::ENV_FILE_NAME),
        data_base: home.join(".local/share/appdata"),
        compose_base: home.join(constants::USER_COMPOSE_BASE_NAME),

        systemctl_cmd: vec!["systemctl".to_string(), "--user".to_string()],
    }
}

/// Resolves a path setting with the following precedence:
/// 1. CLI Argument (if provided)
/// 2. Value in existing env file (if exists)
/// 3. Default value
pub fn resolve_path_setting(
    arg: Option<&Path>,
    env_file: &Path,
    key: &str,
    default: &Path,
) -> PathBuf {
    if let Some(path) = arg {
        return path.to_path_buf();
    }

    if env_file.exists() {
        if let Ok(config) = crate::core::read_env_file(env_file) {
            if let Some(val) = config.get(key) {
                if !val.trim().is_empty() {
                    return PathBuf::from(val);
                }
            }
        }
    }

    default.to_path_buf()
}
