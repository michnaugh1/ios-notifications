# ios-notifications

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A Linux daemon and Plasma 6 plasmoid that bridge iOS notifications to
KDE Plasma via Bluetooth LE using the Apple Notification Center Service
(ANCS) protocol.

iMessages, calendar alerts, app pings, and other iOS notifications appear
directly in the Plasma notification center — no iPhone app required, no
Mac in the loop.

## Status

Early development. See [docs/superpowers/specs/2026-05-18-ios-notifications-design.md](docs/superpowers/specs/2026-05-18-ios-notifications-design.md).

## Requirements

- Ubuntu 26.04 LTS or another modern systemd-based Linux distro
- BlueZ 5.66 or newer (`bluetoothctl --version`)
- KDE Plasma 6.0 or newer
- Bluetooth 4.0+ adapter
- iPhone with iOS 14 or newer (tested on iOS 26)
- Rust toolchain to build (Rust 1.78+; install via `rustup`)

## Install

```bash
git clone https://github.com/michnaugh1/ios-notifications.git
cd ios-notifications
./scripts/install-daemon.sh
./scripts/install-tray.sh
ios-notifications-pair                 # one-shot interactive pairing
systemctl --user enable --now ios-notifications.service
```

Add the "iOS Notifications" widget to a Plasma panel via right-click →
Add Widgets.

## Configuration

`~/.config/ios-notifications/config.toml`:

```toml
[device]
mac = "AA:BB:CC:DD:EE:FF"  # written by pair helper

[filter]
mode = "blacklist"          # "blacklist", "whitelist", or "off"
apps = [
  # "com.apple.Stocks",
  # "com.apple.news",
]
```

Reload without restart:
```bash
busctl --user call io.github.michnaugh1.IosNotifications \
  /IosNotifications \
  io.github.michnaugh1.IosNotifications1 ReloadConfig
```

## Architecture

Three components:

- **`ios-notificationsd`** — Rust daemon. Speaks ANCS, applies filter rules,
  forwards to `org.freedesktop.Notifications` (which Plasma renders natively).
- **`ios-notifications-pair`** — One-shot CLI. Walks you through pairing.
- **iOS Notifications plasmoid** — Plasma 6 widget. Shows connection
  state and exposes Reconnect / Pause / Resume actions.

See the [design spec](docs/superpowers/specs/2026-05-18-ios-notifications-design.md)
for protocol details, state machine, and D-Bus interface.

## Limitations

- **Read-only.** ANCS does not allow replying to iMessages or sending SMS;
  Apple deliberately omits that capability. To reply, you need an iPhone or
  Mac.
- **One paired iPhone at a time.** Multi-device support is a possible
  future feature.
- **Linux + KDE Plasma 6 only.** GNOME/XFCE will receive notifications
  too (any notification daemon listening on
  `org.freedesktop.Notifications`), but the plasmoid is Plasma-specific.

## Testing

```bash
cargo test --workspace
```

Manual integration tests (iPhone required): see
[docs/manual-tests.md](docs/manual-tests.md).

## Credits

This is a fork-and-evolve of
[kmod-midori/ancs-linux](https://github.com/kmod-midori/ancs-linux)
(MIT, © 2024 Midori Kochiya). Upstream provides:

- The ANCS protocol implementation
- The HID-keyboard auto-reconnect trick (advertising as a fake HID
  peripheral so iOS auto-reconnects on wake)
- The GATT plumbing for BlueZ via `bluer`

This fork adds: configuration-driven filtering, systemd integration with
suspend/resume handling, a D-Bus interface for tray applets, a Plasma 6
plasmoid, and a one-shot pairing CLI. Protocol-layer fixes are upstreamed.

Also builds on:

- [`ianmarmour/ancs`](https://github.com/ianmarmour/ancs) — ANCS protocol
  types crate
- [Apple's ANCS specification](https://developer.apple.com/library/archive/documentation/CoreBluetooth/Reference/AppleNotificationCenterServiceSpecification/Specification/Specification.html)
- [`bluer`](https://docs.rs/bluer) — Rust BlueZ binding
- [`zbus`](https://docs.rs/zbus) — Rust D-Bus library
- [KDE Frameworks 6](https://develop.kde.org/) — for the plasmoid

## License

MIT. See [LICENSE](LICENSE).
