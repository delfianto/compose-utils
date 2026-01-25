Compose-Utils Refactoring Guide
This guide focuses on stripping out "smart" but fragile logic (manual YAML parsing, manual image pulling, self-installation) and replacing it with robust delegation to the underlying tools (docker compose and systemctl).
Target Architecture:

- Build: Arch Native (PKGBUILD), removing Python.
- Docker: Shell out to docker compose (replaces bollard).
- Systemd: Shell out to systemctl (optimized existing logic).
- Config: Robust shell expansion (replaces Regex).
  Phase 1: Build Infrastructure (The Purge)
  We remove the self-modifying "malware-style" installer and replace it with proper package management.
  1.1 Delete Legacy Install Files
  Remove these files entirely. The binary should not care about how it is installed.
- setup.py
- src/setup.rs
- src/setup/ (delete the whole directory)
  1.2 Clean src/main.rs
  Remove the code that handled the system subcommand.
  Edit src/main.rs:
  // 1. Remove this line
  mod setup;

// 2. Remove `System(commands::system::SystemCommands),` from the Commands enum.

// 3. Remove the match arm in main() that handles Commands::System.

1.3 Create PKGBUILD
Create a PKGBUILD in your project root. This allows you to install via makepkg -si.

# Maintainer: Dwi Elfianto <dwi@elfianto.com>

pkgname=compose-utils
pkgver=0.1.0
pkgrel=1
pkgdesc="Systemd integration for Docker Compose projects"
arch=('x86_64')
url="[https://github.com/delfianto/compose-utils](https://github.com/delfianto/compose-utils)"
license=('MIT')
depends=('docker' 'systemd')
makedepends=('cargo' 'git')
source=("git+file://$(pwd)") # Assumes you build from local git
sha256sums=('SKIP')

prepare() {
cd "$srcdir/$pkgname"
export RUSTUP_TOOLCHAIN=stable
cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
cd "$srcdir/$pkgname"
export RUSTUP_TOOLCHAIN=stable
export CARGO_TARGET_DIR=target
cargo build --frozen --release --all-features
}

package() {
cd "$srcdir/$pkgname"

# Install binary

install -Dm755 "target/release/compose" "$pkgdir/usr/bin/compose"

# Install Systemd Template (Create this folder structure in your source first)

install -Dm644 "systemd/compose@.service" "$pkgdir/usr/lib/systemd/system/compose@.service"

# Install License

install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}

Phase 2: Dependencies & Env Vars
We trade "heavy API clients" for "smart text handling".
2.1 Update Cargo.toml
Remove the heavy Docker API client (bollard) and the complex progress bar (indicatif) since docker compose pull handles UI better. Add shellexpand for robust config parsing.
[dependencies]

# KEEP:

anyhow = "1.0"
chrono = "0.4"
clap = { version = "4.5", features = ["derive"] }
colored = "3.1"
ctrlc = "3.4"
directories = "6.0"
once_cell = "1.20"
regex = "1.12"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
systemd = { version = "0.10", features = ["journal"] } # Keep for logging
tokio = { version = "1", features = ["full"] }
toml = "0.9"
url = "2.5"
which = "8.0.0"

# ADD:

shellexpand = "3.0"

# REMOVE:

# bollard

# indicatif

# futures-util

# serde_yaml_ng (We stop parsing YAML manually)

2.2 Fix Env Var Expansion (src/compose/env.rs)
Your regex failed on ${VAR:-default}. shellexpand fixes this.
use std::collections::HashMap;
use std::borrow::Cow;

pub fn resolve_env_vars(text: &str, vars: &HashMap<String, String>) -> String {
// Looks up in our hashmap first, falls back to system env vars
let result = shellexpand::env_with_context_no_errors(text, |var_name| {
vars.get(var_name).map(|v| v.as_str())
});

result.into_owned()
}

Phase 3: Docker Delegation (The "Shell Out" Strategy)
We remove the code that tried to be a "Docker Client" and become a "Process Manager".
3.1 Delete Manual Docker Logic
Delete these files. They are maintenance burdens.

