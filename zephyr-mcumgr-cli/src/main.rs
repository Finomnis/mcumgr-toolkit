#![forbid(unsafe_code)]

mod args;
mod errors;
mod file_read_write;
mod groups;
mod progress;

mod formatting;

use std::time::Duration;

use clap::Parser;
use zephyr_mcumgr::{MCUmgrClient, client::UsbSerialError};

use crate::errors::CliError;

fn cli_main() -> Result<(), CliError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = args::App::parse();

    let client = if let Some(serial_name) = args.serial {
        let serial = serialport::new(serial_name, args.baud)
            .open()
            .map_err(CliError::OpenSerialFailed)?;
        MCUmgrClient::new_from_serial(serial)
    } else if let Some(identifier) = args.usb_serial {
        let result = MCUmgrClient::new_from_usb_serial(identifier);

        if let Err(UsbSerialError::IdentifierEmpty { ports }) = &result {
            println!();
            if ports.0.is_empty() {
                println!("No USB serial ports available.");
            } else {
                println!("Available USB serial ports:");
                println!("{}", ports);
            }
            println!();
            std::process::exit(1);
        }

        result?
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

    groups::run(&client, args.common, args.group)
}

fn main() -> miette::Result<()> {
    cli_main().map_err(Into::into)
}
