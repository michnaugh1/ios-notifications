# ios-notifications

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Forward iPhone notifications to your Linux desktop over Bluetooth — no iPhone app, no cloud, no Mac required.

iMessages, calendar alerts, and app notifications appear natively in your desktop notification center. Works with KDE Plasma, GNOME, and any other desktop that supports the standard `org.freedesktop.Notifications` D-Bus interface.

Your iPhone continuously broadcasts its notifications to trusted Bluetooth devices using Apple's [Notification Center Service (ANCS)](https://developer.apple.com/library/archive/documentation/CoreBluetooth/Reference/AppleNotificationCenterServiceSpecification/Specification/Specification.html) protocol. This daemon subscribes to that stream and converts each notification into a standard Linux desktop notification.

To ensure high reliability, the daemon uses several advanced techniques:
- **HID Keyboard Emulation**: It advertises itself as a Bluetooth HID Keyboard. iOS is much more likely to automatically reconnect to a "keyboard" than a generic data device.
- **Smart App Caching**: App names (like "Messages" or "WhatsApp") are cached in memory so that notifications are shown instantly upon reconnection without waiting for a fresh database query.
- **GATT-Aware Heartbeat**: The daemon monitors the health of the notification "pipes" themselves, not just the Bluetooth link, ensuring it can recover if the stream hangs.

Everything runs locally over Bluetooth LE — nothing leaves your home network.

## Features

- **Native Notifications**: Fully integrated with KDE Plasma, GNOME, and standard D-Bus notification servers.
- **Automatic Reconnect**: Seamlessly picks up notifications when you return to your computer.
- **App Filtering**: Blacklist or whitelist specific apps via a simple TOML config.
- **Privacy First**: No cloud, no internet access required, and no third-party iOS app to install.
- **D-Bus Interface**: Real-time state and notification counters accessible via `busctl`.

## Requirements

- **Linux** with systemd (Ubuntu 22.04+, Fedora 36+, Arch, or similar)
- **BlueZ 5.x** (`bluetoothctl --version` to check)
- **Bluetooth 4.0+ adapter** (most laptops and USB adapters from 2012 onward)
- **iPhone** running iOS 7 or later
- **x86_64** CPU (ARM builds coming later)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/michnaugh1/ios-notifications/main/install.sh | bash
```

The script checks prerequisites, downloads the latest release binary, installs it to `~/.local/bin`, and sets up the systemd service.

### Pair your iPhone (once)

Run the pairing wizard and follow the on-screen instructions:

```bash
ios-notifications-pair
```

The wizard will make your computer discoverable, wait for you to tap it in iPhone Settings → Bluetooth, and guide you through granting notification access on the iPhone side.

> **Important:** If you have a Pebble, Fitbit, or other smartwatch connected to your iPhone via Bluetooth, disconnect it before pairing and before using this tool. Apple only allows one Bluetooth device to receive notifications at a time — the watch will silently consume them all.

### Start the service

```bash
systemctl --user enable --now ios-notifications.service
```

To watch the live logs:

```bash
journalctl --user -u ios-notifications -f
```

### KDE Plasma tray widget (optional)

Clone the repo and install the plasmoid:

```bash
git clone https://github.com/michnaugh1/ios-notifications.git
cd ios-notifications
./scripts/install-tray.sh
```

Then right-click a Plasma panel → Add Widgets → search for "iOS Notifications".

## Configuration

The config file lives at `~/.config/ios-notifications/config.toml` and is created automatically by the pair wizard. Edit it to filter which apps can send notifications:

```toml
[device]
mac = "AA:BB:CC:DD:EE:FF"   # written by the pair wizard — do not change

[filter]
mode = "blacklist"           # "blacklist" = block listed apps (default)
                             # "whitelist" = only allow listed apps
                             # "off"       = show everything
apps = [
  "com.apple.Stocks",
  "com.apple.news",
]
```

Reload the config without restarting the service:

```bash
busctl --user call io.github.michnaugh1.IosNotifications \
  /IosNotifications io.github.michnaugh1.IosNotifications1 ReloadConfig
```

## Troubleshooting

**No notifications appearing at all**
- Check the logs: `journalctl --user -u ios-notifications -f`
- On your iPhone, go to Settings → Bluetooth → tap the **(i)** next to your computer → make sure **Share System Notifications** is on
- Disconnect any smartwatches from your iPhone — they silently steal the notification stream

**Notifications stop after iPhone screen turns off**
- This is normal: iOS drops the Bluetooth connection when the screen turns off. The daemon detects this within ~15 seconds and reconnects automatically when the screen comes back on
- If it doesn't reconnect, restart the service: `systemctl --user restart ios-notifications.service`

**"Device not paired" error at startup**
- iOS occasionally removes the pairing when the Bluetooth connection breaks badly. Re-run the pair wizard: `ios-notifications-pair`

**Notification text is missing (shows app name only)**
- On your iPhone: Settings → Notifications → [App] → set **Show Previews** to **Always** (not "When Unlocked")

**Connection keeps dropping**
- Make sure your Bluetooth adapter supports BLE: `btmgmt info | grep le`
- Try toggling Bluetooth off and on on both devices
- If discovery is slow, the daemon now uses an optimized D-Bus introspection method to find services faster.

## Building from source

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install system dependencies
sudo apt install libdbus-1-dev pkg-config   # Ubuntu/Debian
sudo dnf install dbus-devel pkg-config      # Fedora
sudo pacman -S dbus pkgconf                 # Arch

# Build and install
git clone https://github.com/michnaugh1/ios-notifications.git
cd ios-notifications
./scripts/install-daemon.sh
```

Run the tests:

```bash
cargo test --workspace
```

## Limitations

- **Read-only.** ANCS does not allow replying to messages — Apple deliberately omits that capability.
- **One iPhone at a time.** The config holds a single device MAC address.
- **x86_64 only** for pre-built binaries. ARM support is planned.

## License

MIT. See [LICENSE](LICENSE).
