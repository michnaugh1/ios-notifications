# Design — `ios-notifications`: iOS notifications on KDE Plasma

| Field | Value |
|---|---|
| Date | 2026-05-18 |
| Status | Approved (pending user re-review of written form) |
| Author | mike (with Claude) |
| Target distro | Ubuntu 26.04 LTS, KDE Plasma 6.6.4, BlueZ 5.85 |
| Target iOS | iOS 26 (with backward compatibility to 18+) |

## 1. Goal

Receive iOS notifications on a KDE Plasma desktop, rendered through the native Plasma notification system, without requiring any app installed on the iPhone. Use the Apple Notification Center Service (ANCS) protocol — the same mechanism Apple Watch and Pebble use — over Bluetooth Low Energy.

The end-user experience: pair iPhone with the Linux box once, enable "Share System Notifications" on iOS, then every iPhone notification appears in Plasma's notification center automatically and persists in history like any other notification.

## 2. Non-goals

The following are explicitly out of scope:

- **Bidirectional features**: replying to iMessages, sending SMS, sharing clipboard, transferring files. ANCS is read-mostly; reply isn't exposed by Apple to BLE peripherals.
- **KDE Connect integration**: notifications go directly to `org.freedesktop.Notifications`, not through KDE Connect's "phone" UI. Simpler and shippable.
- **Graphical settings panel**: configuration is a TOML file. A future GUI can wrap this; not required for v1.
- **macOS or Windows support**: Linux only. Specifically targeted at KDE Plasma 6 on systemd-based distros.
- **Multi-device pairing**: one paired iPhone at a time. Multi-device support is a future extension if needed.
- **Notification actions**: ANCS exposes Positive/Negative actions (e.g. answer/decline call). Initial release displays read-only notifications; action support is a possible follow-up.

## 3. Constraints and context

- **iOS exposes ANCS as a standard BLE GATT service.** UUIDs and characteristics have been stable since iOS 7 (2013) and will not change in iOS 26 — Apple cannot break this without breaking every smartwatch on the planet.
- **iOS only auto-reconnects to BLE peripherals it perceives as HID keyboards/mice.** This is empirical knowledge from upstream's recent work. The daemon must advertise a fake HID keyboard GATT service to trigger iOS's auto-reconnect behavior.
- **Plasma 6 listens on the standard `org.freedesktop.Notifications` D-Bus interface.** No KDE-specific glue is required for rendering; any notification sent via libnotify or the `notify-rust` crate appears in Plasma automatically.
- **BlueZ 5.85 + `bluer` 0.17 is the integration point.** The local environment is confirmed working: USB Bluetooth adapter on `hci0`, BlueZ service active.
- **Rust toolchain is not yet installed**; will be added via `rustup` during implementation.

## 4. Architecture

Three components, plus the iPhone and existing Plasma notification infrastructure (which we do not build).

```
┌─────────────────────────────────────────────────────────────┐
│  iPhone (iOS 26) — Settings > Bluetooth > Allow Notifications│
└──────────────────────┬──────────────────────────────────────┘
                       │  BLE / GATT / ANCS service
                       ▼
┌─────────────────────────────────────────────────────────────┐
│  BlueZ 5.85 (system bluetoothd)                             │
└──────────────────────┬──────────────────────────────────────┘
                       │  D-Bus org.bluez (via `bluer` crate)
                       ▼
┌─────────────────────────────────────────────────────────────┐
│  COMPONENT 1: ios-notificationsd (the daemon)               │
│  Rust binary, systemd --user unit                           │
│  - Speaks ANCS, receives notifications                      │
│  - Advertises dummy HID GATT for iOS auto-reconnect         │
│  - Applies filter rules from config                         │
│  - Listens to logind PrepareForSleep                        │
│  - Reconnects on resume                                     │
│  - Exposes D-Bus interface for control + status             │
│  - Forwards passed notifications to libnotify               │
└─────┬─────────────────────────────────────────────┬─────────┘
      │ org.freedesktop.Notifications              │ io.github.<x>.IosNotifications
      ▼ (existing, built-in to KDE)                │ (our own)
┌──────────────────────────┐         ┌────────────────────────┐
│  Plasma notification     │         │  COMPONENT 2: tray     │
│  system (existing)       │◀────────│  Plasma 6 plasmoid     │
│  - Pop-ups, history,     │         │  (QML, JS, KF6)        │
│    Do Not Disturb        │         │  - State icon          │
└──────────────────────────┘         │  - Right-click actions │
                                     └────────────────────────┘

                                     ┌────────────────────────┐
                                     │  COMPONENT 3: pair     │
                                     │  Rust CLI, one-shot    │
                                     │  - Discoverable mode   │
                                     │  - Pair from iPhone    │
                                     │  - Verify ANCS         │
                                     │  - Write config        │
                                     └────────────────────────┘
```

