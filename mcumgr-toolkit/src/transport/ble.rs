#[cfg(test)]
mod tests;

mod connection;
pub use connection::BleConnection;
mod identifier;
pub use identifier::BleIdentifier;
use tokio::time::error::Elapsed;

use std::{collections::HashMap, pin::Pin, time::Duration};

use btleplug::{
    api::{
        Central, CentralEvent, Characteristic, Manager, Peripheral as _,
        RetrievePeripheralsOptions, ScanFilter, ValueNotification,
    },
    platform::{Adapter, Peripheral, PeripheralId},
};
use futures::{FutureExt, StreamExt};
use uuid::{Uuid, uuid};

use crate::{
    client::{BleDeviceInfo, BleDevices, BleError},
    transport::{ReceiveError, SMP_HEADER_SIZE, SmpHeader, Transport},
};

/// The error type of [`BleRuntime`].
pub type BleRuntimeError = btleplug::Error;

/// A stream of BLE notifications
type NotificationStream = Pin<Box<dyn futures::Stream<Item = ValueNotification> + Send>>;

/// A runtime manager that encapsulates all the
/// async BLE boilerplate code.
pub struct BleRuntime {
    runtime: Box<tokio::runtime::Runtime>,
    adapter: btleplug::platform::Adapter,
}

/// The BLE service UUID that signals SMP capability
pub const SMP_UUID: Uuid = uuid!("8D53DC1D-1DB7-4CD3-868B-8A527460AA84");
/// The BLE characteristic UUID used to communicate SMP messages
pub const CHARACTERISTIC_UUID: Uuid = uuid!("DA2E7828-FBCE-4E01-AE9E-261174997C48");

/// Attempt to connect to a given BLE device.
///
/// If no identifier is given, return an error that contains all available devices.
pub fn connect_to_device(
    identifier: Option<BleIdentifier>,
    scan_timeout: Duration,
    connect_timeout: Duration,
) -> Result<BleConnection, BleError> {
    let mut runtime = crate::transport::ble::BleRuntime::new()?;

    // First, try to retrieve a peripheral candidate from the cache.
    #[allow(unused_mut)]
    let mut candidate: Option<Peripheral> = None;
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "ios"))]
    if let Some(identifier) = &identifier {
        match runtime.get_peripheral_candidate(identifier.clone().into()) {
            Ok(device) => candidate = Some(device),
            Err(e) => log::debug!("Failed to resolve BLE peripheral directly: {e}"),
        }
    }

    // Try to connect; on windows candidates must not actually exist,
    // and even if they exist, they might only be connectable after scanning.
    // So make sure we can actually connect to the peripheral.
    if let Some(candidate) = candidate {
        match connection::try_connect(&runtime, &candidate, connect_timeout) {
            Ok(ownership) => {
                return Ok(BleConnection {
                    runtime,
                    device: candidate,
                    ownership,
                });
            }
            Err(e) => {
                log::debug!("Direct BLE connection failed, falling back to discovery: {e}");
            }
        };
    }

    let device = runtime.scan_for_device(identifier, scan_timeout)?;

    let ownership = connection::try_connect(&runtime, &device, connect_timeout)?;
    Ok(BleConnection {
        runtime,
        device,
        ownership,
    })
}

