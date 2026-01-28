use clap::{Args, Parser};

use crate::groups::Group;

#[derive(Debug, Args)]
pub struct CommonArgs {
    /// Hide progress bar for data transfer commands
    #[arg(short, long)]
    pub quiet: bool,

    /// Increase the verbosity of some commands
    #[arg(short, long)]
    pub verbose: bool,

    /// Print command results as JSON, if possible
    #[arg(long)]
    pub json: bool,
}

/// Command line client for Zephyr's MCUmgr SMP protocol
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(disable_help_subcommand = true)]
pub struct App {
    /// Use the given serial port as backend
    #[arg(short, long)]
    pub serial: Option<String>,

    /// Use the given usb serial port as backend
    ///
    /// Must contain a regex that matches `vid:pid` or `vid:pid:iface`.
    /// If no argument provided, list all available ports and exit.
    #[arg(short, long, verbatim_doc_comment, num_args = 0..=1, default_missing_value = "")]
    pub usb_serial: Option<String>,

    /// Serial port baud rate
    #[arg(short, long, default_value_t = 115200)]
    pub baud: u32,

    /// Communication timeout (in ms)
    #[arg(short, long, default_value_t = 10000)]
    pub timeout: u64,

    /// Settings that customize runtime behaviour
    #[command(flatten)]
    pub common: CommonArgs,

    /// Command group
    ///
    /// If missing, run a connection test
    #[command(subcommand)]
    pub group: Option<Group>,
}
