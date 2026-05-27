//! ANCS (Apple Notification Center Service) protocol processor.
//!
//! Inherited from kmod-midori/ancs-linux @ 6883f2b with minor adaptations
//! (will be extended for filtering in Task 6).
 
use roxmltree;

use std::{collections::HashMap, io::Cursor, sync::Arc, time::Instant};

use ancs::{
    attributes::{
        app::AppAttributeID,
        command::CommandID,
        event::{EventFlag, EventID},
        notification::NotificationAttributeID,
        AppAttribute,
    },
    characteristics::{
        control_point::{GetAppAttributesRequest, GetNotificationAttributesRequest},
        data_source,
    },
};
use anyhow::{bail, Result};
use bluer::{
    gatt::remote::{Characteristic, CharacteristicWriteRequest},
    Adapter, Address, Uuid,
};
use byteorder_pack::UnpackFrom;
use futures::{pin_mut, StreamExt as _};
use tokio::sync::RwLock;

use crate::filter::Filter;

pub const ANCS_SERVICE_UUID: Uuid = Uuid::from_u128(0x7905F431B5CE4E99A40F4B1E122D00D0);

pub struct AncsProcessor {
    control_point: Option<Characteristic>,
    shared: Arc<RwLock<crate::dbus_iface::SharedState>>,
    filter: Arc<RwLock<Filter>>,
    on_connected: Box<dyn Fn() + Send + Sync>,
    on_delivered: Box<dyn Fn(String, String) + Send + Sync>,
    on_filtered: Box<dyn Fn(String, String) + Send + Sync>,
    // BlueZ delivers each GATT notification twice via D-Bus PropertiesChanged.
    // Track recently processed UIDs and drop duplicates within 5s.
    recent_uids: HashMap<u32, Instant>,
    pending_requests: HashMap<u32, Instant>,
}

impl AncsProcessor {
    #[allow(dead_code)]
    pub fn new(
        shared: Arc<RwLock<crate::dbus_iface::SharedState>>,
        filter: Arc<RwLock<Filter>>,
    ) -> Self {
        Self::with_callbacks(
            shared,
            filter,
            Box::new(|| {}),
            Box::new(|_, _| {}),
            Box::new(|_, _| {}),
        )
    }

    pub fn with_callbacks(
        shared: Arc<RwLock<crate::dbus_iface::SharedState>>,
        filter: Arc<RwLock<Filter>>,
        on_connected: Box<dyn Fn() + Send + Sync>,
        on_delivered: Box<dyn Fn(String, String) + Send + Sync>,
        on_filtered: Box<dyn Fn(String, String) + Send + Sync>,
    ) -> Self {
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
    }

    pub async fn main_loop(mut self, device_addr: Address, adapter: &Adapter) -> Result<()> {
        let device = adapter.device(device_addr)?;

        // Actively connect to the device. BlueZ's Connect() drives the BLE
        // handshake from our side; passively waiting for iOS to reconnect on
        // its own is unreliable — iOS only auto-initiates for profiles it owns.
        // If already connected this is a fast no-op; if the BR/EDR link is up
        // but not LE, this triggers the LE pairing/service-discovery sequence.
        if !device.is_connected().await? {
            log::info!("Device {} not connected, calling connect()…", device_addr);
        }
        match device.connect().await {
            Ok(()) => log::info!("Device {} connected", device_addr),
            // BlueZ returns "Already Exists" when the link is already up — fine.
            Err(e) if format!("{e:#}").to_lowercase().contains("already") => {
                log::info!("Device {} already connected", device_addr);
            }
            Err(e) => bail!("connect() failed: {:#}", e),
        }

        disconnect_audio_profiles(&device).await;

        // Try the standard path first (ServicesResolved fires quickly on fresh connections).
        // If it takes more than 8s it means BlueZ isn't doing auto-discovery for this
        // connection (common when iOS also has a BR/EDR link in progress). In that case
        // fall back to scanning D-Bus objects directly — bluer caches discovered services
        // as D-Bus objects, so they're already there even when ServicesResolved = false.
        log::info!("Waiting for GATT services to resolve…");
        let services = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            device.services(),
        ).await {
            Ok(Ok(svcs)) if !svcs.is_empty() => {
                log::info!("Services resolved via standard path ({} found)", svcs.len());
                svcs
            }
            Ok(Ok(_)) => {
                // ServicesResolved fired before GATT objects were populated (timing race).
                log::warn!("ServicesResolved returned 0 services; falling back to D-Bus scan…");
                scan_services_from_dbus(&device).await?
            }
            Ok(Err(e)) => bail!("GATT service resolution failed: {}", e),
            Err(_) => {
                log::warn!("ServicesResolved not firing; scanning cached D-Bus objects…");
                scan_services_from_dbus(&device).await?
            }
        };
        let mut ancs_service = None;
        for s in services {
            let uuid = s.uuid().await?;
            log::debug!("GATT service: {}", uuid);
            if uuid == ANCS_SERVICE_UUID {
                ancs_service = Some(s);
            }
        }
        let ancs_service = match ancs_service {
            Some(s) => s,
            None => bail!("ANCS service not found"),
        };

