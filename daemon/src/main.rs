use std::sync::Arc;

use anyhow::{Context, Result};
use bluer::Address;
use clap::Parser;
use futures::StreamExt as _;
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
    let (_hid_app, _hid_adv) = hid_bridge::serve_hid_gatt(&adapter)
        .await
        .map_err(|e| {
            log::error!("Failed to set up HID GATT service: {:?}", e);
            e
        })?;

    // Watch for the adapter being powered off (happens when the kernel tears
    // down the BT controller on deep suspend). If detected, exit so systemd
    // restarts us — on restart we re-register the HID GATT app and
    // advertisement with a live adapter instead of stale handles.
    let adapter_for_watch = adapter.clone();

    tokio::select! {
        res = supervisor::run_supervisor(
            config,
            adapter,
            device_addr,
            filter,
            shared,
            event_rx,
            event_tx,
            iface_ref,
        ) => res,

        () = watch_adapter_powered_off(adapter_for_watch) => {
            Err(anyhow::anyhow!(
                "Bluetooth adapter powered off; exiting so systemd can restart with fresh registrations"
            ))
        }
    }
}

/// Returns when the adapter transitions to powered-off. Used to detect a
/// kernel-level adapter reset after deep suspend so we can restart cleanly.
async fn watch_adapter_powered_off(adapter: bluer::Adapter) {
    match adapter.events().await {
        Ok(mut events) => {
            while let Some(evt) = events.next().await {
                if let bluer::AdapterEvent::PropertyChanged(
                    bluer::AdapterProperty::Powered(false),
                ) = evt
                {
                    log::warn!("Bluetooth adapter powered off; will exit for systemd restart");
                    return;
                }
            }
            // Event stream closed — adapter removed entirely.
            log::warn!("Bluetooth adapter event stream ended; will exit for systemd restart");
        }
        Err(e) => {
            log::warn!("Cannot watch adapter events ({}); adapter reset recovery disabled", e);
            std::future::pending::<()>().await;
        }
    }
}
