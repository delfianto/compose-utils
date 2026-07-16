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

The systemd unit template loads environment from multiple sources (in order):

```ini
EnvironmentFile=-/etc/compose.env
EnvironmentFile=-%h/.config/compose.env
EnvironmentFile=-%h/.config/docker/compose.env
```

The `-` prefix means "don't fail if the file doesn't exist." Later files override earlier ones. The `%h` expands to the user's home directory.

These variables are available to `docker compose` via the shell environment, so compose files can reference `${COMPOSE_DATA}`, `${TRAEFIK_ACME_DOMAIN}`, etc.