        let mut notification_source = None;
        let mut data_source = None;
        let mut control_point = None;
        let noti_source_uuid: Uuid = "9FBF120D-6301-42D9-8C58-25E699A21DBD".parse()?;
        let data_source_uuid: Uuid = "22EAC6E9-24D6-4BB5-BE44-B36ACE7C7BFB".parse()?;
        let control_point_uuid: Uuid = "69D1D8F3-45E1-49A8-9821-9BBDFDAAD9D9".parse()?;
        for c in ancs_service.characteristics().await? {
            let uuid = c.uuid().await?;
            log::debug!("  ANCS characteristic: {}", uuid);
            if uuid == noti_source_uuid {
                notification_source = Some(c);
            } else if uuid == data_source_uuid {
                data_source = Some(c);
            } else if uuid == control_point_uuid {
                control_point = Some(c);
            }
        }
        let notification_source = notification_source.ok_or_else(|| anyhow::anyhow!("Notification source not found"))?;
        let data_source = data_source.ok_or_else(|| anyhow::anyhow!("Data source not found"))?;
        let control_point = control_point.ok_or_else(|| anyhow::anyhow!("Control point not found"))?;

        self.control_point = Some(control_point);

        // iOS persists CCCD state for bonded devices. If CCCD was already 0x0001
        // from a previous session, iOS sees our subscribe as a no-op and won't
        // reinitialize the ANCS session or send new events. Force a 0→1 transition
        // by writing 0x0000 first, then subscribing normally via notify().
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

        let data_source_stream = data_source.notify().await?;
        pin_mut!(data_source_stream);
        let notification_stream = notification_source.notify().await?;
        pin_mut!(notification_stream);
        let events_stream = adapter.events().await?;
        pin_mut!(events_stream);

        log::info!("Starting to listen for notifications");
        (self.on_connected)();

