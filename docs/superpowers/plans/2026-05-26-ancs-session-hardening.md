# ANCS Session Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix missed notifications caused by a failed CCCD reset that leaves iOS's ANCS session stale, and stop the reconnection storm that follows iOS-initiated session termination.

**Architecture:** Three targeted changes — CCCD retry/verification in `ancs.rs`, error tagging in `ancs.rs`'s control-point writer, and a new `ConnectFailedIosTerminated` event in `supervisor.rs` that applies a 12s minimum backoff instead of 2s. A fourth change adds a `pending_requests` map to `AncsProcessor` that detects when iOS silently ignores GetNotificationAttributes requests and bails early rather than waiting 30s for iOS to terminate with ATT 0x0e.

**Tech Stack:** Rust, tokio, bluer (BlueZ GATT), anyhow, std::collections::HashMap, std::time::Instant

---

## File Map

| File | Changes |
|------|---------|
| `daemon/src/supervisor.rs` | Add `ConnectFailedIosTerminated` event; add state machine transition; add unit tests; detect tagged error strings in run loop |
| `daemon/src/ancs.rs` | CCCD retry loop; CCCD readback verification; ATT 0x0e error tagging in `write_control_point`; `pending_requests` field; insert on control point write; remove on data response; stale detection on heartbeat |

---

### Task 1: Add `ConnectFailedIosTerminated` event to the state machine

**Files:**
- Modify: `daemon/src/supervisor.rs`

The state machine currently has no way to express "back off longer than usual." This task adds a `ConnectFailedIosTerminated` event that sets `backoff_secs` to 12 before entering `Backoff`, and unit-tests the new transition.

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `#[cfg(test)] mod tests` block at the bottom of `supervisor.rs` (after `error_retry_returns_to_connecting`):

```rust
#[test]
fn ios_terminated_backoff_is_at_least_12s() {
    let mut sm = StateMachine::new();
    sm.handle(Event::Initialized);
    sm.handle(Event::ConnectSucceeded);
    sm.handle(Event::ConnectFailedIosTerminated);
    assert_eq!(sm.state(), State::Backoff);
    assert_eq!(sm.backoff_secs(), 12);
}

#[test]
fn ios_terminated_backoff_from_connecting() {
    let mut sm = StateMachine::new();
    sm.handle(Event::Initialized);
    sm.handle(Event::ConnectFailedIosTerminated);
    assert_eq!(sm.state(), State::Backoff);
    assert_eq!(sm.backoff_secs(), 12);
}

#[test]
fn ios_terminated_backoff_doubles_on_next_failure() {
    let mut sm = StateMachine::new();
    sm.handle(Event::Initialized);
    sm.handle(Event::ConnectFailedIosTerminated);
    assert_eq!(sm.backoff_secs(), 12);
    sm.handle(Event::BackoffElapsed); // → Connecting, doubles backoff to 24
    sm.handle(Event::ConnectFailed);  // regular failure
    assert_eq!(sm.backoff_secs(), 24);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ios-notificationsd supervisor 2>&1 | grep -E "FAILED|error\[|^error"
```

Expected: compilation error — `ConnectFailedIosTerminated` does not exist yet.

- [ ] **Step 3: Add the event variant and constant**

In `supervisor.rs`, add the constant after `BACKOFF_MAX_S` (line 49):

```rust
const BACKOFF_IOS_TERMINATED_S: u32 = 12;
```

Add the variant to the `Event` enum (after `ErrorRetry`, line 45):

```rust
ConnectFailedIosTerminated,
```

- [ ] **Step 4: Add state machine transitions**

In `StateMachine::handle`, add these two arms **before** the `(State::Connecting, Event::ConnectFailed)` arm (around line 117):

