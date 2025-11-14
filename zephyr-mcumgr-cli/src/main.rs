#![forbid(unsafe_code)]

mod args;
use args::Group;

mod progress;
use progress::with_progress_bar;

mod file_read_write;
use file_read_write::{read_input_file, write_output_file};

mod formatting;
mod raw_command;

use std::time::Duration;

use clap::Parser;
use miette::Diagnostic;
use thiserror::Error;
use zephyr_mcumgr::{
    Errno, MCUmgrClient,
    client::{FileDownloadError, FileUploadError},
    connection::ExecuteError,
};

use crate::formatting::structured_print;

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
    SetTimeoutFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync + 'static>),
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
    #[error("File upload failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::file_upload))]
    FileUploadFailed(#[from] FileUploadError),
    #[error("File download failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::file_download))]
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
        .map_err(|e| CliError::SetTimeoutFailed(e.into()))?;

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
            args::FsCommand::Download { remote, local } => {
                let mut data = vec![];
                with_progress_bar(args.progress, Some(&remote), |progress| {
                    client.fs_file_download(remote.as_str(), &mut data, progress)
                })?;
                write_output_file(&local, &data)?;
            }
            args::FsCommand::Upload { local, remote } => {
                let data = read_input_file(&local)?;
                with_progress_bar(args.progress, Some(&remote), |progress| {
                    client.fs_file_upload(remote.as_str(), &*data, data.len() as u64, progress)
                })?;
            }
            args::FsCommand::Status { name } => {
                let status = client.fs_file_status(&name)?;
                structured_print(Some(name), args.json, |s| {
                    s.key_value("length", status.len);
                })?;
            }
            args::FsCommand::Checksum {
                name,
                algo,
                offset,
                length,
            } => {
                let checksum = client.fs_file_checksum(&name, algo, offset, length)?;

                if args.json || args.verbose {
                    structured_print(Some(name), args.json, |s| {
                        s.key_value("checksum", checksum.output.hex());
                        s.key_value("type", checksum.r#type);
                        s.key_value("data offset", checksum.off);
                        s.key_value("data length", checksum.len);
                    })?;
                } else {
                    println!("{}  {}", checksum.output.hex(), name);
                }
            }
            args::FsCommand::SupportedChecksums => {
                let checksums = client.fs_supported_checksum_types()?;

                if args.json || args.verbose {
                    structured_print(None, args.json, |s| {
                        for (algo, properties) in checksums {
                            s.sublist(algo, |s| {
                                s.key_value("format", properties.format.to_string());
                                s.key_value("size", properties.size);
                            });
                        }
                    })?;
                } else {
                    println!(
                        "{}",
                        checksums.keys().cloned().collect::<Vec<_>>().join(",")
                    );
                }
            }
            args::FsCommand::Close => client.fs_file_close()?,
        },
        Group::Shell { argv } => {
            let (returncode, output) = client.shell_execute(&argv)?;
            println!("{output}");
            if returncode < 0 {
                return Err(CliError::ShellExitCode(returncode));
            } else if returncode > 0 {
                println!();
                println!("Exit code: {returncode}")
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
