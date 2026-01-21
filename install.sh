#!/bin/bash
set -e

# Configuration
SYSTEM_SOCKET="/var/run/docker.sock"
ID=$(id -u)
: ${XDG_RUNTIME_DIR:=/run/user/$ID}
ROOTLESS_SOCKET="$XDG_RUNTIME_DIR/docker.sock"

# 1. Environment Detection & Validation
if [ -e "$SYSTEM_SOCKET" ]; then
    # System Docker detected
    if [ "$ID" -ne 0 ]; then
        echo "Error: System-wide Docker detected at $SYSTEM_SOCKET"
        echo "You MUST use sudo to install for System Docker."
        exit 1
    fi
    MODE="root"
    echo "Detected System Docker mode."
elif [ -e "$ROOTLESS_SOCKET" ]; then
    # Rootless Docker detected
    if [ "$ID" -eq 0 ]; then
        echo "Error: Rootless Docker detected at $ROOTLESS_SOCKET"
        echo "You MUST NOT use sudo to install for Rootless Docker."
        exit 1
    fi
    MODE="rootless"
    echo "Detected Rootless Docker mode."
else
    # No daemon detected
    echo "Error: No Docker daemon detected."
    echo "  - System socket ($SYSTEM_SOCKET) not found."
    echo "  - Rootless socket ($ROOTLESS_SOCKET) not found."
    echo "Please ensure Docker is installed and the daemon is running."
    exit 1
fi

# 2. Path Configuration based on Mode
if [ "$MODE" = "root" ]; then
    BIN_DIR="/usr/local/bin"
    SYSTEMD_DIR="/etc/systemd/system"
    CONFIG_DIR="/etc/compose" # Renamed to compose for consistency
    DATA_BASE="/srv/appdata"
    PROJ_BASE="/srv/compose"
    SYSTEMCTL="systemctl"
else
    : ${XDG_CONFIG_HOME:=$HOME/.config}
    BIN_DIR="$HOME/.local/bin"
    SYSTEMD_DIR="$XDG_CONFIG_HOME/systemd/user"
    CONFIG_DIR="$XDG_CONFIG_HOME/compose" # Renamed to compose for consistency
    DATA_BASE="$HOME/.local/share/appdata"
    PROJ_BASE="$HOME/compose-projects"
    SYSTEMCTL="systemctl --user"
fi

# 3. Installation
echo "Installing for $MODE mode..."

# Install Binary
COMPOSE_BINARY_NAME="compose"
echo "Installing binary to $BIN_DIR/${COMPOSE_BINARY_NAME}..."
mkdir -p "$BIN_DIR"
install -m 755 target/release/${COMPOSE_BINARY_NAME} "$BIN_DIR/${COMPOSE_BINARY_NAME}"

# Install Service Template
echo "Installing service template to $SYSTEMD_DIR/compose@.service..."
mkdir -p "$SYSTEMD_DIR"

COMPOSE_BIN_PATH="$BIN_DIR/${COMPOSE_BINARY_NAME}"

# Heredoc for the service file template
read -r -d '' SERVICE_TEMPLATE <<EOF
[Unit]
Description=Compose Service for %i
Requires=docker.service
After=docker.service

[Service]
Type=simple
EnvironmentFile=-/etc/compose/env
EnvironmentFile=-%h/.config/compose/env
ExecStart=${COMPOSE_BIN_PATH} manage start %i
ExecStop=${COMPOSE_BIN_PATH} manage stop %i
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
EOF

echo "$SERVICE_TEMPLATE" > "$SYSTEMD_DIR/compose@.service"

echo "Reloading systemd daemon..."
$SYSTEMCTL daemon-reload

# Generate Env
ENV_FILE="$CONFIG_DIR/env"
echo "Checking env file at $ENV_FILE..."
mkdir -p "$CONFIG_DIR"

if [ ! -f "$ENV_FILE" ]; then
    echo "Generating new env file..."
    cat > "$ENV_FILE" <<EOF
# compose Environment Configuration
# Generated on $(date)

COMPOSE_DATA=$DATA_BASE
COMPOSE_BASE=$PROJ_BASE

TRAEFIK_ACME_DOMAIN=
TRAEFIK_ACME_EMAIL=
TRAEFIK_ACME_SERVER=https://acme-v02.api.letsencrypt.org/directory

$( [ "$MODE" = "rootless" ] && echo "DOCKER_HOST=unix://$ROOTLESS_SOCKET" )
EOF
else
    echo "Env file exists, skipping generation."
fi

echo ""
echo "Installation complete!"
echo "--------------------------------------------------"
echo "Binary location: $BIN_DIR/${COMPOSE_BINARY_NAME}"
echo "Config location: $CONFIG_DIR"
echo "Mode:   $MODE"
echo "--------------------------------------------------"
