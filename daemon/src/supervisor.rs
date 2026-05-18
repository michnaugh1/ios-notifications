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
    // NOTE: signal was renamed connection_state_changed (not state_changed) to avoid
    // collision with the auto-generated property setter.
    let _ = IosNotificationsIface::connection_state_changed(
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
}
