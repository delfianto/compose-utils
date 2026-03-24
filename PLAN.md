# Refactor Plan: Multi-Call Binary Split

## Goal

Split the single `compose` binary into a multi-call binary with two personas:

- **`compose`** — Docker Compose project helper (direct container operations)
- **`composectl`** — systemd service controller for compose projects

Single binary, single crate, shared internals. Behavior determined by `argv[0]`.

---

## Phase 1: Reorganize CLI Entry Point

### 1.1 Create two CLI structs in `src/main.rs`

Replace the single `Cli` + `Commands` enum with two separate sets:

**`ComposeCli` + `ComposeCommands`** (for `compose`):

```rust
#[derive(Parser)]
#[command(name = "compose")]
#[command(about = "Docker Compose project utilities")]
struct ComposeCli {
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: ComposeCommands,
}

#[derive(Subcommand)]
enum ComposeCommands {
    /// Start containers (docker compose up -d).
    #[command(visible_alias = "up")]
    Up {
        services: Vec<String>,
    },
    /// Stop containers (docker compose down).
    #[command(visible_alias = "down")]
    Down {
        services: Vec<String>,
        /// Also remove orphan containers.
        #[arg(long, default_value_t = true)]
        remove_orphans: bool,
    },
    /// Restart containers (docker compose down + up).
    Restart {
        services: Vec<String>,
    },
    /// Pull images for services without restarting.
    Pull {
        services: Vec<String>,
    },
    /// List Docker containers and their statuses.
    Ps {
        services: Vec<String>,
    },
    /// View or update global configuration.
    Config(config::ConfigArgs),
}
```

**`CtlCli` + `CtlCommands`** (for `composectl`):

```rust
#[derive(Parser)]
#[command(name = "composectl")]
#[command(about = "Systemd service controller for Docker Compose projects")]
struct CtlCli {
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: CtlCommands,
}

#[derive(Subcommand)]
enum CtlCommands {
    /// Start services (systemctl start).
    Start {
        services: Vec<String>,
        #[arg(long)]
        deps: Option<String>,
    },
    /// Stop services (systemctl stop).
    Stop {
        services: Vec<String>,
    },
    /// Restart services (systemctl restart).
    Restart {
        services: Vec<String>,
    },
    /// Pull new images and restart services via systemd.
    Update {
        services: Vec<String>,
    },
    /// Show systemd unit status.
    Status {
        services: Vec<String>,
    },
    /// Enable services to start on boot.
    Enable {
        services: Vec<String>,
        #[arg(long)]
        deps: Option<String>,
    },
    /// Disable services from starting on boot.
    Disable {
        services: Vec<String>,
    },
    /// Manage inter-service systemd dependencies.
    Deps(deps::DepsArgs),
    /// View or update global configuration.
    Config(config::ConfigArgs),

    // Internal (hidden) — called by the systemd unit template
    #[command(hide = true)]
    RunService { service: String },
    #[command(hide = true)]
    StopService { service: String },
}
```

### 1.2 Add `argv[0]` dispatch in `main()`

Note: Phase 7 has already removed all async/tokio. Everything is synchronous.

```rust
use std::path::PathBuf;

fn main() -> Result<()> {
    let binary_name = std::env::args()
        .next()
        .and_then(|s| {
            PathBuf::from(s)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "compose".to_string());

    match binary_name.as_str() {
        "composectl" => run_composectl(),
        _ => run_compose(),
    }
}
```

### 1.3 Implement the two dispatch functions

