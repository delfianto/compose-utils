# Project Structure

## Directory Layout

```
compose-utils/
|
+-- Cargo.toml                  # Package manifest, dependencies, build profile
+-- Cargo.lock                  # Dependency lockfile
+-- README.md                   # User-facing documentation
+-- PLAN.md                     # Refactoring plan (temporary)
+-- LICENSE
+-- Justfile                    # Task runner recipes
|
+-- docs/                       # Technical documentation
|   +-- architecture.md         # System design, data flow, patterns
|   +-- structure.md            # This file -- project layout
|   +-- dependencies.md         # External crates and rationale
|   +-- commands.md             # Command reference for both personas
|   +-- configuration.md        # Config file format, paths, validation
|   +-- systemd.md              # Systemd integration details
|
+-- src/                        # Rust source code
|   +-- main.rs                 # Entry point, CLI definitions, dispatch
|   +-- commands.rs             # Command module aggregation
|   +-- commands/
|   |   +-- compose_direct.rs   # Direct docker compose operations
|   |   +-- service.rs          # Systemd service lifecycle
|   |   +-- ps.rs               # Container listing (mirrors the docker-pps CLI plugin)
|   |   +-- pull.rs             # Image pulling
|   |   +-- update.rs           # Pull + restart
|   |   +-- config.rs           # Configuration management
|   |   +-- deps.rs             # Systemd dependency overrides
|   |   +-- internal.rs         # Commands called by systemd unit
|   |
|   +-- core.rs                 # Core module aggregation
|   +-- core/
|   |   +-- constants.rs        # Path constants, config keys
|   |   +-- context.rs          # Context struct, env parsing, mode detection
|   |   +-- validation.rs       # Input validators (domain, email, URL, path)
|   |   +-- verbose.rs          # Global debug flag and macro
|   |   +-- output.rs           # Global JSON-mode flag, Report<T> envelope
|   |
|   +-- systemd.rs              # Systemd module aggregation
|   +-- systemd/
|   |   +-- manager.rs          # systemctl command wrappers
|   |   +-- service.rs          # Unit name normalization, path resolution
|   |   +-- discovery.rs        # CWD-based project auto-detection
|   |
|   +-- compose.rs              # TOML dependency config parser
|
+-- systemd/                    # Systemd integration files
    +-- compose@.service        # Parameterized unit template
    +-- compose.env             # Example configuration file
    +-- install.sh              # Build + install script
```

## Module Descriptions

### `src/main.rs`

Entry point. Defines two separate `clap` CLI structs (`ComposeCli` and `CtlCli`) and dispatches based on `argv[0]`. No business logic lives here -- it only parses args, initializes context, and routes to command handlers.

### `src/commands/`

Each file implements one or more CLI subcommands. All command functions take `&Context` as their first argument and return `anyhow::Result<()>`.

| File | Persona | Purpose |
|------|---------|---------|
| `compose_direct.rs` | compose | `docker compose up/down` directly |
| `service.rs` | composectl | `systemctl start/stop/restart/enable/disable/status` |
| `ps.rs` | compose | Container listing, mirroring the `docker pps` CLI plugin's brief format |
| `pull.rs` | both | `docker compose pull` per project |
| `update.rs` | composectl | Pull images then `systemctl restart` |
| `config.rs` | both | Read/write `compose.env` with validation |
| `deps.rs` | composectl | Manage systemd drop-in override files |
| `internal.rs` | composectl | `run-service`/`stop-service` (called by systemd, not users) |

### `src/core/`

Infrastructure shared by all commands.

| File | Purpose |
|------|---------|
| `constants.rs` | Path constants for root/rootless modes, config key names, compose filenames |
| `context.rs` | `Context` struct, `get_context()` initialization, `read_env_file()` parser |
| `validation.rs` | `validate_directory`, `validate_domain`, `validate_email`, `validate_acme_server`, `validate_docker_host` |
| `verbose.rs` | `enable()`, `is_enabled()`, `verbose!` macro for debug output to stderr |
| `output.rs` | Global JSON-mode flag, `Report<T>` result envelope, `print_json()` |

### `src/systemd/`

Systemd-specific logic.

| File | Purpose |
|------|---------|
| `manager.rs` | Wrappers around `systemctl` (start, stop, enable, daemon-reload, etc.). Adds `--user` flag for rootless mode. |
| `service.rs` | Service name normalization (`myapp` to `compose@myapp.service`), directory path resolution (dash-to-slash conversion), bare name extraction. |
| `discovery.rs` | Auto-detects service from CWD by checking if current directory is under `compose_base` and contains a compose file. |

### `src/compose.rs`

Parses TOML dependency configuration into `DependenciesConfig` / `ServiceConfig` structs.

### `systemd/`

Files installed alongside the binary.

| File | Purpose |
|------|---------|
| `compose@.service` | Systemd unit template. `%i` is substituted with the project name. Calls `composectl run-service %i` on start. |
| `compose.env` | Example/template configuration file with all supported keys. |
| `install.sh` | Builds the binary, installs it as `composectl` with a `compose` symlink, installs the unit template, and guides configuration. |

## Test Distribution

Tests are colocated with their source files via `#[cfg(test)] mod tests`.

| File | Tests |
|------|-------|
| `commands/compose_direct.rs` | 9 |
| `commands/config.rs` | 44 |
| `commands/deps.rs` | 25 |
| `commands/ps.rs` | 38 |
| `commands/secret.rs` | 13 |
| `compose.rs` | 11 |
| `core/context.rs` | 34 |
| `core/output.rs` | 2 |
| `core/validation.rs` | 61 |
| `core/verbose.rs` | 3 |
| `systemd/discovery.rs` | 10 |
| `systemd/service.rs` | 32 |
| **Total** | **282** |
