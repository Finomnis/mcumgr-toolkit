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

    /// Serial port baud rate
    #[arg(short, long, default_value_t = 115200)]
    pub baud: u32,

    /// Communication timeout (in ms)
    #[arg(short, long, default_value_t = 500)]
    pub timeout: u64,

    /// Settings that customize runtime behaviour
    #[command(flatten)]
    pub common: CommonArgs,

    /// Command group
    #[command(subcommand)]
    pub group: Group,
}
