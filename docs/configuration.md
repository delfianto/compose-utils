# Configuration

## Config File

Settings are stored in a `compose.env` file using simple `KEY=VALUE` format:

| Mode | Path |
|------|------|
| Root | `/etc/compose.env` |
| Rootless | `~/.config/docker/compose.env` |

Comments (`#`) and empty lines are ignored. Values can contain `=` characters (only the first `=` is used as the delimiter).

### Example

```bash
# Data storage
COMPOSE_DATA=/home/user/data
COMPOSE_BASE=/home/user/compose-projects

# Traefik SSL
TRAEFIK_ACME_DOMAIN=example.com
TRAEFIK_ACME_EMAIL=admin@example.com
TRAEFIK_ACME_SERVER=https://acme-v02.api.letsencrypt.org/directory

# Docker
DOCKER_HOST=unix:///run/user/1000/docker.sock
```

## Configuration Keys

### COMPOSE_BASE

Root directory where Docker Compose projects are stored. Each subdirectory (or nested subdirectory) is expected to contain a `compose.yaml` or `docker-compose.yml`.

- **Validation:** Must be an existing directory
- **Default (root):** `/srv/compose`
- **Default (rootless):** `~/compose-projects`

### COMPOSE_DATA

Base directory for persistent data volumes. Referenced by compose files via the `${COMPOSE_DATA}` variable.

- **Validation:** Must be an existing directory
- **Default (root):** `/srv/data`
- **Default (rootless):** `~/data`

### TRAEFIK_ACME_DOMAIN

Primary domain for Traefik's Let's Encrypt certificate generation.

- **Validation:** RFC 1035 domain name (e.g., `example.com`, `sub.example.co.uk`)
- **Rejects:** No TLD, leading/trailing hyphens, underscores, spaces

### TRAEFIK_ACME_EMAIL

Contact email for Let's Encrypt certificate registration.

- **Validation:** RFC 5322 simplified email format
- **Rejects:** Missing `@`, missing domain, missing TLD

### TRAEFIK_ACME_SERVER

ACME server URL for certificate issuance.

- **Validation:** Valid HTTP/HTTPS URL with resolvable hostname
- **Default:** `https://acme-v02.api.letsencrypt.org/directory`

### DOCKER_HOST

Docker daemon endpoint. Supports three URI schemes:

| Scheme | Example | Validation |
|--------|---------|------------|
| `unix://` | `unix:///run/user/1000/docker.sock` | Socket file must exist |
| `tcp://` | `tcp://localhost:2375` | Valid URL with host |
| `ssh://` | `ssh://user@host:22` | Valid URL with host |

## Managing Configuration

### View current settings

```bash
compose config
```

Displays all configured keys with their values, formatted as a table.

### Update a setting

```bash
compose config --acme-email user@example.com
compose config --compose-base /opt/compose
compose config --docker-host tcp://docker.local:2375
```

Multiple settings can be updated at once:

```bash
compose config --acme-domain example.com --acme-email admin@example.com
```

All values are validated before writing. If validation fails, the config file is not modified.

### Available flags

| Flag | Key | Validation |
|------|-----|------------|
| `--compose-data <PATH>` | `COMPOSE_DATA` | Directory exists |
| `--compose-base <PATH>` | `COMPOSE_BASE` | Directory exists |
| `--acme-domain <DOMAIN>` | `TRAEFIK_ACME_DOMAIN` | Valid domain |
| `--acme-email <EMAIL>` | `TRAEFIK_ACME_EMAIL` | Valid email |
| `--acme-server <URL>` | `TRAEFIK_ACME_SERVER` | Valid URL, host resolves |
| `--docker-host <URI>` | `DOCKER_HOST` | Valid docker endpoint |

## Environment File in Systemd

Every project command selects existing interpolation files in this order:

1. The tool's global `compose.env` (path above).
2. The project's `.env`, if present.
3. The project's `.env.local`, if present.

The tool passes these as ordered `--env-file` arguments. Missing files are
skipped, so projects do not need an empty `.env`. This also applies to pulls,
systemd starts/stops, and drift checks. The child process ignores inherited
`COMPOSE_ENV_FILES`; no fixed global list is needed. Runtime Compose `env_file:`
entries remain independent of these interpolation files.

For direct commands, explicitly exported shell variables still outrank env-file
values, following Compose's normal behavior.

The systemd unit template loads environment from multiple sources (in order):

```ini
EnvironmentFile=-/etc/compose.env
EnvironmentFile=-%h/.config/compose.env
EnvironmentFile=-%h/.config/docker/compose.env
```

The `-` prefix means "don't fail if the file doesn't exist." Later files override earlier ones. The `%h` expands to the user's home directory.

For systemd launches, global keys from the active config file are removed from
the child process's inherited environment and read through `--env-file` instead.
This allows project `.env`/`.env.local` values to override global defaults.
The configured `DOCKER_HOST` remains authoritative for selecting the daemon.

Raw `docker compose` commands do not use this wrapper. To get the same layering,
pass the global file and whichever project files exist with `--env-file`.
