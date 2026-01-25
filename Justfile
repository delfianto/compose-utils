default:
    @just --list

# Cleanup build artifacts and caches
clean:
    cargo clean

# Run code formatting checks and clippy lints
check:
    cargo fmt --check
    cargo clippy -- -D warnings

# Run unit tests
test:
    cargo test

# Build the binary
build *args:
    cargo build {{ args }}

# Install the application (Interactive or with --env-file)
install *args:
    ./systemd/install.sh {{ args }}