use std::time::Duration;

use macaddr::MacAddr6;
use serde::Serialize;

fn bdaddr_to_str<S>(mac: &MacAddr6, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    ser.collect_str(mac)
}

/// Information about a BLE device
#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
pub struct BleDeviceInfo {
    /// The device BLE MAC address
    #[serde(serialize_with = "bdaddr_to_str")]
    pub mac: MacAddr6,
    /// The device name
    pub name: String,
    /// RSSI, in dBm
    pub rssi: Option<i16>,
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct BleRuntimeError(#[from] Box<dyn std::error::Error>);
pub type BleResult<T> = Result<T, BleRuntimeError>;

pub enum BleScanResult<'a> {
    Connected(Box<dyn BleDevice + 'a>),
    NotFound(Vec<BleDeviceInfo>),
}

/// The runtime environment required for BLE
/// communication
trait BleRuntime {
    async fn scan(
        &mut self,
        name: Option<&str>,
        addr: Option<MacAddr6>,
        timeout: Duration,
    ) -> BleResult<BleScanResult<'_>>;
}

/// A connected device
trait BleDevice {}

/// A BLE transport
pub struct BleTransport {}
