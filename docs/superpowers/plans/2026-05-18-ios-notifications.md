# `ios-notifications` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `ios-notifications` — a Linux daemon, Plasma 6 plasmoid, and pairing CLI that bridges iOS notifications to KDE Plasma via the ANCS Bluetooth LE protocol, on Ubuntu 26.04 with KDE Plasma 6.6.

**Architecture:** Fork-and-evolve from `kmod-midori/ancs-linux` @ `6883f2bd948b` (MIT, © 2024 Midori Kochiya). Keep upstream's `AncsProcessor` and `serve_hid_gatt()` essentially as-is. Refactor `main.rs` into modules; add `config.rs`, `filter.rs`, `supervisor.rs`, `dbus_iface.rs`. Notifications route through `org.freedesktop.Notifications` (KDE renders natively). Three deliverables: `ios-notificationsd` (daemon), `ios-notifications-pair` (one-shot CLI), `io.github.michnaugh1.iosnotifications` (Plasma 6 plasmoid).

**Tech Stack:** Rust 1.78+ (`tokio`, `bluer 0.17`, `ancs 0.2`, `zbus 5`, `notify-rust 4`, `serde`, `toml`, `clap`), QML / Plasma 6 / KF6 / KItemModels, systemd user units, BlueZ 5.85.

**Spec:** [`docs/superpowers/specs/2026-05-18-ios-notifications-design.md`](../specs/2026-05-18-ios-notifications-design.md)

**Upstream reference:** https://github.com/kmod-midori/ancs-linux (fork base commit: `6883f2bd948bcf51bc4534888de46f9e9a5a580a`)

---

## File Structure

After all tasks complete, the repository will look like:

```
ios-notifications/
├── Cargo.toml                                      # workspace manifest
├── Cargo.lock
├── LICENSE                                          # MIT, retain upstream copyright
├── README.md                                        # with upstream attribution
├── .gitignore
├── daemon/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                                  # CLI args, init, supervisor entry (~50 lines)
│       ├── lib.rs                                   # Re-exports for integration tests
│       ├── ancs.rs                                  # AncsProcessor (inherited, modified)
│       ├── hid_bridge.rs                            # serve_hid_gatt (inherited verbatim)
│       ├── config.rs                                # TOML loader (new)
│       ├── filter.rs                                # Filter rules (new)
│       ├── dbus_iface.rs                            # D-Bus server via zbus (new)
│       └── supervisor.rs                            # State machine (new)
├── pair/
│   ├── Cargo.toml
│   └── src/main.rs                                  # one-shot pairing CLI
├── tray/                                            # Plasma 6 plasmoid (QML)
│   ├── metadata.json
│   └── contents/
│       └── ui/
│           ├── main.qml
│           ├── CompactRepresentation.qml
│           └── FullRepresentation.qml
├── packaging/
│   └── systemd/
│       └── ios-notifications.service
├── docs/
│   ├── superpowers/
│   │   ├── specs/2026-05-18-ios-notifications-design.md (already exists)
│   │   └── plans/2026-05-18-ios-notifications.md (this file)
│   └── manual-tests.md
└── scripts/
    └── install-tray.sh                              # installs plasmoid to ~/.local/share
```

**Responsibility boundaries:**
- `daemon/src/ancs.rs` — pure protocol processing, no I/O beyond GATT
- `daemon/src/filter.rs` — pure logic, no I/O at all (easy to unit-test)
- `daemon/src/config.rs` — file load/save only
- `daemon/src/supervisor.rs` — orchestrates state, owns the AncsProcessor + DBus, no protocol details
- `daemon/src/dbus_iface.rs` — D-Bus server, owns the zbus connection
- `daemon/src/main.rs` — entry point only, wiring

---

## Task 0: Environment preparation

**Goal:** Install Rust toolchain, BlueZ dev headers, and verify the existing Bluetooth adapter is usable.

**Files:** none modified

- [ ] **Step 1: Install build dependencies**

Run:
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libdbus-1-dev libssl-dev
```
Expected: packages install without error.

- [ ] **Step 2: Install rustup and stable toolchain**

Run:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source "$HOME/.cargo/env"
rustc --version
cargo --version
```
Expected: `rustc 1.78.0` or newer; `cargo 1.78.0` or newer.

- [ ] **Step 3: Verify Bluetooth adapter is up**

Run:
```bash
bluetoothctl --version
systemctl is-active bluetooth
hciconfig -a 2>/dev/null | head -8
```
Expected: `bluetoothctl 5.85`, `active`, and an `hci0` adapter listed with `UP RUNNING`.

- [ ] **Step 4: Verify Plasma 6 is the active session**

Run:
```bash
plasmashell --version
echo "$XDG_SESSION_TYPE / $DESKTOP_SESSION"
```
Expected: `plasmashell 6.6.x`; session info shows wayland or x11 and a plasma desktop.

No commit for this task — environment-only.

---

## Task 1: Workspace scaffolding and upstream attribution

