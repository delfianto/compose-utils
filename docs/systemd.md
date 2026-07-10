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
Type=simple
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

2. **`Type=simple`** -- `composectl run-service` calls `exec()` to replace itself with `docker compose up` (foreground, no `-d`). The `docker compose` process stays alive as the unit's supervised main process, attached to the containers' lifecycle. If the containers die for any reason, `docker compose up` notices and exits, and systemd sees the real process exit and updates `ActiveState` immediately -- no stale bookkeeping like the old `Type=oneshot` + `RemainAfterExit=yes` setup, which only tracked whether `ExecStart` had last been run, not whether anything was actually still up.

   This only closes the "systemd thinks it's up but it's actually down" gap automatically. If containers are started completely outside systemd (e.g. a manual `docker compose up` in the project directory), systemd never spawned that process and has no way to observe it -- see [Detecting and Fixing Drift](#detecting-and-fixing-drift) below.

3. **`ExecStop`** -- When the unit is stopped, systemd runs `composectl stop-service %i` (which calls `docker compose down --remove-orphans`) while the main process is still attached; the containers going away causes the main `docker compose up` process to exit on its own shortly after.

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
#     -> exec docker compose up   (foreground, tracked as the unit's main process)

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

- **`composectl run-service <name>`** -- Resolves the project directory and calls `exec("docker", ["compose", "up"])` (foreground). The `exec()` replaces the process, so systemd tracks the docker compose process directly as the unit's main PID.

- **`composectl stop-service <name>`** -- Resolves the project directory and calls `exec("docker", ["compose", "down", "--remove-orphans"])`.

These use Unix `exec()` (process replacement) rather than spawning a child process, which gives systemd accurate, direct process tracking for `Type=simple`.

## Detecting and Fixing Drift

Systemd only knows about processes it spawned. If a compose project is started or stopped **outside** systemd -- e.g. running `compose up`/`compose down` (the direct persona) or a bare `docker compose` command in the project directory -- systemd's `ActiveState` can drift out of sync with what's actually running:

| Systemd believes | Containers actually are | Effect on `composectl start`/`stop` |
|---|---|---|
| inactive | running | `composectl stop` no-ops -- systemd has nothing to stop |
| active | gone | `composectl start` no-ops -- systemd already thinks it's up |

`Type=simple` fixes the second case automatically going forward (systemd notices the moment its supervised process dies), but it can't fix the first case -- there's no unit configuration that makes systemd aware of a process it never spawned.

`composectl sync [services...]` detects and corrects both directions:

```bash
composectl sync myapp
```

For each service, it compares `ActiveState` (via `systemctl show`) against the real container state (`docker compose ps --status running` in the project directory):

- **Systemd down, containers up** -- runs `systemctl start`. Since `docker compose up` is idempotent, this doesn't recreate the containers -- it attaches to the already-running ones and adopts them under systemd's supervision.
- **Systemd up, containers down** -- runs `systemctl stop` to clear the stale `active` state.
- **In sync** -- no action, just reports the current state.

Run it after any manual `docker compose`/`compose` invocation on a project that's also managed by systemd, or periodically if the two are mixed regularly.