impl BleRuntime {
    /// Create a new [`BleRuntime`].
    pub fn new() -> Result<Self, BleRuntimeError> {
        let runtime = Box::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| BleRuntimeError::Other(e.into()))?,
        );

        let adapter = runtime.block_on(async {
            let manager = btleplug::platform::Manager::new().await?;

            let adapter = manager
                .adapters()
                .await?
                .into_iter()
                .next()
                .ok_or(BleRuntimeError::NoAdapterAvailable)?;

            Result::<_, BleRuntimeError>::Ok(adapter)
        })?;

        Ok(Self { runtime, adapter })
    }

    /// Scan for a device.
    pub fn scan_for_device(
        &mut self,
        identifier: Option<BleIdentifier>,
        scan_timeout: Duration,
    ) -> Result<Peripheral, BleError> {
        let mut devices = HashMap::new();

        let device = self
            .retrieve_peripherals_with_smp_service(async |previously_known_devices| {
                // Attempt to find the device we search for
                let mut found_device = None;
                for potential_device in &previously_known_devices {
                    #[allow(irrefutable_let_patterns)]
                    #[allow(clippy::unnecessary_fallible_conversions)]
                    if let Ok(current_identifier) = BleIdentifier::try_from(potential_device) {
                        if let Some(identifier) = &identifier
                            && identifier == &current_identifier
                        {
                            found_device = Some(potential_device.clone());
                            break;
                        }
                    }
                }

                // If device is not found, store the other devices that were given to us
                if found_device.is_none() {
                    for potential_device in previously_known_devices {
                        #[allow(irrefutable_let_patterns)]
                        #[allow(clippy::unnecessary_fallible_conversions)]
                        if let Ok(current_identifier) = BleIdentifier::try_from(&potential_device) {
                            if let Ok(Some(properties)) = potential_device.properties().await {
                                devices
                                    .entry(potential_device.id())
                                    .insert_entry(BleDeviceInfo {
                                        id: current_identifier,
                                        name: properties.local_name,
                                        rssi: properties.rssi,
                                    });
                            }
                        }
                    }
                }

                found_device
            })
            .unwrap_or_else(|e| {
                if !matches!(e, btleplug::Error::NotSupported(_)) {
                    log::warn!("Failed to fetch known BLE devices: {e}");
                }
                None
            });

        if let Some(device) = device {
            return Ok(device);
        }

        self.scan(
            async |mut events, central| -> Result<btleplug::platform::Peripheral, BleError> {
                tokio::time::timeout(scan_timeout, async {
                    loop {
                        match events.next().await.ok_or(BleError::ScanStopped)? {
                            btleplug::api::CentralEvent::DeviceDiscovered(id)
                            | btleplug::api::CentralEvent::DeviceConnected(id)
                            | btleplug::api::CentralEvent::DeviceUpdated(id)
                            | btleplug::api::CentralEvent::DeviceServicesModified(id)
                            | btleplug::api::CentralEvent::ServiceDataAdvertisement {
                                id,
                                service_data: _,
                            }
                            | btleplug::api::CentralEvent::ServicesAdvertisement {
                                id,
                                services: _,
                            }
                            | btleplug::api::CentralEvent::ManufacturerDataAdvertisement {
                                id,
                                manufacturer_data: _,
                            } => {
                                if let Ok(device) = central.peripheral(&id).await {
                                    #[allow(irrefutable_let_patterns)]
                                    #[allow(clippy::unnecessary_fallible_conversions)]
                                    if let Ok(current_identifier) = BleIdentifier::try_from(&device)
                                    {
                                        if let Some(identifier) = &identifier
                                            && identifier == &current_identifier
                                        {
                                            break Ok(device);
                                        }

                                        if let Ok(Some(properties)) = device.properties().await
                                            && properties
                                                .services
                                                .contains(&crate::transport::ble::SMP_UUID)
                                        {
                                            devices.entry(id).insert_entry(BleDeviceInfo {
                                                id: current_identifier,
                                                name: properties.local_name,
                                                rssi: properties.rssi,
                                            });
                                        }
                                    }
                                }
                            }
                            btleplug::api::CentralEvent::RssiUpdate { id, rssi } => {
                                if let Some(device) = devices.get_mut(&id) {
                                    device.rssi = Some(rssi);
                                }
                            }
                            _ => (),
                        }
                    }
                })
                .await
                .map_err(|_: Elapsed| {
                    let devices = BleDevices({
                        let mut device_list = devices.into_values().collect::<Vec<_>>();
                        device_list.sort();
                        device_list
                    });
                    if identifier.is_none() {
                        BleError::IdentifierEmpty { devices }
                    } else {
                        BleError::DeviceNotFound { available: devices }
                    }
                })?
            },
        )?
    }

    /// Try to resolve a peripheral candidate from a known peripheral ID
    pub fn get_peripheral_candidate(
        &mut self,
        identifier: PeripheralId,
    ) -> Result<Peripheral, BleRuntimeError> {
        let future = async {
            match self.adapter.add_peripheral(&identifier).await {
                Ok(peripheral) => Ok(peripheral),

                // If add_peripheral is not supported, try to `retrieve_peripherals`
                // and see if the peripheral is contained there
                Err(btleplug::Error::NotSupported(_)) => self
                    .adapter
                    .retrieve_peripherals(RetrievePeripheralsOptions {
                        identifiers: Some(vec![identifier.clone()]),
                        services: None,
                    })
                    .await?
                    .into_iter()
                    .next()
                    .ok_or(btleplug::Error::DeviceNotFound),

                Err(err) => Err(err),
            }
        };

        self.block_on(future)
    }

    /// Execute the given function after retrieving known peripherals
    /// that offer the SMP service
    pub fn retrieve_peripherals_with_smp_service<F, R>(
        &mut self,
        f: F,
    ) -> Result<R, BleRuntimeError>
    where
        F: AsyncFnOnce(Vec<Peripheral>) -> R,
    {
        let future = async {
            let peripherals = self
                .adapter
                .retrieve_peripherals(RetrievePeripheralsOptions {
                    identifiers: None,
                    services: Some(vec![SMP_UUID]),
                })
                .await?;

            Ok(f(peripherals).await)
        };

        self.block_on(future)
    }

    /// Execute the given function while scanning for devices
    pub fn scan<F, R>(&mut self, f: F) -> Result<R, BleRuntimeError>
    where
        F: AsyncFnOnce(Pin<Box<dyn futures::Stream<Item = CentralEvent> + Send>>, &Adapter) -> R,
    {
        let future = async {
            let events = self.adapter.events().await?;

            self.adapter
                .start_scan(ScanFilter { services: vec![] })
                .await?;

            let result = f(events, &self.adapter).await;

            let _ = self.adapter.stop_scan().await;

            Ok(result)
        };

        self.block_on(future)
    }

    /// Run a future to completion
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.runtime.block_on(future)
    }
}

