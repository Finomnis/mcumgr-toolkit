use miette::Diagnostic;
use thiserror::Error;

use zephyr_mcumgr::{
    Errno,
    client::{
        FileDownloadError, FileUploadError, FirmwareUpdateError, ImageUploadError, UsbSerialError,
    },
    connection::ExecuteError,
    mcuboot::ImageParseError,
};

/// Possible CLI errors.
#[derive(Error, Debug, Diagnostic)]
pub enum CliError {
    #[error("Failed to open serial port")]
    #[diagnostic(code(zephyr_mcumgr::cli::open_serial_failed))]
    OpenSerialFailed(#[source] serialport::Error),
    #[error("No backend selected")]
    #[diagnostic(code(zephyr_mcumgr::cli::no_backend))]
    NoBackendSelected,
    // #[error("Setting the timeout failed")]
    // #[diagnostic(code(zephyr_mcumgr::cli::set_timeout_failed))]
    // SetTimeoutFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync + 'static>),
    #[error("Command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::execution_failed))]
    CommandExecutionFailed(#[from] ExecuteError),
    #[error("Json encode failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::json_encode))]
    JsonEncodeError(#[source] serde_json::Error),
    #[error("Shell command returned error exit code: {}", Errno::errno_to_string(*.0))]
    #[diagnostic(code(zephyr_mcumgr::cli::shell_exit_code))]
    ShellExitCode(i32),
    #[error("Failed to read the input data")]
    #[diagnostic(code(zephyr_mcumgr::cli::input))]
    InputReadFailed(#[source] std::io::Error),
    #[error("Failed to write the output data")]
    #[diagnostic(code(zephyr_mcumgr::cli::output))]
    OutputWriteFailed(#[source] std::io::Error),
    #[error("Unable to determine output file name")]
    #[diagnostic(code(zephyr_mcumgr::cli::destination_unknown))]
    DestinationFilenameUnknown,
    #[error("File upload failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::file_upload))]
    FileUploadFailed(#[from] FileUploadError),
    #[error("File download failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::file_download))]
    FileDownloadFailed(#[from] FileDownloadError),
    #[error("Image upload failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::image_upload))]
    ImageUploadFailed(#[from] ImageUploadError),
    #[error("Failed to parse datetime string")]
    #[diagnostic(code(zephyr_mcumgr::cli::chrono_parse))]
    ChronoParseFailed(#[from] chrono::ParseError),
    #[error("Failed to open USB serial port")]
    #[diagnostic(code(zephyr_mcumgr::cli::usb_serial))]
    UsbSerialOpenFailed(#[from] UsbSerialError),
    #[error("Failed to parse MCUboot image")]
    #[diagnostic(code(zephyr_mcumgr::cli::image_parse))]
    ImageParseFailed(#[from] ImageParseError),
    #[error("Firmware update failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::firmware_update))]
    FirmwareUpdateFailed(#[from] FirmwareUpdateError),
}
