#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$HOME/.local/bin"
UNIT_DIR="$HOME/.config/systemd/user"

mkdir -p "$BIN_DIR" "$UNIT_DIR"

echo "Building release binaries..."
cd "$REPO_ROOT"
cargo build --release --workspace

echo "Installing binaries to $BIN_DIR..."
install -m 0755 "$REPO_ROOT/target/release/ios-notificationsd" "$BIN_DIR/"
install -m 0755 "$REPO_ROOT/target/release/ios-notifications-pair" "$BIN_DIR/"

echo "Installing systemd unit to $UNIT_DIR..."
install -m 0644 "$REPO_ROOT/packaging/systemd/ios-notifications.service" "$UNIT_DIR/"

echo "Reloading systemd..."
systemctl --user daemon-reload

echo
echo "Daemon installed. Next steps:"
echo "  1. Pair iPhone:    ios-notifications-pair"
echo "  2. Enable service: systemctl --user enable --now ios-notifications.service"
echo "  3. Check logs:     journalctl --user -u ios-notifications -f"
