# Infisical Integration Plan

## Problem

Every compose service needs a `.env.local` (and sometimes `{service}.env.local`) for secrets -- passwords, API keys, tokens. These files are gitignored, manually created on the server, and easy to forget or misconfigure. Infisical (already self-hosted at `infra/infisical`) should replace them.

## Bootstrap Problem

Infisical itself depends on PostgreSQL and Valkey. Those services must start before Infisical is available, so they cannot pull secrets from it. This creates a two-tier architecture:

| Tier | Services | Secret source | Why |
|------|----------|---------------|-----|
| **Tier 0** (bootstrap) | `db/postgres`, `db/valkey`, `infra/infisical` | File-based (Docker secrets, `.env.local`) | Must start before Infisical is available |
| **Tier 1** (managed) | Everything else | Infisical via `infisical run` | Infisical is available by the time these start |

Startup order: `db-postgres` + `db-valkey` -> `infra-infisical` -> all Tier 1 services.

Valkey currently has no auth, so it has no secrets at all. Only `db/postgres` and `infra/infisical` permanently keep file-based secrets.

## Integration Approach

### Where the change lives

The `compose` wrapper in `src/commands/compose_direct.rs` is the single entry point for all `docker compose` calls. The Infisical injection hooks into this layer -- before running `docker compose up`, the wrapper checks whether to prepend `infisical run`.

### How `infisical run` works

```bash
infisical run \
  --projectId "<project-id>" \
  --env production \
  --path "/db-mariadb" \
  -- docker compose up -d
```

This command:
1. Authenticates with Infisical (via `INFISICAL_TOKEN` or machine identity)
2. Fetches all secrets at the given path + environment
3. Injects them as environment variables into the child process
4. Executes `docker compose up -d` with those variables available

Secrets reach containers through `environment:` directives and shell interpolation in compose files. No `.env.local` files needed.

### Secret path convention

Secrets are organized in Infisical by service name matching the systemd naming convention:

```
/<project-id>/production/
  db-mariadb/
    MARIADB_PASSWORD=...
    MARIADB_ROOT_PASSWORD=...
  db-mongo/
    MONGO_INITDB_ROOT_PASSWORD=...
  ai-ollama/
    OLLAMA_API_KEY=...
  ai-openwebui/
    WEBUI_SECRET_KEY=...
    OPENAI_API_KEY=...
  media-immich/
    DB_PASSWORD=...
  ...
```

The path segment matches what `get_bare_name()` returns with `/` replaced by `-` (e.g., `db/mariadb` -> `db-mariadb`).

## Implementation Plan

### 1. Configuration additions

Add to `compose.env`:

```bash
# Infisical integration (optional -- if unset, Infisical is skipped)
INFISICAL_PROJECT_ID=<project-id>
INFISICAL_ENV=production
INFISICAL_ADDRESS=https://infisical.yourdomain.com
```

Add to `Context` struct:

```rust
pub infisical_project_id: Option<String>,
pub infisical_env: Option<String>,
pub infisical_address: Option<String>,
```

### 2. Bootstrap tier detection

Define bootstrap services that skip Infisical injection. Two options:

**Option A: Hardcoded list** (simplest):
```rust
const BOOTSTRAP_SERVICES: &[&str] = &["db/postgres", "db/valkey", "infra/infisical"];
```

**Option B: Config-driven** (more flexible):
Add `INFISICAL_BOOTSTRAP` to `compose.env`:
```bash
INFISICAL_BOOTSTRAP=db/postgres,db/valkey,infra/infisical
```

Recommendation: Option B. The bootstrap list may grow if future services become Infisical dependencies.

### 3. Token management

The `infisical run` command needs authentication. Two strategies:

**Strategy A: Pre-exported token (current recommendation)**

The systemd unit's `EnvironmentFile` or a drop-in provides `INFISICAL_TOKEN`. The compose wrapper passes it through. Token is generated once via:

```bash
export INFISICAL_TOKEN=$(infisical login \
  --method=universal-auth \
  --client-id=<id> \
  --client-secret=<secret> \
  --silent --plain)
```

Store client-id and client-secret in `/etc/compose.d/infisical.env` or similar.

**Strategy B: Machine identity files**

Store client-id and client-secret as files on disk. The wrapper calls `infisical login` before each `infisical run`. More secure (short-lived tokens) but adds latency to every compose operation.

Recommendation: Strategy A for systemd-managed services (token in environment), Strategy B for interactive CLI usage.

### 4. Code change in compose_direct.rs

The core change is wrapping the `docker compose` command with `infisical run` for Tier 1 services:

