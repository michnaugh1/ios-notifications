#!/usr/bin/env bash
# Tiny wrapper used by the plasmoid to call the daemon's D-Bus interface.
# Usage:
#   dbus-helper.sh get <Property>
#   dbus-helper.sh call <Method>

set -euo pipefail

BUS="io.github.michnaugh1.IosNotifications"
PATH_OBJ="/IosNotifications"
IFACE="io.github.michnaugh1.IosNotifications1"

case "${1:-}" in
    get)
        prop="${2:?usage: get <Property>}"
        busctl --user --json=short get-property "$BUS" "$PATH_OBJ" "$IFACE" "$prop" \
            | python3 -c 'import sys,json; print(json.load(sys.stdin).get("data",""))'
        ;;
    call)
        method="${2:?usage: call <Method>}"
        busctl --user call "$BUS" "$PATH_OBJ" "$IFACE" "$method"
        ;;
    *)
        echo "Usage: $0 {get <Property>|call <Method>}" >&2
        exit 2
        ;;
esac
