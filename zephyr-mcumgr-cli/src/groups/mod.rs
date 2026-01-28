use indicatif::MultiProgress;

use crate::{args::CommonArgs, client::Client, errors::CliError};

mod firmware;
mod fs;
mod image;
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
    /// High level firmware update utilities
    Firmware {
        #[command(subcommand)]
        command: firmware::FirmwareCommand,
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

pub fn run(
    client: &Client,
    multiprogress: &MultiProgress,
    args: CommonArgs,
    group: Group,
) -> Result<(), CliError> {
    match group {
        Group::Os { command } => os::run(client, multiprogress, args, command),
        Group::Image { command } => image::run(client, multiprogress, args, command),
        Group::Firmware { command } => firmware::run(client, multiprogress, args, command),
        Group::Fs { command } => fs::run(client, multiprogress, args, command),
        Group::Shell { argv } => shell::run(client, multiprogress, args, argv),
        Group::Zephyr { command } => zephyr::run(client, multiprogress, args, command),
        Group::Raw(raw_command) => raw::run(client, multiprogress, args, raw_command),
    }
}

fn parse_sha256(s: &str) -> Result<[u8; 32], hex::FromHexError> {
    let mut data = [0u8; 32];
    hex::decode_to_slice(s, &mut data)?;
    Ok(data)
}
