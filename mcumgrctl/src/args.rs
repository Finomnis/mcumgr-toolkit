use clap::{Args, Parser};

use crate::groups::Group;

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
    #[arg(short, long, global=true, default_value_t = mcumgr_toolkit::DEFAULT_TIMEOUT_MS)]
    pub timeout: u64,

    /// Retry count
    #[arg(long, global=true, default_value_t = mcumgr_toolkit::DEFAULT_RETRIES)]
    pub retries: u8,
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

    /// Use the given UDP endpoint as backend (host:port, e.g. 192.168.1.1:1337)
    #[arg(long, conflicts_with_all = ["serial", "usb_serial"])]
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
    #[test]
    fn generates_valid_man_page() {
        use clap::CommandFactory;

        let man = clap_mangen::Man::new(App::command());

        let mut buffer = vec![];
        man.render(&mut buffer).unwrap();
    }
}
