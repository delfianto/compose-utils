# Architecture

## Overview

Compose Utils is a **multi-call binary** that ships two CLI personas from a single compiled executable:

- **`compose`** -- Direct Docker Compose project operations (up, down, pull, ps)
- **`composectl`** -- Systemd service controller for Docker Compose projects (start, stop, enable, deps)

The active persona is determined at runtime by inspecting `argv[0]` (the binary name). A symlink from `compose` to `composectl` enables both names from one binary, following the same pattern used by BusyBox and similar tools.

```
composectl (binary)
  |
  |-- argv[0] == "composectl" --> CtlCli (systemd commands)
  |-- argv[0] == "compose"    --> ComposeCli (docker compose commands)
  `-- argv[0] == anything else --> ComposeCli (default)
```

## Design Principles

1. **Zero runtime dependencies.** The binary is statically optimized and stripped. It shells out to `docker`, `docker compose`, and `systemctl` -- all expected to be present on a system running Docker with systemd.

2. **Root and rootless aware.** The tool detects privilege level at startup via `geteuid()` and selects appropriate paths (system-wide vs. user-local) for systemd units, configuration, and Docker sockets.

3. **Convention over configuration.** Projects live under a single base directory (`COMPOSE_BASE`). The tool discovers them by directory name and the presence of a recognized compose file (`compose.yaml`, `docker-compose.yml`, etc.).

4. **Synchronous execution.** All operations are blocking `std::process::Command` calls. There is no async runtime -- the binary is lean and fast to start.

## Module Dependency Graph

```
main.rs
 |
 +-- commands/
 |    +-- compose_direct.rs   (compose: up, down, restart)
 |    +-- service.rs          (composectl: start, stop, restart, status, enable, disable)
 |    +-- ps.rs               (compose: container listing)
 |    +-- pull.rs             (shared: image pulling)
 |    +-- update.rs           (composectl: pull + systemd restart)
 |    +-- config.rs           (shared: configuration management)
 |    +-- deps.rs             (composectl: systemd dependency overrides)
 |    +-- internal.rs         (composectl: run-service / stop-service for systemd unit)
 |
 +-- core/
 |    +-- constants.rs        (path constants, config keys)
 |    +-- context.rs          (Context struct, env file parsing, mode detection)
 |    +-- validation.rs       (domain, email, URL, path validators)
 |    +-- verbose.rs          (global debug flag + macro)
 |    +-- output.rs           (global JSON-mode flag, Report<T> envelope, print_json)
 |
 +-- systemd/
 |    +-- manager.rs          (systemctl command wrappers)
 |    +-- service.rs          (unit name normalization, path resolution)
 |    +-- discovery.rs        (CWD-based project auto-detection)
 |
 +-- compose/
 |    +-- dependencies.rs     (TOML dependency config parsing)
 |
 +-- display/
      +-- status.rs           (container state emojis, health dots)
      +-- table.rs            (ASCII table renderer with ANSI support)
```

**Key coupling points:**

- Every command receives an immutable `&Context` constructed once in `main()`.
- All service-related commands depend on `systemd::service` for name normalization and `systemd::discovery` for auto-detection.
- Only `commands/service.rs` and `commands/deps.rs` call into `systemd::manager`.
- `commands/compose_direct.rs` bypasses systemd entirely -- it calls `docker compose` directly.

## Data Flow

### compose up (direct)

```
User: compose up myapp
  |
  v
run_compose() --> ComposeCli::parse()
  |
  v
resolve_services(ctx, ["myapp"])
  |  returns ["myapp"]
  v
get_compose_dir(ctx, "myapp")
  |  returns /home/user/compose-projects/myapp
  v
Command::new("docker").args(["compose", "up", "-d"]).current_dir(dir)
  |
  v
docker compose up -d   (runs directly)
```

### composectl start (via systemd)

```
User: composectl start myapp
  |
  v
run_composectl() --> CtlCli::parse()
  |
  v
resolve_services(ctx, ["myapp"])
  |  returns ["myapp"]
  v
normalize_unit_name(ctx, "myapp")
  |  returns "compose@myapp.service"
  v
systemctl [--user] start compose@myapp.service
  |
  v  (systemd reads compose@.service template)
composectl run-service myapp
  |
  v
get_compose_dir(ctx, "myapp")
  |
  v
