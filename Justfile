default:
    @just --list

# Standard build flow
build profile="dev": (check profile)
    cargo test {{ if profile == "release" { "--release" } else { "" } }}
    cargo build {{ if profile == "release" { "--release" } else { "" } }}

# Linting flow
check profile="dev":
    cargo fmt --check
    cargo clippy {{ if profile == "release" { "--release" } else { "" } }} -- -D warnings

# Housekeeping, remove build artifacts
clean:
    cargo clean

# Forces 'release' profile through the dependency
install *args: (build "release")
    python3 install/setup.py install {{ args }}

# Remove the compose binary
uninstall:
    python3 install/setup.py uninstall

# Rebuild and install the compose binary and .env file
reinstall: (build "release")
    python3 install/setup.py reinstall
