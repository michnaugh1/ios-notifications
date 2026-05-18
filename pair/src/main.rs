//! Interactive pairing helper for ios-notifications.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use bluer::{Adapter, Address, AdapterEvent, Device, Uuid};
use bluer::adv::{Advertisement, Type as AdvType};
use clap::Parser;
use futures::{pin_mut, StreamExt};
use tokio::time::timeout;

const ANCS_SERVICE_UUID: Uuid = Uuid::from_u128(0x7905F431B5CE4E99A40F4B1E122D00D0);

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    adapter: Option<String>,

    #[arg(long, default_value = "180")]
    timeout: u32,

    #[arg(long)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .parse_default_env()
        .init();

    let args = Args::parse();
    println!("ios-notifications first-time setup\n");

    let config_path = args.config.clone().unwrap_or_else(default_config_path);
    run(&args, &config_path).await
}

fn default_config_path() -> std::path::PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("ios-notifications").join("config.toml")
}

async fn run(args: &Args, config_path: &Path) -> Result<()> {
    print!("[1/5] Checking BlueZ status…");
    let session = bluer::Session::new().await.context("connect to bluez")?;
    println!("                            ✓ Running");

    print!("[2/5] Identifying adapter…");
    let adapter = match args.adapter.as_deref() {
        Some(name) => session.adapter(name)?,
        None => session.default_adapter().await?,
    };
    let addr = adapter.address().await?;
    println!("                              ✓ {} ({})", adapter.name(), addr);

    print!("[3/5] Making adapter discoverable for {}s…", args.timeout);
    adapter.set_powered(true).await?;
    adapter.set_pairable(true).await?;
    adapter.set_pairable_timeout(args.timeout).await?;
    adapter.set_discoverable(true).await?;
    adapter.set_discoverable_timeout(args.timeout).await?;
    // set_discoverable only covers BR/EDR inquiry; BLE requires an active
    // advertisement so iOS can find the adapter during scanning.
    let adv = Advertisement {
        advertisement_type: AdvType::Peripheral,
        local_name: Some(adapter.name().to_string()),
        discoverable: Some(true),
        ..Default::default()
    };
    let _adv_handle = adapter.advertise(adv).await
        .context("start BLE advertisement")?;
    println!("                ✓ Done");

    println!("\n        ── On your iPhone ──");
    println!("        1. Open Settings → Bluetooth.");
    println!("        2. Wait for \"{}\" to appear and tap it.", adapter.name());
    println!("        3. Confirm the pairing code matches.");
    println!("        4. After pairing, leave the iOS Bluetooth screen OPEN.");
    println!("        5. iOS will ask whether to share notifications — say YES.\n");

    print!("[4/5] Waiting for ANCS service on connected device…");
    let device = wait_for_ancs_device(&adapter, Duration::from_secs(args.timeout as u64))
        .await
        .context("waiting for iPhone with ANCS")?;
    let device_addr = device.address();
    println!("  ✓ Found on {}", device_addr);

    print!("[5/5] Marking device trusted, writing config…");
    device.set_trusted(true).await?;
    write_config(config_path, device_addr, args.adapter.as_deref())?;
    println!("       ✓ {}", config_path.display());

    adapter.set_discoverable(false).await.ok();
    adapter.set_pairable(false).await.ok();

    println!("\nSetup complete!");
    println!("\nStart the service:");
    println!("    systemctl --user enable --now ios-notifications.service");
    println!("\nVerify:");
    println!("    journalctl --user -u ios-notifications -f");

    Ok(())
}

async fn wait_for_ancs_device(adapter: &Adapter, deadline: Duration) -> Result<Device> {
    // Check all already-known devices first.
    if let Some(d) = find_ancs_device(adapter).await? {
        return Ok(d);
    }

    // Then watch for new devices.
    let events = adapter.events().await?;
    pin_mut!(events);

    let fut = async {
        while let Some(event) = events.next().await {
            if let AdapterEvent::DeviceAdded(addr) = event {
                // Give iOS a moment to publish service UUIDs.
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Some(d) = device_with_ancs(adapter, addr).await? {
                    return Ok::<Device, anyhow::Error>(d);
                }
            }
        }
        Err(anyhow!("event stream ended before ANCS device appeared"))
    };

    timeout(deadline, fut).await.map_err(|_| {
        anyhow!(
            "Timed out. Possible causes:\n\
              - iPhone didn't appear in pairing list (check iPhone Bluetooth is on)\n\
              - You tapped pair but didn't confirm the share-notifications prompt\n\
              - The iPhone connected but \"Share System Notifications\" is OFF\n\
                (On iPhone: Settings → Bluetooth → tap (i) next to this computer\n\
                → toggle \"Share System Notifications\" ON, then re-run this tool.)"
        )
    })?
}

async fn find_ancs_device(adapter: &Adapter) -> Result<Option<Device>> {
    for addr in adapter.device_addresses().await? {
        if let Some(d) = device_with_ancs(adapter, addr).await? {
            return Ok(Some(d));
        }
    }
    Ok(None)
}

async fn device_with_ancs(adapter: &Adapter, addr: Address) -> Result<Option<Device>> {
    let device = match adapter.device(addr) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };
    let uuids = device.uuids().await.unwrap_or(None).unwrap_or_default();
    if uuids.iter().any(|u| *u == ANCS_SERVICE_UUID) {
        Ok(Some(device))
    } else {
        Ok(None)
    }
}

fn write_config(path: &Path, mac: Address, adapter: Option<&str>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("creating config directory")?;
    }

    // Don't clobber existing config if it has settings we'd lose; if it
    // exists, preserve everything except [device].
    let existing = std::fs::read_to_string(path).ok();
    let new_device_section = if let Some(adapter) = adapter {
        format!("[device]\nmac = \"{}\"\nadapter = \"{}\"\n", mac, adapter)
    } else {
        format!("[device]\nmac = \"{}\"\n", mac)
    };

    let final_contents = if let Some(existing) = existing {
        replace_device_section(&existing, &new_device_section)
    } else {
        new_device_section
    };

    std::fs::write(path, final_contents).context("writing config file")?;
    Ok(())
}

fn replace_device_section(existing: &str, new_device: &str) -> String {
    // Strip out the [device] section from existing, prepend the new one.
    let mut out = String::new();
    let mut in_device = false;
    for line in existing.lines() {
        if line.trim_start().starts_with('[') {
            in_device = line.trim_start().starts_with("[device]");
        }
        if !in_device {
            out.push_str(line);
            out.push('\n');
        }
    }
    format!("{}{}", new_device, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_device_keeps_other_sections() {
        let existing = "[device]\nmac = \"OLD\"\n\n[filter]\nmode = \"whitelist\"\n";
        let new = "[device]\nmac = \"NEW\"\n";
        let result = replace_device_section(existing, new);
        assert!(result.contains("mac = \"NEW\""));
        assert!(!result.contains("mac = \"OLD\""));
        assert!(result.contains("mode = \"whitelist\""));
    }

    #[test]
    fn replace_device_when_no_other_sections() {
        let existing = "[device]\nmac = \"OLD\"\n";
        let new = "[device]\nmac = \"NEW\"\n";
        let result = replace_device_section(existing, new);
        assert!(result.contains("mac = \"NEW\""));
        assert!(!result.contains("mac = \"OLD\""));
    }
}