        // Detect iOS-initiated disconnects even when streams don't close cleanly.
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(15));
        heartbeat.tick().await; // skip the immediate first tick

        loop {
            tokio::select! {
                maybe_noti = notification_stream.next() => {
                    match maybe_noti {
                        Some(noti) => {
                            log::debug!("Raw notification source bytes: {:02x?}", noti);
                            self.process_notification(noti).await?;
                        }
                        None => bail!("ANCS notification stream closed; will reconnect"),
                    }
                }
                maybe_data = data_source_stream.next() => {
                    match maybe_data {
                        Some(data) => {
                            log::debug!("Raw data source bytes: {:02x?}", data);
                            self.process_data(data).await?;
                        }
                        None => bail!("ANCS data source stream closed; will reconnect"),
                    }
                }
                Some(event) = events_stream.next() => {
                    if let bluer::AdapterEvent::DeviceRemoved(addr) = event {
                        if addr == device_addr {
                            log::info!("Device removed, stopping");
                            break;
                        }
                    }
                }
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
                else => break,
            }
        }
        Ok(())
    }

    async fn process_notification(&mut self, noti: Vec<u8>) -> Result<()> {
        let (event_id, event_flags, _category_id, _category_count, notification_uid) =
            <(u8, u8, u8, u8, u32)>::unpack_from_le(&mut Cursor::new(&noti))?;

        log::info!(
            "ANCS event: id={} flags={:#04x} uid={}",
            event_id, event_flags, notification_uid
        );

        if event_id == EventID::NotificationRemoved as u8 {
            return Ok(());
        }
        if event_flags & EventFlag::PreExisting as u8 != 0 {
            log::info!("Skipping pre-existing notification {}", notification_uid);
            return Ok(());
        }

        // BlueZ emits each GATT notification twice via D-Bus PropertiesChanged.
        // Drop duplicates that arrive within 5 seconds of the first delivery.
        let now = Instant::now();
        self.recent_uids.retain(|_, t| now.duration_since(*t).as_secs() < 5);
        if self.recent_uids.contains_key(&notification_uid) {
            log::debug!("Dropping duplicate ANCS event uid={}", notification_uid);
            return Ok(());
        }
        self.recent_uids.insert(notification_uid, now);

        let cmd = GetNotificationAttributesRequest {
            command_id: CommandID::GetNotificationAttributes,
            notification_uid,
            attribute_ids: vec![
                (NotificationAttributeID::AppIdentifier, None),
                (NotificationAttributeID::Title, Some(64)),
                (NotificationAttributeID::Subtitle, Some(64)),
                (NotificationAttributeID::Message, Some(64)),
            ],
        };
        self.write_control_point(&Vec::from(cmd)).await?;
        self.pending_requests.insert(notification_uid, Instant::now());
        Ok(())
    }

    async fn process_data(&mut self, data: Vec<u8>) -> Result<()> {
        match data[0] {
            0 => {
                let notif = match data_source::GetNotificationAttributesResponse::parse(&data) {
                    Ok((_, app)) => app,
                    Err(e) => bail!("Error parsing notification attributes: {:?}", e),
                };
                self.pending_requests.remove(&notif.notification_uid);
                log::info!("Notif: {:?}", notif);

                let mut app_id_to_query: Option<String> = None;
                let mut current_app_id: Option<String> = None;
                let mut current_title: Option<String> = None;
                let mut current_body: Option<String> = None;
                let mut current_app_name_display: Option<String> = None;

                for attr in notif.attribute_list {
                    match attr.id {
                        NotificationAttributeID::AppIdentifier => {
                            if let Some(id) = attr.value {
                                {
                                    let s = self.shared.read().await;
                                    if let Some(name) = s.app_names.get(&id) {
                                        current_app_name_display = Some(name.clone());
                                    } else {
                                        current_app_name_display = Some(id.clone());
                                        app_id_to_query = Some(id.clone());
                                    }
                                }
                                current_app_id = Some(id);
                            }
                        }
                        NotificationAttributeID::Title => {
                            current_title = attr.value;
                        }
                        NotificationAttributeID::Message => {
                            current_body = attr.value;
                        }
                        _ => {}
                    }
                }

                let app_id = current_app_id.as_deref().unwrap_or("unknown");
                let title = current_title.clone().unwrap_or_default();

                let pass = {
                    let f = self.filter.read().await;
                    f.should_show(app_id)
                };

                if !pass {
                    log::info!("Filtered notification from {}", app_id);
                    (self.on_filtered)(app_id.to_string(), title.clone());
                } else {
                    let mut desktop_notification = notify_rust::Notification::new();
                    if let Some(name) = &current_app_name_display {
                        desktop_notification.appname(name);
                    }
                    if let Some(t) = &current_title {
                        desktop_notification.summary(t);
                    }
                    if let Some(b) = &current_body {
                        desktop_notification.body(b);
                    }
                    match desktop_notification.show_async().await {
                        Ok(handle) => {
                            log::info!(
                                "Shown notification {} with desktop handle {}",
                                notif.notification_uid,
                                handle.id()
                            );
                            (self.on_delivered)(app_id.to_string(), title);
                        }
                        Err(e) if format!("{e}").contains("ExcessNotification") => {
                            log::warn!("Notification rate-limited by desktop (uid={}), continuing", notif.notification_uid);
                        }
                        Err(e) => return Err(e.into()),
                    }
                }

                if let Some(app_id) = app_id_to_query {
                    log::info!("Querying app name for {}", app_id);
                    let cmd = GetAppAttributesRequest {
                        command_id: CommandID::GetAppAttributes,
                        app_identifier: app_id,
                        attribute_ids: vec![AppAttributeID::DisplayName],
                    };
                    self.write_control_point(&Vec::from(cmd)).await?;
                }
            }
            1 => {
                let mut app_id = vec![];
                let mut offset = 1;
                for i in offset..data.len() {
                    offset += 1;
                    if data[i] == 0 {
                        break;
                    }
                    app_id.push(data[i]);
                }
                let app_id = String::from_utf8_lossy(&app_id);

                let attribute = match AppAttribute::parse(&data[offset..]) {
                    Ok((_, attribute)) => attribute,
                    Err(e) => bail!("Error parsing app attributes: {:?}", e),
                };

                if attribute.id == AppAttributeID::DisplayName {
                    if let Some(name) = attribute.value {
                        log::info!("{} => {}", app_id, name);
                        self.shared.write().await.app_names.insert(app_id.to_string(), name);
                    }
                } else {
                    log::info!("Unknown app attribute: {:?}", attribute);
                }
            }
            _ => {}
        }
        Ok(())
    }

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
}

