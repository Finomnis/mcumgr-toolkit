use miette::Diagnostic;
use thiserror::Error;

use crate::{MCUmgrClient, bootloader::BootloaderType};

/// Possible errors that can happen during firmware update.
#[derive(Error, Debug, Diagnostic)]
pub enum FirmwareUpdateError {
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(zephyr_mcumgr::firmware_update::progress_cb_error))]
    ProgressCallbackError,
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
    params: FirmwareUpdateParams,
    progress: Option<&mut FirmwareUpdateProgressCallback>,
) -> Result<(), FirmwareUpdateError> {
    Ok(())
}