async fn next_smp_notification(
    notifications: &mut NotificationStream,
    timeout: tokio::time::Duration,
) -> Result<ValueNotification, super::ReceiveError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let msg = tokio::time::timeout_at(deadline, notifications.next())
            .await
            .map_err(|_| super::ReceiveError::Timeout)?;

        let Some(msg) = msg else {
            return Err(ReceiveError::TransportError(
                "Notify queue closed unexpectedly".into(),
            ));
        };

        if msg.service_uuid == SMP_UUID && msg.uuid == CHARACTERISTIC_UUID && !msg.value.is_empty()
        {
            return Ok(msg);
        }
    }
}

async fn receive_smp_frame<'a>(
    notifications: &mut NotificationStream,
    timeout: tokio::time::Duration,
    buffer: &'a mut [u8; super::SMP_TRANSFER_BUFFER_SIZE],
) -> Result<&'a [u8], super::ReceiveError> {
    let msg = next_smp_notification(notifications, timeout).await?;

    let expected_len: usize = usize::from(
        msg.value
            .first_chunk()
            .copied()
            .map(SmpHeader::from_bytes)
            .ok_or(ReceiveError::UnexpectedResponse)?
            .data_length,
    ) + SMP_HEADER_SIZE;

    if expected_len > buffer.len() {
        return Err(ReceiveError::FrameTooBig);
    }

    let mut len = msg.value.len();
    if len > expected_len {
        return Err(ReceiveError::UnexpectedResponse);
    }
    buffer
        .get_mut(..len)
        .ok_or(ReceiveError::FrameTooBig)?
        .copy_from_slice(&msg.value);

    log::debug!(
        "Received SMP frame chunk: {} (expected: {})",
        len,
        expected_len
    );

    while len < expected_len {
        let msg = next_smp_notification(notifications, timeout).await?;

        let new_len = len + msg.value.len();
        if new_len > expected_len {
            return Err(ReceiveError::UnexpectedResponse);
        }

        buffer
            .get_mut(len..new_len)
            .ok_or(ReceiveError::FrameTooBig)?
            .copy_from_slice(&msg.value);

        len = new_len;

        log::debug!(
            "Received SMP continuation chunk: {} ({}/{})",
            msg.value.len(),
            len,
            expected_len
        );
    }

    log::debug!("Received SMP Frame ({} bytes)", len);

    buffer.get(..len).ok_or(ReceiveError::FrameTooBig)
}

