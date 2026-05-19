# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build everything
cargo build --workspace

# Build release and install to ~/.local/bin + systemd unit
./scripts/install-daemon.sh

# Install plasmoid (copies tray/ to ~/.local/share/plasma/plasmoids/)
./scripts/install-tray.sh

# Run all tests
cargo test --workspace

# Run a single test (example: supervisor state machine tests)
cargo test -p ios-notificationsd supervisor

# Run with verbose logging
RUST_LOG=debug cargo run --bin ios-notificationsd -- --config /path/to/config.toml

# Interactive pairing (run once before starting the daemon)
cargo run --bin ios-notifications-pair

# Reload config without restart
busctl --user call io.github.michnaugh1.IosNotifications /IosNotifications \
  io.github.michnaugh1.IosNotifications1 ReloadConfig

# Watch live daemon logs
journalctl --user -u ios-notifications -f
```

## Architecture

Three components:

**`daemon/` (`ios-notificationsd`)** — the main Rust daemon. Modules:
- `main.rs` — parses config and CLI args, initializes BlueZ adapter, verifies device is paired, wires up all components, and calls `supervisor::run_supervisor`.
- `supervisor.rs` — two layers: a pure `StateMachine` struct (states: `Initializing → Connecting → Connected / Backoff / Paused / Error`) and the async `run_supervisor` function that drives it. The supervisor spawns `AncsProcessor::main_loop` as a task on each connect attempt, races it against incoming events (sleep, reconnect, pause), and applies exponential backoff (2s→60s) on failure. Handles `logind` `PrepareForSleep` signals for suspend/resume.
- `ancs.rs` — `AncsProcessor`: connects to the BlueZ device, waits for GATT services to resolve (with a D-Bus fallback scan when `ServicesResolved` is slow), subscribes to ANCS Notification Source and Data Source characteristics, decodes ANCS protocol packets, calls `Filter::should_show`, and dispatches desktop notifications via `notify-rust`. Maintains an app-name cache (`app_names: HashMap`). Calls CCCD reset (write `0x0000` before subscribing) to force ANCS session reinitialization on reconnect.
- `hid_bridge.rs` — registers a fake HID keyboard GATT service and BLE advertisement. iOS only auto-reconnects to devices it recognizes as HID keyboards; this advertisement makes the Linux machine appear as one.
- `dbus_iface.rs` — `IosNotificationsIface` via `zbus`. Bus name: `io.github.michnaugh1.IosNotifications`, object path: `/IosNotifications`, interface: `io.github.michnaugh1.IosNotifications1`. Exposes properties (`state`, `device_address`, `last_error`, `notifications_today`, `next_backoff_secs`), methods (`Reconnect`, `Pause`, `Resume`, `ReloadConfig`), and signals (`ConnectionStateChanged`, `NotificationDelivered`, `NotificationFiltered`, `ErrorOccurred`). Shares state with the supervisor via `Arc<RwLock<SharedState>>` and sends events via `mpsc::Sender<Event>`.
- `config.rs` — loads `~/.config/ios-notifications/config.toml`. `[device]` section is required (written by the pair helper). All other sections (`[filter]`, `[notifications]`, `[supervisor]`) are optional with sane defaults.
- `filter.rs` — `Filter::should_show(app_id)` implements three modes: `blacklist` (default; drop listed apps), `whitelist` (only allow listed apps), `off` (pass everything). Bundle IDs are matched case-sensitively.

**`pair/` (`ios-notifications-pair`)** — one-shot CLI pairing helper. Makes the adapter discoverable with a HID keyboard BLE advertisement (iOS won't show generic peripherals in its pairing UI), waits for a device advertising the ANCS service UUID, marks it trusted, and writes `~/.config/ios-notifications/config.toml` (preserving any existing non-`[device]` sections).

**`tray/`** — Plasma 6 plasmoid (`io.github.michnaugh1.iosnotifications`). QML-based; reads connection state from the D-Bus interface and exposes Reconnect/Pause/Resume actions. Installed by `./scripts/install-tray.sh`.

## Key design decisions

- **HID advertisement trick**: iOS won't auto-reconnect to a generic BLE peripheral after sleep or Bluetooth restart. Advertising as a HID keyboard (UUID `0x1812`, appearance `0x03C1`) bypasses this restriction. Both the daemon and the pair helper advertise this way.
- **CCCD reset on reconnect**: iOS persists CCCD state for bonded devices. If CCCD is already `0x0001` from a prior session, iOS treats our subscribe as a no-op and won't send new ANCS events. The ANCS processor writes `0x0000` to the CCCD before subscribing to force a fresh session.
- **D-Bus GATT service fallback**: When a BR/EDR link (audio/phone) is active alongside BLE, BlueZ may not set `ServicesResolved = true` even though GATT objects are cached. `scan_services_from_dbus` enumerates service handle IDs 1–256 directly to find cached GATT services without waiting for `ServicesResolved`.
- **D-Bus tests use `#[serial_test::serial]`**: The tests in `dbus_iface.rs` acquire the session bus and must not run concurrently. Use `serial_test::serial` on all `dbus_iface` tests.

## Config reference

```toml
[device]
mac = "AA:BB:CC:DD:EE:FF"   # required; written by pair helper
adapter = "hci1"             # optional; defaults to system default

[filter]
mode = "blacklist"           # "blacklist" | "whitelist" | "off"
apps = ["com.apple.Stocks"]  # bundle IDs (case-sensitive)

[supervisor]
backoff_initial_s = 2
backoff_max_s = 60
resume_grace_ms = 1500       # wait after wake before reconnecting
```
