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

# Install the application
install *args:
    python3 setup.py install {{ args }}

# Remove the application
uninstall:
    python3 setup.py uninstall

# Reinstall the application
reinstall:
    python3 setup.py reinstall