/// Disconnect BR/EDR audio profiles (A2DP, HFP, HSP) on the given device.
/// Called after every connect so the phone never routes audio or calls through
/// the laptop. Errors are ignored — the profile may simply not be connected.
async fn disconnect_audio_profiles(device: &bluer::Device) {
    const AUDIO_PROFILE_UUIDS: &[&str] = &[
        "0000110a-0000-1000-8000-00805f9b34fb", // A2DP Source
        "0000110b-0000-1000-8000-00805f9b34fb", // A2DP Sink
        "0000111e-0000-1000-8000-00805f9b34fb", // HFP Hands-Free
        "0000111f-0000-1000-8000-00805f9b34fb", // HFP Audio Gateway
        "00001108-0000-1000-8000-00805f9b34fb", // HSP Headset
        "00001112-0000-1000-8000-00805f9b34fb", // HSP Audio Gateway
    ];
    for uuid_str in AUDIO_PROFILE_UUIDS {
        if let Ok(uuid) = uuid_str.parse::<Uuid>() {
            match device.disconnect_profile(&uuid).await {
                Ok(()) => log::info!("Disconnected audio profile {}", uuid_str),
                Err(e) => log::debug!("Audio profile {} not connected ({})", uuid_str, e),
            }
        }
    }
}

/// Enumerate GATT services from cached D-Bus objects without waiting for
/// ServicesResolved. BlueZ persists discovered service objects across connections,
/// so they're available even when the ServicesResolved flag is false.
async fn scan_services_from_dbus(device: &bluer::Device) -> Result<Vec<bluer::gatt::remote::Service>> {
    let mut services = Vec::new();
    let adapter_name = device.adapter_name();
    let addr = device.address();
    let device_path = format!(
        "/org/bluez/{}/dev_{}",
        adapter_name,
        addr.to_string().replace(':', "_")
    );

    log::debug!("Introspecting device path: {}", device_path);

    let conn = zbus::Connection::system().await?;
    let proxy = zbus::fdo::IntrospectableProxy::builder(&conn)
        .destination("org.bluez")?
        .path(device_path)?
        .build()
        .await?;

    let xml = proxy.introspect().await?;
    let node = roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() },
    )?;

    for child in node.root_element().children() {
        if child.has_tag_name("node") {
            if let Some(name) = child.attribute("name") {
                if name.starts_with("service") {
                    if let Ok(id) = u16::from_str_radix(&name[7..], 16) {
                        if let Ok(svc) = device.service(id).await {
                            if svc.uuid().await.is_ok() {
                                services.push(svc);
                            }
                        }
                    }
                }
            }
        }
    }

    log::info!("D-Bus scan found {} cached GATT services", services.len());
    if services.is_empty() {
        bail!("no cached GATT services found; device may need a fresh BLE connection");
    }
    Ok(services)
}
