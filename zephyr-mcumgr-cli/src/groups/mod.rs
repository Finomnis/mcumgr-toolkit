use crate::{args::CommonArgs, client::Client, errors::CliError};

mod fs;
mod image;
mod mcuboot;
mod os;
mod raw;
mod shell;
mod zephyr;

#[derive(Debug, clap::Subcommand)]
pub enum Group {
    /// Default/OS Management
    Os {
        #[command(subcommand)]
        command: os::OsCommand,
    },
    /// Application/Software Image Management
    Image {
        #[command(subcommand)]
        command: image::ImageCommand,
    },
    /// MCUboot specific tools
    Mcuboot {
        #[command(subcommand)]
        command: mcuboot::MCUbootCommand,
    },
    /// File Management
    Fs {
        #[command(subcommand)]
        command: fs::FsCommand,
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
        command: zephyr::ZephyrCommand,
    },
    /// Execute a raw SMP command
    Raw(#[command(flatten)] raw::RawCommand),
}

pub fn run(client: &Client, args: CommonArgs, group: Group) -> Result<(), CliError> {
    match group {
        Group::Os { command } => os::run(client, args, command),
        Group::Image { command } => image::run(client, args, command),
        Group::Mcuboot { command } => mcuboot::run(client, args, command),
        Group::Fs { command } => fs::run(client, args, command),
        Group::Shell { argv } => shell::run(client, args, argv),
        Group::Zephyr { command } => zephyr::run(client, args, command),
        Group::Raw(raw_command) => raw::run(client, args, raw_command),
    }
}

fn parse_sha256(s: &str) -> Result<[u8; 32], hex::FromHexError> {
    let mut data = [0u8; 32];
    hex::decode_to_slice(s, &mut data)?;
    Ok(data)
}