- src/docker/images.rs (Manual pulling)
- src/docker/types.rs
- src/compose/project.rs (Manual YAML parsing)
- src/compose/types.rs
  3.2 Rewrite src/commands/pull.rs
  Delegate to the official CLI. It handles layers, auth, and retries better than bollard.
  use crate::core::Context;
  use crate::systemd::service::{get*bare_name, get_compose_dir};
  use anyhow::{Context as *, Result};
  use std::process::Command;

pub async fn run_pull(ctx: &Context, services: &[String]) -> Result<()> {
let services = crate::systemd::discovery::resolve_services(ctx, services)?;

for service in services {
let bare = get_bare_name(&service);
let dir = get_compose_dir(ctx, bare);

       println!(">> Pulling images for '{}'...", bare);

       let status = Command::new("docker")
           .arg("compose")
           .arg("pull")
           .current_dir(&dir)
           .status()
           .with_context(|| format!("Failed to execute docker compose pull in {:?}", dir))?;

       if !status.success() {
           eprintln!("Warning: Failed to pull images for {}", bare);
       }

}
Ok(())
}

3.3 Rewrite src/commands/update.rs
Instead of manually checking hashes, we can blindly pull (Docker handles "already exists" fast) or use docker compose up --pull always logic.
use crate::core::Context;
use crate::systemd::service::{get_bare_name, get_compose_dir};
use anyhow::Result;
use std::process::Command;

pub async fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
let services = crate::systemd::discovery::resolve_services(ctx, services)?;

for service in services {
let bare = get_bare_name(&service);
let dir = get_compose_dir(ctx, bare);

       println!(">> Updating '{}'...", bare);

       // 1. Pull new images
       let pull_status = Command::new("docker")
           .arg("compose")
           .arg("pull")
           .current_dir(&dir)
           .status()?;

       if pull_status.success() {
            // 2. Restart the systemd unit to pick up changes
            // (Systemd will call `docker compose up` which recreates containers if images changed)
            let unit = crate::systemd::service::normalize_unit_name(ctx, bare);
            crate::systemd::manager::restart_unit(ctx, &unit)?;
            println!("Restarted {}", unit);
       }

}
Ok(())
}

Phase 4: Systemd Logic (Refining the Shell Calls)
You chose to keep parsing manual output. Let's make src/systemd/manager.rs cleaner without changing the underlying mechanism.
4.1 Optimize list_units
Your current implementation calls list-units (text) then show (machine-readable) for every unit. This is slow (N+1 process calls).
Optimize it to one call using JSON output (if your systemd version supports it, likely yes on CachyOS) or batched show.
Modified src/systemd/manager.rs:
use crate::core::Context;
use anyhow::{bail, Result};
use std::process::Command;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SystemdUnit {
unit: String,
active: String,
sub: String,
description: String,
}

// Use JSON output if available (Systemd v240+)
pub fn list_units(ctx: &Context, pattern: Option<&str>) -> Result<Vec<UnitInfo>> {
let mut cmd = systemctl_cmd(ctx);
cmd.arg("list-units").arg("--output=json");

if let Some(p) = pattern {
cmd.arg(format!("{}\*", p));
}

let output = cmd.output()?;
if !output.status.success() {
// Fallback or Error handling
bail!("Failed to list units");
}

// Parse JSON directly - safer than splitting whitespace
let units: Vec<SystemdUnit> = serde_json::from_slice(&output.stdout)?;

Ok(units.into_iter().map(|u| UnitInfo {
name: u.unit,
active: u.active,
sub: u.sub,
description: u.description
}).collect())
}

Note: If you strictly prefer text parsing over JSON, ensure you use LC_ALL=C in your command environment to prevent localization breaking your column splitting.
fn systemctl_cmd(ctx: &Context) -> std::process::Command {
let mut cmd = Command::new("systemctl");
// CRITICAL: Ensure consistent output regardless of user language
cmd.env("LC_ALL", "C");
if !ctx.is_root {
cmd.arg("--user");
}
cmd
}