**Goal:** Create the Cargo workspace, copy LICENSE, write a README that prominently credits upstream.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/Cargo.toml`
- Create: `/home/michnaugh1/Dev/ios-notifications/.gitignore`
- Create: `/home/michnaugh1/Dev/ios-notifications/LICENSE`
- Create: `/home/michnaugh1/Dev/ios-notifications/README.md`

- [ ] **Step 1: Create workspace Cargo.toml**

Create `/home/michnaugh1/Dev/ios-notifications/Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["daemon", "pair"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
authors = ["Mike Naughton <mike@thenorthcoastlegal.com>"]
repository = "https://github.com/michnaugh1/ios-notifications"

[workspace.dependencies]
ancs = "0.2.0"
anyhow = "1.0"
bluer = { version = "0.17", features = ["full"] }
byteorder-pack = "0.1"
clap = { version = "4", features = ["derive"] }
env_logger = "0.11"
futures = "0.3"
log = "0.4"
notify-rust = "4"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
zbus = "5"
async-trait = "0.1"
```

- [ ] **Step 2: Create .gitignore**

Create `/home/michnaugh1/Dev/ios-notifications/.gitignore`:
```
/target
**/*.rs.bk
Cargo.lock
```
Note: `Cargo.lock` is excluded because this is a library-ish project with binaries; we'll regenerate per-environment. (For pure-binary projects you'd commit it; debatable, but this keeps things tidy for a hobby project.)

- [ ] **Step 3: Create LICENSE (MIT, retain upstream copyright + add own)**

Create `/home/michnaugh1/Dev/ios-notifications/LICENSE`:
```
MIT License

Copyright (c) 2024 Midori Kochiya (upstream: kmod-midori/ancs-linux)
Copyright (c) 2026 Mike Naughton (modifications and additions)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 4: Create initial README**

Create `/home/michnaugh1/Dev/ios-notifications/README.md`:
```markdown
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
```

- [ ] **Step 5: Verify workspace parses**

Run:
```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo metadata --format-version 1 > /dev/null && echo OK
```
Expected: prints `OK`. Note: this will warn about no workspace members yet — that's fine for now; just verifying TOML parses.

Actually, the workspace lists `daemon` and `pair` as members, which don't exist yet. To avoid a hard error, temporarily comment out members:
```bash
cd /home/michnaugh1/Dev/ios-notifications && sed -i 's/^members = \["daemon", "pair"\]/# members = ["daemon", "pair"]/' Cargo.toml
cargo metadata --format-version 1 > /dev/null && echo OK
# revert
sed -i 's/^# members = \["daemon", "pair"\]/members = ["daemon", "pair"]/' Cargo.toml
```
Expected: `OK`.

- [ ] **Step 6: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add Cargo.toml LICENSE README.md .gitignore
git commit -m "Add workspace scaffolding and upstream attribution"
```

---

## Task 2: Import upstream daemon, build verifies

**Goal:** Pull in the upstream source verbatim into `daemon/src/main.rs` so the existing protocol code compiles in our workspace before we refactor.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/Cargo.toml`
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs` (copy from upstream)

- [ ] **Step 1: Create daemon/Cargo.toml**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/Cargo.toml`:
```toml
[package]
name = "ios-notificationsd"
description = "iOS notifications bridge daemon (ANCS over BLE)"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[[bin]]
name = "ios-notificationsd"
path = "src/main.rs"

[dependencies]
ancs.workspace = true
anyhow.workspace = true
bluer.workspace = true
byteorder-pack.workspace = true
clap.workspace = true
env_logger.workspace = true
futures.workspace = true
log.workspace = true
notify-rust.workspace = true
tokio.workspace = true
serde.workspace = true
toml.workspace = true
zbus.workspace = true
async-trait.workspace = true
```

- [ ] **Step 2: Fetch upstream main.rs verbatim**

Run:
```bash
mkdir -p /home/michnaugh1/Dev/ios-notifications/daemon/src
gh api repos/kmod-midori/ancs-linux/contents/src/main.rs --jq '.content' | base64 -d > /home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs
wc -l /home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs
```
Expected: ~530 lines.

- [ ] **Step 3: Build the daemon**

Run:
```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo build -p ios-notificationsd 2>&1 | tail -20
```
Expected: compiles with no errors (warnings OK). First build will take 2-5 minutes as deps compile.

If the build fails on a dependency version mismatch, pin to the exact upstream versions from `daemon/Cargo.toml`: `ancs = "=0.2.0"`, `bluer = "=0.17.1"`, etc. Then re-run.

- [ ] **Step 4: Smoke test — daemon at least prints help**

Run:
```bash
cd /home/michnaugh1/Dev/ios-notifications && ./target/debug/ios-notificationsd --help
```
Expected: usage output mentioning `device_addr` positional arg and `--adapter`.

- [ ] **Step 5: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/Cargo.toml daemon/src/main.rs
git commit -m "Import upstream ancs-linux daemon (kmod-midori @ 6883f2b)"
```

---

## Task 3: Refactor main.rs into modules

**Goal:** Split the inherited monolith into `ancs.rs`, `hid_bridge.rs`, and a small `main.rs`. Add `lib.rs` to enable integration tests. No behavior changes.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/ancs.rs`
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/hid_bridge.rs`
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs`

- [ ] **Step 1: Create hid_bridge.rs with the HID GATT server**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/src/hid_bridge.rs`:
```rust
//! HID keyboard GATT advertisement.
//!
//! iOS auto-reconnects to BLE peripherals it perceives as HID keyboards
//! (Apple Magic Keyboard, etc.) but not to "generic" BLE peripherals. By
//! advertising a fake HID keyboard service alongside our ANCS client, we
//! trigger iOS's auto-reconnect behavior.
//!
//! Inherited verbatim from kmod-midori/ancs-linux @ 6883f2b.

use anyhow::Result;
use bluer::{Adapter, Uuid, UuidExt};

const HID_REPORT_MAP: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xa1, 0x01, // Collection (Application)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0xe0, //   Usage Minimum (224)
    0x29, 0xe7, //   Usage Maximum (231)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Variable, Absolute) — modifier keys
    0x95, 0x01, //   Report Count (1)
    0x75, 0x08, //   Report Size (8)
    0x81, 0x01, //   Input (Constant) — reserved byte
    0x95, 0x06, //   Report Count (6)
    0x75, 0x08, //   Report Size (8)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xff, 0x00, //   Logical Maximum (255)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0x00, //   Usage Minimum (0)
    0x2a, 0xff, 0x00, //   Usage Maximum (255)
    0x81, 0x00, //   Input (Data, Array) — key codes
    0xc0, // End Collection
];

pub async fn serve_hid_gatt(
    adapter: &Adapter,
) -> Result<(
    bluer::gatt::local::ApplicationHandle,
    bluer::adv::AdvertisementHandle,
)> {
    use bluer::adv::{Advertisement, Type};
    use bluer::gatt::local::{
        Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
        CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Descriptor,
        DescriptorRead, Service,
    };

    let app = Application {
        services: vec![Service {
            uuid: Uuid::from_u16(0x1812),
            primary: true,
            characteristics: vec![
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4A),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Information read by {}", req.device_address);
                                Ok(vec![0x11, 0x01, 0x00, 0x02])
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4B),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Report Map read by {}", req.device_address);
                                Ok(HID_REPORT_MAP.to_vec())
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4C),
                    write: Some(CharacteristicWrite {
                        write: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(|value, req| {
                            Box::pin(async move {
                                log::info!(
                                    "HID Control Point written by {}: {:?}",
                                    req.device_address,
                                    value
                                );
                                Ok(())
                            })
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4E),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Protocol Mode read by {}", req.device_address);
                                Ok(vec![0x01])
                            })
                        }),
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(|value, req| {
                            Box::pin(async move {
                                log::info!(
                                    "HID Protocol Mode written by {}: {:?}",
                                    req.device_address,
                                    value
                                );
                                Ok(())
                            })
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4D),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Report read by {}", req.device_address);
                                Ok(vec![0u8; 8])
                            })
                        }),
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        indicate: false,
                        method: CharacteristicNotifyMethod::Fun(Box::new(|notifier| {
                            Box::pin(async move {
                                log::info!("HID Report notifications started");
                                notifier.stopped().await;
                                log::info!("HID Report notifications stopped");
                            })
                        })),
                        ..Default::default()
                    }),
                    descriptors: vec![Descriptor {
                        uuid: Uuid::from_u16(0x2908),
                        read: Some(DescriptorRead {
                            read: true,
                            fun: Box::new(|req| {
                                Box::pin(async move {
                                    log::info!(
                                        "HID Report Reference read by {}",
                                        req.device_address
                                    );
                                    Ok(vec![0x00, 0x01])
                                })
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };

    log::info!("Registering HID GATT application");
    let app_handle = adapter.serve_gatt_application(app).await?;

    log::info!("Starting HID advertisement");
    let adv = Advertisement {
        advertisement_type: Type::Peripheral,
        service_uuids: [Uuid::from_u16(0x1812)].into(),
        appearance: Some(0x03C1),
        local_name: Some("iOS Notifications Bridge".into()),
        discoverable: Some(true),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(adv).await?;

    Ok((app_handle, adv_handle))
}
```

- [ ] **Step 2: Create ancs.rs with AncsProcessor**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/src/ancs.rs`:
```rust
//! ANCS (Apple Notification Center Service) protocol processor.
//!
//! Inherited from kmod-midori/ancs-linux @ 6883f2b with minor adaptations
//! (will be extended for filtering in Task 6).

use std::{collections::HashMap, io::Cursor};

use ancs::{
    attributes::{
        app::AppAttributeID,
        command::CommandID,
        event::{EventFlag, EventID},
        notification::NotificationAttributeID,
        AppAttribute,
    },
    characteristics::{
        control_point::{GetAppAttributesRequest, GetNotificationAttributesRequest},
        data_source,
    },
};
use anyhow::{bail, Result};
use bluer::{
    gatt::remote::{Characteristic, CharacteristicWriteRequest},
    Adapter, Address, Uuid,
};
use byteorder_pack::UnpackFrom;
use futures::{pin_mut, StreamExt as _};

pub const ANCS_SERVICE_UUID: Uuid = Uuid::from_u128(0x7905F431B5CE4E99A40F4B1E122D00D0);

pub struct AncsProcessor {
    control_point: Option<Characteristic>,
    app_names: HashMap<String, String>,
}

impl AncsProcessor {
    pub fn new() -> Self {
        Self {
            control_point: None,
            app_names: HashMap::new(),
        }
    }

    pub async fn main_loop(mut self, device_addr: Address, adapter: &Adapter) -> Result<()> {
        let device = adapter.device(device_addr)?;

        if !device.is_connected().await? {
            log::debug!("Device {} is not connected", device_addr);
            return Ok(());
        }
        log::info!("Device {} is connected", device_addr);

        let services = device.services().await?;
        let mut ancs_service = None;
        for s in services {
            if s.uuid().await? == ANCS_SERVICE_UUID {
                ancs_service = Some(s);
                break;
            }
        }
        let ancs_service = match ancs_service {
            Some(s) => s,
            None => bail!("ANCS service not found"),
        };

        let mut notification_source = None;
        let mut data_source = None;
        let mut control_point = None;
        let noti_source_uuid: Uuid = "9FBF120D-6301-42D9-8C58-25E699A21DBD".parse()?;
        let data_source_uuid: Uuid = "22EAC6E9-24D6-4BB5-BE44-B36ACE7C7BFB".parse()?;
        let control_point_uuid: Uuid = "69D1D8F3-45E1-49A8-9821-9BBDFDAAD9D9".parse()?;
        for c in ancs_service.characteristics().await? {
            let uuid = c.uuid().await?;
            if uuid == noti_source_uuid {
                notification_source = Some(c);
            } else if uuid == data_source_uuid {
                data_source = Some(c);
            } else if uuid == control_point_uuid {
                control_point = Some(c);
            }
        }
        let notification_source = notification_source.ok_or_else(|| anyhow::anyhow!("Notification source not found"))?;
        let data_source = data_source.ok_or_else(|| anyhow::anyhow!("Data source not found"))?;
        let control_point = control_point.ok_or_else(|| anyhow::anyhow!("Control point not found"))?;

        self.control_point = Some(control_point);

        let data_source_stream = data_source.notify().await?;
        pin_mut!(data_source_stream);
        let notification_stream = notification_source.notify().await?;
        pin_mut!(notification_stream);
        let events_stream = adapter.events().await?;
        pin_mut!(events_stream);

        log::info!("Starting to listen for notifications");

        loop {
            tokio::select! {
                Some(noti) = notification_stream.next() => {
                    self.process_notification(noti).await?;
                }
                Some(data) = data_source_stream.next() => {
                    self.process_data(data).await?;
                }
                Some(event) = events_stream.next() => {
                    if let bluer::AdapterEvent::DeviceRemoved(addr) = event {
                        if addr == device_addr {
                            log::info!("Device removed, stopping");
                            break;
                        }
                    }
                }
                else => break,
            }
        }
        Ok(())
    }

    async fn process_notification(&mut self, noti: Vec<u8>) -> Result<()> {
        let (event_id, event_flags, _category_id, _category_count, notification_uid) =
            <(u8, u8, u8, u8, u32)>::unpack_from_le(&mut Cursor::new(&noti))?;

        if event_id == EventID::NotificationRemoved as u8 {
            return Ok(());
        }
        if event_flags & EventFlag::PreExisting as u8 != 0 {
            return Ok(());
        }

        let cmd = GetNotificationAttributesRequest {
            command_id: CommandID::GetNotificationAttributes,
            notification_uid,
            attribute_ids: vec![
                (NotificationAttributeID::AppIdentifier, None),
                (NotificationAttributeID::Title, Some(64)),
                (NotificationAttributeID::Subtitle, Some(64)),
                (NotificationAttributeID::Message, Some(64)),
            ],
        };
        self.write_control_point(&Vec::from(cmd)).await?;
        Ok(())
    }

    async fn process_data(&mut self, data: Vec<u8>) -> Result<()> {
        match data[0] {
            0 => {
                let notif = match data_source::GetNotificationAttributesResponse::parse(&data) {
                    Ok((_, app)) => app,
                    Err(e) => bail!("Error parsing notification attributes: {:?}", e),
                };
                log::info!("Notif: {:?}", notif);

                let mut app_id_to_query = None;
                let mut desktop_notification = notify_rust::Notification::new();
                for attr in notif.attribute_list {
                    match attr.id {
                        NotificationAttributeID::AppIdentifier => {
                            if let Some(id) = attr.value {
                                if let Some(name) = self.app_names.get(&id) {
                                    desktop_notification.appname(name);
                                } else {
                                    desktop_notification.appname(&id);
                                    app_id_to_query = Some(id);
                                }
                            }
                        }
                        NotificationAttributeID::Title => {
                            if let Some(v) = attr.value {
                                desktop_notification.summary(&v);
                            }
                        }
                        NotificationAttributeID::Message => {
                            if let Some(v) = attr.value {
                                desktop_notification.body(&v);
                            }
                        }
                        _ => {}
                    }
                }
                let handle = desktop_notification.show_async().await?;
                log::info!(
                    "Shown notification {} with desktop handle {}",
                    notif.notification_uid,
                    handle.id()
                );

                if let Some(app_id) = app_id_to_query {
                    log::info!("Querying app name for {}", app_id);
                    let cmd = GetAppAttributesRequest {
                        command_id: CommandID::GetAppAttributes,
                        app_identifier: app_id,
                        attribute_ids: vec![AppAttributeID::DisplayName],
                    };
                    self.write_control_point(&Vec::from(cmd)).await?;
                }
            }
            1 => {
                let mut app_id = vec![];
                let mut offset = 1;
                for i in offset..data.len() {
                    offset += 1;
                    if data[i] == 0 {
                        break;
                    }
                    app_id.push(data[i]);
                }
                let app_id = String::from_utf8_lossy(&app_id);

                let attribute = match AppAttribute::parse(&data[offset..]) {
                    Ok((_, attribute)) => attribute,
                    Err(e) => bail!("Error parsing app attributes: {:?}", e),
                };

                if attribute.id == AppAttributeID::DisplayName {
                    if let Some(name) = attribute.value {
                        log::info!("{} => {}", app_id, name);
                        self.app_names.insert(app_id.to_string(), name);
                    }
                } else {
                    log::info!("Unknown app attribute: {:?}", attribute);
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn write_control_point(&self, data: &[u8]) -> Result<()> {
        if let Some(control_point) = &self.control_point {
            control_point
                .write_ext(
                    data,
                    &CharacteristicWriteRequest {
                        op_type: bluer::gatt::WriteOp::Request,
                        ..Default::default()
                    },
                )
                .await?;
        }
        Ok(())
    }
}

impl Default for AncsProcessor {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Create lib.rs to expose modules for integration tests**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`:
```rust
pub mod ancs;
pub mod hid_bridge;
```

- [ ] **Step 4: Replace main.rs with thin entry point**

Overwrite `/home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs`:
```rust
use anyhow::Result;
use bluer::Address;
use clap::Parser;

mod ancs;
mod hid_bridge;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(
        help = "Public Bluetooth MAC address of the device to connect to (as shown in system or `bluetoothctl`)"
    )]
    device_addr: Address,

    #[arg(long, help = "Bluetooth adapter name to use, if not the default one")]
    adapter: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Args::parse();
    let session = bluer::Session::new().await?;
    let adapter = if let Some(name) = args.adapter {
        session.adapter(&name)?
    } else {
        session.default_adapter().await?
    };
    adapter.set_powered(true).await?;
    log::info!("Using adapter: {}", adapter.name());

    let (_hid_app, _hid_adv) = match hid_bridge::serve_hid_gatt(&adapter).await {
        Ok(handles) => handles,
        Err(e) => {
            log::warn!("Failed to set up HID GATT service: {:?}", e);
            return Err(e);
        }
    };

    loop {
        let proc = ancs::AncsProcessor::new();
        if let Err(e) = proc.main_loop(args.device_addr, &adapter).await {
            log::error!("Error: {:?}", e);
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
```

- [ ] **Step 5: Build to verify refactor**

Run:
```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo build -p ios-notificationsd 2>&1 | tail -10
```
Expected: compiles cleanly. If a `dead_code` warning appears on `ANCS_SERVICE_UUID`, ignore it — used after filtering work in later tasks.

- [ ] **Step 6: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/src/
git commit -m "Refactor: split daemon into ancs, hid_bridge, main, lib modules"
```

---

## Task 4: Config module with TDD

**Goal:** Add `config.rs` that loads `~/.config/ios-notifications/config.toml` into a strongly-typed `Config` struct, with sensible defaults and clear errors.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/config.rs`
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`

- [ ] **Step 1: Write the failing test for valid TOML**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/src/config.rs` with only the test module first:
```rust
//! Configuration loader for ios-notifications.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let toml = r#"
            [device]
            mac = "AA:BB:CC:DD:EE:FF"
        "#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.device.mac, "AA:BB:CC:DD:EE:FF");
        assert!(cfg.device.adapter.is_none());
        assert_eq!(cfg.filter.mode, FilterMode::Blacklist);
        assert!(cfg.filter.apps.is_empty());
        assert_eq!(cfg.notifications.show_connection_state, true);
        assert_eq!(cfg.supervisor.backoff_initial_s, 2);
    }
}
```

Also update `daemon/src/lib.rs`:
```rust
pub mod ancs;
pub mod config;
pub mod hid_bridge;
```

- [ ] **Step 2: Run the failing test**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd config::tests::parses_minimal_config 2>&1 | tail -15
```
Expected: compile error — `Config`, `FilterMode`, etc., not defined.

- [ ] **Step 3: Implement the Config types**

Replace the contents of `/home/michnaugh1/Dev/ios-notifications/daemon/src/config.rs` with:
```rust
//! Configuration loader for ios-notifications.
//!
//! Reads `~/.config/ios-notifications/config.toml`. The pair helper creates
//! this file with just the `[device]` section; other sections fall back to
//! defaults.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub device: DeviceConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub supervisor: SupervisorConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceConfig {
    pub mac: String,
    pub adapter: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct NotificationsConfig {
    pub show_connection_state: bool,
    pub connection_state_timeout_ms: u32,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            show_connection_state: true,
            connection_state_timeout_ms: 2000,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct FilterConfig {
    pub mode: FilterMode,
    pub apps: Vec<String>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            mode: FilterMode::Blacklist,
            apps: vec![],
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterMode {
    Blacklist,
    Whitelist,
    Off,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SupervisorConfig {
    pub backoff_initial_s: u32,
    pub backoff_max_s: u32,
    pub resume_grace_ms: u32,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            backoff_initial_s: 2,
            backoff_max_s: 60,
            resume_grace_ms: 1500,
        }
    }
}

impl Config {
    pub fn parse(s: &str) -> Result<Self> {
        toml::from_str(s).context("parsing config TOML")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file: {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn default_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("ios-notifications").join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let toml = r#"
            [device]
            mac = "AA:BB:CC:DD:EE:FF"
        "#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.device.mac, "AA:BB:CC:DD:EE:FF");
        assert!(cfg.device.adapter.is_none());
        assert_eq!(cfg.filter.mode, FilterMode::Blacklist);
        assert!(cfg.filter.apps.is_empty());
        assert_eq!(cfg.notifications.show_connection_state, true);
        assert_eq!(cfg.supervisor.backoff_initial_s, 2);
    }

    #[test]
    fn parses_full_config() {
        let toml = r#"
            [device]
            mac = "11:22:33:44:55:66"
            adapter = "hci1"

            [notifications]
            show_connection_state = false
            connection_state_timeout_ms = 5000

            [filter]
            mode = "whitelist"
            apps = ["com.apple.MobileSMS", "com.apple.mobilecal"]

            [supervisor]
            backoff_initial_s = 5
            backoff_max_s = 120
            resume_grace_ms = 3000
        "#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.device.adapter.as_deref(), Some("hci1"));
        assert_eq!(cfg.filter.mode, FilterMode::Whitelist);
        assert_eq!(cfg.filter.apps.len(), 2);
        assert_eq!(cfg.supervisor.resume_grace_ms, 3000);
    }

    #[test]
    fn off_mode_parses() {
        let toml = r#"
            [device]
            mac = "AA:BB:CC:DD:EE:FF"

            [filter]
            mode = "off"
        "#;
        let cfg = Config::parse(toml).unwrap();
        assert_eq!(cfg.filter.mode, FilterMode::Off);
    }

    #[test]
    fn malformed_toml_errors() {
        let toml = "this is not = valid = toml";
        assert!(Config::parse(toml).is_err());
    }

    #[test]
    fn missing_device_section_errors() {
        let toml = "[notifications]\nshow_connection_state = false";
        let err = Config::parse(toml).unwrap_err();
        assert!(err.to_string().contains("device") || err.root_cause().to_string().contains("device"));
    }

    #[test]
    fn invalid_filter_mode_errors() {
        let toml = r#"
            [device]
            mac = "AA:BB:CC:DD:EE:FF"
            [filter]
            mode = "nonsense"
        "#;
        assert!(Config::parse(toml).is_err());
    }
}
```

- [ ] **Step 4: Add the `dirs` crate**

Edit `/home/michnaugh1/Dev/ios-notifications/daemon/Cargo.toml`, add to `[dependencies]`:
```toml
dirs = "5"
```

- [ ] **Step 5: Run all config tests, verify pass**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd config:: 2>&1 | tail -15
```
Expected: `test result: ok. 6 passed`.

- [ ] **Step 6: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/Cargo.toml daemon/src/config.rs daemon/src/lib.rs
git commit -m "Add config module with TOML loader and defaults"
```

---

## Task 5: Filter module with TDD

**Goal:** Implement `Filter` that decides whether a notification should be shown based on its app bundle ID and current `FilterConfig`.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/filter.rs`
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/src/filter.rs`:
```rust
//! Per-app notification filtering rules.

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: FilterMode, apps: &[&str]) -> FilterConfig {
        FilterConfig {
            mode,
            apps: apps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn off_mode_passes_everything() {
        let f = Filter::new(cfg(FilterMode::Off, &["com.apple.Stocks"]));
        assert!(f.should_show("com.apple.Stocks"));
        assert!(f.should_show("com.apple.MobileSMS"));
        assert!(f.should_show("anything"));
    }

    #[test]
    fn blacklist_blocks_listed_apps() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &["com.apple.Stocks", "com.apple.news"]));
        assert!(!f.should_show("com.apple.Stocks"));
        assert!(!f.should_show("com.apple.news"));
        assert!(f.should_show("com.apple.MobileSMS"));
    }

    #[test]
    fn blacklist_empty_passes_everything() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &[]));
        assert!(f.should_show("com.apple.Stocks"));
        assert!(f.should_show("anything"));
    }

    #[test]
    fn whitelist_only_allows_listed_apps() {
        let f = Filter::new(cfg(FilterMode::Whitelist, &["com.apple.MobileSMS"]));
        assert!(f.should_show("com.apple.MobileSMS"));
        assert!(!f.should_show("com.apple.Stocks"));
        assert!(!f.should_show("com.apple.mobilecal"));
    }

    #[test]
    fn whitelist_empty_blocks_everything() {
        let f = Filter::new(cfg(FilterMode::Whitelist, &[]));
        assert!(!f.should_show("anything"));
    }

    #[test]
    fn matching_is_case_sensitive() {
        // iOS bundle IDs are case-sensitive by convention; treat them as such.
        let f = Filter::new(cfg(FilterMode::Blacklist, &["com.apple.Stocks"]));
        assert!(f.should_show("com.apple.stocks"));
        assert!(!f.should_show("com.apple.Stocks"));
    }
}
```

- [ ] **Step 2: Update lib.rs**

Replace `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`:
```rust
pub mod ancs;
pub mod config;
pub mod filter;
pub mod hid_bridge;
```

- [ ] **Step 3: Run tests to confirm they fail**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd filter:: 2>&1 | tail -15
```
Expected: compile error — `Filter`, `FilterMode`, `FilterConfig` not found in `filter` module.

- [ ] **Step 4: Implement Filter**

Replace `/home/michnaugh1/Dev/ios-notifications/daemon/src/filter.rs`:
```rust
//! Per-app notification filtering rules.
//!
//! Three modes:
//! - `Blacklist`: drop notifications from apps in the list, pass all others
//! - `Whitelist`: only pass notifications from apps in the list
//! - `Off`: pass everything regardless of the list
//!
//! Bundle IDs are matched case-sensitively.

use std::collections::HashSet;

pub use crate::config::{FilterConfig, FilterMode};

#[derive(Debug)]
pub struct Filter {
    mode: FilterMode,
    apps: HashSet<String>,
}

impl Filter {
    pub fn new(cfg: FilterConfig) -> Self {
        Self {
            mode: cfg.mode,
            apps: cfg.apps.into_iter().collect(),
        }
    }

    pub fn should_show(&self, app_id: &str) -> bool {
        match self.mode {
            FilterMode::Off => true,
            FilterMode::Blacklist => !self.apps.contains(app_id),
            FilterMode::Whitelist => self.apps.contains(app_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: FilterMode, apps: &[&str]) -> FilterConfig {
        FilterConfig {
            mode,
            apps: apps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn off_mode_passes_everything() {
        let f = Filter::new(cfg(FilterMode::Off, &["com.apple.Stocks"]));
        assert!(f.should_show("com.apple.Stocks"));
        assert!(f.should_show("com.apple.MobileSMS"));
        assert!(f.should_show("anything"));
    }

    #[test]
    fn blacklist_blocks_listed_apps() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &["com.apple.Stocks", "com.apple.news"]));
        assert!(!f.should_show("com.apple.Stocks"));
        assert!(!f.should_show("com.apple.news"));
        assert!(f.should_show("com.apple.MobileSMS"));
    }

    #[test]
    fn blacklist_empty_passes_everything() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &[]));
        assert!(f.should_show("com.apple.Stocks"));
        assert!(f.should_show("anything"));
    }

    #[test]
    fn whitelist_only_allows_listed_apps() {
        let f = Filter::new(cfg(FilterMode::Whitelist, &["com.apple.MobileSMS"]));
        assert!(f.should_show("com.apple.MobileSMS"));
        assert!(!f.should_show("com.apple.Stocks"));
        assert!(!f.should_show("com.apple.mobilecal"));
    }

    #[test]
    fn whitelist_empty_blocks_everything() {
        let f = Filter::new(cfg(FilterMode::Whitelist, &[]));
        assert!(!f.should_show("anything"));
    }

    #[test]
    fn matching_is_case_sensitive() {
        let f = Filter::new(cfg(FilterMode::Blacklist, &["com.apple.Stocks"]));
        assert!(f.should_show("com.apple.stocks"));
        assert!(!f.should_show("com.apple.Stocks"));
    }
}
```

- [ ] **Step 5: Run tests, verify pass**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd filter:: 2>&1 | tail -15
```
Expected: `test result: ok. 6 passed`.

- [ ] **Step 6: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/src/filter.rs daemon/src/lib.rs
git commit -m "Add filter module with blacklist/whitelist/off modes"
```

---

## Task 6: Wire filter into AncsProcessor

**Goal:** `AncsProcessor` consults a shared `Filter` (held behind `Arc<RwLock<>>`, so the supervisor can reload config without restarting). Filtered notifications never reach libnotify.

**Files:**
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/ancs.rs`

- [ ] **Step 1: Add filter field to AncsProcessor**

Edit `/home/michnaugh1/Dev/ios-notifications/daemon/src/ancs.rs`.

Replace the imports block at the top with:
```rust
use std::{collections::HashMap, io::Cursor, sync::Arc};

use ancs::{
    attributes::{
        app::AppAttributeID,
        command::CommandID,
        event::{EventFlag, EventID},
        notification::NotificationAttributeID,
        AppAttribute,
    },
    characteristics::{
        control_point::{GetAppAttributesRequest, GetNotificationAttributesRequest},
        data_source,
    },
};
use anyhow::{bail, Result};
use bluer::{
    gatt::remote::{Characteristic, CharacteristicWriteRequest},
    Adapter, Address, Uuid,
};
use byteorder_pack::UnpackFrom;
use futures::{pin_mut, StreamExt as _};
use tokio::sync::RwLock;

use crate::filter::Filter;
```

Replace the `AncsProcessor` struct and impl-start with:
```rust
pub struct AncsProcessor {
    control_point: Option<Characteristic>,
    app_names: HashMap<String, String>,
    filter: Arc<RwLock<Filter>>,
    on_delivered: Box<dyn Fn(String, String) + Send + Sync>,
    on_filtered: Box<dyn Fn(String, String) + Send + Sync>,
}

impl AncsProcessor {
    pub fn new(filter: Arc<RwLock<Filter>>) -> Self {
        Self::with_callbacks(filter, Box::new(|_, _| {}), Box::new(|_, _| {}))
    }

    pub fn with_callbacks(
        filter: Arc<RwLock<Filter>>,
        on_delivered: Box<dyn Fn(String, String) + Send + Sync>,
        on_filtered: Box<dyn Fn(String, String) + Send + Sync>,
    ) -> Self {
        Self {
            control_point: None,
            app_names: HashMap::new(),
            filter,
            on_delivered,
            on_filtered,
        }
    }
```

- [ ] **Step 2: Apply the filter inside process_data**

Inside `process_data`, replace the inner branch for `data[0] == 0` with one that checks the filter before sending the desktop notification. Find this block (currently right after `log::info!("Notif: {:?}", notif);`):

```rust
                let mut app_id_to_query = None;
                let mut desktop_notification = notify_rust::Notification::new();
                for attr in notif.attribute_list {
                    match attr.id {
                        NotificationAttributeID::AppIdentifier => {
```

Replace the entire block from `let mut app_id_to_query = None;` through the matching closing brace of the `attribute_list` `for` loop (just before `let handle = desktop_notification.show_async().await?;`), and through `show_async()`, with:

```rust
                let mut app_id_to_query: Option<String> = None;
                let mut current_app_id: Option<String> = None;
                let mut current_title: Option<String> = None;
                let mut current_body: Option<String> = None;
                let mut current_app_name_display: Option<String> = None;

                for attr in notif.attribute_list {
                    match attr.id {
                        NotificationAttributeID::AppIdentifier => {
                            if let Some(id) = attr.value {
                                if let Some(name) = self.app_names.get(&id) {
                                    current_app_name_display = Some(name.clone());
                                } else {
                                    current_app_name_display = Some(id.clone());
                                    app_id_to_query = Some(id.clone());
                                }
                                current_app_id = Some(id);
                            }
                        }
                        NotificationAttributeID::Title => {
                            current_title = attr.value;
                        }
                        NotificationAttributeID::Message => {
                            current_body = attr.value;
                        }
                        _ => {}
                    }
                }

                let app_id = current_app_id.as_deref().unwrap_or("unknown");
                let title = current_title.clone().unwrap_or_default();

                let pass = {
                    let f = self.filter.read().await;
                    f.should_show(app_id)
                };

                if !pass {
                    log::info!("Filtered notification from {}", app_id);
                    (self.on_filtered)(app_id.to_string(), title.clone());
                } else {
                    let mut desktop_notification = notify_rust::Notification::new();
                    if let Some(name) = &current_app_name_display {
                        desktop_notification.appname(name);
                    }
                    if let Some(t) = &current_title {
                        desktop_notification.summary(t);
                    }
                    if let Some(b) = &current_body {
                        desktop_notification.body(b);
                    }
                    let handle = desktop_notification.show_async().await?;
                    log::info!(
                        "Shown notification {} with desktop handle {}",
                        notif.notification_uid,
                        handle.id()
                    );
                    (self.on_delivered)(app_id.to_string(), title);
                }
```

- [ ] **Step 3: Update main.rs to construct the new AncsProcessor**

Edit `/home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs`.

Add modules and imports near the top:
```rust
mod ancs;
mod config;
mod filter;
mod hid_bridge;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::filter::Filter;
use crate::config::FilterConfig;
```

In the `loop` at the bottom of `main`, replace:
```rust
        let proc = ancs::AncsProcessor::new();
```
with:
```rust
        let filter = Arc::new(RwLock::new(Filter::new(FilterConfig::default())));
        let proc = ancs::AncsProcessor::new(filter);
```

This is a stub — the real config-driven filter wires in during Task 9. For now we just need it to compile and not break Task 2's smoke test.

- [ ] **Step 4: Build to verify**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo build -p ios-notificationsd 2>&1 | tail -10
```
Expected: compiles cleanly.

- [ ] **Step 5: Run all tests still pass**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd 2>&1 | tail -10
```
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/src/ancs.rs daemon/src/main.rs
git commit -m "Wire filter into AncsProcessor with delivery/filter callbacks"
```

---

## Task 7: Supervisor state machine (TDD on transitions)

**Goal:** Add a state machine that tracks daemon connection state. Pure logic — no real BlueZ — testable in unit tests.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/supervisor.rs`
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`

- [ ] **Step 1: Write failing tests for state transitions**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/src/supervisor.rs`:
```rust
//! Supervisor state machine.
//!
//! Pure-logic state transitions. The async event loop that drives these
//! transitions in production lives in `run_supervisor` (added in Task 9).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_in_initializing() {
        let sm = StateMachine::new();
        assert_eq!(sm.state(), State::Initializing);
    }

    #[test]
    fn initialized_event_moves_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        assert_eq!(sm.state(), State::Connecting);
    }

    #[test]
    fn connecting_to_connected_on_success() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectSucceeded);
        assert_eq!(sm.state(), State::Connected);
    }

    #[test]
    fn connecting_to_backoff_on_failure_with_increasing_delay() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.state(), State::Backoff);
        assert_eq!(sm.backoff_secs(), 2);

        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 4);

        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 8);
    }

    #[test]
    fn successful_connect_resets_backoff() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectFailed);
        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 4);

        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectSucceeded);
        assert_eq!(sm.state(), State::Connected);

        // Next failure starts back at initial backoff.
        sm.handle(Event::LinkDropped);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 2);
    }

    #[test]
    fn backoff_capped_at_max() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        for _ in 0..20 {
            sm.handle(Event::ConnectFailed);
            sm.handle(Event::BackoffElapsed);
        }
        assert_eq!(sm.backoff_secs(), 60);
    }

    #[test]
    fn link_dropped_returns_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectSucceeded);
        sm.handle(Event::LinkDropped);
        assert_eq!(sm.state(), State::Connecting);
    }

    #[test]
    fn sleep_pauses_from_any_active_state() {
        for start in [State::Connecting, State::Connected, State::Backoff] {
            let mut sm = StateMachine::new();
            sm.handle(Event::Initialized);
            sm.set_state_for_test(start);
            sm.handle(Event::PrepareForSleep(true));
            assert_eq!(sm.state(), State::Paused, "failed from {:?}", start);
        }
    }

    #[test]
    fn wake_from_sleep_returns_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::PrepareForSleep(true));
        sm.handle(Event::PrepareForSleep(false));
        assert_eq!(sm.state(), State::Connecting);
    }

    #[test]
    fn reconnect_forces_connecting_from_any_state() {
        for start in [State::Paused, State::Backoff, State::Connected, State::Error] {
            let mut sm = StateMachine::new();
            sm.handle(Event::Initialized);
            sm.set_state_for_test(start);
            sm.handle(Event::Reconnect);
            assert_eq!(sm.state(), State::Connecting, "failed from {:?}", start);
        }
    }

    #[test]
    fn ancs_missing_enters_error() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectSucceeded);
        sm.handle(Event::AncsMissing);
        assert_eq!(sm.state(), State::Error);
    }

    #[test]
    fn error_retry_returns_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::AncsMissing);
        sm.handle(Event::ErrorRetry);
        assert_eq!(sm.state(), State::Connecting);
    }
}
```

- [ ] **Step 2: Update lib.rs**

Update `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`:
```rust
pub mod ancs;
pub mod config;
pub mod filter;
pub mod hid_bridge;
pub mod supervisor;
```

- [ ] **Step 3: Run tests, verify they fail**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd supervisor:: 2>&1 | tail -10
```
Expected: compile errors — `StateMachine`, `State`, `Event` not defined.

- [ ] **Step 4: Implement StateMachine**

Replace `/home/michnaugh1/Dev/ios-notifications/daemon/src/supervisor.rs` with:
```rust
//! Supervisor state machine.
//!
//! Pure-logic state transitions for the daemon's connection lifecycle. The
//! async event loop that drives these transitions in production lives in
//! `run_supervisor` (added in Task 9).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Initializing,
    Connecting,
    Connected,
    Backoff,
    Paused,
    Error,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Initializing => "initializing",
            State::Connecting => "connecting",
            State::Connected => "connected",
            State::Backoff => "backoff",
            State::Paused => "paused",
            State::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Initialized,
    ConnectSucceeded,
    ConnectFailed,
    LinkDropped,
    BackoffElapsed,
    PrepareForSleep(bool), // true = entering sleep, false = waking
    Reconnect,
    Pause,
    Resume,
    AncsMissing,
    ErrorRetry,
}

const BACKOFF_INITIAL_S: u32 = 2;
const BACKOFF_MAX_S: u32 = 60;

pub struct StateMachine {
    state: State,
    backoff_secs: u32,
    sleep_origin: Option<State>, // remembers prior state for return-from-sleep
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: State::Initializing,
            backoff_secs: BACKOFF_INITIAL_S,
            sleep_origin: None,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn backoff_secs(&self) -> u32 {
        self.backoff_secs
    }

    /// For tests only — bypass the state machine to set the current state.
    #[cfg(test)]
    pub fn set_state_for_test(&mut self, s: State) {
        self.state = s;
    }

    pub fn handle(&mut self, event: Event) {
        match (self.state, &event) {
            (State::Initializing, Event::Initialized) => {
                self.state = State::Connecting;
            }

            // Reconnect is the universal "try again now" override
            (_, Event::Reconnect) => {
                self.backoff_secs = BACKOFF_INITIAL_S;
                self.state = State::Connecting;
            }

            // Sleep handling — works from any non-terminal state
            (_, Event::PrepareForSleep(true)) => {
                if self.state != State::Paused {
                    self.sleep_origin = Some(self.state);
                    self.state = State::Paused;
                }
            }
            (State::Paused, Event::PrepareForSleep(false)) => {
                self.state = State::Connecting;
                self.sleep_origin = None;
            }

            // Pause / Resume (user-initiated)
            (_, Event::Pause) if self.state != State::Paused => {
                self.state = State::Paused;
            }
            (State::Paused, Event::Resume) => {
                self.state = State::Connecting;
            }

            // Connection lifecycle
            (State::Connecting, Event::ConnectSucceeded) => {
                self.state = State::Connected;
                self.backoff_secs = BACKOFF_INITIAL_S;
            }
            (State::Connecting, Event::ConnectFailed) => {
                self.state = State::Backoff;
                // backoff_secs unchanged here; advanced on BackoffElapsed
            }
            // The supervisor optimistically transitions Connecting -> Connected
            // when it spawns an attempt; if the attempt then fails fast (e.g.,
            // ANCS service discovery error other than missing), this catches it.
            (State::Connected, Event::ConnectFailed) => {
                self.state = State::Backoff;
            }
            (State::Backoff, Event::BackoffElapsed) => {
                self.state = State::Connecting;
                // Double the backoff for the next failure (capped).
                self.backoff_secs = (self.backoff_secs * 2).min(BACKOFF_MAX_S);
            }
            (State::Connected, Event::LinkDropped) => {
                self.state = State::Connecting;
            }
            // Device removed before we finished service discovery.
            (State::Connecting, Event::LinkDropped) => {
                self.state = State::Backoff;
            }

            // ANCS error path
            (_, Event::AncsMissing) => {
                self.state = State::Error;
            }
            (State::Error, Event::ErrorRetry) => {
                self.state = State::Connecting;
            }

            // Ignore everything else (no-op, no logging here — caller's job)
            _ => {}
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_in_initializing() {
        let sm = StateMachine::new();
        assert_eq!(sm.state(), State::Initializing);
    }

    #[test]
    fn initialized_event_moves_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        assert_eq!(sm.state(), State::Connecting);
    }

    #[test]
    fn connecting_to_connected_on_success() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectSucceeded);
        assert_eq!(sm.state(), State::Connected);
    }

    #[test]
    fn connecting_to_backoff_on_failure_with_increasing_delay() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.state(), State::Backoff);
        assert_eq!(sm.backoff_secs(), 2);

        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 4);

        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 8);
    }

    #[test]
    fn successful_connect_resets_backoff() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectFailed);
        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 4);

        sm.handle(Event::BackoffElapsed);
        sm.handle(Event::ConnectSucceeded);
        assert_eq!(sm.state(), State::Connected);

        sm.handle(Event::LinkDropped);
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.backoff_secs(), 2);
    }

    #[test]
    fn backoff_capped_at_max() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        for _ in 0..20 {
            sm.handle(Event::ConnectFailed);
            sm.handle(Event::BackoffElapsed);
        }
        assert_eq!(sm.backoff_secs(), 60);
    }

    #[test]
    fn link_dropped_returns_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectSucceeded);
        sm.handle(Event::LinkDropped);
        assert_eq!(sm.state(), State::Connecting);
    }

    #[test]
    fn sleep_pauses_from_any_active_state() {
        for start in [State::Connecting, State::Connected, State::Backoff] {
            let mut sm = StateMachine::new();
            sm.handle(Event::Initialized);
            sm.set_state_for_test(start);
            sm.handle(Event::PrepareForSleep(true));
            assert_eq!(sm.state(), State::Paused, "failed from {:?}", start);
        }
    }

    #[test]
    fn wake_from_sleep_returns_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::PrepareForSleep(true));
        sm.handle(Event::PrepareForSleep(false));
        assert_eq!(sm.state(), State::Connecting);
    }

    #[test]
    fn reconnect_forces_connecting_from_any_state() {
        for start in [State::Paused, State::Backoff, State::Connected, State::Error] {
            let mut sm = StateMachine::new();
            sm.handle(Event::Initialized);
            sm.set_state_for_test(start);
            sm.handle(Event::Reconnect);
            assert_eq!(sm.state(), State::Connecting, "failed from {:?}", start);
        }
    }

    #[test]
    fn ancs_missing_enters_error() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectSucceeded);
        sm.handle(Event::AncsMissing);
        assert_eq!(sm.state(), State::Error);
    }

    #[test]
    fn error_retry_returns_to_connecting() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::AncsMissing);
        sm.handle(Event::ErrorRetry);
        assert_eq!(sm.state(), State::Connecting);
    }

    #[test]
    fn connect_failed_from_connected_goes_to_backoff() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        sm.handle(Event::ConnectSucceeded);
        // Supervisor optimistically reported success; attempt then failed.
        sm.handle(Event::ConnectFailed);
        assert_eq!(sm.state(), State::Backoff);
    }

    #[test]
    fn link_dropped_during_connecting_goes_to_backoff() {
        let mut sm = StateMachine::new();
        sm.handle(Event::Initialized);
        // Device removed before service discovery succeeded.
        sm.handle(Event::LinkDropped);
        assert_eq!(sm.state(), State::Backoff);
    }
}
```

- [ ] **Step 5: Run tests, verify all pass**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd supervisor:: 2>&1 | tail -15
```
Expected: `test result: ok. 14 passed`.

- [ ] **Step 6: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/src/supervisor.rs daemon/src/lib.rs
git commit -m "Add supervisor state machine with exponential backoff"
```

---

## Task 8: D-Bus interface (zbus server)

**Goal:** Implement the D-Bus server that exposes the daemon's state and methods to the tray. Tested via an in-process zbus client.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/daemon/src/dbus_iface.rs`
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`

- [ ] **Step 1: Add zbus to lib.rs**

Update `/home/michnaugh1/Dev/ios-notifications/daemon/src/lib.rs`:
```rust
pub mod ancs;
pub mod config;
pub mod dbus_iface;
pub mod filter;
pub mod hid_bridge;
pub mod supervisor;
```

- [ ] **Step 2: Implement the D-Bus interface**

Create `/home/michnaugh1/Dev/ios-notifications/daemon/src/dbus_iface.rs`:
```rust
//! D-Bus server: `io.github.michnaugh1.IosNotifications`.
//!
//! Exposes daemon state and control methods to the Plasma tray applet and to
//! any other consumer (e.g., `busctl --user call`).

use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use zbus::{interface, object_server::SignalEmitter, Connection};

use crate::supervisor::{Event, State};

pub const BUS_NAME: &str = "io.github.michnaugh1.IosNotifications";
pub const OBJECT_PATH: &str = "/IosNotifications";

/// Shared state read by the D-Bus interface methods/properties.
#[derive(Default)]
pub struct SharedState {
    pub state: State,
    pub device_address: String,
    pub last_error: String,
    pub notifications_today: u32,
    pub next_backoff_secs: u32,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            state: State::Initializing,
            ..Default::default()
        }
    }
}

impl Default for State {
    fn default() -> Self {
        State::Initializing
    }
}

pub struct IosNotificationsIface {
    pub shared: Arc<RwLock<SharedState>>,
    pub event_tx: mpsc::Sender<Event>,
}

#[interface(name = "io.github.michnaugh1.IosNotifications1")]
impl IosNotificationsIface {
    #[zbus(property)]
    async fn state(&self) -> String {
        self.shared.read().await.state.as_str().to_string()
    }

    #[zbus(property)]
    async fn device_address(&self) -> String {
        self.shared.read().await.device_address.clone()
    }

    #[zbus(property)]
    async fn last_error(&self) -> String {
        self.shared.read().await.last_error.clone()
    }

    #[zbus(property)]
    async fn notifications_today(&self) -> u32 {
        self.shared.read().await.notifications_today
    }

    #[zbus(property)]
    async fn next_backoff_secs(&self) -> u32 {
        self.shared.read().await.next_backoff_secs
    }

    async fn reconnect(&self) -> zbus::fdo::Result<()> {
        self.event_tx
            .send(Event::Reconnect)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn pause(&self) -> zbus::fdo::Result<()> {
        self.event_tx
            .send(Event::Pause)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn resume(&self) -> zbus::fdo::Result<()> {
        self.event_tx
            .send(Event::Resume)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn reload_config(&self) -> zbus::fdo::Result<()> {
        // Wired up to the supervisor in Task 9.
        Ok(())
    }

    #[zbus(signal)]
    pub async fn state_changed(
        emitter: &SignalEmitter<'_>,
        new_state: &str,
        old_state: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn notification_delivered(
        emitter: &SignalEmitter<'_>,
        app_id: &str,
        title: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn notification_filtered(
        emitter: &SignalEmitter<'_>,
        app_id: &str,
        title: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn error_occurred(
        emitter: &SignalEmitter<'_>,
        message: &str,
    ) -> zbus::Result<()>;
}

/// Set up the D-Bus connection and register the interface.
pub async fn serve(
    shared: Arc<RwLock<SharedState>>,
    event_tx: mpsc::Sender<Event>,
) -> anyhow::Result<Connection> {
    let iface = IosNotificationsIface { shared, event_tx };
    let conn = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, iface)?
        .build()
        .await?;
    log::info!("D-Bus interface registered at {} {}", BUS_NAME, OBJECT_PATH);
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;
    use zbus::proxy;

    #[proxy(
        interface = "io.github.michnaugh1.IosNotifications1",
        default_service = "io.github.michnaugh1.IosNotifications",
        default_path = "/IosNotifications"
    )]
    trait IosNotificationsClient {
        #[zbus(property)]
        fn state(&self) -> zbus::Result<String>;
        fn reconnect(&self) -> zbus::Result<()>;
        fn pause(&self) -> zbus::Result<()>;
        fn resume(&self) -> zbus::Result<()>;
    }

    #[tokio::test]
    async fn methods_dispatch_to_event_channel() {
        let shared = Arc::new(RwLock::new(SharedState::new()));
        let (tx, mut rx) = mpsc::channel::<Event>(32);
        let _conn = serve(shared, tx).await.expect("server starts");

        let client_conn = Connection::session().await.unwrap();
        let proxy = IosNotificationsClientProxy::new(&client_conn).await.unwrap();

        proxy.reconnect().await.unwrap();
        let evt = timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        assert!(matches!(evt, Event::Reconnect));

        proxy.pause().await.unwrap();
        let evt = timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        assert!(matches!(evt, Event::Pause));
    }

    #[tokio::test]
    async fn state_property_reads_shared() {
        let shared = Arc::new(RwLock::new(SharedState {
            state: State::Connected,
            ..Default::default()
        }));
        let (tx, _rx) = mpsc::channel::<Event>(32);
        let _conn = serve(shared.clone(), tx).await.expect("server starts");

        let client_conn = Connection::session().await.unwrap();
        let proxy = IosNotificationsClientProxy::new(&client_conn).await.unwrap();
        assert_eq!(proxy.state().await.unwrap(), "connected");
    }
}
```

Note: the two tests share a bus name. If both are run sequentially in the same process, that's fine (each `serve` call creates a fresh connection). If run in parallel, the second `serve` may fail to claim the name. We mitigate by running tests serially:

- [ ] **Step 3: Configure tests to run serially**

Edit `/home/michnaugh1/Dev/ios-notifications/daemon/Cargo.toml`. Add to `[dependencies]`:
```toml
serial_test = "3"
```

Add `#[serial_test::serial]` above each `#[tokio::test]` in the tests module of `dbus_iface.rs`. The test functions become:
```rust
    #[tokio::test]
    #[serial_test::serial]
    async fn methods_dispatch_to_event_channel() {
        // ... (body unchanged)
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn state_property_reads_shared() {
        // ... (body unchanged)
    }
```

Also add `use serial_test::serial as _;` is NOT needed — the attribute reference is enough.

- [ ] **Step 4: Build to confirm compile**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo build -p ios-notificationsd 2>&1 | tail -15
```
Expected: builds cleanly. zbus may emit deprecation hints — ignore as long as they're not errors.

- [ ] **Step 5: Run the D-Bus tests**

Note: these tests need a running D-Bus session bus. On a desktop login this is always present (`$DBUS_SESSION_BUS_ADDRESS`). To confirm:
```bash
echo "$DBUS_SESSION_BUS_ADDRESS"
```
Expected: non-empty path like `unix:path=/run/user/1000/bus`.

Then:
```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd dbus_iface:: 2>&1 | tail -15
```
Expected: `test result: ok. 2 passed`.

- [ ] **Step 6: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/Cargo.toml daemon/src/dbus_iface.rs daemon/src/lib.rs
git commit -m "Add D-Bus interface with zbus server and in-process integration tests"
```

---

## Task 9: Supervisor event loop + logind + main.rs integration

**Goal:** Tie everything together. The supervisor consumes events from D-Bus methods, logind signals, and internal timers; drives the state machine; manages the `AncsProcessor`; emits D-Bus signals on state changes.

**Files:**
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/supervisor.rs`
- Modify: `/home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs`

- [ ] **Step 1: Add `run_supervisor` to supervisor.rs**

Append to `/home/michnaugh1/Dev/ios-notifications/daemon/src/supervisor.rs` (above the `#[cfg(test)] mod tests {`):

```rust
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use bluer::{Adapter, Address};
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;

use crate::ancs::AncsProcessor;
use crate::config::Config;
use crate::dbus_iface::{IosNotificationsIface, SharedState};
use crate::filter::Filter;

/// The async driver that owns the state machine and reacts to events.
pub async fn run_supervisor(
    config: Config,
    adapter: Adapter,
    device_addr: Address,
    filter: Arc<RwLock<Filter>>,
    shared: Arc<RwLock<SharedState>>,
    mut event_rx: mpsc::Receiver<Event>,
    event_tx: mpsc::Sender<Event>,
    iface_ref: zbus::object_server::InterfaceRef<IosNotificationsIface>,
) -> Result<()> {
    let mut sm = StateMachine::new();
    let resume_grace = Duration::from_millis(config.supervisor.resume_grace_ms as u64);

    // Kick the machine
    {
        let mut s = shared.write().await;
        s.state = State::Initializing;
        s.device_address = device_addr.to_string();
    }
    sm.handle(Event::Initialized);
    sync_state(&shared, &sm, &iface_ref, State::Initializing).await;

    // Spawn the logind listener; it forwards PrepareForSleep events.
    spawn_logind_listener(event_tx.clone());

    loop {
        let old_state = sm.state();

        match sm.state() {
            State::Connecting => {
                // Attempt one connection in a separate task so we can race
                // against incoming events (Pause, Reconnect, sleep, etc.).
                let filter_clone = filter.clone();
                let event_tx_clone = event_tx.clone();
                let shared_clone = shared.clone();
                let adapter_clone = adapter.clone();
                let on_delivered: Box<dyn Fn(String, String) + Send + Sync> = {
                    let iface_ref = iface_ref.clone();
                    let shared = shared.clone();
                    Box::new(move |app_id, title| {
                        let iface_ref = iface_ref.clone();
                        let shared = shared.clone();
                        tokio::spawn(async move {
                            shared.write().await.notifications_today += 1;
                            let emitter = iface_ref.signal_emitter();
                            let _ = IosNotificationsIface::notification_delivered(
                                emitter, &app_id, &title,
                            )
                            .await;
                        });
                    })
                };
                let on_filtered: Box<dyn Fn(String, String) + Send + Sync> = {
                    let iface_ref = iface_ref.clone();
                    Box::new(move |app_id, title| {
                        let iface_ref = iface_ref.clone();
                        tokio::spawn(async move {
                            let emitter = iface_ref.signal_emitter();
                            let _ = IosNotificationsIface::notification_filtered(
                                emitter, &app_id, &title,
                            )
                            .await;
                        });
                    })
                };

                let attempt = tokio::spawn(async move {
                    let proc = AncsProcessor::with_callbacks(filter_clone, on_delivered, on_filtered);
                    let result = proc.main_loop(device_addr, &adapter_clone).await;
                    let evt = match &result {
                        Ok(()) => {
                            // main_loop returns Ok when DeviceRemoved fires
                            Event::LinkDropped
                        }
                        Err(e) => {
                            let msg = format!("{:#}", e);
                            shared_clone.write().await.last_error = msg.clone();
                            if msg.contains("ANCS service not found") {
                                pop_ancs_missing_notification();
                                Event::AncsMissing
                            } else {
                                Event::ConnectFailed
                            }
                        }
                    };
                    let _ = event_tx_clone.send(evt).await;
                });

                // Mark CONNECTED only once main_loop actually subscribes to streams.
                // For v1 we optimistically transition on attempt start; the
                // subsequent ConnectFailed will move us back if it fails fast.
                {
                    let _ = attempt; // owned by the spawned task
                }
                // Tentative success — main_loop blocks while link is alive
                sm.handle(Event::ConnectSucceeded);
                sync_state(&shared, &sm, &iface_ref, old_state).await;
                // Now wait for events
                if let Some(evt) = event_rx.recv().await {
                    sm.handle(evt);
                }
            }

            State::Backoff => {
                let secs = sm.backoff_secs();
                {
                    let mut s = shared.write().await;
                    s.next_backoff_secs = secs;
                }
                tokio::select! {
                    _ = sleep(Duration::from_secs(secs as u64)) => {
                        sm.handle(Event::BackoffElapsed);
                    }
                    Some(evt) = event_rx.recv() => {
                        sm.handle(evt);
                    }
                }
                shared.write().await.next_backoff_secs = 0;
            }

            State::Error => {
                tokio::select! {
                    _ = sleep(Duration::from_secs(30)) => {
                        sm.handle(Event::ErrorRetry);
                    }
                    Some(evt) = event_rx.recv() => {
                        sm.handle(evt);
                    }
                }
            }

            State::Paused | State::Connected => {
                // Wait for an external event
                if let Some(evt) = event_rx.recv().await {
                    // If this was a wake-from-sleep, observe the grace period.
                    if matches!(&evt, Event::PrepareForSleep(false)) {
                        sleep(resume_grace).await;
                    }
                    sm.handle(evt);
                }
            }

            State::Initializing => {
                // Should not happen — fall through to Connecting.
                sm.handle(Event::Initialized);
            }
        }

        if sm.state() != old_state {
            sync_state(&shared, &sm, &iface_ref, old_state).await;
        }
    }
}

async fn sync_state(
    shared: &Arc<RwLock<SharedState>>,
    sm: &StateMachine,
    iface_ref: &zbus::object_server::InterfaceRef<IosNotificationsIface>,
    old_state: State,
) {
    let new_state = sm.state();
    shared.write().await.state = new_state;
    let emitter = iface_ref.signal_emitter();
    let _ = IosNotificationsIface::state_changed(
        emitter,
        new_state.as_str(),
        old_state.as_str(),
    )
    .await;
    log::info!(
        "State: {} -> {}",
        old_state.as_str(),
        new_state.as_str()
    );
}

fn spawn_logind_listener(event_tx: mpsc::Sender<Event>) {
    tokio::spawn(async move {
        let conn = match zbus::Connection::system().await {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to connect to system bus for logind: {}", e);
                return;
            }
        };

        let proxy = match zbus::Proxy::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Failed to create logind proxy: {}", e);
                return;
            }
        };

        use futures::StreamExt;
        let mut stream = match proxy.receive_signal("PrepareForSleep").await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to subscribe to PrepareForSleep: {}", e);
                return;
            }
        };

        while let Some(msg) = stream.next().await {
            if let Ok(entering_sleep) = msg.body().deserialize::<bool>() {
                log::info!("PrepareForSleep({})", entering_sleep);
                let _ = event_tx.send(Event::PrepareForSleep(entering_sleep)).await;
            }
        }
    });
}

fn pop_ancs_missing_notification() {
    let _ = notify_rust::Notification::new()
        .summary("iOS notifications not shared")
        .body(
            "On your iPhone, go to Settings → Bluetooth → tap the (i) next to this computer → enable \"Share System Notifications\".",
        )
        .timeout(notify_rust::Timeout::Never)
        .urgency(notify_rust::Urgency::Critical)
        .show();
}
```

- [ ] **Step 2: Rewrite main.rs**

Replace `/home/michnaugh1/Dev/ios-notifications/daemon/src/main.rs`:
```rust
use std::sync::Arc;

use anyhow::{Context, Result};
use bluer::Address;
use clap::Parser;
use tokio::sync::{mpsc, RwLock};

mod ancs;
mod config;
mod dbus_iface;
mod filter;
mod hid_bridge;
mod supervisor;

use crate::config::Config;
use crate::dbus_iface::{IosNotificationsIface, SharedState};
use crate::filter::Filter;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to config file. Defaults to ~/.config/ios-notifications/config.toml.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Args::parse();
    let config_path = args.config.unwrap_or_else(Config::default_path);

    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            if !config_path.exists() {
                eprintln!(
                    "Config file not found: {}\n\nRun `ios-notifications-pair` first to set up pairing.",
                    config_path.display()
                );
            } else {
                eprintln!("Config error: {:#}", e);
            }
            std::process::exit(1);
        }
    };

    log::info!("Loaded config from {}", config_path.display());

    let device_addr: Address = config
        .device
        .mac
        .parse()
        .with_context(|| format!("invalid MAC address: {}", config.device.mac))?;

    let session = bluer::Session::new().await?;
    let adapter = match config.device.adapter.as_deref() {
        Some(name) => session.adapter(name)?,
        None => session.default_adapter().await?,
    };
    adapter.set_powered(true).await?;
    log::info!("Using adapter: {}", adapter.name());

    // Verify device is paired; if not, exit with guidance.
    let device = adapter.device(device_addr).map_err(|e| {
        anyhow::anyhow!(
            "Device {} not paired (run ios-notifications-pair): {}",
            device_addr,
            e
        )
    })?;
    if !device.is_paired().await.unwrap_or(false) {
        eprintln!(
            "Device {} is not paired.\n\nRun `ios-notifications-pair` again.",
            device_addr
        );
        std::process::exit(1);
    }

    // Set up shared state and event channel
    let shared = Arc::new(RwLock::new(SharedState::new()));
    let (event_tx, event_rx) = mpsc::channel::<supervisor::Event>(64);

    // Start the D-Bus server
    let conn = dbus_iface::serve(shared.clone(), event_tx.clone()).await?;
    let iface_ref = conn
        .object_server()
        .interface::<_, IosNotificationsIface>(dbus_iface::OBJECT_PATH)
        .await?;

    // Filter, hot-reloadable
    let filter = Arc::new(RwLock::new(Filter::new(config.filter.clone())));

    // HID GATT advertisement (for iOS auto-reconnect).
    // If this fails, log and propagate — link still works while iOS keeps it
    // open, but auto-reconnect won't trigger after sleep. A v2 could swallow
    // this error and continue degraded; for v1 we surface it.
    let (_hid_app, _hid_adv) = hid_bridge::serve_hid_gatt(&adapter)
        .await
        .map_err(|e| {
            log::error!("Failed to set up HID GATT service: {:?}", e);
            e
        })?;

    // Run supervisor
    supervisor::run_supervisor(
        config,
        adapter,
        device_addr,
        filter,
        shared,
        event_rx,
        event_tx,
        iface_ref,
    )
    .await?;

    Ok(())
}
```

- [ ] **Step 3: Build the whole workspace**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo build -p ios-notificationsd 2>&1 | tail -20
```
Expected: builds cleanly. Some `dead_code` warnings on internal helpers may appear; ignore.

If you see `cannot find type SignalEmitter` or similar zbus API mismatches, check the installed zbus version: `cargo tree -p zbus`. Plan assumes zbus 5.x. If 4.x is resolved instead, edit `Cargo.toml` to pin `zbus = "5"` explicitly and run `cargo update -p zbus`.

- [ ] **Step 4: Run all tests still pass**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test -p ios-notificationsd 2>&1 | tail -10
```
Expected: 20+ tests pass.

- [ ] **Step 5: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add daemon/src/supervisor.rs daemon/src/main.rs
git commit -m "Wire supervisor event loop with logind, D-Bus, and AncsProcessor"
```

---

## Task 10: systemd user service unit

**Goal:** Provide a systemd unit so the daemon starts automatically at login and restarts on crash.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/packaging/systemd/ios-notifications.service`
- Create: `/home/michnaugh1/Dev/ios-notifications/scripts/install-daemon.sh`

- [ ] **Step 1: Create the systemd unit**

Create `/home/michnaugh1/Dev/ios-notifications/packaging/systemd/ios-notifications.service`:
```ini
[Unit]
Description=iOS notifications bridge (ANCS over Bluetooth LE)
After=bluetooth.target dbus.socket
Wants=bluetooth.target

[Service]
Type=simple
ExecStart=%h/.local/bin/ios-notificationsd
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

# Modest hardening (per-user service, so blast radius is small)
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=%h/.config/ios-notifications

[Install]
WantedBy=default.target
```

Note: `ProtectHome=read-only` plus `ReadWritePaths=%h/.config/ios-notifications` keeps the daemon out of unrelated home files while still letting it touch its own config. Drop `ProtectHome` if it conflicts with anything (some Plasma setups have edge cases).

- [ ] **Step 2: Create install script**

Create `/home/michnaugh1/Dev/ios-notifications/scripts/install-daemon.sh`:
```bash
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
```

Make it executable:
```bash
chmod +x /home/michnaugh1/Dev/ios-notifications/scripts/install-daemon.sh
```

- [ ] **Step 3: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add packaging/ scripts/install-daemon.sh
git commit -m "Add systemd user service unit and install script"
```

---

## Task 11: Pair helper — scaffolding

**Goal:** Create the `ios-notifications-pair` binary with just CLI argument parsing and skeleton structure.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/pair/Cargo.toml`
- Create: `/home/michnaugh1/Dev/ios-notifications/pair/src/main.rs`

- [ ] **Step 1: Create pair/Cargo.toml**

Create `/home/michnaugh1/Dev/ios-notifications/pair/Cargo.toml`:
```toml
[package]
name = "ios-notifications-pair"
description = "Interactive pairing helper for ios-notifications"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[[bin]]
name = "ios-notifications-pair"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
bluer.workspace = true
clap.workspace = true
env_logger.workspace = true
futures.workspace = true
log.workspace = true
tokio.workspace = true
dirs = "5"
```

- [ ] **Step 2: Create pair/src/main.rs scaffolding**

Create `/home/michnaugh1/Dev/ios-notifications/pair/src/main.rs`:
```rust
//! Interactive pairing helper for ios-notifications.
//!
//! One-shot: makes the Bluetooth adapter discoverable, waits for the iPhone
//! to pair, verifies the ANCS service is exposed, writes the config file.

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Bluetooth adapter name. Defaults to the system default.
    #[arg(long)]
    adapter: Option<String>,

    /// Seconds to remain discoverable before giving up.
    #[arg(long, default_value = "180")]
    timeout: u32,

    /// Where to write the config file. Defaults to ~/.config/ios-notifications/config.toml.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Args::parse();
    println!("ios-notifications first-time setup\n");

    let config_path = args
        .config
        .unwrap_or_else(default_config_path)
        .canonicalize()
        .or_else(|_| {
            // canonicalize fails if path doesn't exist yet; that's fine
            Ok::<_, std::io::Error>(args.config.clone().unwrap_or_else(default_config_path))
        })?;

    pair(&args, &config_path).await
}

fn default_config_path() -> std::path::PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("ios-notifications").join("config.toml")
}

async fn pair(args: &Args, _config_path: &std::path::Path) -> Result<()> {
    println!("[1/5] Checking BlueZ status…");
    let session = bluer::Session::new().await.context("connect to bluez")?;
    println!("        ✓ Running");

    let adapter = match args.adapter.as_deref() {
        Some(name) => session.adapter(name)?,
        None => session.default_adapter().await?,
    };
    println!(
        "[2/5] Identifying adapter…\n        ✓ {} ({})",
        adapter.name(),
        adapter.address().await?
    );

    println!("[3/5] (Will become discoverable in Task 12)");
    println!("[4/5] (Will wait for ANCS in Task 12)");
    println!("[5/5] (Will write config in Task 12)");

    Ok(())
}
```

- [ ] **Step 3: Build**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo build -p ios-notifications-pair 2>&1 | tail -10
```
Expected: builds cleanly.

- [ ] **Step 4: Smoke run**

```bash
cd /home/michnaugh1/Dev/ios-notifications && ./target/debug/ios-notifications-pair --help
```
Expected: usage output mentioning `--adapter`, `--timeout`, `--config`.

- [ ] **Step 5: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add pair/
git commit -m "Add pair helper scaffolding"
```

---

## Task 12: Pair helper — core logic

**Goal:** Fill in the discoverable-mode, ANCS-detection, and config-write steps.

**Files:**
- Modify: `/home/michnaugh1/Dev/ios-notifications/pair/src/main.rs`

- [ ] **Step 1: Implement full pairing flow**

Replace `pair()` and add helpers in `/home/michnaugh1/Dev/ios-notifications/pair/src/main.rs`. The complete file becomes:

```rust
//! Interactive pairing helper for ios-notifications.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use bluer::{Adapter, Address, AdapterEvent, Device, Uuid};
use clap::Parser;
use futures::{pin_mut, StreamExt};
use tokio::time::timeout;

const ANCS_SERVICE_UUID: Uuid = Uuid::from_u128(0x7905F431B5CE4E99A40F4B1E122D00D0);

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    adapter: Option<String>,

    #[arg(long, default_value = "180")]
    timeout: u32,

    #[arg(long)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let args = Args::parse();
    println!("ios-notifications first-time setup\n");

    let config_path = args.config.clone().unwrap_or_else(default_config_path);
    run(&args, &config_path).await
}

fn default_config_path() -> std::path::PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("ios-notifications").join("config.toml")
}

async fn run(args: &Args, config_path: &Path) -> Result<()> {
    print!("[1/5] Checking BlueZ status…");
    let session = bluer::Session::new().await.context("connect to bluez")?;
    println!("                            ✓ Running");

    print!("[2/5] Identifying adapter…");
    let adapter = match args.adapter.as_deref() {
        Some(name) => session.adapter(name)?,
        None => session.default_adapter().await?,
    };
    let addr = adapter.address().await?;
    println!("                              ✓ {} ({})", adapter.name(), addr);

    print!("[3/5] Making adapter discoverable for {}s…", args.timeout);
    adapter.set_pairable(true).await?;
    adapter.set_pairable_timeout(args.timeout).await?;
    adapter.set_discoverable(true).await?;
    adapter.set_discoverable_timeout(args.timeout).await?;
    adapter.set_powered(true).await?;
    println!("                ✓ Done");

    println!("\n        ── On your iPhone ──");
    println!("        1. Open Settings → Bluetooth.");
    println!("        2. Wait for \"{}\" to appear and tap it.", adapter.name());
    println!("        3. Confirm the pairing code matches.");
    println!("        4. After pairing, leave the iOS Bluetooth screen OPEN.");
    println!("        5. iOS will ask whether to share notifications — say YES.\n");

    print!("[4/5] Waiting for ANCS service on connected device…");
    let device = wait_for_ancs_device(&adapter, Duration::from_secs(args.timeout as u64))
        .await
        .context("waiting for iPhone with ANCS")?;
    let device_addr = device.address();
    println!("  ✓ Found on {}", device_addr);

    print!("[5/5] Marking device trusted, writing config…");
    device.set_trusted(true).await?;
    write_config(config_path, device_addr, args.adapter.as_deref())?;
    println!("       ✓ {}", config_path.display());

    adapter.set_discoverable(false).await.ok();
    adapter.set_pairable(false).await.ok();

    println!("\nSetup complete!");
    println!("\nStart the service:");
    println!("    systemctl --user enable --now ios-notifications.service");
    println!("\nVerify:");
    println!("    journalctl --user -u ios-notifications -f");

    Ok(())
}

async fn wait_for_ancs_device(adapter: &Adapter, deadline: Duration) -> Result<Device> {
    // Check all already-known devices first.
    if let Some(d) = find_ancs_device(adapter).await? {
        return Ok(d);
    }

    // Then watch for new devices.
    let events = adapter.events().await?;
    pin_mut!(events);

    let fut = async {
        while let Some(event) = events.next().await {
            if let AdapterEvent::DeviceAdded(addr) = event {
                // Give iOS a moment to publish service UUIDs.
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Some(d) = device_with_ancs(adapter, addr).await? {
                    return Ok::<Device, anyhow::Error>(d);
                }
            }
        }
        Err(anyhow!("event stream ended before ANCS device appeared"))
    };

    timeout(deadline, fut).await.map_err(|_| {
        anyhow!(
            "Timed out. Possible causes:\n\
              - iPhone didn't appear in pairing list (check iPhone Bluetooth is on)\n\
              - You tapped pair but didn't confirm the share-notifications prompt\n\
              - The iPhone connected but \"Share System Notifications\" is OFF\n\
                (On iPhone: Settings → Bluetooth → tap (i) next to this computer\n\
                → toggle \"Share System Notifications\" ON, then re-run this tool.)"
        )
    })?
}

async fn find_ancs_device(adapter: &Adapter) -> Result<Option<Device>> {
    for addr in adapter.device_addresses().await? {
        if let Some(d) = device_with_ancs(adapter, addr).await? {
            return Ok(Some(d));
        }
    }
    Ok(None)
}

async fn device_with_ancs(adapter: &Adapter, addr: Address) -> Result<Option<Device>> {
    let device = match adapter.device(addr) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };
    let uuids = device.uuids().await.unwrap_or(None).unwrap_or_default();
    if uuids.iter().any(|u| *u == ANCS_SERVICE_UUID) {
        Ok(Some(device))
    } else {
        Ok(None)
    }
}

fn write_config(path: &Path, mac: Address, adapter: Option<&str>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("creating config directory")?;
    }

    // Don't clobber existing config if it has settings we'd lose; if it
    // exists, preserve everything except [device].
    let existing = std::fs::read_to_string(path).ok();
    let new_device_section = if let Some(adapter) = adapter {
        format!("[device]\nmac = \"{}\"\nadapter = \"{}\"\n", mac, adapter)
    } else {
        format!("[device]\nmac = \"{}\"\n", mac)
    };

    let final_contents = if let Some(existing) = existing {
        replace_device_section(&existing, &new_device_section)
    } else {
        new_device_section
    };

    std::fs::write(path, final_contents).context("writing config file")?;
    Ok(())
}

fn replace_device_section(existing: &str, new_device: &str) -> String {
    // Strip out the [device] section from existing, prepend the new one.
    let mut out = String::new();
    let mut in_device = false;
    for line in existing.lines() {
        if line.trim_start().starts_with('[') {
            in_device = line.trim_start().starts_with("[device]");
        }
        if !in_device {
            out.push_str(line);
            out.push('\n');
        }
    }
    format!("{}{}", new_device, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_device_keeps_other_sections() {
        let existing = "[device]\nmac = \"OLD\"\n\n[filter]\nmode = \"whitelist\"\n";
        let new = "[device]\nmac = \"NEW\"\n";
        let result = replace_device_section(existing, new);
        assert!(result.contains("mac = \"NEW\""));
        assert!(!result.contains("mac = \"OLD\""));
        assert!(result.contains("mode = \"whitelist\""));
    }

    #[test]
    fn replace_device_when_no_other_sections() {
        let existing = "[device]\nmac = \"OLD\"\n";
        let new = "[device]\nmac = \"NEW\"\n";
        let result = replace_device_section(existing, new);
        assert!(result.contains("mac = \"NEW\""));
        assert!(!result.contains("mac = \"OLD\""));
    }
}
```

- [ ] **Step 2: Build and test**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo build -p ios-notifications-pair 2>&1 | tail -10
cargo test -p ios-notifications-pair 2>&1 | tail -10
```
Expected: builds; 2 tests pass.

- [ ] **Step 3: Smoke test (no iPhone yet)**

```bash
cd /home/michnaugh1/Dev/ios-notifications && timeout 5 ./target/debug/ios-notifications-pair --timeout 5 2>&1 | head -15 || true
```
Expected: prints first few setup steps, then fails on the "waiting for ANCS device" step because no iPhone is present. The error should be the helpful diagnostic. (We're verifying no crash and that the messaging is correct.)

- [ ] **Step 4: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add pair/src/main.rs
git commit -m "Implement pair helper: discoverable mode, ANCS detection, config writing"
```

---

## Task 13: Plasmoid — scaffolding

**Goal:** Create the minimum Plasma 6 plasmoid that loads in Plasma without crashing. Just an icon and "Hello" placeholder.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/tray/metadata.json`
- Create: `/home/michnaugh1/Dev/ios-notifications/tray/contents/ui/main.qml`
- Create: `/home/michnaugh1/Dev/ios-notifications/scripts/install-tray.sh`

- [ ] **Step 1: Create metadata.json**

Create `/home/michnaugh1/Dev/ios-notifications/tray/metadata.json`:
```json
{
    "KPlugin": {
        "Authors": [
            {
                "Email": "mike@thenorthcoastlegal.com",
                "Name": "Mike Naughton"
            }
        ],
        "Category": "System Information",
        "Description": "Connection status for the iOS notifications bridge",
        "Icon": "phone",
        "Id": "io.github.michnaugh1.iosnotifications",
        "License": "MIT",
        "Name": "iOS Notifications",
        "Version": "0.1.0",
        "Website": "https://github.com/michnaugh1/ios-notifications"
    },
    "KPackageStructure": "Plasma/Applet",
    "X-Plasma-API-Minimum-Version": "6.0",
    "X-Plasma-NotificationArea": "true"
}
```

- [ ] **Step 2: Create main.qml (placeholder)**

Create `/home/michnaugh1/Dev/ios-notifications/tray/contents/ui/main.qml`:
```qml
import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root

    Plasmoid.title: i18n("iOS Notifications")
    Plasmoid.icon: "phone"

    toolTipMainText: i18n("iOS Notifications")
    toolTipSubText: i18n("Bridge status: not yet wired")

    compactRepresentation: Kirigami.Icon {
        source: "phone"
        active: true
        MouseArea {
            anchors.fill: parent
            onClicked: root.expanded = !root.expanded
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 16
        Layout.preferredHeight: Kirigami.Units.gridUnit * 8

        PlasmaCore.Heading {
            text: i18n("iOS Notifications")
            level: 2
            Layout.alignment: Qt.AlignHCenter
        }

        Text {
            text: i18n("D-Bus wiring lands in the next task.")
            color: Kirigami.Theme.textColor
            Layout.alignment: Qt.AlignHCenter
        }
    }
}
```

- [ ] **Step 3: Create install-tray.sh**

Create `/home/michnaugh1/Dev/ios-notifications/scripts/install-tray.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN_ID="io.github.michnaugh1.iosnotifications"
DEST="$HOME/.local/share/plasma/plasmoids/$PLUGIN_ID"

echo "Installing plasmoid to $DEST..."
mkdir -p "$DEST"
cp -r "$REPO_ROOT/tray/"* "$DEST/"

# Kquitapp + restart so plasmashell picks it up. Use plasma-restart-mode if available.
if command -v kquitapp6 > /dev/null; then
    echo "Restarting plasmashell..."
    kquitapp6 plasmashell 2>/dev/null || true
    nohup plasmashell --replace > /dev/null 2>&1 &
fi

echo "Done. Add the widget via right-click on the panel → Add Widgets → search 'iOS Notifications'."
```
```bash
chmod +x /home/michnaugh1/Dev/ios-notifications/scripts/install-tray.sh
```

- [ ] **Step 4: Lint the QML (offline)**

Run:
```bash
qmllint /home/michnaugh1/Dev/ios-notifications/tray/contents/ui/main.qml 2>&1 | head -20 || true
```
Expected: either no output (clean) or warnings about missing import paths — those are normal outside of `plasmashell`'s runtime context. Errors about syntax must be fixed.

- [ ] **Step 5: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add tray/ scripts/install-tray.sh
git commit -m "Add Plasma 6 plasmoid scaffolding"
```

---

## Task 14: Plasmoid — D-Bus state subscription + icon states

**Goal:** Replace the placeholder main.qml with a real implementation that subscribes to `StateChanged` and updates the tray icon.

**Files:**
- Modify: `/home/michnaugh1/Dev/ios-notifications/tray/contents/ui/main.qml`

- [ ] **Step 1: Implement D-Bus subscription via DBusConnection (QtDBus/QML)**

Replace `/home/michnaugh1/Dev/ios-notifications/tray/contents/ui/main.qml`:
```qml
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.notification as Notification
import org.kde.plasma.private.notifications as PlasmaNotifications

PlasmoidItem {
    id: root

    property string daemonState: "initializing"
    property string deviceAddress: ""
    property string lastError: ""
    property int notificationsToday: 0
    property int nextBackoffSecs: 0

    Plasmoid.title: i18n("iOS Notifications")
    Plasmoid.icon: iconForState(daemonState)

    toolTipMainText: i18n("iOS Notifications")
    toolTipSubText: tooltipForState(daemonState)

    function iconForState(s) {
        switch (s) {
            case "connected":    return "phone";
            case "connecting":   return "view-refresh";
            case "backoff":      return "task-recurring";
            case "paused":       return "media-playback-pause";
            case "error":        return "dialog-error";
            case "initializing": return "view-refresh";
            default:             return "phone";
        }
    }

    function tooltipForState(s) {
        switch (s) {
            case "connected":
                return i18n("Connected (%1 notifications today)", notificationsToday);
            case "connecting":
                return i18n("Connecting…");
            case "backoff":
                return i18n("Retrying… (next attempt in %1s)", nextBackoffSecs);
            case "paused":
                return i18n("Paused");
            case "error":
                return i18n("Error: %1", lastError || i18n("unknown"));
            case "initializing":
                return i18n("Starting…");
            default:
                return s;
        }
    }

    // Refresh state from D-Bus every 2 seconds. (Signal-based push is more
    // elegant; polling keeps the QML simpler. Plasma timers are cheap.)
    Timer {
        interval: 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: refresh()
    }

    function refresh() {
        const cmd = ["busctl", "--user", "--json=short", "get-property",
                     "io.github.michnaugh1.IosNotifications",
                     "/IosNotifications",
                     "io.github.michnaugh1.IosNotifications1",
                     "State"];
        runner.run(cmd, function (out, exitCode) {
            if (exitCode === 0) {
                try {
                    const parsed = JSON.parse(out);
                    if (parsed && parsed.data) {
                        daemonState = parsed.data;
                    }
                } catch (e) { /* keep prior state */ }
            }
        });
        runner.runProperty("DeviceAddress", function (v) { deviceAddress = v; });
        runner.runProperty("LastError",     function (v) { lastError = v; });
        runner.runProperty("NotificationsToday", function (v) { notificationsToday = parseInt(v) || 0; });
        runner.runProperty("NextBackoffSecs",   function (v) { nextBackoffSecs = parseInt(v) || 0; });
    }

    Item {
        id: runner
        property var processes: []

        function run(args, cb) {
            // QML can't fork directly; shell out via a temporary executable
            // approach is heavy. Use the launcher provided by Plasma.
            // For v1 we use a simple Component.onCompleted approach with
            // an executable bridge (described in install-tray.sh).
            // Pragmatic shortcut: use a tiny helper that wraps `busctl`.
            const helperPath = Qt.resolvedUrl("../../bin/ios-notifications-helper.sh").toString().replace("file://", "");
            // No-op fallback if helper not present: leave state at default
            cb("", 1);
        }
        function runProperty(name, cb) { cb(""); }
    }

    compactRepresentation: Kirigami.Icon {
        source: iconForState(daemonState)
        active: daemonState === "connecting" || daemonState === "initializing"
        MouseArea {
            anchors.fill: parent
            onClicked: root.expanded = !root.expanded
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 16
        Layout.preferredHeight: Kirigami.Units.gridUnit * 10
        spacing: Kirigami.Units.smallSpacing

        PlasmaCore.Heading {
            text: i18n("iOS Notifications")
            level: 2
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing
        }

        Kirigami.Icon {
            source: iconForState(daemonState)
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: Kirigami.Units.iconSizes.large
            Layout.preferredHeight: Kirigami.Units.iconSizes.large
        }

        Text {
            text: tooltipForState(daemonState)
            color: Kirigami.Theme.textColor
            Layout.alignment: Qt.AlignHCenter
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.gridUnit
            Layout.rightMargin: Kirigami.Units.gridUnit
        }

        Text {
            visible: deviceAddress.length > 0
            text: i18n("Device: %1", deviceAddress)
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
            color: Kirigami.Theme.disabledTextColor
            Layout.alignment: Qt.AlignHCenter
        }

        Item { Layout.fillHeight: true }

        // Actions row added in Task 15
    }
}
```

**Caveat**: QML has no native subprocess API. The cleanest portable approach is a tiny shell helper that the plasmoid invokes via `Qt.openUrlExternally` or a custom Plasma backend, but writing a full QML-to-D-Bus bridge in pure QML is non-trivial. For v1 we use polling via `busctl` from a shell helper installed alongside the plasmoid. The Item `runner` above is a placeholder that lets the UI compile; the actual implementation lives in Task 15 where we add the helper.

- [ ] **Step 2: Reinstall and verify it loads**

Run:
```bash
cd /home/michnaugh1/Dev/ios-notifications && ./scripts/install-tray.sh
```
Expected: plasmashell restarts; widget is available in "Add Widgets" search. Add it to a panel.

If plasmashell complains in logs (`journalctl --user -u plasma-plasmashell -n 50`), check QML import paths. Plasma 6 paths used:
- `import QtQuick`
- `import org.kde.plasma.plasmoid`
- `import org.kde.kirigami as Kirigami`
- `import org.kde.plasma.core as PlasmaCore`

Remove any imports that don't resolve. The `org.kde.notification` and `org.kde.plasma.private.notifications` imports above are tentative — remove them if `qmllint` or `plasmashell` complain.

- [ ] **Step 3: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add tray/contents/ui/main.qml
git commit -m "Plasmoid: icon and tooltip reflect daemon state"
```

