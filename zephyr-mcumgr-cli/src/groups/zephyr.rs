use zephyr_mcumgr::MCUmgrClient;

use crate::{args::CommonArgs, errors::CliError};

#[derive(Debug, clap::Subcommand)]
pub enum ZephyrCommand {
    /// Erase the `storage_partition` flash partition
    EraseStorage,
}

pub fn run(
    client: &MCUmgrClient,
    _args: CommonArgs,
    command: ZephyrCommand,
) -> Result<(), CliError> {
    match command {
        ZephyrCommand::EraseStorage => client.zephyr_erase_storage()?,
    }

    Ok(())
}