```rust
fn run_compose() -> Result<()> {
    let cli = ComposeCli::parse();
    if cli.verbose { enable_verbose(); }
    let ctx = get_context()?;

    match cli.command {
        ComposeCommands::Up { services } => commands::compose_up(&ctx, &services),
        ComposeCommands::Down { services, remove_orphans } => {
            commands::compose_down(&ctx, &services, remove_orphans)
        }
        ComposeCommands::Restart { services } => commands::compose_restart(&ctx, &services),
        ComposeCommands::Pull { services } => commands::run_pull(&ctx, &services),
        ComposeCommands::Ps { services } => commands::ps::run_ps(&ctx, &services),
        ComposeCommands::Config(args) => config::run(&ctx, args),
    }
}

fn run_composectl() -> Result<()> {
    let cli = CtlCli::parse();
    if cli.verbose { enable_verbose(); }
    let ctx = get_context()?;

    match cli.command {
        CtlCommands::Start { services, deps } => commands::run_start(&ctx, &services, deps),
        CtlCommands::Stop { services } => commands::run_stop(&ctx, &services),
        CtlCommands::Restart { services } => commands::run_restart(&ctx, &services),
        CtlCommands::Update { services } => commands::run_update(&ctx, &services),
        CtlCommands::Status { services } => commands::run_status(&ctx, &services),
        CtlCommands::Enable { services, deps } => commands::run_enable(&ctx, &services, deps),
        CtlCommands::Disable { services } => commands::run_disable(&ctx, &services),
        CtlCommands::Deps(args) => deps::run(&ctx, args),
        CtlCommands::Config(args) => config::run(&ctx, args),
        CtlCommands::RunService { service } => commands::internal::run_service(&ctx, &service),
        CtlCommands::StopService { service } => commands::internal::stop_service(&ctx, &service),
    }
}
```

### Files changed
- `src/main.rs` — rewrite

---

## Phase 2: Add Direct Docker Compose Commands

The `compose` persona needs new command implementations that call `docker compose` directly (no systemd indirection).

### 2.1 Create `src/commands/compose_direct.rs`

New module for direct docker compose operations:

```rust
//! Direct Docker Compose operations (no systemd indirection).

use crate::core::Context;
use crate::systemd::discovery::resolve_services;
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::{Context as _, Result};
use std::process::Command;

/// Run `docker compose up -d` directly in the project directory.
pub fn compose_up(ctx: &Context, names: &[String]) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        let dir = get_compose_dir(ctx, bare);

        println!("Starting {}...", bare);

        let mut cmd = Command::new("docker");
        cmd.args(["compose", "up", "-d"]).current_dir(&dir);

        if let Some(ref host) = ctx.docker_host {
            cmd.env("DOCKER_HOST", host);
        }

        let status = cmd
            .status()
            .with_context(|| format!("Failed to run docker compose up in {}", dir.display()))?;

        if !status.success() {
            anyhow::bail!("docker compose up failed for {}", bare);
        }

        println!("Started {}", bare);
    }

    Ok(())
}

/// Run `docker compose down` directly in the project directory.
pub fn compose_down(ctx: &Context, names: &[String], remove_orphans: bool) -> Result<()> {
    let services = resolve_services(ctx, names)?;

    for name in services {
        let bare = get_bare_name(&name);
        let dir = get_compose_dir(ctx, bare);

        println!("Stopping {}...", bare);

        let mut args = vec!["compose", "down"];
        if remove_orphans {
            args.push("--remove-orphans");
        }

        let mut cmd = Command::new("docker");
        cmd.args(&args).current_dir(&dir);

        if let Some(ref host) = ctx.docker_host {
            cmd.env("DOCKER_HOST", host);
        }

        let status = cmd
            .status()
            .with_context(|| format!("Failed to run docker compose down in {}", dir.display()))?;

        if !status.success() {
            anyhow::bail!("docker compose down failed for {}", bare);
        }

        println!("Stopped {}", bare);
    }

    Ok(())
}

/// Run `docker compose down` then `docker compose up -d`.
pub fn compose_restart(ctx: &Context, names: &[String]) -> Result<()> {
    compose_down(ctx, names, true)?;
    compose_up(ctx, names)
}
```

### 2.2 Register in `src/commands.rs`

Add `pub mod compose_direct;` and re-export the new functions:

```rust
pub mod compose_direct;
// ...
pub use compose_direct::{compose_up, compose_down, compose_restart};
```

### Files changed
- `src/commands/compose_direct.rs` — new file
- `src/commands.rs` — add module + re-exports

