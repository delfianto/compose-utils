# Command Reference

## compose -- Docker Compose Project Helper

Direct container operations with project discovery and centralized configuration. Calls `docker compose` directly without systemd indirection.

### compose up

Start containers for one or more projects.

```
compose up [services...]
```

Alias: `compose up` is the subcommand name; no separate alias needed.

- Resolves service names (auto-detects from CWD if none provided)
- Runs `docker compose up -d` in each project directory
- Sets `DOCKER_HOST` if configured

### compose down

Stop containers for one or more projects.

```
compose down [services...]
```

- Runs `docker compose down --remove-orphans` in each project directory

### compose restart

Restart containers (down + up).

```
compose restart [services...]
```

Alias: `reup`

- Calls `compose down` then `compose up` in sequence

### compose pull

Pull latest images without restarting.

```
compose pull [services...]
```

- Runs `docker compose pull` in each project directory
- Continues on failure (prints warning)

### compose ps

List all Docker containers with formatted status.

```
compose ps [services...]
```

- Runs `docker ps -a --format '{{json .}}'`
- Renders ASCII table with columns: ID, IMAGE/TAG, NAME, PORTS, STATUS
- Status includes emoji for state and colored dot for health

### compose config

View or update global configuration.

```
compose config [options]
```

See [configuration.md](configuration.md) for details.

---

## composectl -- Systemd Service Controller

Manages Docker Compose projects as systemd services. All lifecycle commands go through `systemctl`.

### composectl start

Start services via systemd.

```
composectl start [services...] [--deps <path>]
```

- Optionally loads dependency configuration from a TOML file
- Calls `systemctl start compose@<name>.service` for each service
- Reports final ActiveState

### composectl stop

Stop services via systemd.

```
composectl stop [services...]
```

- Calls `systemctl stop compose@<name>.service`

### composectl restart

Restart services via systemd.

```
composectl restart [services...]
```

- Calls `systemctl restart compose@<name>.service`
- Reports final ActiveState

### composectl update

Pull images and restart services.

```
composectl update [services...]
```

- For each service: runs `docker compose pull`, then `systemctl restart`

### composectl status

Show systemd unit status.

```
composectl status [services...]
```

- Calls `systemctl status compose@<name>.service --lines=0`

### composectl sync

Reconcile systemd's tracked unit state against the actual container state.

```
composectl sync [services...]
```

- Compares `ActiveState` against `docker compose ps --status running` for each service
- If systemd thinks a service is down but containers are actually running, runs `systemctl start` to adopt them (idempotent -- doesn't recreate anything)
- If systemd thinks a service is up but containers are actually gone, runs `systemctl stop` to clear the stale state
- No-op (just reports) if already in sync

Use this after mixing the `compose` persona (or a bare `docker compose` command) with `composectl` on the same project. See [systemd.md](systemd.md#detecting-and-fixing-drift) for details.

### composectl enable

Enable services to start on boot.

```
composectl enable [services...] [--deps <path>]
```

- Optionally loads and applies dependency configuration
- Calls `systemctl enable compose@<name>.service`

### composectl disable

Disable services from starting on boot.

```
composectl disable [services...]
```

- Calls `systemctl disable compose@<name>.service`

### composectl pull

Pull latest images without restarting.

```
composectl pull [services...]
```

- Same as `compose pull`

### composectl deps

Manage inter-service systemd dependencies.

```
composectl deps <service> --add <deps...> [--requires]
composectl deps <service> --remove <deps...>
composectl deps <service> --list
composectl deps <service> --clear
composectl deps                    # list all dependencies
```

| Flag | Description |
|------|-------------|
| `--add <deps...>` | Add services as dependencies (default: `Wants=`) |
| `--requires` | Use `Requires=` instead of `Wants=` when adding |
| `--remove <deps...>` | Remove services from dependencies |
| `--list` | Show dependency tree and explicit overrides |
| `--clear` | Remove all dependency overrides for the service |

Dependencies are stored as systemd drop-in files. See [systemd.md](systemd.md) for details.

### composectl config

View or update global configuration.

```
composectl config [options]
```

Same as `compose config`. See [configuration.md](configuration.md).

---

## Service Name Resolution

All commands accept service names in multiple formats:

| Input | Resolved Unit | Project Directory |
|-------|--------------|-------------------|
| `myapp` | `compose@myapp.service` | `COMPOSE_BASE/myapp` |
| `genai/ollama` | `compose@genai-ollama.service` | `COMPOSE_BASE/genai/ollama` |
| `genai-ollama` | `compose@genai-ollama.service` | `COMPOSE_BASE/genai/ollama` (if dir exists) |
| `compose@myapp.service` | `compose@myapp.service` | `COMPOSE_BASE/myapp` |
| `docker.service` | `docker.service` | (standard systemd unit) |

## Auto-Detection

If no service names are provided, the tool attempts to detect the project from the current working directory:

1. Check if CWD is under `COMPOSE_BASE`
2. Check if CWD contains a recognized compose file
3. Derive service name from the relative path (slashes become dashes)

```bash
cd ~/compose-projects/genai/ollama
compose up    # auto-detects "genai-ollama"
```

## Global Options

Both personas support:

| Flag | Description |
|------|-------------|
| `-v`, `--verbose` | Enable debug output to stderr |
| `--json` | Emit machine-readable JSON on stdout instead of human-readable text |
| `-h`, `--help` | Show help |

## JSON Output (`--json`)

Every command supports `--json`, which switches stdout to a single parseable JSON document per invocation instead of prose. This is meant for scripting and agentic harnesses that need to consume results programmatically.

- Progress/diagnostic lines (auto-detected service, "Loading dependencies...", etc.) are suppressed or moved to stderr in JSON mode, so stdout only ever contains the JSON document.
- Most commands emit `{"command": "<name>", "results": [ ... ]}`, one object per requested service.
- Commands that don't operate over a list of services (`config`, single-service `secret`/`deps` actions) emit a flat JSON object instead.
- On failure, the error is emitted as `{"status": "error", "error": "..."}` on stdout (rather than the default `Error: ...` text on stderr), and the process still exits non-zero.
- `secret list`/`secret get` wrap Infisical's own CLI output as an opaque `"raw"` string field, since that formatting isn't under this tool's control.
- `deps list` (with or without a service) returns `"edges"` (a flat `unit -> [direct reverse-dependents]` map) and `"states"` (`unit -> ActiveState`), built by recursively querying `systemctl show --property=RequiredBy,WantedBy,UpheldBy,PartOf,BoundBy --value` (the same edges `systemctl list-dependencies --reverse` draws), not by parsing that command's human-oriented tree/bullet rendering. Traversal is filtered to this tool's own `compose@*.service` units, dropping `default.target`/other systemd noise that every enabled unit points at but that conveys no real dependency information. A single service name also adds an `"overrides"` field with its explicit `Requires`/`Wants`/`After` drop-in config.

Example:

```bash
composectl status infra-traefik --json
```
```json
{
  "command": "status",
  "results": [
    {
      "service": "infra-traefik",
      "unit": "compose@infra-traefik.service",
      "active_state": "active",
      "sub_state": "exited",
      "load_state": "loaded",
      "unit_file_state": "enabled",
      "description": "Compose Service for infra-traefik"
    }
  ]
}
```
