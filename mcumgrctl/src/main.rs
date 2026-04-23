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
use mcumgr_toolkit::{MCUmgrClient, client::UsbSerialError};

const DEFAULT_UDP_PORT: u16 = 1337;

/// Append the default SMP UDP port when the user did not specify one.
///
/// Detects a port by looking for a valid u16 after the last colon,
/// or a closing bracket (bracketed IPv6 form `[::1]:port`).
/// Bare IPv6 addresses without brackets (e.g. `::1`) should be written
/// as `[::1]` or `[::1]:port`.
fn with_default_udp_port(host: &str) -> String {
    let colon_count = host.matches(':').count();
    let has_port = if host.starts_with('[') {
        host.contains("]:")
    } else {
        // Only treat the last segment as a port for hostname:port or ipv4:port
        // (exactly one colon).  Bare IPv6 addresses have more than one colon and
        // must not be mistaken for having a port.
        colon_count == 1
            && host
                .rfind(':')
                .map_or(false, |i| host[i + 1..].parse::<u16>().is_ok())
    };

    if has_port {
        host.to_owned()
    } else if !host.starts_with('[') && colon_count > 1 {
        // Bare IPv6 address — wrap in brackets before appending the default port.
        format!("[{host}]:{DEFAULT_UDP_PORT}")
    } else {
        format!("{host}:{DEFAULT_UDP_PORT}")
    }
}

use crate::errors::CliError;

fn cli_main(multiprogress: &MultiProgress) -> Result<(), CliError> {
    let args = args::App::parse();

    let client = if let Some(serial_name) = args.serial {
        if serial_name.is_empty() {
            let ports = serialport::available_ports()
                .map_err(CliError::ListSerialPortsFailed)?
                .into_iter()
                .map(|port| port.port_name)
                .collect::<Vec<_>>();
            if args.common.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&ports).map_err(CliError::JsonEncodeError)?
                );
            } else {
                println!();
                if ports.is_empty() {
                    println!("No serial ports available.");
                } else {
                    println!("Available serial ports:");
                    println!();
                    for port in ports {
                        println!(" - {port}");
                    }
                }
                println!();
            }
            return Ok(());
        }

        let serial = serialport::new(serial_name, args.baud)
            .timeout(Duration::from_millis(args.common.timeout))
            .open()
            .map_err(CliError::OpenSerialFailed)?;
        Client::new(MCUmgrClient::new_from_serial(serial))
    } else if let Some(identifier) = args.usb_serial {
        let result = MCUmgrClient::new_from_usb_serial(
            identifier,
            args.baud,
            Duration::from_millis(args.common.timeout),
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
            return Ok(());
        }

        Client::new(result?)
    } else if let Some(host) = args.udp {
        Client::new(
            MCUmgrClient::new_from_udp(
                with_default_udp_port(&host).as_str(),
                Duration::from_millis(args.common.timeout),
            )
            .map_err(CliError::OpenUdpFailed)?,
        )
    } else {
        Client::default()
    };

    if let Ok(client) = client.get() {
        client.set_retries(args.common.retries);

        if let Err(e) = client.use_auto_frame_size() {
            let mut lowest_err: &dyn std::error::Error = &e;
            while let Some(lower_err) = lowest_err.source() {
                lowest_err = lower_err;
            }
            log::warn!("Failed to read SMP frame size from device, using slow default");
            log::warn!("Reason: {lowest_err}");
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
