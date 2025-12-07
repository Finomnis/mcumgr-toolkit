use crate::{args::CommonArgs, client::Client, errors::CliError, file_read_write::read_input_file};

#[derive(Debug, clap::Subcommand)]
pub enum MCUbootCommand {
    /// Shows information about an MCUboot image file
    GetImageInfo {
        /// The image file to analyze. '-' for stdin.
        file: String,
    },
}

pub fn run(_client: &Client, _args: CommonArgs, command: MCUbootCommand) -> Result<(), CliError> {
    match command {
        MCUbootCommand::GetImageInfo { file } => {
            let (data, _source_filename) = read_input_file(&file)?;

            println!("{}", data.len());

            todo!()
        }
    }

    Ok(())
}
