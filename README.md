# Compose Utils

`compose` is a command-line utility written in Rust, designed to streamline the management of docker compose projects integrated with systemd. It provides robust tools for service lifecycle management and dependency handling, with strict support for both standard and rootless docker environments. This project offers a single, portable binary with no runtime dependencies for simplicy of deployment process.

## Features

- **Systemd Integration**: Seamlessly manage Docker Compose services using `systemctl`.
- **Root & Rootless Support**: Automatically detects and validates system-wide (Root) or user-level (Rootless) Docker installations.
- **Direct Subcommands**: intuitive interface like `compose up`, `compose logs`, etc.
- **Automatic Project Detection**: Run commands from inside a project directory without specifying the service name.
- **Improved Reliability**: Uses `oneshot` systemd services with `ExecStopPost` cleanup to prevent orphaned containers on startup failure.
- **Configuration Management**: Easily view and update environment variables (like `COMPOSE_BASE` or `TRAEFIK_ACME_EMAIL`) via `compose config`.

## Installation

`compose` uses `just` for its installation process. The installer automatically detects your Docker environment and ensures correct privilege usage.

1.  **Ensure Rust and Cargo are installed.** ([rustup.rs](https://rustup.rs/))
2.  **Ensure `just` is installed.**
3.  **Build and Install:**
    ```bash
    just install
    ```
4.  **Update/Reinstall:** (Rebuilds and re-applies changes)
    ```bash
    just reinstall
    ```

## Usage

Commands can be run from anywhere. If run inside a directory containing a `compose.yaml` (under your `COMPOSE_BASE`), the project name is automatically detected.

### Service Management

| Command   | Alias     | Description                                       |
| :-------- | :-------- | :------------------------------------------------ |
| `up`      | `start`   | Start services and show immediate systemd status. |
| `down`    | `stop`    | Stop services and perform cleanup.                |
| `reup`    | `restart` | Restart services.                                 |
| `update`  |           | Pull latest images and restart if updated.        |
| `pull`    |           | Download images without restarting.               |
| `status`  |           | Show current systemd unit status.                 |
| `enable`  |           | Enable services to start on boot.                 |
| `disable` |           | Disable services from starting on boot.           |
| `ls`      | `list`    | List all managed services under `COMPOSE_BASE`.   |
| `ps`      |           | Global container overview with status.            |
| `logs`    |           | View last 100 lines of logs (scrolls to end).     |

### Examples

```bash
# Start a specific project
compose up genai/ollama

# View logs (auto-scroll to bottom, default 100 lines)
compose logs genai/ollama -f

# Check system containers (Global view)
compose ps

# Update images and restart project only if changes detected
compose update genai/ollama

# Just pull images
compose pull genai/ollama
```

### Configuration

Manage your `compose.env` settings (stored in `/etc/compose.env` or `~/.config/docker/compose.env`):

```bash
# View current configuration
compose config

# Update a setting
compose config --acme-email user@example.com
```

### Dependency Management

Configure Systemd dependencies (`Wants=`, `Requires=`, `After=`) using drop-in files:

```bash
# Add a dependency
compose deps web-app --add db-service

# Add a required dependency
compose deps main-app --add keycloak --requires

# List dependencies
compose deps web-app --list
```

## Environment Variables

The tool respects the following variables in `compose.env`:

- `COMPOSE_BASE`: Root directory for all compose projects.
- `COMPOSE_DATA`: Base directory for persistent data.
- `DOCKER_HOST`: Docker daemon endpoint.
- `TRAEFIK_ACME_*`: Traefik-specific SSL configuration.

## Development

```bash
# Build binary
cargo build --release

# Run tests
cargo test
```