---

## Phase 3: Update Existing Modules (Minimal Changes)

The existing command modules (`service.rs`, `pull.rs`, `update.rs`, `ps.rs`, `deps.rs`, `config.rs`, `internal.rs`) remain **unchanged**. They already work correctly for the `composectl` persona.

### 3.1 `src/commands/pull.rs` — no changes needed

`run_pull` calls `docker compose pull` directly (no systemd). Used by both:
- `compose pull` (compose persona)
- `composectl update` (via `run_update` which calls `run_pull` then `restart_unit`)

### 3.2 `src/commands/ps.rs` — no changes needed

`run_ps` calls `docker ps` directly (no systemd). Used by `compose ps`.

### 3.3 `src/commands/service.rs` — no changes needed

All functions (`run_start`, `run_stop`, `run_restart`, `run_status`, `run_enable`, `run_disable`) call systemctl. Used exclusively by `composectl`.

### 3.4 `src/commands/deps.rs` — no changes needed

`deps` is purely a systemd concept. Used exclusively by `composectl deps`.

### 3.5 `src/commands/config.rs` — no changes needed

Shared configuration management. Available under both personas.

### 3.6 `src/commands/internal.rs` — no changes needed

`run_service` / `stop_service` are called by the systemd unit template via `composectl run-service`.

### Files changed
- None (verification pass only)

---

## Phase 4: Update Systemd Unit Template

### 4.1 Update `systemd/compose@.service`

Change `ExecStart` and `ExecStop` to reference `composectl`:

**Before:**
```ini
ExecStart=BINARY_PATH run-service %i
ExecStop=BINARY_PATH stop-service %i
```

**After:**
```ini
ExecStart=COMPOSECTL_PATH run-service %i
ExecStop=COMPOSECTL_PATH stop-service %i
```

The placeholder name change is optional but clarifies intent. The actual binary path is still substituted by `install.sh`.

### Files changed
- `systemd/compose@.service` — update ExecStart/ExecStop comments/placeholders

---

## Phase 5: Update Installation Script

### 5.1 Update `systemd/install.sh`

The install script needs to:

1. Build the single binary (still one `cargo build --release`)
2. Install it as `composectl` (the primary name)
3. Create a symlink `compose -> composectl`
4. Update the systemd unit template path substitution

**Key changes:**

```bash
# Build
cargo build --release

# Install binary as composectl
install -m 755 target/release/compose "$BIN_DIR/composectl"

# Create symlink for compose persona
ln -sf "$BIN_DIR/composectl" "$BIN_DIR/compose"

# Update unit template (use composectl path)
sed -i "s|BINARY_PATH|$BIN_DIR/composectl|g" "$SYSTEMD_DIR/compose@.service"
```

### 5.2 Update `Cargo.toml` binary name

```toml
[[bin]]
name = "composectl"
path = "src/main.rs"
```

This makes `cargo build` produce `target/release/composectl`. The symlink handles the `compose` name.

### Files changed
- `systemd/install.sh` — update install logic
- `Cargo.toml` — rename binary output

---

## Phase 6: Deduplicate Dependency-Loading Logic

### 6.1 Extract shared dependency-application helper

Currently, `run_start` and `run_enable` in `src/commands/service.rs` have near-identical blocks (lines 12-38 and 112-137):

```rust
if let Some(path) = deps_path {
    let path = std::path::Path::new(&path);
    println!("Loading dependencies from {}...", path.display());
    let config = crate::compose::dependencies::load_dependencies(path)?;
    let mut updated = false;
    for (service_name, service_config) in &config.services {
        let bare = get_bare_name(service_name);
        let dir = get_compose_dir(ctx, bare);
        if dir.exists() {
            crate::commands::deps::apply_dependencies(ctx, service_name, service_config)?;
            updated = true;
        } else {
            println!("Warning: Service '{}' ...", service_name, dir.display());
        }
    }
    if updated {
        crate::systemd::manager::daemon_reload(ctx)?;
    }
}
```

Extract to a helper in `src/commands/service.rs`:

