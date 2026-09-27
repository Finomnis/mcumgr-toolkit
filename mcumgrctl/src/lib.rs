//!
//! This crate is primarily meant as a cli binary crate.
//!
//! It can, however, be used as a library crate to extend
//! the cli with custom backends.
//!

#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![doc(issue_tracker_base_url = "https://github.com/Finomnis/mcumgr-toolkit/issues")]
// That's just a bad lint, in many cases I want two ifs for readability
#![allow(clippy::collapsible_if)]
#![cfg_attr(docsrs, feature(doc_cfg))]

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

use clap::{CommandFactory as _, Parser};
use mcumgr_toolkit::{MCUmgrClient, client::UsbSerialError};

#[cfg(feature = "ble")]
use mcumgr_toolkit::client::BleError;

pub use crate::args::CommonArgs;
use crate::errors::CliError;

/// The result of a backend init function, in case it ran.
pub enum BackendInitResult {
    /// The backend ran some action, printed some result and is
    /// now finished
    Finished,
    /// The backend successfully created a client
    Connected(MCUmgrClient),
}

fn ble_init<T: clap::Args>(
    #[allow(unused_variables)] args: &args::App<T>,
    #[allow(unused_variables)] multiprogress: &MultiProgress,
) -> Result<Option<BackendInitResult>, CliError> {
    #[cfg(feature = "ble")]
    if let Some(ble_identifier) = &args.ble {
        use indicatif::ProgressBar;

        let mut scan_spinner = None;

        let result = MCUmgrClient::new_from_ble_with_scan_callback(
            ble_identifier.clone(),
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
            return Ok(Some(BackendInitResult::Finished));
        }

        return Ok(Some(BackendInitResult::Connected(result?)));
    }

    Ok(None)
}

fn cli_main_internal<T: clap::Args>(
    multiprogress: &MultiProgress,
    custom_backends: impl FnOnce(&T, &CommonArgs) -> miette::Result<Option<BackendInitResult>>,
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
    } else if let Some(init_result) = ble_init(&args, multiprogress)? {
        match init_result {
            BackendInitResult::Finished => return Ok(()),
            BackendInitResult::Connected(client) => Client::new(client),
        }
    } else if let Some(addr) = args.udp {
        Client::new(
            MCUmgrClient::new_from_udp(addr, Duration::from_millis(args.common.timeout))
                .map_err(CliError::UdpOpenFailed)?,
        )
    } else if let Some(init_result) = custom_backends(&args.custom_backends, &args.common)
        .map_err(|e| CliError::CustomBackendError(e.into()))?
    {
        match init_result {
            BackendInitResult::Finished => return Ok(()),
            BackendInitResult::Connected(client) => Client::new(client),
        }
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
/// * `custom_backends` - A handler function that can initialize custom backends.
///
/// The handler function takes custom CLI arguments that will be added to the normal CLI.
///
/// The custom backend must respect the parameters in [`CommonArgs`], like timeout or verbosity.
///
/// The backend-activating CLI arguments should be tagged with `group = "transport"` to make them
/// mutually exclusive with other backend-activating arguments like `--ble` or `--serial`.
///
/// Example:
///
/// ```rust,no_run
/// use mcumgr_toolkit::{MCUmgrClient, transport::Transport};
/// use mcumgrctl::{BackendInitResult, CommonArgs};
///
/// #[derive(Debug, clap::Args)]
/// pub struct CustomBackends {
///     /// Dummy backend for demonstration
///     #[arg(long, group = "transport")]
///     pub dummy: Option<String>,
/// }
///
/// fn custom_backends(
///     args: &CustomBackends,
///     common: &CommonArgs,
/// ) -> miette::Result<Option<BackendInitResult>> {
///     if let Some(dummy) = &args.dummy {
///         println!("Custom Backend: Dummy: {dummy}");
///         println!("Common Args: {common:?}");
///
///         // Create custom transport here
///         let custom_transport: Box<dyn Transport + Send> = todo!();
///
///         let client = MCUmgrClient::new_from_transport(custom_transport);
///         return Ok(Some(BackendInitResult::Connected(client)));
///     }
///
///     Ok(None)
/// }
///
/// pub fn main() -> miette::Result<()> {
///     mcumgrctl::cli_main(custom_backends)
/// }
/// ```
///
pub fn cli_main<T: clap::Args>(
    custom_backends: impl FnOnce(&T, &CommonArgs) -> miette::Result<Option<BackendInitResult>>,
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

    let result = cli_main_internal(&multiprogress, custom_backends).map_err(Into::into);

    multiprogress.clear().ok();

    result
}

/// Usable as argument for [`cli_main`] to indicate that no custom backends exist.
pub fn no_custom_backends(_: &(), _: &CommonArgs) -> miette::Result<Option<BackendInitResult>> {
    Ok(None)
}
