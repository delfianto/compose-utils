# Dependencies

## Rust Toolchain

- **Edition:** 2024
- **Minimum Supported Rust Version (MSRV):** 1.85
- **Build Profile:** Release builds use `opt-level = "z"` (size), LTO, single codegen unit, symbol stripping, and `panic = "abort"` for minimal binary size.

## Runtime Dependencies

The binary has **zero library runtime dependencies** -- everything is statically linked. It does require these tools to be present on the system:

| Tool | Used by | Purpose |
|------|---------|---------|
| `docker` | compose_direct, ps, pull, internal | Container operations |
| `docker compose` | compose_direct, pull, internal | Compose project management |
| `systemctl` | manager, service, deps | Systemd unit control |

## Crate Dependencies

### Core

| Crate | Version | Purpose | Used in |
|-------|---------|---------|---------|
| **anyhow** | 1.0 | Ergonomic error handling with context. All functions return `anyhow::Result`. | Everywhere |
| **clap** | 4.6 | CLI argument parsing using derive macros. Defines both `ComposeCli` and `CtlCli` structs. | `main.rs`, `config.rs`, `deps.rs` |

### System

| Crate | Version | Purpose | Used in |
|-------|---------|---------|---------|
| **nix** | 0.31 | Unix system calls -- specifically `geteuid()` and `getuid()` for privilege detection. Feature: `user`. | `context.rs` |
| **directories** | 6.0 | XDG Base Directory Specification. Used to find `$HOME` reliably via `BaseDirs::new()`. | `context.rs` |
| **which** | 8.0 | Executable path resolution (available but not heavily used in current code). | -- |

### Parsing & Serialization

| Crate | Version | Purpose | Used in |
|-------|---------|---------|---------|
| **serde** | 1.0 | Serialization framework. Feature: `derive`. Used for `DockerContainer` and dependency config structs. | `ps.rs`, `dependencies.rs` |
| **serde_json** | 1.0 | JSON deserialization of `docker ps --format '{{json .}}'` output. | `ps.rs` |
| **toml** | 1.1 | TOML deserialization for dependency configuration files. | `dependencies.rs` |

### Validation & Text

| Crate | Version | Purpose | Used in |
|-------|---------|---------|---------|
| **regex** | 1.12 | Regular expressions for domain/email validation and ANSI code stripping. | `validation.rs`, `table.rs` |
| **url** | 2.5 | URL parsing and validation for ACME server and Docker host URIs. | `validation.rs` |
| **shellexpand** | 3.1 | Shell variable expansion (e.g., `$HOME` in paths). | `context.rs` |

### Display

| Crate | Version | Purpose | Used in |
|-------|---------|---------|---------|
| **colored** | 3.1 | ANSI terminal colors for health dots and output formatting. | `status.rs` |

### Utilities

| Crate | Version | Purpose | Used in |
|-------|---------|---------|---------|
| **tempfile** | 3 | Temporary file creation. Used in production for atomic writes and in tests for fixtures. | Tests, install logic |

### Dev Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| **tempfile** | 3.8 | Test fixtures -- temporary directories and files for isolated filesystem tests. |

## What Was Removed

| Crate | Reason |
|-------|--------|
| **tokio** | Removed in v0.2.0. No actual async I/O existed -- all operations are blocking `Command::new().output()` calls. Removing tokio reduced binary size and compile time significantly. |
| **once_cell** | Replaced by `std::sync::LazyLock` (stable since Rust 1.80). Only two static regex patterns were using it. |

## Dependency Philosophy

- Prefer standard library where possible (`std::sync::LazyLock` over `once_cell`, `std::process::Command` over async alternatives).
- No Docker API client library -- the tool calls `docker` and `docker compose` CLI directly. This avoids version coupling with the Docker API and keeps the binary small.
- No systemd D-Bus bindings -- `systemctl` CLI is used instead. Simpler, more portable, and avoids linking against `libdbus`.
- Every dependency earns its place. If it can be done in 20 lines of std, don't add a crate.