```rust
/// Load and apply dependencies from a TOML file, reloading systemd if any were applied.
fn apply_deps_from_file(ctx: &Context, deps_path: &str) -> Result<()> {
    let path = std::path::Path::new(deps_path);
    println!("Loading dependencies from {}...", path.display());

    let config = crate::compose::dependencies::load_dependencies(path)?;
    let mut updated = false;

    for (service_name, service_config) in &config.services {
        let bare = get_bare_name(service_name);
        let dir = get_compose_dir(ctx, bare);

        if dir.exists() {
            crate::commands::deps::apply_dependencies(ctx, service_name, service_config)?;
            updated = true;
        } else {
            println!(
                "Warning: Service '{}' defined in dependency file not found in projects (checked at {}).",
                service_name,
                dir.display()
            );
        }
    }

    if updated {
        crate::systemd::manager::daemon_reload(ctx)?;
    }

    Ok(())
}
```

Then `run_start` and `run_enable` become:

```rust
if let Some(path) = deps_path {
    apply_deps_from_file(ctx, &path)?;
}
```

### Files changed
- `src/commands/service.rs` — extract helper, simplify `run_start` and `run_enable`

---

## Phase 7: Rust 2024 Edition + Dependency Upgrade + Drop Tokio

Upgrade to Rust edition 2024 (stable since Rust 1.85.0, February 2025), update all
dependencies to latest stable, remove `tokio` entirely, and drop `once_cell` in favor of
`std::sync::LazyLock` (stable since Rust 1.80).

This is a **mandatory** phase. Do it **first** (before Phases 1-6) to establish a clean
foundation. Everything else builds on a modern, lean crate.

---

### 7.0 Update Rust toolchain

Ensure the toolchain is latest stable (>= 1.85.0 for edition 2024):

```bash
rustup update stable
rustc --version   # must be >= 1.85.0
```

---

### 7.1 Upgrade Cargo.toml

**Before:**
```toml
[package]
name = "compose"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
clap = { version = "4.5", features = ["derive"] }
colored = "3.1"
directories = "6.0"
nix = { version = "0.31", features = ["user"] }
once_cell = "1.20"
regex = "1.12"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
shellexpand = "3.0"
tempfile = "3"
tokio = { version = "1", features = ["full"] }
toml = "0.9"
url = "2.5"
which = "8.0.0"

[dev-dependencies]
tempfile = "3.8"
```

**After:**
```toml
[package]
name = "compose"
version = "0.2.0"
edition = "2024"
rust-version = "1.85"

[dependencies]
anyhow = "1.0"
clap = { version = "4.6", features = ["derive"] }
colored = "3.1"
directories = "6.0"
nix = { version = "0.31", features = ["user"] }
regex = "1.12"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
shellexpand = "3.1"
tempfile = "3"
toml = "1.1"
url = "2.5"
which = "8.0"

[dev-dependencies]
tempfile = "3.8"
```

**What changed:**
| Dependency | Old | New | Notes |
|------------|-----|-----|-------|
| edition | 2021 | **2024** | Rust 2024 edition |
| rust-version | (none) | **1.85** | MSRV for edition 2024 |
| version | 0.1.0 | **0.2.0** | Bump for breaking changes |
| clap | 4.5 | **4.6** | Minor bump, backward compatible |
| shellexpand | 3.0 | **3.1** | Minor bump, backward compatible |
| toml | 0.9 | **1.1** | **Major bump** — see migration below |
| **once_cell** | 1.20 | **REMOVED** | Replaced by `std::sync::LazyLock` |
| **tokio** | 1 (full) | **REMOVED** | Nothing is async; pure bloat |

After editing `Cargo.toml`, run:

```bash
cargo update
```

This regenerates `Cargo.lock` with the latest compatible patch versions for all transitive
dependencies.

---

### 7.2 Migrate `once_cell::sync::Lazy` → `std::sync::LazyLock`

`std::sync::LazyLock` is stable since Rust 1.80 and is a drop-in replacement for
`once_cell::sync::Lazy`.