/// An active connection to a BLE device
pub struct BleTransport {
    connection: BleConnection,
    characteristic: Characteristic,
    notifications: Option<Pin<Box<dyn futures::Stream<Item = ValueNotification> + Send>>>,
    timeout: Duration,
    send_buffer: Vec<u8>,
}

impl BleTransport {
    /// Creates a BLE transport from a given BLE connection.
    pub fn from_connection(
        connection: BleConnection,
        timeout: Duration,
    ) -> Result<BleTransport, BleRuntimeError> {
        connection
            .runtime
            .block_on(connection.device.discover_services_with_timeout(timeout))?;

        let characteristic = connection
            .device
            .characteristics()
            .iter()
            .find(|ch| ch.service_uuid == SMP_UUID && ch.uuid == CHARACTERISTIC_UUID)
            .cloned()
            .ok_or(BleRuntimeError::NoSuchCharacteristic)?;

        let _ = connection
            .runtime
            .block_on(connection.device.unsubscribe(&characteristic));
        if let Err(e) = connection
            .runtime
            .block_on(connection.device.subscribe(&characteristic))
        {
            let _ = connection
                .runtime
                .block_on(connection.device.unsubscribe(&characteristic));
            return Err(e);
        }

        let notifications = connection.runtime.block_on(async {
            match connection.device.notifications().await {
                Ok(not) => Ok(not),
                Err(e) => {
                    let _ = connection.device.unsubscribe(&characteristic).await;
                    Err(e)
                }
            }
        })?;

        Ok(BleTransport {
            connection,
            characteristic,
            notifications: Some(notifications),
            timeout,
            send_buffer: Vec::new(),
        })
    }
}

impl Transport for BleTransport {
    fn send_raw_frame(
        &mut self,
        header: [u8; super::SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), super::SendError> {
        log::debug!("Sending SMP Frame ({} bytes)", data.len());

        // Clear pending notifications
        let notifications = self.notifications.as_mut().unwrap();
        while let Some(Some(_)) = notifications.next().now_or_never() {
            // discard pending notification
        }

        self.send_buffer.clear();
        self.send_buffer.extend_from_slice(&header);
        self.send_buffer.extend_from_slice(data);

        async fn send_frame_parts(
            device: &Peripheral,
            characteristic: &Characteristic,
            data: &[u8],
        ) -> Result<(), super::SendError> {
            let chunk_size = usize::from(device.mtu().saturating_sub(3));
            if chunk_size == 0 {
                return Err(super::SendError::TransportError(
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid BLE MTU").into(),
                ));
            }
            log::debug!("Chunk size: {}", chunk_size);

            for chunk in data.chunks(chunk_size) {
                log::debug!("Sending SMP Frame Chunk ({} bytes)", chunk.len());
                device
                    .write(
                        characteristic,
                        chunk,
                        btleplug::api::WriteType::WithoutResponse,
                    )
                    .await?;
            }

            Ok(())
        }

        self.connection.runtime.block_on(send_frame_parts(
            &self.connection.device,
            &self.characteristic,
            &self.send_buffer,
        ))?;

        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; super::SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], super::ReceiveError> {
        let notifications = self.notifications.as_mut().unwrap();
        let timeout = self.timeout;

        self.connection
            .runtime
            .block_on(receive_smp_frame(notifications, timeout, buffer))
    }

    fn set_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.timeout = timeout;
        Ok(())
    }
}

impl Drop for BleTransport {
    fn drop(&mut self) {
        {
            // Drop of notifications seems to contain a tokio::spawn,
            // so it requires being inside of a runtime or it will panic
            let _guard = self.connection.runtime.runtime.enter();
            self.notifications.take();
        }

        if std::thread::panicking() {
            return;
        }

        const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

        let _ = self.connection.runtime.block_on(async {
            tokio::time::timeout(
                CLEANUP_TIMEOUT,
                self.connection.device.unsubscribe(&self.characteristic),
            )
            .await
        });
    }
}
