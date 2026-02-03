use miette::Diagnostic;
use thiserror::Error;

use mcumgr_toolkit::{
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
    #[diagnostic(code(mcumgr_toolkit::cli::open_serial_failed))]
    OpenSerialFailed(#[source] serialport::Error),
    #[error("No backend selected")]
    #[diagnostic(code(mcumgr_toolkit::cli::no_backend))]
    NoBackendSelected,
    // #[error("Setting the timeout failed")]
    // #[diagnostic(code(mcumgr_toolkit::cli::set_timeout_failed))]
    // SetTimeoutFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync + 'static>),
    #[error("Command execution failed")]
    #[diagnostic(code(mcumgr_toolkit::cli::execution_failed))]
    CommandExecutionFailed(#[from] ExecuteError),
    #[error("Json encode failed")]
    #[diagnostic(code(mcumgr_toolkit::cli::json_encode))]
    JsonEncodeError(#[source] serde_json::Error),
    #[error("Shell command returned error exit code: {}", Errno::errno_to_string(*.0))]
    #[diagnostic(code(mcumgr_toolkit::cli::shell_exit_code))]
    ShellExitCode(i32),
    #[error("Failed to read the input data")]
    #[diagnostic(code(mcumgr_toolkit::cli::input))]
    InputReadFailed(#[source] std::io::Error),
    #[error("Failed to write the output data")]
    #[diagnostic(code(mcumgr_toolkit::cli::output))]
    OutputWriteFailed(#[source] std::io::Error),
    #[error("Unable to determine output file name")]
    #[diagnostic(code(mcumgr_toolkit::cli::destination_unknown))]
    DestinationFilenameUnknown,
    #[error("File upload failed")]
    #[diagnostic(code(mcumgr_toolkit::cli::file_upload))]
    FileUploadFailed(#[from] FileUploadError),
    #[error("File download failed")]
    #[diagnostic(code(mcumgr_toolkit::cli::file_download))]
    FileDownloadFailed(#[from] FileDownloadError),
    #[error("Image upload failed")]
    #[diagnostic(code(mcumgr_toolkit::cli::image_upload))]
    ImageUploadFailed(#[from] ImageUploadError),
    #[error("Failed to parse datetime string")]
    #[diagnostic(code(mcumgr_toolkit::cli::chrono_parse))]
    ChronoParseFailed(#[from] chrono::ParseError),
    #[error("Failed to open USB serial port")]
    #[diagnostic(code(mcumgr_toolkit::cli::usb_serial))]
    UsbSerialOpenFailed(#[from] UsbSerialError),
    #[error("Failed to parse MCUboot image")]
    #[diagnostic(code(mcumgr_toolkit::cli::image_parse))]
    ImageParseFailed(#[from] ImageParseError),
    #[error("Firmware update failed")]
    #[diagnostic(code(mcumgr_toolkit::cli::firmware_update))]
    FirmwareUpdateFailed(#[from] FirmwareUpdateError),
}