**File:** `src/core/validation.rs`

**Before:**
```rust
use once_cell::sync::Lazy;
use regex::Regex;

static DOMAIN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap()
});

static EMAIL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap());
```

**After:**
```rust
use std::sync::LazyLock;
use regex::Regex;

static DOMAIN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap()
});

static EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap());
```

This is the **only** file that uses `once_cell`. One import change, one type rename (`Lazy` → `LazyLock`). The `.new()` closure API is identical.

---

### 7.3 Migrate `toml` 0.9 → 1.1

**File:** `src/compose/dependencies.rs` (only usage)

The only call is:
```rust
let config: DependenciesConfig = toml::from_str(&content)
    .with_context(|| format!("Failed to parse dependency config: {}", path.display()))?;
```

`toml::from_str` works identically in 1.x — **no code changes needed** for deserialization-only usage. The breaking changes in toml 1.0 affect serialization APIs, `FromStr for Value`, and order-preservation features, none of which are used here.

**Verify:** `cargo test` passes with the new version.

---

### 7.4 Remove tokio — strip all async/await

Nothing in the codebase is truly async. Every function does synchronous
`Command::new().output()` / `.status()` calls. The `async` keywords and `#[tokio::main]`
add ~300KB+ to binary size and significant compile time for zero benefit.

#### 7.4.1 Change `main()` signature

```rust
// Before:
#[tokio::main]
async fn main() -> Result<()> {
    // ...
    match cli.command {
        Commands::Start { services, deps } => commands::run_start(&ctx, &services, deps).await,
        // ...
    }
}

// After:
fn main() -> Result<()> {
    // ...
    match cli.command {
        Commands::Start { services, deps } => commands::run_start(&ctx, &services, deps),
        // ...
    }
}
```

#### 7.4.2 Remove `async` from all command functions

Mechanical change: `pub async fn` → `pub fn`, remove all `.await` suffixes.

| File | Functions to de-async |
|------|-----------------------|
| `src/commands/service.rs` | `run_start`, `run_stop`, `run_restart`, `run_status`, `run_enable`, `run_disable` |
| `src/commands/ps.rs` | `run_ps` |
| `src/commands/pull.rs` | `run_pull` |
| `src/commands/update.rs` | `run_update` |
| `src/commands/deps.rs` | `run`, `add_deps`, `remove_deps`, `list_deps`, `clear_deps` |

For each function:
1. Remove `async` keyword from signature
2. Remove `.await` from any call sites inside the function body
3. Remove `.await` from callers in `main.rs`

**Grep check:** After this step, `grep -r "\.await" src/` and `grep -r "async fn" src/`
should both return zero results.

#### 7.4.3 Remove tokio from Cargo.toml

Already done in step 7.1. Verify no other code imports from tokio:

```bash
grep -r "tokio" src/   # should return nothing
```

---

### 7.5 Rust 2024 Edition Migration Lint Check

Run the automated migration tool to catch any edition 2024 incompatibilities:

```bash
cargo fix --edition
```

**Known edition 2024 changes that could affect this codebase:**

| Change | Impact on this project |
|--------|----------------------|
| `env::set_var` / `env::remove_var` now `unsafe` | **Not affected** — grep confirms no usage |
| `gen` is a reserved keyword | **Not affected** — grep confirms no usage as identifier |
| `expr` fragment matches `const {}` and `_` | **Not affected** — only one `macro_rules!` (`verbose!`) uses `tt`, not `expr` |
| Prelude adds `Future` / `IntoFuture` | **Not affected** — tokio is removed, no futures in scope |
| MSRV-aware resolver enabled by default | **Beneficial** — Cargo will respect `rust-version = "1.85"` |
| Never type fallback changes | **Not affected** — no `!` type usage |

**Expected result:** `cargo fix --edition` should produce zero or near-zero changes for this codebase.

---

### 7.6 Verification

