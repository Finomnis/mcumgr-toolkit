use miette::Diagnostic;
use thiserror::Error;

/// Possible Python library errors.
#[derive(Error, Debug, Diagnostic)]
pub enum McubootPythonError {
    #[error("Failed to find bootloader type '{1}'")]
    #[diagnostic(code(zephyr_mcumgr::python::parse_bootloader_type))]
    InvalidBootloaderString(#[source] strum::ParseError, String),
}