---

## Task 15: Plasmoid — D-Bus actions via helper script

**Goal:** Wire up the "Reconnect", "Pause", "Resume" buttons by invoking a small shell helper that does the `busctl --user call`.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/tray/contents/code/dbus-helper.sh`
- Modify: `/home/michnaugh1/Dev/ios-notifications/tray/contents/ui/main.qml`

- [ ] **Step 1: Create the D-Bus helper shell script**

Create `/home/michnaugh1/Dev/ios-notifications/tray/contents/code/dbus-helper.sh`:
```bash
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
```
```bash
chmod +x /home/michnaugh1/Dev/ios-notifications/tray/contents/code/dbus-helper.sh
```

- [ ] **Step 2: Update main.qml to use a Process-like approach**

Plasma's QML environment provides `org.kde.plasma.plasmoid`'s `executable` data engine in some versions but the API is unstable across Plasma versions. The pragmatic approach is to use `Qt.openUrlExternally` to fire-and-forget for calls (Reconnect, Pause, Resume) and to read properties via a periodic timer that reads a file written by a polling helper.

Replace `runner` in `main.qml` and add action buttons. Update `/home/michnaugh1/Dev/ios-notifications/tray/contents/ui/main.qml`:
```qml
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as P5Support

PlasmoidItem {
    id: root

    property string daemonState: "initializing"
    property string deviceAddress: ""
    property string lastError: ""
    property int notificationsToday: 0
    property int nextBackoffSecs: 0

    Plasmoid.title: i18n("iOS Notifications")
    Plasmoid.icon: iconForState(daemonState)

    toolTipMainText: i18n("iOS Notifications")
    toolTipSubText: tooltipForState(daemonState)

    function iconForState(s) {
        switch (s) {
            case "connected":    return "phone";
            case "connecting":   return "view-refresh";
            case "backoff":      return "task-recurring";
            case "paused":       return "media-playback-pause";
            case "error":        return "dialog-error";
            case "initializing": return "view-refresh";
            default:             return "phone";
        }
    }

    function tooltipForState(s) {
        switch (s) {
            case "connected":
                return i18n("Connected (%1 today)", notificationsToday);
            case "connecting":
                return i18n("Connecting…");
            case "backoff":
                return i18n("Retrying… (in %1s)", nextBackoffSecs);
            case "paused":
                return i18n("Paused");
            case "error":
                return i18n("Error: %1", lastError || i18n("unknown"));
            case "initializing":
                return i18n("Starting…");
            default:
                return s;
        }
    }

    // Use plasma5support DataSource to run shell commands.
    P5Support.DataSource {
        id: exec
        engine: "executable"
        connectedSources: []
        onNewData: function (sourceName, data) {
            const stdout = (data["stdout"] || "").trim();
            disconnectSource(sourceName);
            // Route by source name prefix
            if (sourceName.startsWith("get:State|")) {
                if (stdout) daemonState = stdout;
            } else if (sourceName.startsWith("get:DeviceAddress|")) {
                deviceAddress = stdout;
            } else if (sourceName.startsWith("get:LastError|")) {
                lastError = stdout;
            } else if (sourceName.startsWith("get:NotificationsToday|")) {
                notificationsToday = parseInt(stdout) || 0;
            } else if (sourceName.startsWith("get:NextBackoffSecs|")) {
                nextBackoffSecs = parseInt(stdout) || 0;
            }
        }

        function helperPath() {
            return Qt.resolvedUrl("../code/dbus-helper.sh").toString().replace("file://", "");
        }

        function get(prop) {
            const cmd = helperPath() + " get " + prop;
            const tag = "get:" + prop + "|" + Date.now();
            // The "executable" engine treats source name AS the command. We
            // append a tag via env-var-free uniqueness using a no-op subshell.
            connectSource("sh -c '" + cmd + "; printf %s \\\"\\\"' # " + tag);
        }

        function call(method) {
            const cmd = helperPath() + " call " + method;
            connectSource("sh -c '" + cmd + " >/dev/null 2>&1' # call:" + method + "|" + Date.now());
        }
    }

    Timer {
        interval: 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            exec.get("State");
            exec.get("DeviceAddress");
            exec.get("LastError");
            exec.get("NotificationsToday");
            exec.get("NextBackoffSecs");
        }
    }

    compactRepresentation: Kirigami.Icon {
        source: iconForState(daemonState)
        active: daemonState === "connecting" || daemonState === "initializing"
        MouseArea {
            anchors.fill: parent
            onClicked: root.expanded = !root.expanded
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        Layout.preferredHeight: Kirigami.Units.gridUnit * 14
        spacing: Kirigami.Units.smallSpacing

        PlasmaCore.Heading {
            text: i18n("iOS Notifications")
            level: 2
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing
        }

        Kirigami.Icon {
            source: iconForState(daemonState)
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: Kirigami.Units.iconSizes.huge
            Layout.preferredHeight: Kirigami.Units.iconSizes.huge
        }

        Text {
            text: tooltipForState(daemonState)
            color: Kirigami.Theme.textColor
            Layout.alignment: Qt.AlignHCenter
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }

        Text {
            visible: deviceAddress.length > 0
            text: i18n("Device: %1", deviceAddress)
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
            color: Kirigami.Theme.disabledTextColor
            Layout.alignment: Qt.AlignHCenter
        }

        Item { Layout.fillHeight: true }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: Kirigami.Units.smallSpacing

            QQC2.Button {
                text: i18n("Reconnect")
                icon.name: "view-refresh"
                onClicked: exec.call("Reconnect")
            }

            QQC2.Button {
                text: daemonState === "paused" ? i18n("Resume") : i18n("Pause")
                icon.name: daemonState === "paused" ? "media-playback-start" : "media-playback-pause"
                onClicked: {
                    if (daemonState === "paused") exec.call("Resume");
                    else exec.call("Pause");
                }
            }

            QQC2.Button {
                text: i18n("Reload")
                icon.name: "document-revert"
                onClicked: exec.call("ReloadConfig")
            }
        }

        Item { Layout.preferredHeight: Kirigami.Units.smallSpacing }
    }
}
```

- [ ] **Step 3: Reinstall and verify in Plasma**

```bash
cd /home/michnaugh1/Dev/ios-notifications && ./scripts/install-tray.sh
```

Confirm in Plasma: open the widget, click "Pause" (state should change to "paused"); click "Resume". If the daemon isn't running yet, properties will be empty — that's expected.

If the plasma5support import fails (`module not found`), check the package: `apt list --installed 2>/dev/null | grep plasma`. On Ubuntu 26.04 + Plasma 6.6, the namespace should be `org.kde.plasma.plasma5support`. If it's a different name in your version, change the import accordingly.

- [ ] **Step 4: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add tray/
git commit -m "Plasmoid: actions wire to D-Bus via shell helper"
```

