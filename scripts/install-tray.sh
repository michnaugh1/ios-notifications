#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN_ID="io.github.michnaugh1.iosnotifications"
DEST="$HOME/.local/share/plasma/plasmoids/$PLUGIN_ID"

echo "Installing plasmoid to $DEST..."
mkdir -p "$DEST"
cp -r "$REPO_ROOT/tray/"* "$DEST/"

# Restart plasmashell to pick up the new plasmoid.
if command -v kquitapp6 > /dev/null; then
    echo "Restarting plasmashell..."
    kquitapp6 plasmashell 2>/dev/null || true
    nohup plasmashell --replace > /dev/null 2>&1 &
fi

echo "Done. Add the widget via right-click on the panel → Add Widgets → search 'iOS Notifications'."
