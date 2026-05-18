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