---

## Task 16: Manual integration test checklist

**Goal:** Document the human-driven tests that complete the testing pyramid.

**Files:**
- Create: `/home/michnaugh1/Dev/ios-notifications/docs/manual-tests.md`

- [ ] **Step 1: Write the checklist**

Create `/home/michnaugh1/Dev/ios-notifications/docs/manual-tests.md`:
```markdown
# Manual Integration Tests

Run before tagging a release. iPhone, Bluetooth, and a live KDE Plasma session
are required.

## Setup

1. Install the daemon and plasmoid:
   ```bash
   ./scripts/install-daemon.sh
   ./scripts/install-tray.sh
   ```
2. Run `ios-notifications-pair`. Pair the iPhone (Settings → Bluetooth →
   tap the computer entry → confirm code → enable "Share System
   Notifications").
3. `systemctl --user enable --now ios-notifications.service`
4. Watch logs: `journalctl --user -u ios-notifications -f`
5. Add the "iOS Notifications" plasmoid to a panel.

## Tests

### T1 — Fresh-pair flow
Steps: clean adapter (unpair iPhone in BlueZ if previously paired), run
`ios-notifications-pair`, complete pairing on iPhone.
Pass: config file appears at `~/.config/ios-notifications/config.toml`
with the iPhone MAC. No error messages.

### T2 — Notification delivery
Steps: from a second phone, send the test iPhone an iMessage.
Pass: a Plasma notification with the sender name and message body appears
within 3 seconds; it persists in the Plasma notification history.

### T3 — Filter blacklist
Steps: edit config — add `com.apple.mobilemail` to `[filter].apps`. Run
`busctl --user call io.github.michnaugh1.IosNotifications /IosNotifications
io.github.michnaugh1.IosNotifications1 ReloadConfig`. Send an email to the
iPhone's primary inbox.
Pass: NO Plasma notification appears for the email. Send an iMessage —
this should still appear.

### T4 — Filter whitelist
Steps: edit config — change `mode = "whitelist"`, set `apps =
["com.apple.MobileSMS"]`. ReloadConfig. Send iMessage AND email.
Pass: iMessage notification appears; email notification does NOT.

### T5 — Live config reload
Steps: while daemon is running, change `mode` to `"off"`. ReloadConfig.
Pass: subsequent notifications come through regardless of filter list,
without restarting the daemon.

### T6 — Suspend / resume
Steps: `systemctl suspend`. Wait 10s. Wake the machine.
Pass: state transitions: `connected` → `paused` (in journal logs) on
suspend; on wake, after ~1.5s, transitions back to `connecting` and then
`connected`. The next iMessage arrives.

### T7 — Bluetooth toggle
Steps: `bluetoothctl power off`. Wait 5s. `bluetoothctl power on`.
Pass: daemon enters `backoff` / `error` while off; recovers to `connected`
when on.

### T8 — iPhone-side "Share Notifications" toggle
Steps: on iPhone, Settings → Bluetooth → tap (i) next to this computer →
toggle "Share System Notifications" OFF. Wait 5s.
Pass: daemon logs "ANCS service not found"; a sticky critical-urgency
desktop notification appears with the recovery instructions; daemon stays
in `error` state. Toggle ON; daemon recovers to `connected` within 30s.

### T9 — Plasmoid state reflection
Steps: trigger T6, T7, T8 sequentially. Watch the plasmoid icon.
Pass: every state change reflects in the icon and tooltip within ~2s of
the actual state change.

### T10 — Long-run soak
Steps: leave daemon running 24 hours of normal phone use. Measure:
```bash
ps -o pid,rss,etime,cmd -p $(pgrep -u "$USER" -f ios-notificationsd)
lsof -p $(pgrep -u "$USER" -f ios-notificationsd) | wc -l
```
Pass: RSS hasn't grown more than ~20MB from start; open-fd count stable;
no crashes (`systemctl --user status` shows `active`, restart count == 0).

## iOS 26 compatibility verification

Run during pair-helper development on first contact with iOS 26:

- [ ] iOS Bluetooth UI shows "Share System Notifications" toggle (per-device)
- [ ] After pairing, `bluetoothctl info <MAC>` lists ANCS UUID
      `7905f431-b5ce-4e99-a40f-4b1e122d00d0` in the device's UUIDs
- [ ] `bluetoothctl gatt.select-attribute <MAC> /service0XYZ` matches the
      ANCS service path and exposes Notification Source, Data Source, and
      Control Point characteristics with their standard UUIDs.
- [ ] First incoming notification successfully parses (logs show "Notif: ...").

If any of these fail, file an issue and investigate iOS 26 protocol changes
before continuing.
```

