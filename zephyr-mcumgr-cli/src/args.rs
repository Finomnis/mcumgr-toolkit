use clap::{Parser, Subcommand};

/// Command line client for Zephyr's MCUmgr SMP protocol
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct App {
    /// Use the given serial port as backend
    #[arg(short, long)]
    pub serial: Option<String>,

    /// Serial port baud rate
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,

    /// Communication timeout (in ms)
    #[arg(long, default_value_t = 500)]
    pub timeout: u64,

    /// Command group
    #[command(subcommand)]
    pub group: Group,
}

#[derive(Debug, Subcommand)]
pub enum OsCommand {
    /// Executes an echo command
    Echo {
        /// The message to echo
        msg: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum FsCommand {
    Foo,
}

#[derive(Debug, Subcommand)]
pub enum Group {
    /// Help message for read.
    Os {
        #[command(subcommand)]
        command: OsCommand,
    },
    /// FS command
    Fs {
        #[command(subcommand)]
        command: FsCommand,
    },
    // ...other commands (can #[clap(flatten)] other enum variants here)
}
