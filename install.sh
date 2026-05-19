#!/usr/bin/env bash
# ios-notifications installer
# Usage: curl -fsSL https://raw.githubusercontent.com/michnaugh1/ios-notifications/main/install.sh | bash
set -euo pipefail

REPO="michnaugh1/ios-notifications"
BIN_DIR="$HOME/.local/bin"
UNIT_DIR="$HOME/.config/systemd/user"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()  { echo -e "${GREEN}[✓]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
error() { echo -e "${RED}[✗]${NC} $*" >&2; exit 1; }

echo "ios-notifications installer"
echo "==========================="
echo

# ── Prerequisites ────────────────────────────────────────────────────────────

ARCH=$(uname -m)
case "$ARCH" in
  x86_64) ASSET="ios-notifications-x86_64-linux.tar.gz" ;;
  *) error "Unsupported architecture: $ARCH. Only x86_64 is currently supported." ;;
esac

if ! systemctl --user status >/dev/null 2>&1; then
  error "systemd user session not found. This tool requires systemd."
fi

if ! command -v bluetoothctl >/dev/null 2>&1; then
  error "bluetoothctl not found. Install BlueZ first:
    Ubuntu/Debian:  sudo apt install bluez
    Fedora:         sudo dnf install bluez
    Arch:           sudo pacman -S bluez bluez-utils"
fi

if ! groups | grep -qw bluetooth; then
  warn "You are not in the 'bluetooth' group. Add yourself and log out/in to apply:"
  warn "  sudo usermod -aG bluetooth $USER"
  echo
fi

# ── Download ─────────────────────────────────────────────────────────────────

URL="https://github.com/$REPO/releases/latest/download/$ASSET"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading latest release…"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$URL" -o "$TMPDIR/release.tar.gz"
elif command -v wget >/dev/null 2>&1; then
  wget -q "$URL" -O "$TMPDIR/release.tar.gz"
else
  error "Neither curl nor wget found. Install one and try again."
fi

tar -xzf "$TMPDIR/release.tar.gz" -C "$TMPDIR"

# ── Install ───────────────────────────────────────────────────────────────────

mkdir -p "$BIN_DIR"
install -m 0755 "$TMPDIR/ios-notificationsd"       "$BIN_DIR/"
install -m 0755 "$TMPDIR/ios-notifications-pair"   "$BIN_DIR/"
info "Installed binaries to $BIN_DIR"

mkdir -p "$UNIT_DIR"
install -m 0644 "$TMPDIR/ios-notifications.service" "$UNIT_DIR/"
systemctl --user daemon-reload
info "Installed systemd service"

# ── PATH check ───────────────────────────────────────────────────────────────

if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
  warn "$BIN_DIR is not in your PATH."
  warn "Add this line to your ~/.bashrc or ~/.zshrc, then open a new terminal:"
  warn "  export PATH=\"\$HOME/.local/bin:\$PATH\""
  echo
fi

# ── Done ─────────────────────────────────────────────────────────────────────

echo
echo "Installation complete! Next steps:"
echo
echo "  1. Pair your iPhone (run once):"
echo "       ios-notifications-pair"
echo
echo "  2. Start and enable the service:"
echo "       systemctl --user enable --now ios-notifications.service"
echo
echo "  3. Watch the logs:"
echo "       journalctl --user -u ios-notifications -f"
echo
echo "  Note: iPhone notifications are shared via Bluetooth. If you have a Pebble"
echo "  or other smartwatch connected to your iPhone, disconnect it first — only"
echo "  one Bluetooth device can receive iPhone notifications at a time."
