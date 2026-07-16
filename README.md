# Compose Utils

Two tools, one binary — for managing Docker Compose projects.

`compose` is a direct Docker Compose helper with project discovery and configuration management. `composectl` is a systemd service controller that manages compose projects as persistent system services. Both are delivered as a single portable binary (multi-call via `argv[0]`), with zero runtime dependencies.

## Features

- **Dual persona**: `compose` for direct container operations, `composectl` for systemd integration
- **Root & Rootless Support**: Automatically detects system-wide or user-level Docker installations
- **Automatic Project Detection**: Run commands from inside a project directory without specifying the service name
- **Dependency Management**: Configure systemd inter-service dependencies via TOML files or CLI
- **Configuration Management**: Validate and update environment variables via `compose config`

## Installation

1. **Ensure Rust (>= 1.85) and Cargo are installed.** ([rustup.rs](https://rustup.rs/))
2. **Build and install:**
    ```bash
    ./systemd/install.sh
    ```

The installer builds the binary, installs it as `composectl`, creates a `compose` symlink, sets up the systemd unit template, and guides you through configuration.

## Tools

### `compose` — Docker Compose helper

Direct container operations with project discovery and centralized configuration.

| Command | Alias | Description |
| :------ | :---- | :---------- |
| `compose up [services]` | `up` | Start containers (`docker compose up -d`) |
| `compose down [services]` | `down` | Stop containers (`docker compose down`) |
| `compose restart [services]` | `reup` | Restart containers |
| `compose pull [services]` | | Pull images without restarting |
| `compose ps [services]` | | List containers with status |
| `compose config [options]` | | View or update configuration |

```bash
# Start a project
compose up genai/ollama

# Check all containers
compose ps

# Pull latest images
compose pull genai/ollama
```

### `composectl` — systemd service controller

Manage compose projects as systemd services with boot persistence and dependency ordering.

| Command | Description |
| :------ | :---------- |
| `composectl start [services]` | Start via systemd (`systemctl start`) |
| `composectl stop [services]` | Stop via systemd (`systemctl stop`) |
| `composectl restart [services]` | Restart via systemd |
| `composectl update [services]` | Pull images + restart via systemd |
| `composectl status [services]` | Show systemd unit status |
| `composectl enable [services]` | Enable auto-start on boot |
| `composectl disable [services]` | Disable auto-start on boot |
| `composectl deps [options]` | Manage inter-service dependencies |
| `composectl config [options]` | View or update configuration |

```bash
# Enable a service to start on boot
composectl enable genai/ollama

# Update images and restart via systemd
composectl update genai/ollama

# Add a dependency
composectl deps web-app --add db-service --requires

# Check systemd status
composectl status genai/ollama
```

## Configuration

Settings are stored in `compose.env` (`/etc/compose.env` for root, `~/.config/docker/compose.env` for rootless):

```bash
# View current configuration
compose config

# Update a setting
compose config --acme-email user@example.com
```

| Variable | Description |
| :------- | :---------- |
| `COMPOSE_BASE` | Root directory for compose projects |
| `COMPOSE_DATA` | Base directory for persistent data |
| `DOCKER_HOST` | Docker daemon endpoint |
| `TRAEFIK_ACME_DOMAIN` | Domain for SSL certificates |
| `TRAEFIK_ACME_EMAIL` | Contact email for Let's Encrypt |
| `TRAEFIK_ACME_SERVER` | ACME server URL |

## Documentation

Detailed technical documentation is in the [`docs/`](docs/) directory:

| Document | Description |
| :------- | :---------- |
| [Architecture](docs/architecture.md) | System design, multi-call binary pattern, data flow, module graph |
| [Project Structure](docs/structure.md) | Directory layout, module descriptions, test distribution |
| [Dependencies](docs/dependencies.md) | External crates, rationale, what was removed and why |
| [Command Reference](docs/commands.md) | Full command reference for both `compose` and `composectl` |
| [Configuration](docs/configuration.md) | Config file format, all keys, validation rules, management |
| [Systemd Integration](docs/systemd.md) | Unit template, dependency overrides, root vs rootless, lifecycle |

## Development

```bash
# Build
cargo build --release

# Run tests
cargo test

# The binary is at target/release/composectl
# Create a compose symlink for local testing:
ln -s target/release/composectl target/release/compose
```
