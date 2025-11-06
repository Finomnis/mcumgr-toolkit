#![forbid(unsafe_code)]

mod args;

use std::time::Duration;

use clap::Parser;
use log;
use miette::IntoDiagnostic;
use miette::miette;
use zephyr_mcumgr::MCUmgrClient;

fn main() -> miette::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = args::App::parse();

    let mut client;
    if let Some(serial_name) = args.serial {
        let serial = serialport::new(serial_name, args.baud)
            .timeout(Duration::from_millis(args.timeout))
            .open()
            .into_diagnostic()?;

        client = MCUmgrClient::new_from_serial(serial);
        if client.use_auto_frame_size().is_err() {
            log::warn!("Failed to read SMP frame size from device, using default! (might be slow)");
        }
    } else {
        return Err(miette!("No backend selected!"));
    }

    match args.group {
        args::Group::Os { command } => match command {
            args::OsCommand::Echo { msg } => println!("{}", client.os_echo(msg)?),
        },
        args::Group::Fs { command } => match command {
            args::FsCommand::Foo => println!("Bar"),
        },
    }

    Ok(())
}