```bash
# 1. Ensure toolchain is up to date
rustc --version   # >= 1.85.0

# 2. Clean build
cargo clean && cargo build --release

# 3. All tests pass
cargo test

# 4. No async remnants
grep -r "async fn" src/    # should be empty
grep -r "\.await" src/     # should be empty
grep -r "tokio" src/       # should be empty
grep -r "once_cell" src/   # should be empty

# 5. Binary size check (expect significant reduction without tokio)
ls -lh target/release/compose
```

### Impact
- Binary size reduction (tokio + once_cell removed)
- Faster compilation (~15-20% fewer crate dependencies)
- Simpler code (no async coloring throughout the entire codebase)
- Modern Rust idioms (edition 2024, `std::sync::LazyLock`)
- If parallel execution is needed later, use `std::thread::scope` or `rayon`

### Files changed
| File | Change |
|------|--------|
| `Cargo.toml` | Edition 2024, remove tokio + once_cell, bump clap/shellexpand/toml |
| `src/main.rs` | Remove `#[tokio::main]`, `async`, `.await` |
| `src/core/validation.rs` | `once_cell::sync::Lazy` → `std::sync::LazyLock` |
| `src/commands/service.rs` | Remove `async` from 6 functions, remove `.await` |
| `src/commands/ps.rs` | Remove `async` from `run_ps` |
| `src/commands/pull.rs` | Remove `async` from `run_pull` |
| `src/commands/update.rs` | Remove `async` from `run_update`, remove `.await` |
| `src/commands/deps.rs` | Remove `async` from 5 functions |

---

## Phase 8: Update README

### 8.1 Restructure documentation

The README should reflect the two-persona model:

```markdown
# compose-utils

Two tools, one binary — for managing Docker Compose projects.

## Tools

### `compose` — Docker Compose helper
Direct container operations with project discovery and configuration.

| Command | Description |
|---------|-------------|
| `compose up [services]` | Start containers (docker compose up -d) |
| `compose down [services]` | Stop containers (docker compose down) |
| `compose restart [services]` | Restart containers |
| `compose pull [services]` | Pull images |
| `compose ps [services]` | List containers |
| `compose config [options]` | View/update configuration |

### `composectl` — systemd service controller
Manage compose projects as systemd services with boot persistence and dependencies.

| Command | Description |
|---------|-------------|
| `composectl start [services]` | Start via systemd |
| `composectl stop [services]` | Stop via systemd |
| `composectl restart [services]` | Restart via systemd |
| `composectl update [services]` | Pull + restart via systemd |
| `composectl status [services]` | Show systemd unit status |
| `composectl enable [services]` | Enable auto-start on boot |
| `composectl disable [services]` | Disable auto-start |
| `composectl deps [options]` | Manage service dependencies |
| `composectl config [options]` | View/update configuration |
```

### Files changed
- `README.md` — rewrite

---

## Execution Order & Dependencies

```
Phase 7 (Rust 2024 + deps upgrade + drop tokio)    ← DO THIS FIRST
  └─> Phase 6 (dedup deps logic)                    ← clean up before restructuring
        └─> Phase 1 (CLI split)
              └─> Phase 2 (new compose_direct commands)
                    └─> Phase 3 (verify existing modules untouched)
                          └─> Phase 4 (systemd unit template)
                                └─> Phase 5 (install script + Cargo.toml)
                                      └─> Phase 8 (README)
```

