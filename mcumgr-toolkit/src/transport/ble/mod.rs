mod async_reactor;

/// BLE backends
pub mod backend;

use macaddr::MacAddr6;
use serde::Serialize;
use uuid::{Uuid, uuid};

/// The BLE service UUID that signals SMP capability
pub const SMP_UUID: Uuid = uuid!("8D53DC1D-1DB7-4CD3-868B-8A527460AA84");
/// The BLE characteristic UUID used to communicate SMP messages
pub const CHARACTERISTIC_UUID: Uuid = uuid!("DA2E7828-FBCE-4E01-AE9E-261174997C48");

/// Serialize macaddres as a string opposed to its internal representation
fn serialize_mac_as_string<S>(mac: &MacAddr6, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    ser.collect_str(mac)
}

/// Information about a BLE device
#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
pub struct BleDeviceInfo {
    /// The device BLE MAC address
    #[serde(serialize_with = "serialize_mac_as_string")]
    pub mac: MacAddr6,
    /// The device name
    pub name: String,
    /// RSSI, in dBm
    pub rssi: Option<i16>,
}

/// Backend agnostic BLE transport
pub struct BleTransport {}
