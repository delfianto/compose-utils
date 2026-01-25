#!/usr/bin/env python3
"""
Thin wrapper for installing the compose utility.
"""

import argparse
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent.resolve()


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Install or uninstall the compose utility"
    )
    parser.add_argument(
        "command",
        choices=["install", "uninstall", "reinstall"],
        help="Command to run",
    )

    parser.add_argument("--compose-data", help="Set COMPOSE_DATA directory path")
    parser.add_argument("--compose-base", help="Set COMPOSE_BASE directory path")
    parser.add_argument("--acme-domain", help="Set ACME domain for Traefik")
    parser.add_argument("--acme-email", help="Set ACME email for Traefik")
    parser.add_argument("--acme-server", help="Set ACME server URL for Traefik")
    parser.add_argument("--docker-host", help="Set DOCKER_HOST")

    args, _ = parser.parse_known_args()
    cmd = ["cargo", "run", "--release", "--", "system"]

    if args.command == "install" or args.command == "reinstall":
        cmd.append("install")
        if args.compose_data:
            cmd.extend(["--compose-data", args.compose_data])
        if args.compose_base:
            cmd.extend(["--compose-base", args.compose_base])
        if args.acme_domain:
            cmd.extend(["--acme-domain", args.acme_domain])
        if args.acme_email:
            cmd.extend(["--acme-email", args.acme_email])
        if args.acme_server:
            cmd.extend(["--acme-server", args.acme_server])
        if args.docker_host:
            cmd.extend(["--docker-host", args.docker_host])
    elif args.command == "uninstall":
        cmd.append("uninstall")

    try:
        subprocess.run(cmd, check=True, cwd=SCRIPT_DIR)
    except subprocess.CalledProcessError as e:
        sys.exit(e.returncode)
    except Exception as e:
        print(f"Error execution operation: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
