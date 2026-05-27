# ANCS Session Hardening Design

**Date:** 2026-05-26
**Status:** Approved

## Problem

The daemon connects successfully but misses nearly all notifications. Root cause is a two-failure-mode chain:

1. **CCCD reset fails with "Not Connected"** — the reset is attempted too early, before BlueZ has fully set up the GATT write path. Without a successful reset, iOS treats the old ANCS session as still active. It accepts control point writes but never sends data source responses, so `GetNotificationAttributes` requests are silently dropped.

2. **Reconnection storm after ATT `0x0e`** — after ~30 seconds of unanswered requests, iOS terminates the session with ATT error `0x0e` ("Unlikely Error"). The supervisor's default 2s backoff hammers iOS with reconnects immediately, causing repeated `le-connection-abort-by-local` failures because iOS needs time to reset its ANCS state machine.

## Approach

Three targeted changes to `ancs.rs` and `supervisor.rs`. No structural rewrite.

---

## Section 1: CCCD Retry with Verification

**File:** `daemon/src/ancs.rs`

Replace the single-attempt CCCD reset with a retry loop (up to 3 attempts, 300ms delay between attempts) for each of the two characteristics (Notification Source and Data Source).

```
for attempt in 0..3 {
    match desc.write(&[0x00, 0x00]).await {
        Ok(()) => { log success; break; }
        Err(e) if attempt < 2 => { sleep 300ms; continue; }
        Err(e) => { log warning; break; }
    }
}
```

After the retry loop, **read the CCCD back** to verify the value is `0x0000`. Log the result either way.

If verification shows the CCCD is still `0x0001` after all retries (iOS treating old session as live), bail from `main_loop` with a distinct error: `"CCCD reset ineffective — iOS session may be stale"`. This prevents subscribing to a broken ANCS stream and lets the supervisor trigger a clean reconnect.

**Success criteria:** Logs show "CCCD reset ok" and readback confirms `0x0000` on a normal reconnect. On failure, logs show the specific "CCCD reset ineffective" message rather than proceeding silently.

---

## Section 2: ATT `0x0e` and `le-connection-abort-by-local` Backoff

**Files:** `daemon/src/ancs.rs`, `daemon/src/supervisor.rs`

**In `ancs.rs`:** When the GATT layer returns ATT error `0x0e`, bail with a tagged error message: `"ancs-session-terminated: ATT error 0x0e"`. Similarly, the existing `connect()` failure producing `le-connection-abort-by-local` should be preserved as-is — the supervisor matches on it.

**In `supervisor.rs`:** After receiving an error from `main_loop`, check the error message for:
- `"ancs-session-terminated"` — iOS explicitly terminated; apply minimum 12s initial backoff before next connect attempt.
- `"le-connection-abort-by-local"` — iOS not ready to accept LE connection; apply the same 12s minimum.

For all other errors, keep the existing 2s initial backoff.

Add distinct log lines so the two failure modes are visually distinguishable in `journalctl`:
- `"iOS terminated ANCS session (ATT 0x0e) — backing off 12s before reconnect"`
- `"LE connection aborted by local stack — backing off 12s before reconnect"`

The normal exponential backoff schedule (2s → 4s → 8s → … → 60s) resumes from the 12s floor on subsequent failures.

**Success criteria:** After an ATT `0x0e` termination, the next connect attempt is at least 12s later. `le-connection-abort-by-local` errors no longer appear in rapid succession.

---

## Section 3: Data Source Silence Detection

**File:** `daemon/src/ancs.rs`

Add a `pending_requests: HashMap<u32, Instant>` field to `AncsProcessor`.

- **On `process_notification`:** After writing the `GetNotificationAttributes` control point request, insert `(notification_uid, Instant::now())` into `pending_requests`.
- **On `process_data`:** When handling a response, remove the corresponding UID from `pending_requests`.
- **On heartbeat tick (every 15s):** Scan `pending_requests` for entries older than 10s. Log a warning for each stale entry. If 2 or more stale entries are found, bail from `main_loop` with `"data source unresponsive — iOS not responding to GetNotificationAttributes"`.

The 2-entry threshold avoids false positives from a single slow response. The 10s window gives iOS reasonable time to respond before we flag it as silent.

No supervisor changes needed — the existing backoff handles the reconnect from this bail.

**Success criteria:** When iOS accepts control point writes but doesn't respond, the daemon detects it within ~25s (next heartbeat after 10s staleness) and reconnects rather than waiting for ATT `0x0e` 30s later.

---

## Files Changed

| File | Change |
|------|--------|
| `daemon/src/ancs.rs` | CCCD retry loop + verification; pending_requests map; staleness detection on heartbeat; ATT 0x0e error tagging |
| `daemon/src/supervisor.rs` | Match on tagged error strings for 12s minimum backoff; improved log messages |

## What This Does Not Change

- The HID keyboard advertisement trick
- The D-Bus GATT fallback scan (`scan_services_from_dbus`)
- The audio profile disconnect logic
- Supervisor state machine structure
- D-Bus interface
