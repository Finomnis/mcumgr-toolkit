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
    #[error("Failed to list serial ports")]
    #[diagnostic(code(mcumgrctl::list_serial_ports_failed))]
    ListSerialPortsFailed(#[source] serialport::Error),
    #[error("Failed to open serial port")]
    #[diagnostic(code(mcumgrctl::open_serial_failed))]
    OpenSerialFailed(#[source] serialport::Error),
    #[error("No backend selected")]
    #[diagnostic(code(mcumgrctl::no_backend))]
    NoBackendSelected,
    // #[error("Setting the timeout failed")]
    // #[diagnostic(code(mcumgrctl::set_timeout_failed))]
    // SetTimeoutFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync + 'static>),
    #[error("Command execution failed")]
    #[diagnostic(code(mcumgrctl::execution_failed))]
    CommandExecutionFailed(#[from] ExecuteError),
    #[error("Json encode failed")]
    #[diagnostic(code(mcumgrctl::json_encode))]
    JsonEncodeError(#[source] serde_json::Error),
    #[error("Shell command returned error exit code: {}", Errno::errno_to_string(*.0))]
    #[diagnostic(code(mcumgrctl::shell_exit_code))]
    ShellExitCode(i32),
    #[error("Failed to read the input data")]
    #[diagnostic(code(mcumgrctl::input))]
    InputReadFailed(#[source] std::io::Error),
    #[error("Failed to write the output data")]
    #[diagnostic(code(mcumgrctl::output))]
    OutputWriteFailed(#[source] std::io::Error),
    #[error("Unable to determine output file name")]
    #[diagnostic(code(mcumgrctl::destination_unknown))]
    DestinationFilenameUnknown,
    #[error("File upload failed")]
    #[diagnostic(code(mcumgrctl::file_upload))]
    FileUploadFailed(#[from] FileUploadError),
    #[error("File download failed")]
    #[diagnostic(code(mcumgrctl::file_download))]
    FileDownloadFailed(#[from] FileDownloadError),
    #[error("Image upload failed")]
    #[diagnostic(code(mcumgrctl::image_upload))]
    ImageUploadFailed(#[from] ImageUploadError),
    #[error("Failed to parse datetime string")]
    #[diagnostic(code(mcumgrctl::chrono_parse))]
    ChronoParseFailed(#[from] chrono::ParseError),
    #[error("Failed to open USB serial port")]
    #[diagnostic(code(mcumgrctl::usb_serial))]
    UsbSerialOpenFailed(#[from] UsbSerialError),
    #[error("Failed to parse MCUboot image")]
    #[diagnostic(code(mcumgrctl::image_parse))]
    ImageParseFailed(#[from] ImageParseError),
    #[error("Firmware update failed")]
    #[diagnostic(code(mcumgrctl::firmware_update))]
    FirmwareUpdateFailed(#[from] FirmwareUpdateError),
}
