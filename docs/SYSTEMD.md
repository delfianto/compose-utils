# Systemd Integration

## Unit Template

The tool installs a parameterized systemd unit template `compose@.service`:

```ini
[Unit]
Description=Compose Service for %i
Requires=docker.service
After=docker.service
BindsTo=docker.service
StartLimitBurst=5
StartLimitIntervalSec=100

[Service]
Type=oneshot
RemainAfterExit=yes
EnvironmentFile=-/etc/compose.env
EnvironmentFile=-%h/.config/compose.env
EnvironmentFile=-%h/.config/docker/compose.env
ExecStart=/path/to/composectl run-service %i
ExecStop=/path/to/composectl stop-service %i
Restart=on-failure
RestartSec=10
TimeoutStopSec=60

[Install]
WantedBy=default.target
```

### How it works

1. **`%i` substitution** -- systemd replaces `%i` with the instance name. For `compose@myapp.service`, `%i` is `myapp`.

2. **`Type=oneshot` + `RemainAfterExit=yes`** -- `composectl run-service` calls `exec()` to replace itself with `docker compose up -d`. Once the containers are started, the process exits. `RemainAfterExit=yes` keeps the unit in "active" state even though no process is running (the containers are managed by Docker).

3. **`ExecStop`** -- When the unit is stopped, systemd runs `composectl stop-service %i`, which calls `docker compose down --remove-orphans`.

4. **`BindsTo=docker.service`** -- If the Docker daemon stops, all compose services are automatically stopped too.

5. **Rate limiting** -- `StartLimitBurst=5` and `StartLimitIntervalSec=100` prevent rapid restart loops. Max 5 restart attempts per 100 seconds.

6. **Graceful shutdown** -- `TimeoutStopSec=60` gives containers 60 seconds to shut down before being killed.

### Unit location

| Mode | Path |
|------|------|
| Root | `/etc/systemd/system/compose@.service` |
| Rootless | `~/.config/systemd/user/compose@.service` |

## Dependency Management

Dependencies between compose services are managed via **systemd drop-in override files**. These are small INI files placed in a `.d` directory alongside the unit.

### File structure

```
<systemd_dir>/compose@<service>.service.d/dependencies.conf
```

Example for rootless:
```
~/.config/systemd/user/compose@web-app.service.d/dependencies.conf
```

### File format

```ini
[Unit]
Requires=compose@db.service
Wants=compose@cache.service
BindsTo=docker.service
After=compose@db.service
After=docker.service
After=compose@cache.service
```

### Dependency types

| Directive | Behavior | Use case |
|-----------|----------|----------|
| `Requires=` | Hard dependency. If the required unit fails to start, this unit fails too. | Database must be running |
| `Wants=` | Soft dependency. This unit starts even if the wanted unit fails. | Optional cache service |
| `BindsTo=` | Like Requires, but also stops this unit if the bound unit stops. | Docker daemon |
| `After=` | Ordering only. Start this unit after the listed units. Applied automatically when adding Requires/Wants. | Startup sequencing |

### Managing dependencies

**Add a soft dependency:**
```bash
composectl deps web-app --add cache
```
Adds `Wants=compose@cache.service` and `After=compose@cache.service`.

**Add a hard dependency:**
```bash
composectl deps web-app --add db --requires
```
Adds `Requires=compose@db.service` and `After=compose@db.service`.

**Remove a dependency:**
```bash
composectl deps web-app --remove cache
```
Removes from `Requires=`, `Wants=`, and `After=`.

**View dependencies:**
```bash
composectl deps web-app --list
```
Shows the systemd dependency tree and any explicit overrides.

**Clear all overrides:**
```bash
composectl deps web-app --clear
```
Deletes the `dependencies.conf` file entirely.

### Bulk configuration via TOML

Dependencies can be defined in a TOML file and applied in bulk:

```toml
[dependencies.web-app]
requires = ["db", "docker.service"]
wants = ["cache", "queue"]

[dependencies.worker]
requires = ["db"]
binds_to = ["docker.service"]
after = ["web-app"]
```

Apply with:
```bash
composectl start web-app worker --deps deps.toml
composectl enable web-app worker --deps deps.toml
```

The `--deps` flag loads the file, applies dependencies for all listed services, runs `systemctl daemon-reload`, then proceeds with the start/enable operation.

### Automatic dependencies

When applying dependencies via `apply_dependencies()`, the tool always adds:

- `Requires=docker.service`
- `BindsTo=docker.service`

These standard dependencies ensure compose services are tied to the Docker daemon lifecycle.

If `requires` is set but `binds_to` is not explicitly provided, `BindsTo` defaults to matching `Requires` (ensuring bound lifecycle).

## Root vs Rootless

All systemd operations detect the privilege level and adjust commands:

| Operation | Root | Rootless |
|-----------|------|----------|
| systemctl command | `systemctl <action>` | `systemctl --user <action>` |
| Unit directory | `/etc/systemd/system` | `~/.config/systemd/user` |
| Override directory | `/etc/systemd/system/compose@*.service.d/` | `~/.config/systemd/user/compose@*.service.d/` |
| Default target | `default.target` | `default.target` |

The `--user` flag is added automatically when `Context.is_root` is false. All systemctl commands also set `LC_ALL=C.UTF-8` for consistent output encoding.

## Lifecycle Example

```
# 1. Enable a service to start on boot
composectl enable myapp

# 2. Start it now
composectl start myapp

# Systemd runs:
#   composectl run-service myapp
#     -> cd /home/user/compose-projects/myapp
#     -> exec docker compose up -d

# 3. Check status
composectl status myapp
#   Shows systemctl status output

# 4. Update images and restart
composectl update myapp
#   -> docker compose pull (in project dir)
#   -> systemctl --user restart compose@myapp.service

# 5. Stop
composectl stop myapp
#   Systemd runs:
#     composectl stop-service myapp
#       -> exec docker compose down --remove-orphans
```

## Internal Commands

The hidden `run-service` and `stop-service` subcommands are called by the systemd unit, not by users directly:

- **`composectl run-service <name>`** -- Resolves the project directory and calls `exec("docker", ["compose", "up", "-d"])`. The `exec()` replaces the process, so systemd tracks the docker compose process directly.

- **`composectl stop-service <name>`** -- Resolves the project directory and calls `exec("docker", ["compose", "down", "--remove-orphans"])`.

These use Unix `exec()` (process replacement) rather than spawning a child process, which gives systemd accurate process tracking for the oneshot unit type.
