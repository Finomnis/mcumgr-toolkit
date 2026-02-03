/// Information about the bootloader
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootloaderInfo {
    /// MCUboot bootloader
    MCUboot {
        /// Bootloader mode
        ///
        /// See [`MCUbootMode`] for more information.
        mode: i32,
        /// Bootloader has downgrade prevention enabled
        no_downgrade: bool,
    },
    /// Unknown bootloader
    Unknown {
        /// Name of the bootloader
        name: String,
    },
}

impl serde::Serialize for BootloaderInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let struct_name = "BootloaderInfo";

        match self {
            BootloaderInfo::MCUboot { mode, no_downgrade } => {
                let mut s = serializer.serialize_struct(struct_name, 3)?;
                s.serialize_field("name", &BootloaderType::MCUboot.to_string())?;
                s.serialize_field("mode", mode)?;
                s.serialize_field("no_downgrade", no_downgrade)?;
                s.end()
            }

            BootloaderInfo::Unknown { name } => {
                let mut s = serializer.serialize_struct(struct_name, 1)?;
                s.serialize_field("name", name)?;
                s.end()
            }
        }
    }
}

/// Supported bootloader/image types
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, strum::Display, strum::EnumString,
)]
pub enum BootloaderType {
    /// MCUboot Bootloader
    MCUboot,
}

impl BootloaderInfo {
    /// Extract the bootloader type
    ///
    /// If the type is unknown, returns the name of the bootloader `Err` value.
    pub fn get_bootloader_type(&self) -> Result<BootloaderType, String> {
        match self {
            BootloaderInfo::MCUboot { .. } => Ok(BootloaderType::MCUboot),
            BootloaderInfo::Unknown { name } => Err(name.clone()),
        }
    }
}

/// MCUboot modes
///
/// See [`enum mcuboot_mode`](https://github.com/mcu-tools/mcuboot/blob/main/boot/bootutil/include/bootutil/boot_status.h).
#[derive(
    strum::FromRepr, strum::IntoStaticStr, strum::Display, Debug, Copy, Clone, PartialEq, Eq,
)]
#[repr(i32)]
#[allow(non_camel_case_types)]
#[allow(missing_docs)]
pub enum MCUbootMode {
    MCUBOOT_MODE_SINGLE_SLOT = 0,
    MCUBOOT_MODE_SWAP_USING_SCRATCH,
    MCUBOOT_MODE_UPGRADE_ONLY,
    MCUBOOT_MODE_SWAP_USING_MOVE,
    MCUBOOT_MODE_DIRECT_XIP,
    MCUBOOT_MODE_DIRECT_XIP_WITH_REVERT,
    MCUBOOT_MODE_RAM_LOAD,
    MCUBOOT_MODE_FIRMWARE_LOADER,
    MCUBOOT_MODE_SINGLE_SLOT_RAM_LOAD,
    MCUBOOT_MODE_SWAP_USING_OFFSET,
}