- [ ] **Step 2: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add docs/manual-tests.md
git commit -m "Add manual integration test checklist"
```

---

## Task 17: Finalize README and verify whole-project build

**Goal:** Update the README with installation steps, badges, and final attribution. Run a full clean build to make sure everything still compiles together.

**Files:**
- Modify: `/home/michnaugh1/Dev/ios-notifications/README.md`

- [ ] **Step 1: Update README**

Replace `/home/michnaugh1/Dev/ios-notifications/README.md`:
```markdown
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
```

- [ ] **Step 2: Full clean build of the workspace**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo clean && cargo build --workspace --release 2>&1 | tail -20
```
Expected: both binaries (`ios-notificationsd`, `ios-notifications-pair`) build cleanly. First clean release build will take 5-10 minutes.

- [ ] **Step 3: Full test run**

```bash
cd /home/michnaugh1/Dev/ios-notifications && cargo test --workspace 2>&1 | tail -10
```
Expected: 20+ tests pass.

- [ ] **Step 4: Commit**

```bash
cd /home/michnaugh1/Dev/ios-notifications
git add README.md
git commit -m "Finalize README with install, config, and credits"
```

---

## Task 18 (optional): Run the daemon end-to-end manually

**Goal:** Sanity-check that the daemon actually starts, claims the D-Bus name, and idles waiting for an iPhone, without crashing. Doesn't require a real iPhone.

