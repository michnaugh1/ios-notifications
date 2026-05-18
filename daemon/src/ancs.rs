//! ANCS (Apple Notification Center Service) protocol processor.
//!
//! Inherited from kmod-midori/ancs-linux @ 6883f2b with minor adaptations
//! (will be extended for filtering in Task 6).

use std::{collections::HashMap, io::Cursor, sync::Arc};

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
    app_names: HashMap<String, String>,
    filter: Arc<RwLock<Filter>>,
    on_delivered: Box<dyn Fn(String, String) + Send + Sync>,
    on_filtered: Box<dyn Fn(String, String) + Send + Sync>,
}

impl AncsProcessor {
    #[allow(dead_code)]
    pub fn new(filter: Arc<RwLock<Filter>>) -> Self {
        Self::with_callbacks(filter, Box::new(|_, _| {}), Box::new(|_, _| {}))
    }

    pub fn with_callbacks(
        filter: Arc<RwLock<Filter>>,
        on_delivered: Box<dyn Fn(String, String) + Send + Sync>,
        on_filtered: Box<dyn Fn(String, String) + Send + Sync>,
    ) -> Self {
        Self {
            control_point: None,
            app_names: HashMap::new(),
            filter,
            on_delivered,
            on_filtered,
        }
    }

    pub async fn main_loop(mut self, device_addr: Address, adapter: &Adapter) -> Result<()> {
        let device = adapter.device(device_addr)?;

        if !device.is_connected().await? {
            log::debug!("Device {} is not connected", device_addr);
            return Ok(());
        }
        log::info!("Device {} is connected", device_addr);

        // When iOS connects to us (HID peripheral role), BlueZ has an HCI
        // connection but hasn't run GATT service discovery or requested
        // encryption. device.connect() does both — it blocks until GATT
        // discovery completes and ServicesResolved becomes true.
        log::info!("Triggering GATT service discovery and encryption…");
        device.connect().await?;
        log::info!("Services resolved");

        let services = device.services().await?;
        let mut ancs_service = None;
        for s in services {
            if s.uuid().await? == ANCS_SERVICE_UUID {
                ancs_service = Some(s);
                break;
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

        let data_source_stream = data_source.notify().await?;
        pin_mut!(data_source_stream);
        let notification_stream = notification_source.notify().await?;
        pin_mut!(notification_stream);
        let events_stream = adapter.events().await?;
        pin_mut!(events_stream);

        log::info!("Starting to listen for notifications");

        loop {
            tokio::select! {
                Some(noti) = notification_stream.next() => {
                    self.process_notification(noti).await?;
                }
                Some(data) = data_source_stream.next() => {
                    self.process_data(data).await?;
                }
                Some(event) = events_stream.next() => {
                    if let bluer::AdapterEvent::DeviceRemoved(addr) = event {
                        if addr == device_addr {
                            log::info!("Device removed, stopping");
                            break;
                        }
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

        if event_id == EventID::NotificationRemoved as u8 {
            return Ok(());
        }
        if event_flags & EventFlag::PreExisting as u8 != 0 {
            return Ok(());
        }

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
        Ok(())
    }

    async fn process_data(&mut self, data: Vec<u8>) -> Result<()> {
        match data[0] {
            0 => {
                let notif = match data_source::GetNotificationAttributesResponse::parse(&data) {
                    Ok((_, app)) => app,
                    Err(e) => bail!("Error parsing notification attributes: {:?}", e),
                };
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
                                if let Some(name) = self.app_names.get(&id) {
                                    current_app_name_display = Some(name.clone());
                                } else {
                                    current_app_name_display = Some(id.clone());
                                    app_id_to_query = Some(id.clone());
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
                    let handle = desktop_notification.show_async().await?;
                    log::info!(
                        "Shown notification {} with desktop handle {}",
                        notif.notification_uid,
                        handle.id()
                    );
                    (self.on_delivered)(app_id.to_string(), title);
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
                        self.app_names.insert(app_id.to_string(), name);
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
                .await?;
        }
        Ok(())
    }
}