```rust
fn build_compose_command(ctx: &Context, bare: &str, args: &[&str]) -> Command {
    let dir = get_compose_dir(ctx, bare);
    let service_path = bare.replace('/', "-");

    // Check if Infisical is configured and this isn't a bootstrap service
    if let Some(ref project_id) = ctx.infisical_project_id {
        if !ctx.is_bootstrap_service(bare) && which::which("infisical").is_ok() {
            let mut cmd = Command::new("infisical");
            cmd.args(["run",
                "--projectId", project_id,
                "--env", ctx.infisical_env.as_deref().unwrap_or("production"),
                "--path", &format!("/{}", service_path),
                "--",
                "docker", "compose",
            ]);
            cmd.args(args);
            cmd.current_dir(&dir);

            if let Some(ref addr) = ctx.infisical_address {
                cmd.env("INFISICAL_API_URL", addr);
            }
            if let Some(ref host) = ctx.docker_host {
                cmd.env("DOCKER_HOST", host);
            }

            return cmd;
        }
    }

    // Fallback: plain docker compose
    let mut cmd = Command::new("docker");
    cmd.args(["compose"]);
    cmd.args(args);
    cmd.current_dir(&dir);

    if let Some(ref host) = ctx.docker_host {
        cmd.env("DOCKER_HOST", host);
    }

    cmd
}
```

This extracts command construction so `compose_up`, `compose_down`, `compose_pull`, etc. all benefit.

### 5. Systemd unit changes

The systemd template unit needs the Infisical token available. Add to the global environment file or a drop-in:

```ini
# /etc/systemd/system/docker-compose@.service.d/infisical.conf
[Service]
EnvironmentFile=-/etc/compose.d/infisical.env
```

Contents of `/etc/compose.d/infisical.env`:
```bash
INFISICAL_TOKEN=<long-lived-machine-identity-token>
```

The `-` prefix makes the file optional -- if it doesn't exist, systemd ignores it. This means bootstrap services (which start before Infisical) won't fail on a missing token.

### 6. Compose file changes (in the compose repo)

For Tier 1 services, secrets currently in `.env.local` files move to Infisical. The compose files need their secret variables listed in `environment:` so that `infisical run` can inject them:

**Before** (file-based):
```yaml
env_file:
  - ./mariadb.env
  - ./mariadb.env.local    # <-- contains MARIADB_PASSWORD
```

**After** (Infisical-injected):
```yaml
env_file:
  - ./mariadb.env          # non-secret config only
environment:
  MARIADB_PASSWORD: ${MARIADB_PASSWORD}  # injected by infisical run
```

Or, if the service reads `_FILE` variants (Docker secrets pattern), keep using Docker secrets for bootstrap and `environment:` for managed services.

### 7. Graceful degradation

If Infisical is unavailable (network issue, token expired, first deploy):
- The wrapper should fall back to plain `docker compose` (which will use `.env.local` if present)
- Log a warning, don't hard-fail
- This means `.env.local` files can coexist during migration -- remove them per-service once Infisical is confirmed working

```rust
// Try infisical run, fall back to plain docker compose on failure
let status = infisical_cmd.status();
if status.is_err() || !status.unwrap().success() {
    eprintln!("Warning: infisical run failed for {}, falling back to docker compose", bare);
    return run_plain_compose(ctx, bare, args);
}
```

## Migration Checklist

For each Tier 1 service:

1. [ ] Create secret path in Infisical (`/db-mariadb`, `/ai-ollama`, etc.)
2. [ ] Add all secrets from `.env.local` to Infisical at that path
3. [ ] Update compose.yaml: add `environment:` entries for injected vars
4. [ ] Remove `env_file:` reference to `.env.local`
5. [ ] Test with `infisical run -- docker compose config` to verify interpolation
6. [ ] Test with `compose up` to verify the wrapper injects correctly
7. [ ] Delete `.env.local` file from server

## Services to Migrate

### Tier 0 (no migration -- keep file-based)
- `db/postgres` -- `password.txt` (Docker secret)
- `db/valkey` -- no secrets
- `infra/infisical` -- `db_uri.txt`, `encryption_key.txt`, `auth_secret.txt` (Docker secrets)

### Tier 1 (migrate to Infisical)
- `db/mariadb` -- `MARIADB_PASSWORD`, `MARIADB_ROOT_PASSWORD`
- `db/mongo` -- `MONGO_INITDB_ROOT_PASSWORD` (currently in `mongodb.env.local`)
- `db/qdrant` -- API key (if configured)
- `ai/bifrost` -- `BIFROST_ENCRYPTION_KEY`, API keys
- `ai/ollama` -- API keys (if any)
- `ai/embedding` -- `HF_TOKEN` (HuggingFace API token)
- `ai/openwebui` -- `WEBUI_SECRET_KEY`, upstream API keys
- `ai/librechat` -- `CREDS_KEY`, `CREDS_IV`, API keys, `MONGO_PASSWORD`
- `ai/comfyui` -- API keys (if any)
- `ai/forge` -- (if any)
- `infra/forgejo` -- database password, secret keys
- `media/immich` -- `DB_PASSWORD`, upload credentials
- `media/photoprism` -- `PHOTOPRISM_ADMIN_PASSWORD`, database password
- `media/plex` -- `PLEX_CLAIM` token
- `media/stash` -- API key (if any)
- `panel/*` -- Traefik Cloudflare token, Homepage API keys, Portainer credentials

## Dependencies

- `infisical` CLI must be installed on the host (`/usr/local/bin/infisical`)
- Machine identity created in Infisical with access to the project
- `which` crate already in Cargo.toml for binary detection
