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
