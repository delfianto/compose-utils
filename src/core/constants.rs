/// Directory where global environment files are stored when running as root.
pub const ROOT_ENV_DIR: &str = "/etc";
/// Default base directory for docker-compose projects when running as root.
pub const ROOT_COMPOSE_BASE: &str = "/srv/compose";
/// System-wide systemd unit file directory.
pub const ROOT_SYSTEMD_DIR: &str = "/etc/systemd/system";
/// Standard path to the system-wide Docker socket.
pub const ROOT_DOCKER_SOCKET: &str = "/var/run/docker.sock";

/// Name of the environment configuration file used by this tool.
pub const ENV_FILE_NAME: &str = "compose.env";
/// Default subdirectory name in user's home for docker-compose projects (rootless).
pub const USER_COMPOSE_BASE_NAME: &str = "compose-projects";
/// Relative path from user's config home to systemd user unit directory.
pub const USER_SYSTEMD_DIR_REL: &str = "systemd/user";
/// Relative path from user's config home to the environment file directory.
pub const USER_ENV_DIR_REL: &str = "docker";
/// Name of the Docker socket file in rootless mode.
pub const USER_DOCKER_SOCKET_NAME: &str = "docker.sock";

/// List of standard filenames recognized as Docker Compose configuration files.
pub const COMPOSE_FILES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// List of environment variable keys managed or recognized by this tool's configuration.
pub const CONFIG_KEYS: &[&str] = &[
    "COMPOSE_DATA",
    "COMPOSE_BASE",
    "TRAEFIK_ACME_DOMAIN",
    "TRAEFIK_ACME_EMAIL",
    "TRAEFIK_ACME_SERVER",
    "DOCKER_HOST",
];
