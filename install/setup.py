#!/usr/bin/env python3
"""
Installer for compose utility.
Supports both system-wide (root) and rootless Docker modes.
"""

import argparse
import os
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from string import Template

SCRIPT_DIR = Path(__file__).parent.resolve()
PROJECT_ROOT = SCRIPT_DIR.parent

SYSTEM_SOCKET = Path("/var/run/docker.sock")
BINARY_NAME = "compose"


class Config:
    """Installation configuration based on detected mode and user arguments."""

    def __init__(self, mode: str, args: argparse.Namespace):
        self.mode = mode
        self.uid = os.getuid()
        self.xdg_runtime_dir = os.environ.get(
            "XDG_RUNTIME_DIR", f"/run/user/{self.uid}"
        )
        self.xdg_config_home = os.environ.get(
            "XDG_CONFIG_HOME", str(Path.home() / ".config")
        )

        # Find docker binary
        docker_bin = shutil.which("docker") or "/usr/bin/docker"
        self.compose_bin_path = f"{docker_bin} compose"

        # Default ACME settings
        self.acme_domain = args.acme_domain or "example.com"
        self.acme_email = args.acme_email or "admin@example.com"
        self.acme_server = (
            args.acme_server or "https://acme-v02.api.letsencrypt.org/directory"
        )

        if mode == "root":
            self.bin_dir = Path("/usr/local/bin")
            self.systemd_dir = Path("/etc/systemd/system")
            self.env_file = Path("/etc/compose.env")
            self.old_env_file = None
            self.data_base = (
                Path(args.compose_data) if args.compose_data else Path("/srv/appdata")
            )
            self.compose_base = (
                Path(args.compose_base) if args.compose_base else Path("/srv/compose")
            )
            self.systemctl = ["systemctl"]
            self.docker_host = args.docker_host or "unix:///var/run/docker.sock"
        else:
            self.bin_dir = Path.home() / ".local/bin"
            self.systemd_dir = Path(self.xdg_config_home) / "systemd/user"
            self.env_file = Path(self.xdg_config_home) / "docker" / "compose.env"
            self.old_env_file = Path(self.xdg_config_home) / "compose.env"
            self.data_base = (
                Path(args.compose_data)
                if args.compose_data
                else Path.home() / ".local/share/appdata"
            )
            self.compose_base = (
                Path(args.compose_base)
                if args.compose_base
                else Path.home() / "compose-projects"
            )
            self.systemctl = ["systemctl", "--user"]
            self.docker_host = (
                args.docker_host or f"unix://{self.xdg_runtime_dir}/docker.sock"
            )

    @property
    def binary_path(self) -> Path:
        return self.bin_dir / BINARY_NAME

    @property
    def service_path(self) -> Path:
        return self.systemd_dir / "compose@.service"


def detect_mode() -> str:
    """Detect Docker mode and validate privileges."""
    uid = os.getuid()
    xdg_runtime_dir = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{uid}")
    rootless_socket = Path(xdg_runtime_dir) / "docker.sock"

    if SYSTEM_SOCKET.exists():
        if uid != 0:
            print(f"Error: System-wide Docker detected at {SYSTEM_SOCKET}")
            print("You MUST use sudo to install for System Docker.")
            sys.exit(1)
        print("Detected System Docker mode.")
        return "root"

    if rootless_socket.exists():
        if uid == 0:
            print(f"Error: Rootless Docker detected at {rootless_socket}")
            print("You MUST NOT use sudo to install for Rootless Docker.")
            sys.exit(1)
        print("Detected Rootless Docker mode.")
        return "rootless"

    print("Error: No Docker daemon detected.")
    print(f"  - System socket ({SYSTEM_SOCKET}) not found.")
    print(f"  - Rootless socket ({rootless_socket}) not found.")
    print("Please ensure Docker is installed and the daemon is running.")
    sys.exit(1)


def load_template(name: str) -> Template:
    """Load a template file from the install directory."""
    template_path = SCRIPT_DIR / name
    return Template(template_path.read_text())


def run_systemctl(cfg: Config, *args: str) -> None:
    """Run systemctl with appropriate flags for the mode."""
    cmd = cfg.systemctl + list(args)
    subprocess.run(cmd, check=True)


def install(cfg: Config) -> None:
    """Install the compose utility."""
    print(f"Installing for {cfg.mode} mode...")

    # Install binary
    binary_src = PROJECT_ROOT / "target/release" / BINARY_NAME
    if not binary_src.exists():
        print(f"Error: Binary not found at {binary_src}")
        print("Please run 'cargo build --release' first.")
        sys.exit(1)

    print(f"Installing binary to {cfg.binary_path}...")
    cfg.bin_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary_src, cfg.binary_path)
    cfg.binary_path.chmod(0o755)

    # Install service template
    print(f"Installing service template to {cfg.service_path}...")
    cfg.systemd_dir.mkdir(parents=True, exist_ok=True)

    service_template = load_template("compose@.service")
    service_content = service_template.substitute(
        compose_base=cfg.compose_base,
        compose_bin_path=cfg.compose_bin_path,
    )
    cfg.service_path.write_text(service_content)

    # Reload systemd
    print("Reloading systemd daemon...")
    run_systemctl(cfg, "daemon-reload")

    # Generate env file (only if it doesn't exist)
    print(f"Checking env file at {cfg.env_file}...")

    # Migrate from old location if it exists
    if cfg.mode == "rootless" and cfg.old_env_file.exists() and not cfg.env_file.exists():
        print(f"Migrating existing env file from {cfg.old_env_file} to {cfg.env_file}...")
        cfg.env_file.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(cfg.old_env_file), str(cfg.env_file))

    cfg.env_file.parent.mkdir(parents=True, exist_ok=True)

    if not cfg.env_file.exists():
        print("Generating new env file...")
        env_template = load_template("compose.env")
        env_content = env_template.substitute(
            date=datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
            data_base=cfg.data_base,
            compose_base=cfg.compose_base,
            acme_domain=cfg.acme_domain,
            acme_email=cfg.acme_email,
            docker_host=cfg.docker_host,
        )
        cfg.env_file.write_text(env_content)
    else:
        print("Env file exists, skipping generation.")

    # Create base directories
    cfg.data_base.mkdir(parents=True, exist_ok=True)
    cfg.compose_base.mkdir(parents=True, exist_ok=True)

    print()
    print("Installation complete!")
    print("-" * 50)
    print(f"Binary location: {cfg.binary_path}")
    print(f"Environment file: {cfg.env_file}")
    print(f"Compose projects: {cfg.compose_base}")
    print(f"Mode: {cfg.mode}")
    print("-" * 50)


