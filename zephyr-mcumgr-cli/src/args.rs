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

    /// Show progress bar for data transfer commands
    #[arg(short, long)]
    pub progress: bool,

    /// Increase the verbosity of some commands
    #[arg(short, long)]
    pub verbose: bool,

    /// Print command results as JSON, if possible
    #[arg(long)]
    pub json: bool,

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
    /// Queries live task statistics
    TaskStatistics,
    /// Set the device's RTC datetime
    SetDatetime {
        /// The datetime value, as RFC3339; host time if omitted
        value: Option<String>,
        /// Use UTC time instead of local time
        #[arg(long)]
        utc: bool,
    },
    /// Retrieve the device's RTC datetime
    GetDatetime,
}

#[derive(Debug, Subcommand)]
pub enum FsCommand {
    /// Downloads a file from the device
    Download {
        /// The file path on the device.
        remote: String,
        /// The target path. '-' for stdout.
        local: String,
    },
    /// Uploads a file to the device
    Upload {
        /// The file to copy. '-' for stdin.
        local: String,
        /// The target path on the device.
        remote: String,
    },
    /// Shows status details about a file
    Status {
        /// The path of the file on the device
        name: String,
    },
    /// Computes the checksum of a file
    Checksum {
        /// The path of the file on the device
        name: String,
        /// The checksum algorithm to use
        /// For more info, see `fs supported-checksums`
        #[arg(verbatim_doc_comment)]
        algo: Option<String>,
        /// How many bytes in the file to skip
        #[arg(long, default_value_t = 0)]
        offset: u64,
        /// How many bytes to read from the file; if not specified, read all
        #[arg(long)]
        length: Option<u64>,
    },
    /// Shows supported checksum algorithms
    SupportedChecksums,
    /// Closes all files currently opened by MCUmgr
    Close,
}

#[derive(Debug, Subcommand)]
pub enum ZephyrCommand {
    /// Erase the `storage_partition` flash partition
    EraseStorage,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum RawCommandOp {
    /// Perform a read command
    Read,
    /// Perform a write command
    Write,
}

fn parse_raw_command_data(s: &str) -> Result<ciborium::Value, serde_json::Error> {
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
    pub data: ciborium::Value,
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
    /// Zephyr Management
    Zephyr {
        #[command(subcommand)]
        command: ZephyrCommand,
    },
    /// Execute a raw SMP command
    Raw(#[command(flatten)] RawCommand),
}
