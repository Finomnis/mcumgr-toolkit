use std::borrow::Cow;

use miette::Diagnostic;
use thiserror::Error;

use crate::{MCUmgrClient, bootloader::BootloaderType, connection::ExecuteError, mcuboot};

/// Possible errors that can happen during firmware update.
#[derive(Error, Debug, Diagnostic)]
pub enum FirmwareUpdateError {
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::progress_cb_error))]
    ProgressCallbackError,
    /// An error occurred while trying to detect the bootloader
    #[error("Failed to detect bootloader")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::detect_bootloader))]
    BootloaderDetectionFailed(#[source] ExecuteError),
    /// The device contains a bootloader that is not supported
    #[error("Bootloader '{0}' not supported")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::unknown_bootloader))]
    BootloaderNotSupported(String),
    /// The device contains a bootloader that is not supported
    #[error("Firmare is not a valid MCUboot image")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::mcuboot_image))]
    InvalidMcuBootFirmwareImage(#[from] mcuboot::ImageParseError),
}

/// Configurable parameters for [`firmware_update`].
#[derive(Default)]
pub struct FirmwareUpdateParams {
    /// The bootloader type.
    ///
    /// Auto-detect bootloader if `None`.
    pub bootloader_type: Option<BootloaderType>,
    /// Do not reboot device after the update
    pub skip_reboot: bool,
    /// Skip test boot and confirm directly
    pub force_confirm: bool,
}

/// The progress callback type of [`firmware_update`].
///
/// # Arguments
///
/// * `&str` - Human readable description of the current step
/// * `Option<(u64, u64)>` - The (current, total) progress of the current step, if available.
///
/// # Return
///
/// `false` on error; this will cancel the update
///
pub type FirmwareUpdateProgressCallback<'a> = dyn FnMut(&str, Option<(u64, u64)>) -> bool + 'a;

/// High level firmware update routine
///
/// # Arguments
///
/// * `client` - The MCUmgr client.
/// * `params` - Configurable parameters.
/// * `progress` - A callback that receives progress updates.
///
pub fn firmware_update(
    client: &MCUmgrClient,
    firmware: impl AsRef<[u8]>,
    params: FirmwareUpdateParams,
    mut progress: Option<&mut FirmwareUpdateProgressCallback>,
) -> Result<(), FirmwareUpdateError> {
    let firmware = firmware.as_ref();

    let mut progress = |msg: Cow<str>, prog| {
        if let Some(progress) = &mut progress {
            if !progress(msg.as_ref(), prog) {
                return Err(FirmwareUpdateError::ProgressCallbackError);
            }
        }
        Ok(())
    };

    let bootloader_type = if let Some(bootloader_type) = params.bootloader_type {
        bootloader_type
    } else {
        progress("Detecting bootloader ...".into(), None)?;

        let bootloader_type = client
            .os_bootloader_info()
            .map_err(FirmwareUpdateError::BootloaderDetectionFailed)?
            .get_bootloader_type()
            .map_err(FirmwareUpdateError::BootloaderNotSupported)?;

        progress(format!("Found bootloader '{bootloader_type}'").into(), None)?;

        bootloader_type
    };

    progress("Parsing firmware image ...".into(), None)?;
    let image_id_hash = match bootloader_type {
        BootloaderType::McuBoot => {
            let info = mcuboot::get_image_info(std::io::Cursor::new(firmware))?;
            progress(format!("Image version: {}", info.version).into(), None)?;
            info.hash
        }
    };

    Ok(())
}
