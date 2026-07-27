use btleplug::api::{Central, Manager, Peripheral};
use macaddr::MacAddr6;

use crate::transport::ble::{
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
    async fn connect(
        &mut self,
        name: Option<&str>,
        addr: Option<MacAddr6>,
        timeout: std::time::Duration,
    ) -> Result<(), crate::client::BleError> {
        for peripheral in self.adapter.peripherals().await.unwrap() {
            println!("{:?}", peripheral.is_connected().await);
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
}
