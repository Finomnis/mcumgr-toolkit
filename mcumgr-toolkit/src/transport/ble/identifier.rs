use std::str::FromStr;

use btleplug::{api::Peripheral as _, platform::Peripheral};

#[cfg(any(target_os = "macos", target_os = "ios"))]
type BleIdentifierRepr = uuid::Uuid;
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
type BleIdentifierRepr = btleplug::api::BDAddr;

/// An identifier that uniquely identifies a BLE device
///
/// Note that this differs based on OS.
///
/// On most operating systems, this is the BLE device MAC address.
/// The notable exception is MacOS/iOS where it is the
/// device UUID.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct BleIdentifier(pub BleIdentifierRepr);

impl From<BleIdentifierRepr> for BleIdentifier {
    fn from(value: BleIdentifierRepr) -> Self {
        Self(value)
    }
}

impl FromStr for BleIdentifier {
    type Err = <BleIdentifierRepr as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BleIdentifierRepr::from_str(s).map(BleIdentifier)
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
impl TryFrom<&Peripheral> for BleIdentifier {
    type Error = <Self as FromStr>::Err;
    fn try_from(peripheral: &Peripheral) -> Result<Self, Self::Error> {
        peripheral.id().to_string().parse()
    }
}
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
impl From<&Peripheral> for BleIdentifier {
    fn from(peripheral: &Peripheral) -> Self {
        Self(peripheral.address())
    }
}

impl BleIdentifier {
    /**
     * A human readable description of what the identifier contains
     */
    pub const fn help_name() -> &'static str {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            "BLE_UUID"
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            "BLE_MAC"
        }
    }
}

impl std::fmt::Display for BleIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
