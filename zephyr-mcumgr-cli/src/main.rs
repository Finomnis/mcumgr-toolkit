#![forbid(unsafe_code)]

mod args;

use std::time::Duration;

use clap::Parser;
use miette::Diagnostic;
use thiserror::Error;
use zephyr_mcumgr::{MCUmgrClient, connection::ExecuteError};

/// Possible CLI errors.
#[derive(Error, Debug, Diagnostic)]
pub enum CliError {
    #[error("Failed to open serial port")]
    #[diagnostic(code(zephyr_mcumgr::cli::open_serial_failed))]
    OpenSerialFailed(#[source] serialport::Error),
    #[error("No backend selected")]
    #[diagnostic(code(zephyr_mcumgr::cli::no_backend))]
    NoBackendSelected,
    #[error("Command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::cli::execution_failed))]
    CommandExecutionFailed(#[from] ExecuteError),
}

fn cli_main() -> Result<(), CliError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = args::App::parse();

    let mut client;
    if let Some(serial_name) = args.serial {
        let serial = serialport::new(serial_name, args.baud)
            .timeout(Duration::from_millis(args.timeout))
            .open()
            .map_err(CliError::OpenSerialFailed)?;

        client = MCUmgrClient::new_from_serial(serial);
        if client.use_auto_frame_size().is_err() {
            log::warn!("Failed to read SMP frame size from device, using default! (might be slow)");
        }
    } else {
        return Err(CliError::NoBackendSelected);
    }

    match args.group {
        args::Group::Os { command } => match command {
            args::OsCommand::Echo { msg } => println!(
                "{}",
                client
                    .os_echo(msg)
                    .map_err(CliError::CommandExecutionFailed)?
            ),
        },
        args::Group::Fs { command } => match command {},
    }

    Ok(())
}

fn main() -> miette::Result<()> {
    cli_main().map_err(Into::into)
}
