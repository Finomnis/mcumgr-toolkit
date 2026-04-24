use std::net::ToSocketAddrs;

use clap::{Args, Parser};
use miette::IntoDiagnostic;

use crate::groups::Group;

const DEFAULT_SMP_UDP_PORT: u16 = 1337;

fn parse_udp_addr(s: &str) -> miette::Result<std::net::SocketAddr> {
    let addr = match s.to_socket_addrs() {
        Ok(mut iter) => iter.next(),
        Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => (s, DEFAULT_SMP_UDP_PORT)
            .to_socket_addrs()
            .into_diagnostic()?
            .next(),
        Err(err) => return Err(err).into_diagnostic(),
    };

    addr.ok_or_else(|| miette::miette!("Failed to resolve address"))
}

#[derive(Debug, Args)]
pub struct CommonArgs {
    /// Hide progress bar for data transfer commands
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Increase the verbosity of some commands
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Print command results as JSON, if possible
    #[arg(long, global = true)]
    pub json: bool,

    /// Communication timeout (in ms)
    #[arg(short, long, global = true, default_value_t = mcumgr_toolkit::DEFAULT_TIMEOUT_MS)]
    pub timeout: u64,

    /// Retry count
    #[arg(long, global = true, default_value_t = mcumgr_toolkit::DEFAULT_RETRIES)]
    pub retries: u8,

    /// SMP frame size limit (in bytes)
    ///
    /// If unset, try to fetch frame size from device.
    /// If that also fails, use default frame size.
    #[arg(long, global = true, verbatim_doc_comment)]
    pub smp_frame_size: Option<usize>,
}

/// Command line client for Zephyr's MCUmgr SMP protocol
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(disable_help_subcommand = true)]
pub struct App {
    /// Use the given serial port as backend
    ///
    /// If no argument provided, list all available ports and exit.
    #[arg(short, long, verbatim_doc_comment, num_args = 0..=1, default_missing_value = "",
      conflicts_with_all = ["usb_serial", "udp"])]
    pub serial: Option<String>,

    /// Use the given usb serial port as backend
    ///
    /// Must contain a regex that matches `vid:pid` or `vid:pid:iface`.
    /// If no argument provided, list all available ports and exit.
    #[arg(short, long, verbatim_doc_comment, num_args = 0..=1, default_missing_value = "",
      conflicts_with_all = ["serial", "udp"])]
    pub usb_serial: Option<String>,

    /// Use the given UDP endpoint as backend
    ///
    /// Accepts a hostname or IP address with an optional port.
    /// Port defaults to 1337 if omitted (e.g. "mydevice.local" or "192.168.1.1:1337").
    #[arg(long, verbatim_doc_comment, conflicts_with_all = ["serial", "usb_serial"],
      value_parser=parse_udp_addr, value_name="ADDR")]
    pub udp: Option<std::net::SocketAddr>,

    /// Serial port baud rate
    #[arg(short, long, default_value_t = 115200)]
    pub baud: u32,

    /// Settings that customize runtime behaviour
    #[command(flatten)]
    pub common: CommonArgs,

    /// Command group
    ///
    /// If missing, run a connection test
    #[command(subcommand)]
    pub group: Option<Group>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn generates_valid_man_page() {
        let man = clap_mangen::Man::new(App::command());

        let mut buffer = vec![];
        man.render(&mut buffer).unwrap();
    }

    #[test]
    fn check_cli() {
        App::command().debug_assert();
    }
}