exec("docker", ["compose", "up", "-d"])  (replaces process)
```

### composectl deps (systemd overrides)

```
User: composectl deps web-app --add db --requires
  |
  v
normalize_unit_name(ctx, "db")
  |  returns "compose@db.service"
  v
parse_override_file(~/.config/systemd/user/compose@web-app.service.d/dependencies.conf)
  |
  v
Insert "compose@db.service" into Requires= and After=
  |
  v
write_override_file(...)
  |
  v
systemctl --user daemon-reload
```

## Context Initialization

The `Context` struct is the single source of truth for all path and mode decisions:

```rust
pub struct Context {
    pub is_root: bool,          // geteuid() == 0
    pub systemd_dir: PathBuf,   // where unit files live
    pub compose_base: PathBuf,  // where projects live
    pub env_file: PathBuf,      // compose.env location
    pub docker_host: Option<String>,  // DOCKER_HOST override
}
```

**Root mode paths:**
| Field | Value |
|-------|-------|
| `systemd_dir` | `/etc/systemd/system` |
| `compose_base` | `/srv/compose` (or from config) |
| `env_file` | `/etc/compose.env` |

**Rootless mode paths:**
| Field | Value |
|-------|-------|
| `systemd_dir` | `~/.config/systemd/user` |
| `compose_base` | `~/compose-projects` (or from config) |
| `env_file` | `~/.config/docker/compose.env` |

Startup validation ensures the Docker socket exists for the detected mode and fails early with a clear error if the wrong privilege level is used.

## Service Name Resolution

Projects can be referenced multiple ways. The normalization pipeline handles all of them:

| Input | Normalized | Directory |
|-------|-----------|-----------|
| `myapp` | `compose@myapp.service` | `compose_base/myapp` |
| `genai/ollama` | `compose@genai-ollama.service` | `compose_base/genai/ollama` |
| `genai-ollama` | `compose@genai-ollama.service` | `compose_base/genai/ollama` (if exists) |
| `compose@myapp.service` | `compose@myapp.service` | `compose_base/myapp` |
| `docker.service` | `docker.service` | (standard unit, not a project) |

The resolution prefers flat directory names over dash-to-slash conversion, so `my-project/` as a literal directory takes precedence over `my/project/`.

## Systemd Integration

The tool installs a **parameterized unit template** `compose@.service`:

```ini
[Service]
Type=simple
ExecStart=/path/to/composectl run-service %i
ExecStop=/path/to/composectl stop-service %i
```

- `%i` is the instance parameter (e.g., `myapp` from `compose@myapp.service`).
- `Type=simple` means `ExecStart` (`exec`'d into `docker compose up`, foreground) stays alive as the unit's tracked main process, so systemd's `ActiveState` reflects real container state going forward -- no `RemainAfterExit` bookkeeping that can silently go stale.
- `ExecStop` ensures `docker compose down` runs on service stop.
- `BindsTo=docker.service` ties the lifecycle to the Docker daemon.
- Systemd still can't observe containers started **outside** it (e.g. a manual `docker compose up`). `composectl sync` reconciles that drift -- see [systemd.md](systemd.md#detecting-and-fixing-drift).

**Dependency overrides** are managed via systemd drop-in files:

```
~/.config/systemd/user/compose@web-app.service.d/dependencies.conf
```

These add `Requires=`, `Wants=`, `BindsTo=`, and `After=` directives without modifying the base unit template.

## Error Handling

All functions return `anyhow::Result<T>`. Errors are enriched with context at each layer:

```rust
fs::read_to_string(path)
    .with_context(|| format!("Failed to read dependency file: {}", path.display()))?;
```

External command failures capture stderr and include the action + unit name in the error message:

```
Failed to start compose@myapp.service: Unit not found.
```

## Testing Strategy

The test suite (206 tests) focuses on **pure functions and file-based operations** that can be tested without external services:

- **Unit name normalization** -- all input variants and idempotency
- **File parsing** -- env files, TOML configs, systemd override files
- **Validation** -- domain, email, URL, path validators with edge cases
- **Display** -- table rendering, ANSI handling, status formatting
- **JSON deserialization** -- docker ps output parsing

Commands that shell out to `docker` or `systemctl` are not unit tested -- they require integration test infrastructure with real Docker and systemd.
