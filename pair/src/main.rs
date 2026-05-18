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
        .clone()
        .unwrap_or_else(default_config_path);

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
