use indicatif::MultiProgress;

use crate::{args::CommonArgs, client::Client, errors::CliError};

#[derive(Debug, clap::Subcommand)]
pub enum ZephyrCommand {
    /// Erase the `storage_partition` flash partition
    EraseStorage,
}

pub fn run(
    client: &Client,
    _multiprogress: &MultiProgress,
    _args: CommonArgs,
    command: ZephyrCommand,
) -> Result<(), CliError> {
    let client = client.get()?;

    match command {
        ZephyrCommand::EraseStorage => client.zephyr_erase_storage()?,
    }

    Ok(())
}
