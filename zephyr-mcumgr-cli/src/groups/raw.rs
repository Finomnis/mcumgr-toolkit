use zephyr_mcumgr::MCUmgrClient;

use crate::{args::CommonArgs, errors::CliError};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum RawCommandOp {
    /// Perform a read command
    Read,
    /// Perform a write command
    Write,
}

fn parse_raw_command_data(s: &str) -> Result<ciborium::Value, serde_json::Error> {
    serde_json::from_str(s)
}

#[derive(Debug, clap::Args)]
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

impl zephyr_mcumgr::commands::McuMgrCommand for RawCommand {
    type Payload = ciborium::Value;
    type Response = ciborium::Value;

    fn is_write_operation(&self) -> bool {
        match self.op {
            RawCommandOp::Read => false,
            RawCommandOp::Write => true,
        }
    }

    fn group_id(&self) -> u16 {
        self.group_id
    }

    fn command_id(&self) -> u8 {
        self.command_id
    }

    fn data(&self) -> &ciborium::Value {
        &self.data
    }
}

pub fn run(client: &MCUmgrClient, _args: CommonArgs, command: RawCommand) -> Result<(), CliError> {
    let response = client.raw_command(&command)?;

    let json_response =
        serde_json::to_string_pretty(&response).map_err(CliError::JsonEncodeError)?;

    println!("{json_response}");

    Ok(())
}
