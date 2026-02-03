#![forbid(unsafe_code)]

mod args;
mod client;
mod errors;
mod file_read_write;
mod formatting;
mod groups;
mod progress;

use client::Client;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;

use std::time::Duration;

use clap::Parser;
use zephyr_mcumgr::{MCUmgrClient, client::UsbSerialError};

use crate::errors::CliError;

fn cli_main(multiprogress: &MultiProgress) -> Result<(), CliError> {
    let args = args::App::parse();

    let client = if let Some(serial_name) = args.serial {
        let serial = serialport::new(serial_name, args.baud)
            .timeout(Duration::from_millis(args.timeout))
            .open()
            .map_err(CliError::OpenSerialFailed)?;
        Client::new(MCUmgrClient::new_from_serial(serial))
    } else if let Some(identifier) = args.usb_serial {
        let result = MCUmgrClient::new_from_usb_serial(
            identifier,
            args.baud,
            Duration::from_millis(args.timeout),
        );

        if let Err(UsbSerialError::IdentifierEmpty { ports }) = &result {
            if args.common.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(ports).map_err(CliError::JsonEncodeError)?
                );
            } else {
                println!();
                if ports.0.is_empty() {
                    println!("No USB serial ports available.");
                } else {
                    println!("Available USB serial ports:");
                    println!("{}", ports);
                }
                println!();
            }
            std::process::exit(0);
        }

        Client::new(result?)
    } else {
        Client::default()
    };

    if let Ok(client) = client.get() {
        if let Err(e) = client.use_auto_frame_size() {
            log::warn!("Failed to read SMP frame size from device, using slow default");
            log::warn!("Reason: {e}");
            log::warn!("Hint: Make sure that `CONFIG_MCUMGR_GRP_OS_MCUMGR_PARAMS` is enabled.");
        }
    }

    if let Some(group) = args.group {
        groups::run(&client, multiprogress, args.common, group)?;
    } else {
        client.get()?.check_connection()?;
        println!("Device alive and responsive.");
    }

    Ok(())
}

fn main() -> miette::Result<()> {
    let multiprogress = {
        let logger =
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .build();
        let level = logger.filter();
        let multiprogress = MultiProgress::new();
        LogWrapper::new(multiprogress.clone(), logger)
            .try_init()
            .unwrap();
        log::set_max_level(level);

        multiprogress
    };

    let result = cli_main(&multiprogress).map_err(Into::into);

    multiprogress.clear().ok();

    result
}
