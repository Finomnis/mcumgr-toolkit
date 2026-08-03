//! BLE backends

use std::time::Duration;

use macaddr::MacAddr6;

use crate::{
    client::BleError,
    transport::{ReceiveError, SMP_HEADER_SIZE, SendError, Transport},
};

mod btleplug;
pub use btleplug::BtleplugBackend;

/// Newtype wrapper around a BLE backend specific error
#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct BleBackendError(Box<dyn std::error::Error + Send + Sync>);

/// Functionality required from a BLE backend
pub trait BleBackend {
    /// Attempt to connect to a BLE device.
    fn connect(
        &mut self,
        name: Option<&str>,
        addr: Option<MacAddr6>,
        timeout: Duration,
    ) -> Result<(), BleError>;

    /// Retrieve the MTU of the connected device
    fn mtu(&mut self) -> Result<usize, BleBackendError>;

    /// Send a raw SMP frame over the bus.
    ///
    /// This function must be provided by the implementing struct
    /// but should not be called directly.
    fn send_chunk(&mut self, data: &[u8]) -> Result<(), BleBackendError>;

    /// Drain the receive queue to clear out stale notifications
    fn drain_recv_queue(&mut self) -> Result<(), BleBackendError>;

    /// Receive a raw SMP chunk from the bus.
    ///
    /// Should internally read the next object from the
    /// mcumgr characteristic notification queue.
    fn recv_chunk<'a>(&mut self, buffer: &'a mut [u8]) -> Result<usize, ReceiveError>;
}
