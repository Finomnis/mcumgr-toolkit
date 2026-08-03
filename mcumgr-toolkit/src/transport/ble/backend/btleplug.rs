use btleplug::api::{Central, Manager, Peripheral};
use macaddr::MacAddr6;

use crate::transport::ble::{
    SMP_UUID,
    async_reactor::AsyncReactor,
    backend::{BleBackend, BleBackendError},
};

/// The actual async implementation of the backend
struct BtleplugInner {
    adapter: btleplug::platform::Adapter,
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
            inner: BtleplugInner { adapter },
        })
    }
}

impl BtleplugInner {
    async fn try_connect(
        &mut self,
        peripheral: Peripheral,
        name: Option<&str>,
        addr: Option<MacAddr6>,
    ) -> Result<bool, crate::client::BleError> {
        peripheral
            .properties()
            .await?
            .is_some_and(|props| props.services.contains(&SMP_UUID))
    }

    async fn connect(
        &mut self,
        name: Option<&str>,
        addr: Option<MacAddr6>,
        timeout: std::time::Duration,
    ) -> Result<(), crate::client::BleError> {
        for peripheral in self.adapter.peripherals().await.unwrap() {
            // if self.try_connect(peripheral) {
            //     return Ok();
            // }
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
        todo!()
    }

    fn send_chunk(&mut self, data: &[u8]) -> Result<(), crate::transport::SendError> {
        todo!()
    }

    fn drain_recv_queue(&mut self) -> Result<(), BleBackendError> {
        todo!()
    }

    fn recv_chunk<'a>(
        &mut self,
        buffer: &'a mut [u8],
    ) -> Result<usize, crate::transport::ReceiveError> {
        todo!()
    }
}