**Rationale for order:**
- **Phase 7 first:** Establish clean foundation — edition 2024, no async, latest deps. Every subsequent phase writes code against this baseline. Doing this first means we never write new `async` code in Phase 2 only to remove it later.
- **Phase 6 before Phase 1:** Dedup the shared logic before splitting the CLI. Cleaner to refactor once in the unified codebase than to deal with it during the split.
- **Phase 8 last:** Documentation reflects the final state.

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Existing users have `compose` in scripts/aliases | `compose` symlink preserves the name; subcommands like `pull`, `ps`, `config` are identical |
| `compose up` behavior change (direct vs systemd) | This is intentional — document in README and CHANGELOG |
| systemd unit template references old binary | Phase 4+5 update the template; existing installs need re-install |
| Tests break | No module internals change; only `main.rs` is restructured; existing tests pass as-is |
| Multi-call detection fails (e.g., called via absolute path) | `PathBuf::file_name()` handles `/usr/bin/compose` correctly |
| `toml` 0.9 → 1.1 breaks parsing | Only `toml::from_str` is used (deserialization) — API unchanged in 1.x |
| Rust edition 2024 introduces breakage | `cargo fix --edition` handles it; grep confirms no `env::set_var`, no `gen` identifier |
| Removing tokio breaks something | Grep confirms no actual async I/O; all calls are `Command::new().output()/.status()` |
| `once_cell` removal misses a usage | Only one file (`validation.rs`) uses it; `LazyLock` is a drop-in replacement |

---

## Files Summary

| File | Action | Phase |
|------|--------|-------|
| `Cargo.toml` | **Edit** — edition 2024, drop tokio + once_cell, bump deps | 7 |
| `src/main.rs` | **Edit** (P7) remove async; **Rewrite** (P1) two CLI structs, argv[0] dispatch | 7, 1 |
| `src/core/validation.rs` | **Edit** — `once_cell::sync::Lazy` → `std::sync::LazyLock` | 7 |
| `src/commands/service.rs` | **Edit** — remove async (P7), extract deps helper (P6) | 7, 6 |
| `src/commands/ps.rs` | **Edit** — remove async | 7 |
| `src/commands/pull.rs` | **Edit** — remove async | 7 |
| `src/commands/update.rs` | **Edit** — remove async | 7 |
| `src/commands/deps.rs` | **Edit** — remove async | 7 |
| `src/commands/compose_direct.rs` | **New** — direct docker compose operations | 2 |
| `src/commands.rs` | **Edit** — add module + re-exports | 2 |
| `src/commands/config.rs` | No change | — |
| `src/commands/internal.rs` | No change | — |
| `src/core/constants.rs` | No change | — |
| `src/core/context.rs` | No change | — |
| `src/core/verbose.rs` | No change | — |
| `src/systemd/*` | No change | — |
| `src/compose/*` | No change (toml 1.1 API compatible) | — |
| `src/display/*` | No change | — |
| `systemd/compose@.service` | **Edit** — update ExecStart/ExecStop placeholder | 4 |
| `systemd/install.sh` | **Edit** — install as composectl + symlink | 5 |
| `README.md` | **Rewrite** | 8 |

---

## Verification Checklist

### After Phase 7 (Rust 2024 + deps + drop tokio):
- [ ] `rustc --version` >= 1.85.0
- [ ] `cargo build --release` succeeds with edition 2024
- [ ] `cargo test` passes (100+ existing tests)
- [ ] `grep -r "async fn" src/` returns nothing
- [ ] `grep -r "\.await" src/` returns nothing
- [ ] `grep -r "tokio" src/` returns nothing
- [ ] `grep -r "once_cell" src/` returns nothing
- [ ] Binary size decreased vs. pre-upgrade

### After Phase 6 (dedup):
- [ ] `cargo test` still passes
- [ ] No duplicate deps-loading blocks in `service.rs`

### After Phases 1-5 (multi-call binary split):
- [ ] `cargo build --release` succeeds
- [ ] `cargo test` passes
- [ ] Running as `composectl start myapp` → calls systemctl
- [ ] Running as `compose up myapp` → calls docker compose directly
- [ ] Running as `compose pull myapp` → calls docker compose pull
- [ ] Running as `composectl update myapp` → pulls then systemctl restart
- [ ] `compose config` and `composectl config` both work
- [ ] `composectl deps` works
- [ ] `compose ps` works
- [ ] Symlink detection: `ln -s target/release/composectl /tmp/compose && /tmp/compose up --help` shows compose help
- [ ] `composectl run-service X` still works (systemd unit compatibility)

### After Phase 8 (README):
- [ ] README accurately reflects both personas and their commands
- [ ] Installation instructions mention both `compose` and `composectl`
