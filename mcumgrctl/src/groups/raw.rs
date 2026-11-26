use clap_complete::{ArgValueCompleter, PathCompleter};
use indicatif::MultiProgress;

use crate::{
    args::CommonArgs, client::Client, errors::CliError, file_read_write::read_input_file,
    progress::with_progress_bar,
};

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
    pub op: RawCommandOp,
    /// The group ID of the command
    pub group_id: u16,
    /// The command ID
    pub command_id: u8,
    /// The payload of the command, as JSON
    #[arg(value_parser=parse_raw_command_data, default_value = "{}")]
    pub data: ciborium::Value,
}

impl mcumgr_toolkit::commands::McuMgrCommand for RawCommand {
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

pub fn run(
    client: &Client,
    _multiprogress: &MultiProgress,
    _args: CommonArgs,
    command: RawCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    let response = client.raw_command(&command)?;

    let json_response =
        serde_json::to_string_pretty(&response).map_err(CliError::JsonEncodeError)?;

    println!("{json_response}");

    Ok(())
}

fn parse_raw_upload_fields(s: &str) -> Result<ciborium::Value, String> {
    let fields = parse_raw_command_data(s).map_err(|e| e.to_string())?;
    if !fields.is_map() {
        return Err("expected a JSON object".to_string());
    }
    Ok(fields)
}

#[derive(Debug, clap::Args)]
pub struct RawUploadCommand {
    /// The group ID of the command
    pub group_id: u16,
    /// The command ID
    pub command_id: u8,
    /// The file to upload. '-' for stdin.
    #[arg(add=ArgValueCompleter::new(PathCompleter::file()))]
    pub file: String,
    /// Additional fields of every chunk, as a JSON object
    #[arg(value_parser=parse_raw_upload_fields, default_value = "{}")]
    pub fields: ciborium::Value,
}

pub fn run_upload(
    client: &Client,
    multiprogress: &MultiProgress,
    args: CommonArgs,
    command: RawUploadCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    let (data, source_filename) = read_input_file(&command.file)?;

    with_progress_bar(
        multiprogress,
        !args.quiet,
        source_filename.as_deref(),
        |progress| {
            client.raw_upload(
                command.group_id,
                command.command_id,
                &command.fields,
                &data,
                progress,
            )
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_upload_fields_parse_a_json_object() {
        let fields = parse_raw_upload_fields(r#"{"slot": 1}"#).unwrap();
        assert_eq!(
            fields,
            ciborium::Value::Map(vec![("slot".into(), 1.into())])
        );
    }

    #[test]
    fn raw_upload_fields_reject_anything_else() {
        assert!(parse_raw_upload_fields("[1, 2]").is_err());
        assert!(parse_raw_upload_fields("{slot: 1}").is_err());
    }
}
