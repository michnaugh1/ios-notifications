# ios-notifications

A Linux daemon that bridges iOS notifications to KDE Plasma via Bluetooth LE
using the Apple Notification Center Service (ANCS) protocol.

Receive iMessages, calendar alerts, app pings, and other iOS notifications
directly in your Plasma notification center — no iPhone app required, no Mac
in the middle.

## Status

Early development. See [docs/superpowers/specs/2026-05-18-ios-notifications-design.md](docs/superpowers/specs/2026-05-18-ios-notifications-design.md)
for the design.

## Target Environment

- Ubuntu 26.04 LTS (or any modern Linux distro with BlueZ 5.66+)
- KDE Plasma 6.6 or newer
- iOS 26 (or any iOS 7+; ANCS has been stable for over a decade)

## Credits

This project is a fork-and-evolve of [kmod-midori/ancs-linux](https://github.com/kmod-midori/ancs-linux)
(MIT, © 2024 Midori Kochiya). Upstream provides the protocol implementation,
the HID-keyboard auto-reconnect trick, and the GATT plumbing. This fork adds:

- Configuration-driven filtering (per-app blacklist/whitelist)
- systemd user service with proper logind suspend/resume handling
- D-Bus interface for tray applet integration
- KDE Plasma 6 plasmoid for connection status and quick controls
- One-shot CLI pairing helper

Protocol-layer fixes are upstreamed where possible. Daemon-architecture
changes (the D-Bus interface, the supervisor state machine, the config layer)
stay in this fork.

## License

MIT. See [LICENSE](LICENSE).
