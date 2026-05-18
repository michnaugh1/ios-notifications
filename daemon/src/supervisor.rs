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
}
