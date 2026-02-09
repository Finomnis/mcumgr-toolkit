use std::fmt::Display;

use miette::Diagnostic;
use thiserror::Error;

use crate::{MCUmgrClient, bootloader::BootloaderType, client::MCUmgrClientError, mcuboot};

/// Possible error values of [`MCUmgrClient::firmware_update`].
#[derive(Error, Debug, Diagnostic)]
pub enum FirmwareUpdateError {
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::progress_cb_error))]
    ProgressCallbackError,
    /// An error occurred while trying to detect the bootloader.
    #[error("Failed to detect bootloader")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::detect_bootloader))]
    #[diagnostic(help("try to specify the bootloader type manually"))]
    BootloaderDetectionFailed(#[source] MCUmgrClientError),
    /// The device contains a bootloader that is not supported.
    #[error("Bootloader '{0}' not supported")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::unknown_bootloader))]
    BootloaderNotSupported(String),
    /// Failed to parse the firmware image as MCUboot firmware.
    #[error("Firmware is not a valid MCUboot image")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::mcuboot_image))]
    InvalidMcuBootFirmwareImage(#[from] mcuboot::ImageParseError),
    /// Fetching the image state returned an error.
    #[error("Failed to fetch image state from device")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::get_image_state))]
    GetStateFailed(#[source] MCUmgrClientError),
    /// Uploading the firmware image returned an error.
    #[error("Failed to upload firmware image to device")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::image_upload))]
    ImageUploadFailed(#[source] MCUmgrClientError),
    /// Writing the new image state to the device failed
    #[error("Failed to activate new firmware image")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::set_image_state))]
    SetStateFailed(#[source] MCUmgrClientError),
    /// Performing device reset failed
    #[error("Failed to trigger device reboot")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::reboot))]
    RebootFailed(#[source] MCUmgrClientError),
    /// The given firmware is already installed on the device
    #[error("The device is already running the given firmware")]
    #[diagnostic(code(mcumgr_toolkit::firmware_update::already_installed))]
    AlreadyInstalled,
}

/// Configurable parameters for [`MCUmgrClient::firmware_update`].
#[derive(Clone, Debug, Default)]
pub struct FirmwareUpdateParams {
    /// Default: `None`
    ///
    /// The bootloader type.
    /// Auto-detect bootloader if `None`.
    pub bootloader_type: Option<BootloaderType>,
    /// Default: `false`
    ///
    /// Do not reboot device after the update.
    pub skip_reboot: bool,
    /// Default: `false`
    ///
    /// Skip test boot and confirm directly.
    pub force_confirm: bool,
    /// Default: `false`
    ///
    /// Prevent firmware downgrades.
    pub upgrade_only: bool,
}

/// The step of the firmware update that is currently being performed
#[derive(Clone, Debug)]
pub enum FirmwareUpdateStep {
    /// Querying which bootloader the device is running
    DetectingBootloader,
    /// The bootloader was found
    BootloaderFound(BootloaderType),
    /// Extracting meta information from the new firmware image
    ParsingFirmwareImage,
    /// Querying the current firmware state of the device
    QueryingDeviceState,
    /// A summary of what update exactly we will perform now
    UpdateInfo {
        /// The current version with the current ID hash, if available
        current_version: Option<(String, Option<[u8; 32]>)>,
        /// The new version with the new ID hash
        new_version: (String, [u8; 32]),
    },
    /// Uploading the new firmware to the device
    UploadingFirmware,
    /// Marking the new firmware to be swapped to active during next boot
    ActivatingFirmware,
    /// Triggering a system reboot so that the bootloader switches to the new image
    TriggeringReboot,
}

impl Display for FirmwareUpdateStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DetectingBootloader => f.write_str("Detecting bootloader ..."),
            Self::BootloaderFound(bootloader_type) => {
                write!(f, "Found bootloader: {bootloader_type}")
            }
            Self::ParsingFirmwareImage => f.write_str("Parsing firmware image ..."),
            Self::QueryingDeviceState => f.write_str("Querying device state ..."),
            Self::UpdateInfo {
                current_version,
                new_version,
            } => {
                f.write_str("Update: ")?;

                if let Some((version_str, version_hash)) = &current_version {
                    f.write_str(version_str)?;

                    if let Some(version_hash) = version_hash {
                        write!(f, "-{}", hex::encode(&version_hash[..SHOWN_HASH_DIGITS]))?;
                    }
                } else {
                    f.write_str("Empty")?;
                };

                write!(
                    f,
                    " -> {}-{}",
                    new_version.0,
                    hex::encode(&new_version.1[..SHOWN_HASH_DIGITS])
                )
            }
            Self::UploadingFirmware => f.write_str("Uploading new firmware ..."),
            Self::ActivatingFirmware => f.write_str("Activating new firmware ..."),
            Self::TriggeringReboot => f.write_str("Triggering device reboot ..."),
        }
    }
}

/// The progress callback type of [`MCUmgrClient::firmware_update`].
///
/// # Arguments
///
/// * `FirmwareUpdateStep` - The current step that is being executed
/// * `Option<(u64, u64)>` - The (current, total) progress of the current step, if available.
///
/// # Return
///
/// `false` on error; this will cancel the update
///
pub type FirmwareUpdateProgressCallback<'a> =
    dyn FnMut(FirmwareUpdateStep, Option<(u64, u64)>) -> bool + 'a;

const SHOWN_HASH_DIGITS: usize = 4;

/// High-level firmware update routine
///
/// # Arguments
///
/// * `client` - The MCUmgr client.
/// * `firmware` - The firmware image data.
/// * `checksum` - SHA256 of the firmware image. Optional.
/// * `params` - Configurable parameters.
/// * `progress` - A callback that receives progress updates.
///
pub(crate) fn firmware_update(
    client: &MCUmgrClient,
    firmware: impl AsRef<[u8]>,
    checksum: Option<[u8; 32]>,
    params: FirmwareUpdateParams,
    mut progress: Option<&mut FirmwareUpdateProgressCallback>,
) -> Result<(), FirmwareUpdateError> {
    // Might become a params member in the future
    let target_image: Option<u32> = Default::default();
    let actual_target_image = target_image.unwrap_or(0);

    let firmware = firmware.as_ref();

    let has_progress = progress.is_some();
    let mut progress = |state: FirmwareUpdateStep, prog| {
        if let Some(progress) = &mut progress {
            if !progress(state, prog) {
                return Err(FirmwareUpdateError::ProgressCallbackError);
            }
        }
        Ok(())
    };

    let bootloader_type = if let Some(bootloader_type) = params.bootloader_type {
        bootloader_type
    } else {
        progress(FirmwareUpdateStep::DetectingBootloader, None)?;

        let bootloader_type = client
            .os_bootloader_info()
            .map_err(FirmwareUpdateError::BootloaderDetectionFailed)?
            .get_bootloader_type()
            .map_err(FirmwareUpdateError::BootloaderNotSupported)?;

        progress(FirmwareUpdateStep::BootloaderFound(bootloader_type), None)?;

        bootloader_type
    };

    progress(FirmwareUpdateStep::ParsingFirmwareImage, None)?;
    let (image_version, image_id_hash) = match bootloader_type {
        BootloaderType::MCUboot => {
            let info = mcuboot::get_image_info(std::io::Cursor::new(firmware))?;
            (info.version, info.hash)
        }
    };

    progress(FirmwareUpdateStep::QueryingDeviceState, None)?;
    let image_state = client
        .image_get_state()
        .map_err(FirmwareUpdateError::GetStateFailed)?;

    let active_image = image_state
        .iter()
        .find(|img| img.image == actual_target_image && img.active)
        .or_else(|| {
            image_state
                .iter()
                .find(|img| img.image == actual_target_image && img.slot == 0)
        });

    progress(
        FirmwareUpdateStep::UpdateInfo {
            current_version: active_image.map(|img| (img.version.clone(), img.hash)),
            new_version: (image_version.to_string(), image_id_hash),
        },
        None,
    )?;

    if active_image.and_then(|img| img.hash) == Some(image_id_hash) {
        return Err(FirmwareUpdateError::AlreadyInstalled);
    }

    progress(FirmwareUpdateStep::UploadingFirmware, None)?;
    let mut upload_progress_cb = |current, total| {
        progress(
            FirmwareUpdateStep::UploadingFirmware,
            Some((current, total)),
        )
        .is_ok()
    };

    client
        .image_upload(
            firmware,
            target_image,
            checksum,
            params.upgrade_only,
            has_progress.then_some(&mut upload_progress_cb),
        )
        .map_err(|err| {
            if let MCUmgrClientError::ProgressCallbackError = err {
                // Users expect this error when the progress callback errors
                FirmwareUpdateError::ProgressCallbackError
            } else {
                FirmwareUpdateError::ImageUploadFailed(err)
            }
        })?;

    progress(FirmwareUpdateStep::ActivatingFirmware, None)?;
    let set_state_result = client.image_set_state(Some(image_id_hash), params.force_confirm);
    if let Err(set_state_error) = set_state_result {
        let mut image_already_active = false;

        // Special case: if the command isn't supported, we are most likely in
        // the MCUmgr recovery shell, which writes directly to the active slot
        // and does not support swapping.
        // Sanity check that the image is on the first position already to avoid false
        // positives of this exception.
        if bootloader_type == BootloaderType::MCUboot && set_state_error.command_not_supported() {
            progress(FirmwareUpdateStep::QueryingDeviceState, None)?;
            let image_state = client
                .image_get_state()
                .map_err(FirmwareUpdateError::GetStateFailed)?;
            if image_state.iter().any(|img| {
                img.image == actual_target_image && img.slot == 0 && img.hash == Some(image_id_hash)
            }) {
                image_already_active = true;
            }
        }

        if !image_already_active {
            return Err(FirmwareUpdateError::SetStateFailed(set_state_error));
        }
    }

    if !params.skip_reboot {
        progress(FirmwareUpdateStep::TriggeringReboot, None)?;
        client
            .os_system_reset(false, None)
            .map_err(FirmwareUpdateError::RebootFailed)?;
    }

    Ok(())
}
