use clap::{Args, Parser, Subcommand, ValueEnum};

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

    /// Command group
    #[command(subcommand)]
    pub group: Group,
}

#[derive(Debug, Subcommand)]
pub enum OsCommand {
    /// Executes an echo command on the device
    Echo {
        /// The message to echo
        msg: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum FsCommand {}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum RawCommandOp {
    /// Perform a read command
    Read,
    /// Perform a write command
    Write,
}

fn parse_raw_command_data(s: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(s)
}

#[derive(Debug, Args)]
pub struct RawCommand {
    /// Whether this is a read or write command
    #[arg(value_enum)]
    pub op: RawCommandOp,
    /// The group ID of the command
    pub group_id: u16,
    /// The command ID
    pub command_id: u8,
    /// The payload of the command, as JSON
    #[arg(value_parser=parse_raw_command_data, default_value = "{}")]
    pub data: serde_json::Value,
}

#[derive(Debug, Subcommand)]
pub enum Group {
    /// Default/OS Management
    Os {
        #[command(subcommand)]
        command: OsCommand,
    },
    /// File Management
    Fs {
        #[command(subcommand)]
        command: FsCommand,
    },
    /// Shell command execution
    Shell {
        /// The shell command to execute
        #[arg(required = true, trailing_var_arg = true)]
        argv: Vec<String>,
    },
    /// Execute a raw SMP command
    Raw(#[command(flatten)] RawCommand),
}
