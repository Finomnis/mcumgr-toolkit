use std::sync::Arc;

use btleplug::api::Manager;

/// The error type of [`BleRuntime`].
pub type BleRuntimeError = btleplug::Error;

/// A runtime manager that encapsulates all the
/// async BLE boilerplate code.
pub struct BleRuntime {
    runtime: Arc<tokio::runtime::Runtime>,
    adapter: btleplug::platform::Adapter,
}

impl BleRuntime {
    /// Create a new [`BleRuntime`].
    ///
    /// # Arguments
    ///
    /// * `serial` - A serial port object, like [`serialport::SerialPort`].
    ///
    pub fn new() -> Result<Self, BleRuntimeError> {
        let runtime = Arc::new(
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
}
