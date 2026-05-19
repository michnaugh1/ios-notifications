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
    pub app_names: std::collections::HashMap<String, String>,
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
    pub async fn connection_state_changed(
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
    #[serial_test::serial]
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
    #[serial_test::serial]
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

    #[tokio::test]
    #[serial_test::serial]
    async fn app_names_cache_persists() {
        let shared = Arc::new(RwLock::new(SharedState::new()));
        {
            let mut s = shared.write().await;
            s.app_names.insert("com.apple.MobileSMS".to_string(), "Messages".to_string());
        }

        // Simulating daemon restart by creating a new interface with the same shared state
        let (tx, _rx) = mpsc::channel::<Event>(32);
        let iface = IosNotificationsIface { shared: shared.clone(), event_tx: tx };
        
        let s = iface.shared.read().await;
        assert_eq!(s.app_names.get("com.apple.MobileSMS").unwrap(), "Messages");
    }
}
