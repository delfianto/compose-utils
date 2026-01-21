# compose - Rust Edition

`compose` is a command-line utility written in Rust, designed to streamline the management of Docker Compose projects integrated with Systemd. It provides robust tools for service lifecycle management and dependency handling, with strict support for both Root and Rootless Docker environments.

This project has been ported from its original Python implementation to Rust for a single, portable binary with no runtime dependencies, offering improved performance and simplified deployment.

## Features

- **Systemd Integration**: Seamlessly manage Docker Compose services using `systemctl`.
- **Root & Rootless Support**: Strictly detects and supports both system-wide (Root) and user-level (Rootless) Docker installations, enforcing correct privilege usage during installation and operation.
- **Service Management**: Start, stop, restart, enable, disable, and view the status of your Docker Compose services.
- **Dependency Management**: Configure Systemd `Wants=`, `Requires=`, and `After=` dependencies for your compose services.

## Installation

`compose` uses `just` for its installation process. The installer automatically detects your Docker environment (Root or Rootless) and guides you through the setup, enforcing correct `sudo` usage.

1.  **Ensure Rust and Cargo are installed.** If not, follow the instructions at [rustup.rs](https://rustup.rs/).
2.  **Ensure `just` is installed.** If not, you can usually install it via your system's package manager (e.g., `sudo apt install just` or `brew install just`).
3.  **Build and Install:**
    ```bash
    just install
    ```
    _The installer will prompt you if `sudo` is required or if you should run without `sudo` based on your Docker setup._

## Usage

Once installed, the `compose` binary provides two main commands: `manage` and `deps`.

### `compose manage` - Manage Docker Compose Services

This command acts as a wrapper around `systemctl` for your `compose@.service` instances.

```bash
compose manage --help
# Example: Start a Docker Compose project named 'my-app'
compose manage start my-app
# Example: View logs for 'my-app'
compose manage logs my-app -f
# Example: List all managed services
compose manage list
```

### `compose deps` - Manage Service Dependencies

This command allows you to configure Systemd dependencies for your Docker Compose services using drop-in `.conf` files.

```bash
compose deps --help
# Example: Add 'db-service' as a dependency for 'web-app'
compose deps web-app --add db-service
# Example: List dependencies for 'web-app'
compose deps web-app --list
# Example: Add a required dependency
compose deps main-app --add keycloak --requires
```

## Development

To build the project:

```bash
cargo build --release
```
