use std::collections::HashSet;

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
    /// Issue a system reset
    SystemReset {
        /// Issue a force reset
        #[arg(short, long)]
        force: bool,
        /// Overwrite the boot mode
        ///
        /// - 0: Normal system boot
        /// - 1: Bootloader recovery mode
        ///
        /// Requires `CONFIG_MCUMGR_GRP_OS_RESET_BOOT_MODE`
        #[arg(verbatim_doc_comment)]
        #[arg(long)]
        bootmode: Option<u8>,
    },
    /// Fetch parameters from the MCUmgr library
    McumgrParameters,
    /// Fetch information on the running image, similar to `uname`
    ///
    /// Run this without flags to get a structured overview
    /// of all available information.
    ///
    /// Specify one or more flags to get the raw output
    /// without any post-processing.
    ApplicationInfo(#[command(flatten)] ApplicationInfoFlags),
}

#[derive(Debug, Args)]
pub struct ApplicationInfoFlags {
    /// Kernel name
    #[arg(short = 's', long)]
    kernel_name: bool,
    /// Node name
    #[arg(short = 'n', long)]
    node_name: bool,
    /// Kernel release
    #[arg(short = 'r', long)]
    kernel_release: bool,
    /// Kernel version
    #[arg(short = 'v', long)]
    kernel_version: bool,
    /// Build date and time (requires CONFIG_MCUMGR_GRP_OS_INFO_BUILD_DATE_TIME)
    #[arg(short = 'b', long)]
    build_time: bool,
    /// Machine
    #[arg(short = 'm', long)]
    machine: bool,
    /// Processor
    #[arg(short = 'p', long)]
    processor: bool,
    /// Hardware platform
    #[arg(short = 'i', long)]
    hardware_platform: bool,
    /// Operating system
    #[arg(short = 'o', long)]
    operating_system: bool,
    /// All fields (shorthand for all above options)
    #[arg(short = 'a', long)]
    all: bool,
}

impl ApplicationInfoFlags {
    pub fn accumulate(&self) -> HashSet<char> {
        let mut flags = HashSet::new();

        if self.kernel_name {
            flags.insert('s');
        }
        if self.node_name {
            flags.insert('n');
        }
        if self.kernel_release {
            flags.insert('r');
        }
        if self.kernel_version {
            flags.insert('v');
        }
        if self.build_time {
            flags.insert('b');
        }
        if self.machine {
            flags.insert('m');
        }
        if self.processor {
            flags.insert('p');
        }
        if self.hardware_platform {
            flags.insert('i');
        }
        if self.operating_system {
            flags.insert('o');
        }
        if self.all {
            flags.insert('a');
        }

        flags
    }
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