```rust
(State::Connecting, Event::ConnectFailedIosTerminated) => {
    self.backoff_secs = BACKOFF_IOS_TERMINATED_S;
    self.state = State::Backoff;
}
(State::Connected, Event::ConnectFailedIosTerminated) => {
    self.backoff_secs = BACKOFF_IOS_TERMINATED_S;
    self.state = State::Backoff;
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p ios-notificationsd supervisor 2>&1 | tail -20
```

Expected: all tests pass including the three new ones.

- [ ] **Step 6: Commit**

```bash
git add daemon/src/supervisor.rs
git commit -m "Add ConnectFailedIosTerminated event with 12s minimum backoff"
```

---

### Task 2: Detect tagged errors in the supervisor run loop

**Files:**
- Modify: `daemon/src/supervisor.rs`

Wire up `ConnectFailedIosTerminated` in `run_supervisor`. The task closure that calls `proc.main_loop()` currently sends `AncsMissing` for "ANCS service not found" and `ConnectFailed` for everything else. Add two new cases.

- [ ] **Step 1: Replace the error-matching block in the task closure**

Find this block in `run_supervisor` (around lines 270–279):

```rust
if msg.contains("ANCS service not found") {
    pop_ancs_missing_notification();
    Event::AncsMissing
} else {
    Event::ConnectFailed
}
```

Replace it with:

```rust
if msg.contains("ANCS service not found") {
    pop_ancs_missing_notification();
    Event::AncsMissing
} else if msg.contains("ancs-session-terminated") {
    log::warn!("iOS terminated ANCS session — backing off {}s before reconnect", BACKOFF_IOS_TERMINATED_S);
    Event::ConnectFailedIosTerminated
} else if msg.contains("le-connection-abort-by-local") {
    log::warn!("LE connection aborted by local stack — backing off {}s before reconnect", BACKOFF_IOS_TERMINATED_S);
    Event::ConnectFailedIosTerminated
} else {
    Event::ConnectFailed
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
cargo build -p ios-notificationsd 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3: Run tests**

```bash
cargo test -p ios-notificationsd supervisor 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add daemon/src/supervisor.rs
git commit -m "Wire up ConnectFailedIosTerminated for ATT 0x0e and LE abort errors"
```

---

### Task 3: Tag ATT `0x0e` errors in `write_control_point`

**Files:**
- Modify: `daemon/src/ancs.rs`

The ATT "Unlikely Error" (0x0e) comes from bluer when iOS terminates the GATT session. It surfaces as a string like `"Operation failed with ATT error: 0x0e"` in the anyhow chain. Tag it so the supervisor can distinguish it from generic failures.

- [ ] **Step 1: Replace `write_control_point`**

Find the current `write_control_point` method (around line 395):

```rust
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
```

Replace it with:

```rust
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
            .await
            .map_err(|e| {
                let msg = format!("{:#}", e);
                if msg.contains("ATT error: 0x0e") || msg.contains("ATT error: 14") {
                    anyhow::anyhow!("ancs-session-terminated: {}", msg)
                } else {
                    anyhow::anyhow!("{}", msg)
                }
            })?;
    }
    Ok(())
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
cargo build -p ios-notificationsd 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all 27+ tests pass.

- [ ] **Step 4: Commit**

```bash
git add daemon/src/ancs.rs
git commit -m "Tag ATT 0x0e errors as ancs-session-terminated for supervisor backoff"
```

---

### Task 4: CCCD retry with backoff

**Files:**
- Modify: `daemon/src/ancs.rs`

Replace the single-attempt CCCD reset with a 3-attempt retry loop, 300ms between attempts.

- [ ] **Step 1: Replace the CCCD reset block**

Find the current block in `main_loop` (around lines 168–179):

```rust
let cccd_uuid: Uuid = "00002902-0000-1000-8000-00805f9b34fb".parse()?;
for char_ref in [&data_source, &notification_source] {
    for desc in char_ref.descriptors().await.unwrap_or_default() {
        if desc.uuid().await.unwrap_or_default() == cccd_uuid {
            match desc.write(&[0x00, 0x00]).await {
                Ok(()) => log::info!("CCCD reset to 0x0000 ok"),
                Err(e) => log::warn!("CCCD reset failed (BlueZ may suppress it): {}", e),
            }
            break;
        }
    }
}
```