def uninstall(cfg: Config) -> None:
    """Uninstall the compose utility."""
    print(f"Uninstalling for {cfg.mode} mode...")

    # Stop and disable any running services
    print("Stopping any running compose services...")
    try:
        result = subprocess.run(
            cfg.systemctl + ["list-units", "compose@*.service", "--no-legend", "-q"],
            capture_output=True,
            text=True,
        )
        for line in result.stdout.strip().split("\n"):
            if line:
                unit = line.split()[0]
                print(f"  Stopping {unit}...")
                subprocess.run(cfg.systemctl + ["stop", unit], check=False)
                subprocess.run(cfg.systemctl + ["disable", unit], check=False)
    except Exception as e:
        print(f"  Warning: Could not stop services: {e}")

    # Remove service template
    if cfg.service_path.exists():
        print(f"Removing service template {cfg.service_path}...")
        cfg.service_path.unlink()

    # Reload systemd
    print("Reloading systemd daemon...")
    try:
        run_systemctl(cfg, "daemon-reload")
    except Exception:
        pass

    # Remove binary
    if cfg.binary_path.exists():
        print(f"Removing binary {cfg.binary_path}...")
        cfg.binary_path.unlink()

    print()
    print("Uninstall complete!")
    print("-" * 50)
    print(f"Note: Environment file preserved at {cfg.env_file}")
    print(f"Note: Data directories preserved at {cfg.data_base} and {cfg.compose_base}")
    print("Remove these manually if no longer needed.")
    print("-" * 50)


def reinstall(cfg: Config) -> None:
    """Reinstall the binary and service file based on existing configuration."""
    print(f"Reinstalling for {cfg.mode} mode...")

    # Migrate from old location if it exists
    if cfg.mode == "rootless" and cfg.old_env_file.exists() and not cfg.env_file.exists():
        print(f"Migrating existing env file from {cfg.old_env_file} to {cfg.env_file}...")
        cfg.env_file.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(cfg.old_env_file), str(cfg.env_file))

    if not cfg.env_file.exists():
        print(f"Error: Environment file not found at {cfg.env_file}")
        print("Cannot reinstall without existing configuration.")
        sys.exit(1)

    # Install binary
    binary_src = PROJECT_ROOT / "target/release" / BINARY_NAME
    if not binary_src.exists():
        print(f"Error: Binary not found at {binary_src}")
        print("Please run 'cargo build --release' first.")
        sys.exit(1)

    print(f"Installing binary to {cfg.binary_path}...")
    cfg.bin_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary_src, cfg.binary_path)
    cfg.binary_path.chmod(0o755)

    # Read existing configuration
    print(f"Reading configuration from {cfg.env_file}...")
    content = cfg.env_file.read_text()
    for line in content.splitlines():
        if line.startswith("COMPOSE_BASE="):
            path_str = line.split("=", 1)[1].strip()
            cfg.compose_base = Path(path_str)
            print(f"Found COMPOSE_BASE: {cfg.compose_base}")
            break

    # Regenerate service file
    print(f"Regenerating service template to {cfg.service_path}...")
    # Ensure directory exists (in case it was deleted)
    cfg.systemd_dir.mkdir(parents=True, exist_ok=True)

    service_template = load_template("compose@.service")
    service_content = service_template.substitute(
        compose_base=cfg.compose_base,
        compose_bin_path=cfg.compose_bin_path,
    )
    cfg.service_path.write_text(service_content)

    # Reload systemd
    print("Reloading systemd daemon...")
    run_systemctl(cfg, "daemon-reload")

    print()
    print("Reinstall complete!")
    print("-" * 50)
    print(f"Binary location: {cfg.binary_path}")
    print(f"Service file: {cfg.service_path}")
    print(f"Environment file: {cfg.env_file}")
    print("-" * 50)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Install or uninstall the compose utility"
    )
    parser.add_argument(
        "command",
        choices=["install", "uninstall", "reinstall"],
        help="Command to run",
    )

    # Optional configuration arguments
    parser.add_argument("--compose-data", help="Set COMPOSE_DATA directory path")
    parser.add_argument("--compose-base", help="Set COMPOSE_BASE directory path")
    parser.add_argument("--acme-domain", help="Set ACME domain for Traefik")
    parser.add_argument("--acme-email", help="Set ACME email for Traefik")
    parser.add_argument("--acme-server", help="Set ACME server URL for Traefik")
    parser.add_argument("--docker-host", help="Set DOCKER_HOST")

    args = parser.parse_args()

    mode = detect_mode()
    cfg = Config(mode, args)

    if args.command == "install":
        install(cfg)
    elif args.command == "uninstall":
        uninstall(cfg)
    elif args.command == "reinstall":
        reinstall(cfg)


if __name__ == "__main__":
    main()
