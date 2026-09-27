#![forbid(unsafe_code)]

mod args;
mod client;
mod errors;
mod file_read_write;
mod formatting;
mod groups;
mod progress;

use client::Client;
use indicatif::{MultiProgress, ProgressBar};
use indicatif_log_bridge::LogWrapper;

use std::time::Duration;

use clap::{CommandFactory as _, Parser};
use mcumgr_toolkit::{
    MCUmgrClient,
    client::{BleError, UsbSerialError},
};

/// Re-export for convenience
pub use mcumgr_toolkit::transport::Transport;

use crate::errors::CliError;

fn cli_main_internal<T: clap::Args>(
    multiprogress: &MultiProgress,
    custom_transports: impl FnOnce(
        &T,
    ) -> Result<
        Option<Box<dyn Transport + Send>>,
        Box<dyn std::error::Error + Send + Sync>,
    >,
) -> Result<(), CliError> {
    let args = args::App::<T>::parse();

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
    } else if let Some(ble_identifier) = args.ble {
        let mut scan_spinner = None;

        let result = MCUmgrClient::new_from_ble_with_scan_callback(
            ble_identifier,
            Duration::from_millis(args.common.timeout),
            || {
                if !(args.common.quiet || args.common.json) {
                    let scan_spinner =
                        scan_spinner.insert(multiprogress.add(ProgressBar::new_spinner()));
                    scan_spinner.set_message("Scanning ...");
                    scan_spinner.enable_steady_tick(Duration::from_millis(100));
                }
            },
        );

        if let Some(scan_spinner) = scan_spinner {
            scan_spinner.finish_and_clear();
            multiprogress.remove(&scan_spinner);
        }

        if let Err(BleError::IdentifierEmpty { devices }) = &result {
            if args.common.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(devices).map_err(CliError::JsonEncodeError)?
                );
            } else {
                println!();
                if devices.0.is_empty() {
                    println!("No BLE MCUmgr devices available.");
                } else {
                    println!("Available BLE MCUmgr devices:");
                    println!("{}", devices);
                }
                println!();
            }
            return Ok(());
        }

        Client::new(result?)
    } else if let Some(addr) = args.udp {
        Client::new(
            MCUmgrClient::new_from_udp(addr, Duration::from_millis(args.common.timeout))
                .map_err(CliError::UdpOpenFailed)?,
        )
    } else if let Some(custom_transport) =
        custom_transports(&args.custom_transports).map_err(CliError::CustomTransportError)?
    {
        Client::new(MCUmgrClient::new_from_transport(custom_transport))
    } else {
        Client::default()
    };

    if let Ok(client) = client.get() {
        client.set_retries(args.common.retries);

        if let Some(smp_frame_size) = args.common.smp_frame_size {
            client.set_frame_size(smp_frame_size);
        } else {
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
    }

    if let Some(group) = args.group {
        groups::run(&client, multiprogress, args.common, group)?;
    } else {
        client.get()?.check_connection()?;
        println!("Device alive and responsive.");
    }

    Ok(())
}

/// Runs the mcumgrctl CLI app.
///
/// # Arguments
///
/// * `custom_transports` - A handler function that can create custom transports.
///
/// The handler function takes custom CLI arguments that will be added to the normal CLI.
///
/// Example:
///
/// ```rust,no_run
/// #[derive(Debug, clap::Args)]
/// pub struct CustomTransports {
///     /// Dummy argument for demonstration
///     #[arg(long)]
///     pub foo: String,
/// }
///
/// fn custom_transports(
///     args: &CustomTransports,
/// ) -> miette::Result<Option<Box<dyn mcumgrctl::Transport + Send>>> {
///     println!("Custom Transport: Foo: {}", args.foo);
///     // Create custom transport here if `args` commands it
///     Ok(None)
/// }
///
/// pub fn main() -> miette::Result<()> {
///     mcumgrctl::cli_main(custom_transports)
/// }
/// ```
///
pub fn cli_main<T: clap::Args>(
    custom_transports: impl FnOnce(
        &T,
    ) -> Result<
        Option<Box<dyn Transport + Send>>,
        Box<dyn std::error::Error + Send + Sync>,
    >,
) -> miette::Result<()> {
    clap_complete::env::CompleteEnv::with_factory(args::App::<T>::command).complete();

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

    let result = cli_main_internal(&multiprogress, custom_transports).map_err(Into::into);

    multiprogress.clear().ok();

    result
}

/// Usable as arguemtn for [`cli_main`] to indicate that no custom transports exist.
pub fn no_custom_transports(
    _: &(),
) -> Result<Option<Box<dyn Transport + Send>>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(None)
}
