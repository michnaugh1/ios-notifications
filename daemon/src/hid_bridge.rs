//! HID keyboard GATT advertisement.
//!
//! iOS auto-reconnects to BLE peripherals it perceives as HID keyboards
//! (Apple Magic Keyboard, etc.) but not to "generic" BLE peripherals. By
//! advertising a fake HID keyboard service alongside our ANCS client, we
//! trigger iOS's auto-reconnect behavior.
//!
//! Inherited verbatim from kmod-midori/ancs-linux @ 6883f2b.

use anyhow::Result;
use bluer::{Adapter, Uuid, UuidExt};

const HID_REPORT_MAP: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xa1, 0x01, // Collection (Application)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0xe0, //   Usage Minimum (224)
    0x29, 0xe7, //   Usage Maximum (231)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Variable, Absolute) — modifier keys
    0x95, 0x01, //   Report Count (1)
    0x75, 0x08, //   Report Size (8)
    0x81, 0x01, //   Input (Constant) — reserved byte
    0x95, 0x06, //   Report Count (6)
    0x75, 0x08, //   Report Size (8)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xff, 0x00, //   Logical Maximum (255)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0x00, //   Usage Minimum (0)
    0x2a, 0xff, 0x00, //   Usage Maximum (255)
    0x81, 0x00, //   Input (Data, Array) — key codes
    0xc0, // End Collection
];

pub async fn serve_hid_gatt(
    adapter: &Adapter,
) -> Result<(
    bluer::gatt::local::ApplicationHandle,
    bluer::adv::AdvertisementHandle,
)> {
    use bluer::adv::{Advertisement, Type};
    use bluer::gatt::local::{
        Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
        CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Descriptor,
        DescriptorRead, Service,
    };

    let app = Application {
        services: vec![Service {
            uuid: Uuid::from_u16(0x1812),
            primary: true,
            characteristics: vec![
                // HID Information: version 1.11, not localized, NormallyConnectable
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4A),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Information read by {}", req.device_address);
                                Ok(vec![0x11, 0x01, 0x00, 0x02])
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // Report Map
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4B),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Report Map read by {}", req.device_address);
                                Ok(HID_REPORT_MAP.to_vec())
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // HID Control Point — accept Suspend/Exit Suspend, no-op
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4C),
                    write: Some(CharacteristicWrite {
                        write: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(|value, req| {
                            Box::pin(async move {
                                log::info!(
                                    "HID Control Point written by {}: {:?}",
                                    req.device_address,
                                    value
                                );
                                Ok(())
                            })
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // Protocol Mode — report protocol, accept writes
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4E),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Protocol Mode read by {}", req.device_address);
                                Ok(vec![0x01])
                            })
                        }),
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(|value, req| {
                            Box::pin(async move {
                                log::info!(
                                    "HID Protocol Mode written by {}: {:?}",
                                    req.device_address,
                                    value
                                );
                                Ok(())
                            })
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // Report (input) with Report Reference descriptor
                Characteristic {
                    uuid: Uuid::from_u16(0x2A4D),
                    read: Some(CharacteristicRead {
                        read: true,
                        fun: Box::new(|req| {
                            Box::pin(async move {
                                log::info!("HID Report read by {}", req.device_address);
                                Ok(vec![0u8; 8])
                            })
                        }),
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        indicate: false,
                        method: CharacteristicNotifyMethod::Fun(Box::new(|notifier| {
                            Box::pin(async move {
                                log::info!("HID Report notifications started");
                                notifier.stopped().await;
                                log::info!("HID Report notifications stopped");
                            })
                        })),
                        ..Default::default()
                    }),
                    descriptors: vec![Descriptor {
                        uuid: Uuid::from_u16(0x2908),
                        read: Some(DescriptorRead {
                            read: true,
                            fun: Box::new(|req| {
                                Box::pin(async move {
                                    log::info!(
                                        "HID Report Reference read by {}",
                                        req.device_address
                                    );
                                    Ok(vec![0x00, 0x01])
                                })
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };

    log::info!("Registering HID GATT application");
    let app_handle = adapter.serve_gatt_application(app).await?;

    log::info!("Starting HID advertisement");
    let adv = Advertisement {
        advertisement_type: Type::Peripheral,
        service_uuids: [Uuid::from_u16(0x1812)].into(),
        appearance: Some(0x03C1),
        local_name: Some("iOS Notifications Bridge".into()),
        discoverable: Some(true),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(adv).await?;

    Ok((app_handle, adv_handle))
}
