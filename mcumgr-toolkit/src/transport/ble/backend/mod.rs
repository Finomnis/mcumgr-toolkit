use std::time::Duration;

use macaddr::MacAddr6;

use crate::client::BleError;

mod btleplug;

/// Newtype wrapper around a BLE backend specific error
#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct BleBackendError(Box<dyn std::error::Error>);

/// Functionality required from a BLE backend
pub trait BleBackend {
    fn connect(
        &mut self,
        name: Option<&str>,
        addr: Option<MacAddr6>,
        timeout: Duration,
    ) -> Result<(), BleError>;
}
