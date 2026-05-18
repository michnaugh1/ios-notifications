use anyhow::Result;
use bluer::Address;
use clap::Parser;

use ios_notificationsd::ancs;
use ios_notificationsd::config::FilterConfig;
use ios_notificationsd::filter::Filter;
use ios_notificationsd::hid_bridge;
use std::sync::Arc;
use tokio::sync::RwLock;

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
        let filter = Arc::new(RwLock::new(Filter::new(FilterConfig::default())));
        let proc = ancs::AncsProcessor::new(filter);
        if let Err(e) = proc.main_loop(args.device_addr, &adapter).await {
            log::error!("Error: {:?}", e);
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
