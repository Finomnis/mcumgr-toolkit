use std::time::Duration;

use btleplug::{api::Peripheral as _, platform::Peripheral};

use crate::transport::ble::{BleRuntime, BleRuntimeError};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum ConnectionOwnership {
    Inherited,
    Ours,
}

/// A BLE connection
///
/// Disconnects if we connected to it.
pub struct BleConnection {
    pub(crate) runtime: BleRuntime,
    pub(crate) device: Peripheral,
    pub(crate) ownership: ConnectionOwnership,
}

impl Drop for BleConnection {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }

        const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

        if ConnectionOwnership::Ours == self.ownership {
            let _ = self.runtime.block_on(async {
                tokio::time::timeout(CLEANUP_TIMEOUT, self.device.disconnect()).await
            });
        }
    }
}

pub(crate) fn try_connect(
    runtime: &BleRuntime,
    device: &Peripheral,
    timeout: Duration,
) -> Result<ConnectionOwnership, BleRuntimeError> {
    runtime.block_on(async {
        if device.is_connected().await? {
            return Ok(ConnectionOwnership::Inherited);
        }

        device.connect_with_timeout(timeout).await?;
        Ok(ConnectionOwnership::Ours)
    })
}