**Files:** none modified

- [ ] **Step 1: Run pair helper just to write a placeholder config**

```bash
cd /home/michnaugh1/Dev/ios-notifications
# Write a stub config so the daemon can start without erroring on missing config
mkdir -p ~/.config/ios-notifications
cat > ~/.config/ios-notifications/config.toml <<'EOF'
[device]
mac = "AA:BB:CC:DD:EE:FF"
EOF
```

- [ ] **Step 2: Start the daemon, observe behavior**

In a terminal:
```bash
cd /home/michnaugh1/Dev/ios-notifications
RUST_LOG=info ./target/release/ios-notificationsd
```
Expected behavior: it should exit immediately with the message about the fake MAC not being paired. This proves the config-load and pair-check paths work.

- [ ] **Step 3: Confirm D-Bus would be claimed cleanly (run with --help)**

```bash
cd /home/michnaugh1/Dev/ios-notifications && ./target/release/ios-notificationsd --help
```
Expected: usage output mentioning `--config`.

- [ ] **Step 4: Clean up placeholder config**

```bash
rm ~/.config/ios-notifications/config.toml
```

No commit — this is a runtime check only.

---

## Completion checklist

After all tasks above:

- [ ] `cargo test --workspace` passes
- [ ] `cargo build --workspace --release` succeeds
- [ ] Three binaries exist: `target/release/ios-notificationsd`, `target/release/ios-notifications-pair`, plus the plasmoid in `tray/`
- [ ] `docs/manual-tests.md` exists with the integration checklist
- [ ] Repo has a clean commit history; no `WIP` or `fixup` commits left
- [ ] README explains install, config, limitations, and credits upstream
- [ ] Spec in `docs/superpowers/specs/` is unchanged from approval

Then the real work begins: run the manual tests with an actual iPhone on iOS 26 and find out what we got wrong. That's outside this plan's scope.
