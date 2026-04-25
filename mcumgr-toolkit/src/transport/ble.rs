use std::pin::Pin;

use btleplug::{
    api::{Central, CentralEvent, Manager, ScanFilter},
    platform::Adapter,
};
use uuid::{Uuid, uuid};

/// The error type of [`BleRuntime`].
pub type BleRuntimeError = btleplug::Error;

/// A runtime manager that encapsulates all the
/// async BLE boilerplate code.
pub struct BleRuntime {
    runtime: Box<tokio::runtime::Runtime>,
    adapter: btleplug::platform::Adapter,
}

/// The BLE service UUID that signals SMP capability
pub const SMP_UUID: Uuid = uuid!("8D53DC1D-1DB7-4CD3-868B-8A527460AA84");

impl BleRuntime {
    /// Create a new [`BleRuntime`].
    ///
    /// # Arguments
    ///
    /// * `serial` - A serial port object, like [`serialport::SerialPort`].
    ///
    pub fn new() -> Result<Self, BleRuntimeError> {
        let runtime = Box::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
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

    /// Execute the given function while scanning for devices
    pub fn scan<F, R>(&mut self, f: F) -> Result<R, BleRuntimeError>
    where
        F: AsyncFnOnce(Pin<Box<dyn futures::Stream<Item = CentralEvent> + Send>>, &Adapter) -> R,
    {
        let future = async {
            self.adapter.clear_peripherals().await?;

            let events = self.adapter.events().await?;

            self.adapter
                .start_scan(ScanFilter {
                    services: vec![SMP_UUID],
                })
                .await?;

            let result = f(events, &self.adapter).await;

            self.adapter.stop_scan().await?;

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