The three components communicate only through the daemon's D-Bus interface. The tray applet never touches Bluetooth directly. The pair helper runs once and exits; it does not run alongside the daemon.

## 5. Engine strategy: fork-and-evolve

The daemon is a fork of [`kmod-midori/ancs-linux`](https://github.com/kmod-midori/ancs-linux), an actively-maintained Rust ANCS client (last commit 2026-05-16).

**Rationale**: The upstream project solves three hard problems we would otherwise have to solve ourselves:

1. **The HID auto-reconnect trick** (~120 lines): advertise as a fake HID keyboard so iOS treats us like an Apple Magic Keyboard and reconnects automatically. This was the result of approximately 2 years of empirical work.
2. **ANCS protocol plumbing** (~200 lines): GATT service discovery, characteristic notification subscription, control-point writes, data-source parsing.
3. **App-name caching** (~50 lines): the protocol delivers notifications by bundle ID; app display names are fetched on first sight and cached.

Total upstream size: ~530 lines of Rust in a single `main.rs`. License: MIT.

**Our changes**:

- Refactor the single `main.rs` into modules: `ancs`, `hid_bridge`, `config`, `filter`, `dbus_iface`, `supervisor`, `main`.
- Replace the bare `loop { try; sleep 10s }` reconnect with a proper state machine driven by logind events.
- Add a filter check inside `process_data` before `notify_rust::Notification::show_async()`.
- Add a config loader (`~/.config/ios-notifications/config.toml`).
- Add a D-Bus server that exposes `State`, `Reconnect()`, `Pause()`, `Resume()`, `ReloadConfig()`, plus `StateChanged` / `NotificationDelivered` / `NotificationFiltered` signals.

**Upstream relationship**: protocol-layer bug fixes we discover get sent upstream as PRs. Daemon-architecture changes stay in our fork — they're scope-specific. README credits upstream prominently.

## 6. Component details

### 6.1 Daemon (`ios-notificationsd`)

Single Rust binary. Async via `tokio`. Roughly 600 lines of new code on top of the inherited 530.

Module layout:

| Module | Responsibility | Source |
|---|---|---|
| `ancs.rs` | `AncsProcessor`, protocol parsing | inherited, ~350 lines |
| `hid_bridge.rs` | `serve_hid_gatt()` and HID descriptor | inherited, ~150 lines |
| `config.rs` | TOML load/reload, schema validation | new, ~80 lines |
| `filter.rs` | Per-app blacklist/whitelist logic | new, ~50 lines |
| `dbus_iface.rs` | D-Bus server, signals, methods | new, ~200 lines |
| `supervisor.rs` | State machine, logind integration, backoff | new, ~250 lines |
| `main.rs` | Wire everything together, CLI args | new, ~50 lines |

Dependencies (Cargo.toml additions on top of upstream):
- `zbus = "5"` — D-Bus server
- `toml = "0.8"` + `serde = { version = "1", features = ["derive"] }` — config
- `notify-rust` — already present upstream
- `bluer`, `ancs`, `tokio`, `anyhow`, `clap`, `log`, `env_logger` — already present upstream

### 6.2 Tray applet (`ios-notifications-tray`)

A Plasma 6 plasmoid using KF6 / QML / JavaScript. Installs to `~/.local/share/plasma/plasmoids/io.github.<repo-owner>.iosnotifications/`.

Files:
- `metadata.json` — plasmoid descriptor
- `contents/ui/main.qml` — root component, system tray icon
- `contents/ui/CompactRepresentation.qml` — icon in tray
- `contents/ui/FullRepresentation.qml` — popup with state + actions
- `contents/config/config.qml` — plasmoid settings (refresh interval, icon style)

The applet uses `org.kde.plasma.private.system.dbus` or equivalent to:
1. Subscribe to `StateChanged` and `NotificationDelivered` signals.
2. Read `State`, `LastError`, `NotificationsToday` properties.
3. Call `Reconnect()`, `Pause()`, `Resume()`, `ReloadConfig()` on user action.

Icon states:

| State | Visual | Tooltip |
|---|---|---|
| `connected` | green badge over phone icon | "iOS Notifications — Connected (N today)" |
| `connecting` | spinner | "iOS Notifications — Connecting…" |
| `backoff` | yellow badge | "iOS Notifications — Retrying… (next attempt in Ns)" |
| `paused` | gray badge | "iOS Notifications — Paused" |
| `error` | red badge | "iOS Notifications — Error: <last_error>" |
| `initializing` | gray spinner | "iOS Notifications — Starting…" |

Right-click menu:
- Reconnect now
- Pause / Resume (toggle, label switches)
- Reload config
- Open config file in $EDITOR
- Show recent notifications (opens journalctl with appropriate filter)
- About

### 6.3 Pair helper (`ios-notifications-pair`)

One-shot Rust CLI. Run during initial setup. Idempotent — re-running re-pairs.

Flow:

```
$ ios-notifications-pair
ios-notifications first-time setup

[1/5] Checking BlueZ status…                            ✓ Running
[2/5] Identifying default adapter…                      ✓ hci0 (30:89:4A:AE:CA:B2)
[3/5] Making adapter discoverable for 180s…             ✓ Done

        ── On your iPhone ──
        1. Open Settings → Bluetooth.
        2. Wait for this computer to appear and tap it.
        3. Confirm the pairing code matches.
        4. After pairing, leave the iOS Bluetooth screen OPEN.

[4/5] Waiting for ANCS service on connected device…     [waiting…]
                                                        ✓ Found on AA:BB:CC:DD:EE:FF
[5/5] Marking device trusted, writing config…           ✓ ~/.config/ios-notifications/config.toml

Setup complete!

Start the service:
    systemctl --user enable --now ios-notifications.service

Verify:
    journalctl --user -u ios-notifications -f
```

Implementation notes:

- Uses `bluer` to set adapter `Pairable = true`, `Discoverable = true`, `DiscoverableTimeout = 180`.
- Watches `org.bluez` for new `Device1` objects whose ANCS service UUID is in their `UUIDs` array.
- On success: sets `Trusted = true` on the device, writes `~/.config/ios-notifications/config.toml` with `[device].mac = <addr>`.
- On failure modes (no ANCS service, pairing rejected, timeout): prints actionable error pointing to the relevant iPhone setting.

## 7. Data flow

Single notification, happy path:

1. iPhone publishes notification (e.g. iMessage from Alice).
2. iOS writes the Notification Source GATT characteristic over the existing BLE link.
3. BlueZ dispatches via D-Bus `org.bluez`.
4. `bluer` crate delivers bytes to `AncsProcessor::process_notification()`.
5. `process_notification` parses: `event_id`, `event_flags`, `category_id`, `notification_uid`.
   - If `event_id == NotificationRemoved` → drop.
   - If `EventFlag::PreExisting` set → drop.
6. `process_notification` writes a `GetNotificationAttributes` request to the Control Point GATT, asking for AppIdentifier, Title, Subtitle, Message.
7. iOS responds via the Data Source GATT characteristic.
8. `bluer` delivers to `process_data()`.
9. `process_data` parses the response. Extracts app_id, title, message. If app_name not cached, fires a `GetAppAttributes` request (response handled separately).
10. **`filter::should_show(app_id)` consulted.** If muted → emit `NotificationFiltered` D-Bus signal, return.
11. Otherwise: build `notify_rust::Notification`, call `show_async()`.
12. `notify-rust` sends via `org.freedesktop.Notifications`.
13. Plasma renders pop-up; notification enters notification history.
14. Emit `NotificationDelivered` D-Bus signal (used by tray for counter and recent-notifications view).

## 8. Connection state machine

States: `INITIALIZING`, `CONNECTING`, `CONNECTED`, `BACKOFF`, `PAUSED`, `ERROR`.

`ERROR` is a *transient* recoverable state used for runtime conditions that may resolve on their own (BlueZ momentarily unavailable, ANCS service not yet shared). Conditions that cannot recover without user action (missing config, unpaired device, unknown adapter) bypass the state machine and exit the process with a clear message — they do not enter `ERROR`.

```
            ┌──────────────────────────┐
            │       INITIALIZING       │
            │  Load config, open       │
            │  BlueZ session           │
            └────────────┬─────────────┘
                         ▼
            ┌──────────────────────────┐
   ┌───────▶│       CONNECTING         │◀──── Reconnect() D-Bus call
   │        └──┬───────────────┬───────┘      DeviceRemoved event
   │ success  │               │ failure       Resume from sleep
   │           ▼               ▼
   │   ┌──────────────┐  ┌──────────────────┐
   │   │  CONNECTED   │  │  BACKOFF         │
   │   │  HID adv on, │  │  2s,4s,8s,16s,   │
   │   │  notifs flow │  │  32s,60s,60s...  │
   │   └──────┬───────┘  └─────────┬────────┘
   │          │                     │ timer
   │          │ link drop OR        │ fires
   │          │ DeviceRemoved       │
   │          └──────┬──────────────┘
   │                 ▼
   │           CONNECTING (return to top)
   │
   │   ┌──────────────────────────┐
   ├──▶│       PAUSED              │◀── Pause() OR sleep signal
   │   │  HID adv stopped,         │
   │   │  BLE link torn down       │
   │   └──────────┬────────────────┘
   │              │
   │              │ Resume() OR wake signal
   │              ▼
   │       CONNECTING (with resume_grace_ms delay)
   │
   │   ┌──────────────────────────┐
   ├──▶│       ERROR               │◀── Transient runtime failure
   │   │  HID adv on if possible,  │    (BlueZ down, ANCS missing)
   │   │  retry on a timer         │
   │   └──────────┬────────────────┘
   │              │
   │              │ retry timer fires (5s for BlueZ;
   │              │ 30s for ANCS-missing)
   │              ▼
   └──────  CONNECTING
```

Driver events:

| Event | Triggers transition to |
|---|---|
| Daemon start | `INITIALIZING` |
| Initialization done | `CONNECTING` |
| ANCS link established | `CONNECTED` |
| GATT connect fails (recoverable) | `BACKOFF` |
| BlueZ unavailable | `ERROR` (retry timer 5s) |
| Connected but ANCS service absent | `ERROR` (retry timer 30s) |
| `BACKOFF` timer fires | `CONNECTING` |
| `ERROR` retry timer fires | `CONNECTING` |
| `bluer::AdapterEvent::DeviceRemoved` | `CONNECTING` (after 500ms) |
| logind `PrepareForSleep(true)` | `PAUSED` |
| logind `PrepareForSleep(false)` | `CONNECTING` (after `resume_grace_ms`) |
| D-Bus `Pause()` | `PAUSED` |
| D-Bus `Resume()` | `CONNECTING` |
| D-Bus `Reconnect()` | `CONNECTING` (any state) |

Backoff is exponential with cap: 2, 4, 8, 16, 32, 60, 60, 60s. Reset on successful connect.

Conditions that bypass the state machine and exit the process: missing config, malformed config, configured adapter not present, configured device not paired. These are described in Section 11.

## 9. D-Bus interface

| Field | Value |
|---|---|
| Bus | session (per-user) |
| Bus name | `io.github.<repo-owner>.IosNotifications` (finalized at first commit) |
| Object path | `/IosNotifications` |
| Interface | `io.github.<repo-owner>.IosNotifications1` |

Properties (read-only):
- `State` — string: `"initializing"`, `"connecting"`, `"connected"`, `"paused"`, `"backoff"`, `"error"`
- `DeviceAddress` — string: paired MAC; empty if unpaired
- `LastError` — string: last error text; empty when none
- `NotificationsToday` — uint32: counter since midnight local time (resets at 00:00)
- `NextBackoffSecs` — uint32: seconds until next reconnect attempt when in `backoff`; zero otherwise

Methods (all return void unless noted):
- `Reconnect()` — forces immediate transition to `CONNECTING` from any state
- `Pause()` — transitions to `PAUSED`
- `Resume()` — transitions out of `PAUSED` to `CONNECTING`
- `ReloadConfig()` — re-reads `config.toml`; returns parse error as D-Bus error if invalid

Signals:
- `StateChanged(string new_state, string old_state)`
- `NotificationDelivered(string app_id, string title)`
- `NotificationFiltered(string app_id, string title)`
- `ErrorOccurred(string message)` — emitted on transition into `error`

## 10. Configuration

File: `~/.config/ios-notifications/config.toml`. XDG-compliant. Auto-created by `ios-notifications-pair`. Hand-editable. Re-read on `ReloadConfig()` — no daemon restart needed for filter changes.

```toml
[device]
mac = "AA:BB:CC:DD:EE:FF"   # written by pair helper
adapter = "hci0"             # optional; omit for default adapter

[notifications]
show_connection_state = true
connection_state_timeout_ms = 2000

[filter]
# "blacklist" (default), "whitelist", or "off"
mode = "blacklist"

# Bundle IDs as iOS reports them. Find by tailing journalctl --user
# -u ios-notifications -f and noting incoming app identifiers.
apps = [
  # "com.apple.Stocks",
  # "com.apple.news",
]

[supervisor]
backoff_initial_s = 2
backoff_max_s = 60
resume_grace_ms = 1500
```

The pair helper writes only `[device]`. All other sections fall back to defaults if missing. Invalid TOML or schema mismatch produces a clear error pointing to the offending line.

## 11. Error handling

Philosophy: never crash; always degrade visibly. Distinguish errors that may resolve on their own (recoverable → enter `error` state, retry) from errors that need user action (terminal → exit 1 with clear message).

### Terminal conditions (exit 1 with logged guidance)

These are user-action-required problems where retrying would just spin pointlessly.

| Condition | Exit message |
|---|---|
| Config file missing | "Run `ios-notifications-pair` first to set up pairing." |
| Config malformed | "Config error at line N: <parse error>. Edit `~/.config/ios-notifications/config.toml`." |
| Configured adapter not present | "Adapter `<name>` not found. Available adapters: <list>. Edit `[device].adapter` in config." |
| Configured device not paired | "Device `<MAC>` not paired. Run `ios-notifications-pair` again." |

### Recoverable conditions (enter `ERROR` state, auto-retry)

These conditions may resolve on their own; the daemon stays alive and retries.

| Condition | Behavior |
|---|---|
| BlueZ not running | `ERROR`, retry every 5s forever, log warning. Returns to `CONNECTING` when BlueZ is back. |
| Connected but ANCS service absent | `ERROR`, retry every 30s. Pop sticky desktop notification: "iOS notifications aren't being shared. On iPhone, Settings → Bluetooth → tap (i) next to this device → enable 'Share System Notifications'." |
| BLE link drops | `BACKOFF` (exponential up to 60s), then `CONNECTING`. |
| `bluer::AdapterEvent::DeviceRemoved` | `CONNECTING` after 500ms. |

### Normal lifecycle events (not errors)

| Event | Behavior |
|---|---|
| Suspend signal (`PrepareForSleep(true)`) | Clean disconnect, state → `PAUSED`. |
| Resume signal (`PrepareForSleep(false)`) | After `resume_grace_ms`, state → `CONNECTING`. |

### Best-effort recovery (log and continue)

| Condition | Behavior |
|---|---|
| `notify-rust` send fails | Log, continue. Missing notification server must not kill the daemon. |
| HID GATT registration fails | Log warning, continue without HID. Link still works while iOS keeps it open; auto-reconnect won't trigger until next restart. |

Errors are surfaced three ways: structured logs to journald, the `LastError` D-Bus property (read by tray), and — for user-actionable problems only — a desktop notification.

## 12. Testing

End-to-end automation isn't realistic because the iPhone is a hard dependency. Strategy is layered.

### Unit tests (`cargo test`)

- `filter`: blacklist/whitelist/off modes against representative app-ID lists, including empty lists and case sensitivity
- `config`: valid TOML, missing optional sections, malformed TOML, type mismatch, partial sections
- `supervisor`: state-machine transitions table-driven, no real BlueZ
- Inherited protocol parsing: keep upstream's tests, add coverage for filter integration

### Integration tests (no iPhone needed)

- `bluer` mock GATT peer exposing a fake ANCS service. Daemon connects, subscribes, parses fixture notification bytes, forwards to a captured `notify-rust` sink. Verifies the protocol layer.
- D-Bus interface tests via `busctl introspect` and `busctl call`. Drives daemon through `Pause`/`Resume`/`Reconnect` from the command line. Verifies `StateChanged` signals fire correctly.

### Manual integration checklist (real iPhone, real KDE)

A markdown checklist in `docs/manual-tests.md`, run before any release tag:

1. Fresh-pair flow: `ios-notifications-pair` end-to-end from a clean adapter
2. Notification delivery: send iMessage from second phone, verify Plasma pop-up + history entry
3. Filter blacklist: add `com.apple.mobilemail` to mute, send mail to test address, verify suppression
4. Filter whitelist: switch to whitelist with only `com.apple.MobileSMS`, send mail + iMessage, verify only iMessage shows
5. Live reload: edit TOML, call `busctl --user call ... ReloadConfig`, verify filter change without restart
6. Suspend/resume: `systemctl suspend`, wake, verify auto-reconnect within ~3s
7. Bluetooth toggle: `bluetoothctl power off; sleep 5; bluetoothctl power on`, verify backoff and recovery
8. iPhone-side: toggle "Share System Notifications" OFF, verify clear error notification; toggle ON, verify recovery
9. Tray: every state transition produces correct icon and tooltip
10. Long-run soak: 24h continuous operation, verify no FD leak / RSS growth (`ps -o rss`, `lsof -p`)

### iOS 26 compatibility check (the original concern)

Executed as part of `ios-notifications-pair` development on the first iPhone pairing:

- Bluetooth pairing prompt UX hasn't changed materially from iOS 18
- "Share System Notifications" toggle still exists in the per-device settings panel
- ANCS UUID and characteristic structure unchanged (`bluetoothctl gatt.list-attributes` after pairing)
- Notification Source GATT delivers bytes in the expected wire format

If any of these reveal iOS 26 changes, scope expands to address them. Protocol stability makes this very unlikely.

## 13. Packaging and deployment

Single git repository, multi-binary cargo workspace plus a QML plasmoid directory.

```
ios-notifications/
├── Cargo.toml                      # workspace manifest
├── daemon/Cargo.toml
├── pair/Cargo.toml
├── tray/                           # QML, no Cargo
├── packaging/
│   ├── systemd/ios-notifications.service
│   └── debian/                     # debhelper rules for .deb on Ubuntu 26.04
├── docs/
│   ├── superpowers/specs/          # this file lives here
│   └── manual-tests.md
└── README.md
```

systemd user unit (`packaging/systemd/ios-notifications.service`):

```ini
[Unit]
Description=iOS notifications bridge (ANCS)
After=bluetooth.target
Wants=bluetooth.target

[Service]
ExecStart=%h/.local/bin/ios-notificationsd
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Installation:
- Phase 1 (developer): `cargo install --path daemon` + manual file copy for plasmoid + `systemctl --user enable`
- Phase 2 (later): `.deb` for Ubuntu 26.04 that drops binaries, plasmoid, systemd unit, and a `bluetooth` group requirement

## 14. Risks and open questions

| Risk | Mitigation |
|---|---|
| iOS 26 protocol changes break ANCS | Very low probability. If it happens, fork's protocol layer needs patching. Detect at first run during pair-helper development. |
| BlueZ 5.85 has quirks vs. upstream's tested versions | Upstream tested on Manjaro / Debian variants. Ubuntu 26.04 likely fine. Address during integration tests. |
| HID GATT trick stops working in future iOS | Fallback: manual reconnect via tray button. Already supported by the state machine. |
| Bluetooth adapter changes (e.g., USB unplug) | `bluer` emits adapter events; supervisor handles `AdapterRemoved` → `error` state. |
| Multi-iPhone households | Out of scope for v1. Single MAC in config. Architecture allows future extension to multiple `[device]` sections. |
| Plasma 6 plasmoid API changes in 6.7 | KF6 plasmoid API is stable in the 6.x series. Re-test on Plasma upgrades. |

Open questions to resolve before or during implementation (not blockers):

- Final D-Bus bus name (depends on GitHub username / repo owner)
- Whether to support multiple Bluetooth adapters in v1 (probably no — single `adapter` field)
- Whether to expose notification actions (Positive/Negative) — likely a v1.1 feature

## 15. References

- [`kmod-midori/ancs-linux`](https://github.com/kmod-midori/ancs-linux) — upstream Rust ANCS client (MIT)
- [`ianmarmour/ancs`](https://github.com/ianmarmour/ancs) — ANCS protocol type crate (used by upstream)
- [Apple ANCS specification](https://developer.apple.com/library/archive/documentation/CoreBluetooth/Reference/AppleNotificationCenterServiceSpecification/Specification/Specification.html)
- [BlueZ D-Bus API](https://github.com/bluez/bluez/tree/master/doc)
- [`bluer` crate documentation](https://docs.rs/bluer)
- [KDE Plasmoid development](https://develop.kde.org/docs/plasma/widget/)
- [`zbus` crate documentation](https://docs.rs/zbus)
- [freedesktop notifications spec](https://specifications.freedesktop.org/notification-spec/latest/)