Replace it with:

```rust
let cccd_uuid: Uuid = "00002902-0000-1000-8000-00805f9b34fb".parse()?;
for char_ref in [&data_source, &notification_source] {
    for desc in char_ref.descriptors().await.unwrap_or_default() {
        if desc.uuid().await.unwrap_or_default() == cccd_uuid {
            for attempt in 0u8..3 {
                match desc.write(&[0x00, 0x00]).await {
                    Ok(()) => {
                        log::info!("CCCD reset to 0x0000 ok (attempt {})", attempt + 1);
                        break;
                    }
                    Err(e) if attempt < 2 => {
                        log::debug!(
                            "CCCD reset attempt {} failed: {}; retrying in 300ms",
                            attempt + 1,
                            e
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    }
                    Err(e) => {
                        log::warn!("CCCD reset failed after 3 attempts: {}", e);
                    }
                }
            }
            // Always verify: read back the CCCD regardless of whether reset succeeded.
            match desc.read().await {
                Ok(val) if val.first() == Some(&0x01) => {
                    log::warn!(
                        "CCCD readback shows {:02x?} after reset — iOS session may be stale; bailing",
                        val
                    );
                    bail!("CCCD reset ineffective — iOS session may be stale");
                }
                Ok(val) => {
                    log::info!("CCCD readback: {:02x?} (ok)", val);
                }
                Err(e) => {
                    log::warn!("CCCD readback failed (continuing): {}", e);
                }
            }
            break;
        }
    }
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
cargo build -p ios-notificationsd 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add daemon/src/ancs.rs
git commit -m "Retry CCCD reset up to 3 times and verify with readback"
```

---

### Task 5: Add `pending_requests` field to `AncsProcessor`

**Files:**
- Modify: `daemon/src/ancs.rs`

Add the `pending_requests` map to track in-flight `GetNotificationAttributes` calls. Insert on control-point write; remove on data-source response.

- [ ] **Step 1: Add the field to the struct**

Find the `AncsProcessor` struct definition (around line 36). Add the field after `recent_uids`:

```rust
pub struct AncsProcessor {
    control_point: Option<Characteristic>,
    shared: Arc<RwLock<crate::dbus_iface::SharedState>>,
    filter: Arc<RwLock<Filter>>,
    on_connected: Box<dyn Fn() + Send + Sync>,
    on_delivered: Box<dyn Fn(String, String) + Send + Sync>,
    on_filtered: Box<dyn Fn(String, String) + Send + Sync>,
    recent_uids: HashMap<u32, Instant>,
    pending_requests: HashMap<u32, Instant>,
}
```

- [ ] **Step 2: Initialize the field in `with_callbacks`**

Find `with_callbacks` (around line 63). Add `pending_requests: HashMap::new()` alongside the existing `recent_uids: HashMap::new()`:

```rust
Self {
    control_point: None,
    shared,
    filter,
    on_connected,
    on_delivered,
    on_filtered,
    recent_uids: HashMap::new(),
    pending_requests: HashMap::new(),
}
```

- [ ] **Step 3: Insert into `pending_requests` in `process_notification`**

Find `process_notification` (around line 261). After `self.write_control_point(&Vec::from(cmd)).await?;`, add:

```rust
self.pending_requests.insert(notification_uid, Instant::now());
```

The full tail of `process_notification` should now read:

```rust
self.write_control_point(&Vec::from(cmd)).await?;
self.pending_requests.insert(notification_uid, Instant::now());
Ok(())
```

- [ ] **Step 4: Remove from `pending_requests` in `process_data`**

Find `process_data` (around line 275). At the top of the `0 =>` arm, after parsing `notif`, add:

```rust
self.pending_requests.remove(&notif.notification_uid);
```

The top of the `0 =>` arm should now read:

```rust
0 => {
    let notif = match data_source::GetNotificationAttributesResponse::parse(&data) {
        Ok((_, app)) => app,
        Err(e) => bail!("Error parsing notification attributes: {:?}", e),
    };
    self.pending_requests.remove(&notif.notification_uid);
    log::info!("Notif: {:?}", notif);
    // ... rest unchanged
```

- [ ] **Step 5: Build to verify it compiles**

```bash
cargo build -p ios-notificationsd 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 6: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add daemon/src/ancs.rs
git commit -m "Track pending GetNotificationAttributes requests for silence detection"
```

---

### Task 6: Detect stale requests on heartbeat and bail early

**Files:**
- Modify: `daemon/src/ancs.rs`

On each heartbeat tick, scan `pending_requests` for entries older than 10s. Log a warning per stale entry. If 2 or more are stale, bail with "data source unresponsive" to trigger a clean reconnect.

- [ ] **Step 1: Extend the heartbeat arm in `main_loop`**

Find the heartbeat arm inside the `loop` / `tokio::select!` (around line 223):

```rust
_ = heartbeat.tick() => {
    if !device.is_connected().await.unwrap_or(true) {
        bail!("device disconnected (heartbeat); will reconnect");
    }
}
```

Replace it with:

```rust
_ = heartbeat.tick() => {
    if !device.is_connected().await.unwrap_or(true) {
        bail!("device disconnected (heartbeat); will reconnect");
    }
    let now = Instant::now();
    let stale: Vec<u32> = self.pending_requests
        .iter()
        .filter_map(|(&uid, &sent_at)| {
            if now.duration_since(sent_at).as_secs() >= 10 {
                Some(uid)
            } else {
                None
            }
        })
        .collect();
    for uid in &stale {
        log::warn!(
            "GetNotificationAttributes for uid={} has no response after 10s — data source may be stale",
            uid
        );
    }
    if stale.len() >= 2 {
        bail!("data source unresponsive — iOS not responding to GetNotificationAttributes");
    }
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
cargo build -p ios-notificationsd 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add daemon/src/ancs.rs
git commit -m "Bail on data source silence after 2+ stale requests at heartbeat"
```

---

### Task 7: Install and smoke-test

**Files:** none (runtime verification only)

- [ ] **Step 1: Build and install**

```bash
./scripts/install-daemon.sh
```

Expected: daemon builds in release mode, binary copied to `~/.local/bin/`, systemd unit reloaded.

- [ ] **Step 2: Restart the daemon**

```bash
systemctl --user restart ios-notifications
```

- [ ] **Step 3: Watch logs**

```bash
journalctl --user -u ios-notifications -f
```

Expected within 30s:
- `"CCCD reset to 0x0000 ok (attempt N)"` — retry is working
- `"CCCD readback: [00, 00] (ok)"` — verification passed
- `State: connecting -> connected`
- No immediate `"CCCD reset failed"` warning

- [ ] **Step 4: Trigger a notification on the phone and verify delivery**

Send yourself a message or trigger any iOS notification.

Expected in logs:
- `"ANCS event: id=0 flags=0x10 uid=N"` (not pre-existing)
- `"Notif: ..."` (data source responded)
- `"Shown notification N with desktop handle ..."`

Expected on desktop: a notification popup appears.

- [ ] **Step 5: If ATT 0x0e or silence is detected, verify improved behavior**

If a failure occurs, check logs for:
- `"iOS terminated ANCS session — backing off 12s before reconnect"` (not "backing off 2s")
- Or `"data source unresponsive — iOS not responding to GetNotificationAttributes"` followed by a clean reconnect within ~25s

Expected: no rapid-fire `le-connection-abort-by-local` storm; next connect attempt waits at least 12s.
