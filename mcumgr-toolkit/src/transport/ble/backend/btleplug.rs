use std::{pin::Pin, sync::Arc, time::Duration};

use btleplug::{
    api::{Central, Characteristic, Manager, Peripheral as _, ValueNotification},
    platform::Peripheral,
};
use futures::{Stream, StreamExt as _};
use macaddr::MacAddr6;
use tokio::sync::{SetOnce, SetOnceError};

use crate::transport::ble::{
    CHARACTERISTIC_UUID, SMP_UUID,
    async_reactor::AsyncReactor,
    backend::{BleBackend, BleBackendError},
};

/// A stream of characteristic notifications
type NotificationStream = Pin<Box<dyn Stream<Item = ValueNotification> + Send>>;

/// The actual async implementation of the backend
struct BtleplugInner {
    adapter: btleplug::platform::Adapter,
    peripheral: Option<(Peripheral, NotificationStream)>,
}

/// BLE backend based on btleplug
pub struct BtleplugBackend {
    reactor: AsyncReactor,
    inner: BtleplugInner,
}

impl From<btleplug::Error> for BleBackendError {
    fn from(value: btleplug::Error) -> Self {
        BleBackendError(Box::new(value).into())
    }
}

impl BtleplugBackend {
    /// Create a new btleplug based BLE backend
    pub fn new() -> Result<Self, BleBackendError> {
        let reactor = AsyncReactor::new();

        let adapter = reactor.block_on(async {
            let manager = btleplug::platform::Manager::new().await?;

            let adapter = manager
                .adapters()
                .await?
                .into_iter()
                .next()
                .ok_or(btleplug::Error::NoAdapterAvailable)?;

            Result::<_, BleBackendError>::Ok(adapter)
        })?;

        Ok(Self {
            reactor,
            inner: BtleplugInner {
                adapter,
                peripheral: None,
            },
        })
    }

    fn block_on<F, T>(&mut self, f: F) -> Result<T, BleBackendError>
    where
        F: AsyncFnOnce(&mut BtleplugInner) -> Result<T, Box<dyn std::error::Error + Send + Sync>>,
    {
        self.reactor
            .block_on(f(&mut self.inner))
            .map_err(BleBackendError)
    }

    fn connected_block_on<F, T>(&mut self, f: F) -> Result<T, BleBackendError>
    where
        F: AsyncFnOnce(
            &mut Peripheral,
            &mut NotificationStream,
        ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>,
    {
        if let Some((peripheral, notifications)) = &mut self.inner.peripheral {
            self.reactor
                .block_on(f(peripheral, notifications))
                .map_err(BleBackendError)
        } else {
            Err(BleBackendError("Backend not connected!".into()))
        }
    }
}

impl BtleplugInner {
    fn process_peripheral(
        &mut self,
        peripheral: Peripheral,
        name: Option<Arc<str>>,
        addr: Option<MacAddr6>,
    ) {
        async fn process_peripheral_internal(
            peripheral: &Peripheral,
            name: Option<Arc<str>>,
            addr: Option<MacAddr6>,
        ) -> Result<Option<NotificationStream>, BleBackendError> {
            if !peripheral
                .properties()
                .await?
                .is_some_and(|props| props.services.contains(&SMP_UUID))
            {
                // Does not contain required service UUID.
                return Ok(None);
            }

            if !peripheral.is_connected().await? {
                peripheral
                    .connect_with_timeout(Duration::from_secs(3))
                    .await?;
            }

            peripheral
                .discover_services_with_timeout(Duration::from_secs(3))
                .await?;

            let Some(characteristic) = peripheral
                .characteristics()
                .iter()
                .find(|ch| ch.service_uuid == SMP_UUID && ch.uuid == CHARACTERISTIC_UUID)
                .cloned()
            else {
                return Ok(None);
            };

            let notifications = peripheral.notifications().await?;

            let _ = peripheral.unsubscribe(&characteristic).await;
            if let Err(e) = peripheral.subscribe(&characteristic).await {
                let _ = peripheral.unsubscribe(&characteristic).await;
                return Err(e.into());
            }

            Ok(Some(notifications))
        }

        async fn cleanup(peripheral: Peripheral) {
            if let Ok(true) = peripheral.is_connected().await {
                let _ = peripheral.disconnect().await;
            }
        }

        let result = Arc::clone(&self.peripheral);
        tokio::spawn(async move {
            let peripheral = peripheral;
            match process_peripheral_internal(&peripheral, name, addr).await {
                Ok(Some(c)) => {
                    if let Err(SetOnceError(peripheral)) = result.set(peripheral) {
                        cleanup(peripheral).await;
                    }
                }
                Ok(None) => {
                    cleanup(peripheral).await;
                }
                Err(e) => {
                    log::warn!("Failed to process periphal: {:?}", e);
                }
            }
        });
    }

    async fn connect(
        &mut self,
        name: Option<&str>,
        addr: Option<MacAddr6>,
        timeout: std::time::Duration,
    ) -> Result<(), crate::client::BleError> {
        let name = name.map(Arc::from);

        for peripheral in self.adapter.peripherals().await.unwrap() {
            self.process_peripheral(peripheral, name.clone(), addr);
        }
        todo!()
    }
}

struct ScanState {}

impl BleBackend for BtleplugBackend {
    fn connect(
        &mut self,
        name: Option<&str>,
        addr: Option<MacAddr6>,
        timeout: std::time::Duration,
    ) -> Result<(), crate::client::BleError> {
        self.reactor
            .block_on(self.inner.connect(name, addr, timeout))
    }

    fn mtu(&mut self) -> Result<usize, BleBackendError> {
        self.connected_block_on(async |peripheral, _| Ok(peripheral.mtu().into()))
    }

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), BleBackendError> {
        self.connected_block_on(async |peripheral, _| {
            peripheral
                .write(
                    characteristic,
                    chunk,
                    btleplug::api::WriteType::WithoutResponse,
                )
                .await
        })
    }

    fn drain_recv_queue(&mut self) -> Result<(), BleBackendError> {
        if let Some((_, notification_stream)) = &mut self.inner.peripheral {
            let notification_stream = notification_stream.by_ref();
            self.reactor
                .block_on(notification_stream.for_each(|_| async {}));
            Ok(())
        } else {
            Err(BleBackendError("Backend not connected!".into()))
        }
    }

    fn recv_chunk<'a>(
        &mut self,
        buffer: &'a mut [u8],
    ) -> Result<usize, crate::transport::ReceiveError> {
        todo!()
    }
}
