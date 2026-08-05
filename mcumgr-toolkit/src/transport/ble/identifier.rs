use std::str::FromStr;

use btleplug::{api::Peripheral as _, platform::Peripheral};

cfg_select! {
    any(target_os = "macos", target_os = "ios") => {
        type BleIdentifierRepr = uuid::Uuid;
    }
    _ => {
        type BleIdentifierRepr = btleplug::api::BDAddr;
    }
}

/// An identifier that uniquely identifies a BLE device
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BleIdentifier(BleIdentifierRepr);

impl FromStr for BleIdentifier {
    type Err = <BleIdentifierRepr as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BleIdentifierRepr::from_str(s).map(BleIdentifier)
    }
}

cfg_select! {
    any(target_os = "macos", target_os = "ios") => {
        impl<'a> TryFrom<&'a Peripheral> for BleIdentifier {
            type Error = <Self as FromStr>::Err;
            fn try_from(peripheral: &'a Peripheral) -> Result<Self, Self::Error> {
                Self::from_str(peripheral.id().to_string())
            }
        }
    }
    _ => {
        impl<'a> From<&'a Peripheral> for BleIdentifier {
            fn from(peripheral: &'a Peripheral) -> Self {
                Self(peripheral.address())
            }
        }
    }
}

impl BleIdentifier {
    /**
     * A human readable description of what the identifier contains
     */
    pub const fn help_name() -> &'static str {
        cfg_select! {
            any(target_os = "macos", target_os = "ios") => {
                "UUID"
            }
            _ => {
                "MAC_ADDR"
            }
        }
    }
}

impl std::fmt::Display for BleIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
