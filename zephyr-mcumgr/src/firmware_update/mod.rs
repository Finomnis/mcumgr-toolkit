use std::borrow::Cow;

use miette::Diagnostic;
use thiserror::Error;

use crate::{
    MCUmgrClient, bootloader::BootloaderType, client::ImageUploadError, connection::ExecuteError,
    mcuboot,
};

/// Possible errors that can happen during firmware update.
#[derive(Error, Debug, Diagnostic)]
pub enum FirmwareUpdateError {
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::progress_cb_error))]
    ProgressCallbackError,
    /// An error occurred while trying to detect the bootloader.
    #[error("Failed to detect bootloader")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::detect_bootloader))]
    BootloaderDetectionFailed(#[source] ExecuteError),
    /// The device contains a bootloader that is not supported.
    #[error("Bootloader '{0}' not supported")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::unknown_bootloader))]
    BootloaderNotSupported(String),
    /// Failed to parse the firmware image as MCUboot firmware.
    #[error("Firmare is not a valid MCUboot image")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::mcuboot_image))]
    InvalidMcuBootFirmwareImage(#[from] mcuboot::ImageParseError),
    /// Fetching the image state returned an error.
    #[error("Failed to fetch image state from device")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::get_image_state))]
    GetStateFailed(#[source] ExecuteError),
    /// Uploading the firmware image returned an error.
    #[error("Failed to upload firmware image to device")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::image_upload))]
    ImageUploadFailed(#[from] ImageUploadError),
    /// Writing the new image state to the device failed
    #[error("Failed to activate new firmware image")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::set_image_state))]
    SetStateFailed(#[source] ExecuteError),
    /// Performing device reset failed
    #[error("Failed to trigger device reboot")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::reboot))]
    RebootFailed(#[source] ExecuteError),
    /// The given firmware is already installed on the device
    #[error("The device is already running the given firmware")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::already_installed))]
    AlreadyInstalled,
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
    /// Prevent firmware downgrades
    pub upgrade_only: bool,
    /// SHA-256 checksum of the image file
    pub checksum: Option<[u8; 32]>,
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

    let has_progress = progress.is_some();
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
        progress("Detect bootloader ...".into(), None)?;

        let bootloader_type = client
            .os_bootloader_info()
            .map_err(FirmwareUpdateError::BootloaderDetectionFailed)?
            .get_bootloader_type()
            .map_err(FirmwareUpdateError::BootloaderNotSupported)?;

        progress(format!("Found bootloader: {bootloader_type}").into(), None)?;

        bootloader_type
    };

    progress("Parse firmware image ...".into(), None)?;
    let image_id_hash = match bootloader_type {
        BootloaderType::McuBoot => {
            let info = mcuboot::get_image_info(std::io::Cursor::new(firmware))?;
            progress(format!("Image version: {}", info.version).into(), None)?;
            info.hash
        }
    };

    progress("Query device state ...".into(), None)?;
    let image_state = client
        .image_get_state()
        .map_err(FirmwareUpdateError::GetStateFailed)?;
    // Check if the firmware we want to install is already active on the device
    if image_state
        .iter()
        .any(|img| img.hash == Some(image_id_hash) && img.active)
    {
        return Err(FirmwareUpdateError::AlreadyInstalled);
    }

    progress("Upload new firmware ...".into(), None)?;
    let upload_progress_cb: Option<&mut dyn FnMut(u64, u64) -> bool> = if has_progress {
        Some(&mut |current, total| {
            if let Err(e) = progress("Upload new firmware ...".into(), Some((current, total))) {
                log::error!("{e:?}");
                false
            } else {
                true
            }
        })
    } else {
        None
    };
    client.image_upload(
        firmware,
        None,
        params.checksum,
        params.upgrade_only,
        upload_progress_cb,
    )?;

    progress("Activate new firmware ...".into(), None)?;
    let set_state_result = client.image_set_state(Some(image_id_hash), params.force_confirm);
    if let Err(set_state_error) = set_state_result {
        let mut image_already_active = false;

        // Special case: if the command isn't supported, we are most likely in
        // the MCUmgr recovery shell, which writes directly to the active slot
        // and does not support swapping.
        // Sanity check that the image is on the first position already to avoid false
        // positives of this exception.
        if bootloader_type == BootloaderType::McuBoot && set_state_error.command_not_supported() {
            progress("Query device state ...".into(), None)?;
            let image_state = client
                .image_get_state()
                .map_err(FirmwareUpdateError::GetStateFailed)?;
            if image_state
                .iter()
                .any(|img| img.image == 0 && img.slot == 0 && img.hash == Some(image_id_hash))
            {
                image_already_active = true;
            }
        }

        if !image_already_active {
            return Err(FirmwareUpdateError::SetStateFailed(set_state_error));
        }
    }

    progress("Trigger device reboot ...".into(), None)?;
    client
        .os_system_reset(false, None)
        .map_err(FirmwareUpdateError::RebootFailed)?;

    Ok(())
}
