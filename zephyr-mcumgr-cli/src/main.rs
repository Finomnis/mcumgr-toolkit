#![forbid(unsafe_code)]

mod args;
use args::Group;

mod progress;

mod raw_command;

use std::{
    fs::File,
    io::{Read, Write},
    time::Duration,
};

use clap::Parser;
use miette::Diagnostic;
use thiserror::Error;
use zephyr_mcumgr::{
    MCUmgrClient,
    client::{FileDownloadError, FileUploadError},
    connection::ExecuteError,
};

use crate::progress::with_progress_bar;

/// Possible CLI errors.
#[derive(Error, Debug, Diagnostic)]
pub enum CliError {
    #[error("Failed to open serial port")]
    #[diagnostic(code(zephyr_mcumgr::cli::open_serial_failed))]
    OpenSerialFailed(#[source] serialport::Error),
    #[error("No backend selected")]
    #[diagnostic(code(zephyr_mcumgr::cli::no_backend))]
    NoBackendSelected,
    #[error("Setting the timeout failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::set_timeout_failed))]
    SetTimeoutFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("Command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::execution_failed))]
    CommandExecutionFailed(#[from] ExecuteError),
    #[error("Json encode failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::json_encode))]
    JsonEncodeError(#[source] serde_json::Error),
    #[error("Shell command returned exit code '{0}'")]
    #[diagnostic(code(zephyr_mcumgr::cli::shell_exit_code))]
    ShellExitCode(i32),
    #[error("Failed to read the input data")]
    #[diagnostic(code(zephyr_mcumgr::cli::input))]
    InputReadFailed(#[source] std::io::Error),
    #[error("Failed to write the output data")]
    #[diagnostic(code(zephyr_mcumgr::cli::output))]
    OutputWriteFailed(#[source] std::io::Error),
    #[error("File upload failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::file_upload_failed))]
    FileUploadFailed(#[from] FileUploadError),
    #[error("File download failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::file_download_failed))]
    FileDownloadFailed(#[from] FileDownloadError),
}

fn cli_main() -> Result<(), CliError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = args::App::parse();

    let mut client = if let Some(serial_name) = args.serial {
        let serial = serialport::new(serial_name, args.baud)
            .open()
            .map_err(CliError::OpenSerialFailed)?;
        MCUmgrClient::new_from_serial(serial)
    } else {
        return Err(CliError::NoBackendSelected);
    };

    client
        .set_timeout(Duration::from_millis(args.timeout))
        .map_err(CliError::SetTimeoutFailed)?;

    if let Err(e) = client.use_auto_frame_size() {
        log::warn!("Failed to read SMP frame size from device, using slow default");
        log::warn!("Reason: {e}");
        log::warn!("Hint: Make sure that `CONFIG_MCUMGR_GRP_OS_MCUMGR_PARAMS` is enabled.");
    }

    match args.group {
        Group::Os { command } => match command {
            args::OsCommand::Echo { msg } => println!(
                "{}",
                client
                    .os_echo(msg)
                    .map_err(CliError::CommandExecutionFailed)?
            ),
        },
        Group::Fs { command } => match command {
            args::FsCommand::FileUpload { local, remote } => {
                let data = if local == "-" {
                    let mut data = Vec::new();
                    std::io::stdin()
                        .lock()
                        .read_to_end(&mut data)
                        .map_err(CliError::InputReadFailed)?;
                    data
                } else {
                    let mut f = File::open(local).map_err(CliError::InputReadFailed)?;
                    let mut data = vec![];
                    f.read_to_end(&mut data)
                        .map_err(CliError::InputReadFailed)?;
                    data
                };

                with_progress_bar(
                    args.progress,
                    Some(data.len() as u64),
                    Some(remote.clone()),
                    |progress| {
                        client.fs_file_upload(remote, data.as_slice(), data.len() as u64, progress)
                    },
                )?;
            }
            args::FsCommand::FileDownload { remote, local } => {
                let mut data = vec![];
                with_progress_bar(args.progress, None, Some(remote.clone()), |progress| {
                    client.fs_file_download(remote, &mut data, progress)
                })?;
                if local == "-" {
                    std::io::stdout()
                        .lock()
                        .write_all(&data)
                        .map_err(CliError::OutputWriteFailed)?;
                } else {
                    File::create(local)
                        .map_err(CliError::OutputWriteFailed)?
                        .write_all(&data)
                        .map_err(CliError::OutputWriteFailed)?;
                }
            }
        },
        Group::Shell { argv } => {
            let (returncode, output) = client.shell_execute(&argv)?;
            println!("{output}");
            if returncode != 0 {
                return Err(CliError::ShellExitCode(returncode));
            }
        }
        Group::Raw(command) => {
            let response = client.raw_command(&command)?;
            let json_response =
                serde_json::to_string_pretty(&response).map_err(CliError::JsonEncodeError)?;
            println!("{json_response}")
        }
    }

    Ok(())
}

fn main() -> miette::Result<()> {
    cli_main().map_err(Into::into)
}
