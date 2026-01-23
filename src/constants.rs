pub const ROOT_ENV_DIR: &str = "/etc";
pub const ROOT_COMPOSE_BASE: &str = "/srv/compose";
pub const ROOT_SYSTEMD_DIR: &str = "/etc/systemd/system";
pub const ROOT_DOCKER_SOCKET: &str = "/var/run/docker.sock";

pub const ENV_FILE_NAME: &str = "compose.env";
pub const USER_COMPOSE_BASE_NAME: &str = "compose-projects";
pub const USER_SYSTEMD_DIR_REL: &str = "systemd/user";
pub const USER_DOCKER_SOCKET_NAME: &str = "docker.sock";

pub const SYSTEMCTL_CMD: &str = "systemctl";

pub const COMPOSE_FILES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

pub const CONFIG_KEYS: &[&str] = &[
    "COMPOSE_DATA",
    "COMPOSE_BASE",
    "TRAEFIK_ACME_DOMAIN",
    "TRAEFIK_ACME_EMAIL",
    "TRAEFIK_ACME_SERVER",
    "DOCKER_HOST",
];
