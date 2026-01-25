#!/bin/bash
set -e

# Detect directories
XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_DIR="$HOME/.local/bin"
SYSTEMD_DIR="$XDG_CONFIG_HOME/systemd/user"
CONFIG_DIR="$XDG_CONFIG_HOME/docker"

echo ">> Building release binary..."
cargo build --release

echo ">> Installing binary to $BIN_DIR..."
mkdir -p "$BIN_DIR"
install -Dm755 "target/release/compose" "$BIN_DIR/compose"

echo ">> Installing systemd unit to $SYSTEMD_DIR..."
mkdir -p "$SYSTEMD_DIR"
install -Dm644 "systemd/compose@.service" "$SYSTEMD_DIR/compose@.service"

echo ">> Checking configuration in $CONFIG_DIR..."
mkdir -p "$CONFIG_DIR"
CONF_FILE="$CONFIG_DIR/compose.env"

if [ ! -f "$CONF_FILE" ]; then
    echo "   Creating default config at $CONF_FILE"
    cp "systemd/compose.env" "$CONF_FILE"
    
    # Update defaults for rootless user
    sed -i "s|/srv/data|$HOME/data|g" "$CONF_FILE"
    sed -i "s|/srv/compose|$HOME/compose-projects|g" "$CONF_FILE"
    
    echo "   (Updated default paths to match user home directory)"
else
    echo "   Config exists, skipping overwrite to preserve dotfiles."
fi

echo ">> Reloading systemd --user..."
systemctl --user daemon-reload

echo ">> Done!"
echo "   - Binary: $BIN_DIR/compose"
echo "   - Config: $CONF_FILE"
echo "   - Service: $SYSTEMD_DIR/compose@.service"
echo ""
echo "   Ensure $BIN_DIR is in your PATH."
