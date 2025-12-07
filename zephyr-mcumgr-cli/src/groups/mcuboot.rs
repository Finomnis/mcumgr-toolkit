use crate::{
    args::CommonArgs, client::Client, errors::CliError, file_read_write::read_input_file,
    formatting::structured_print,
};

#[derive(Debug, clap::Subcommand)]
pub enum MCUbootCommand {
    /// Shows information about an MCUboot image file
    GetImageInfo {
        /// The image file to analyze. '-' for stdin.
        file: String,
    },
}

pub fn run(_client: &Client, args: CommonArgs, command: MCUbootCommand) -> Result<(), CliError> {
    match command {
        MCUbootCommand::GetImageInfo { file } => {
            let (image_data, _source_filename) = read_input_file(&file)?;
            let image_info =
                zephyr_mcumgr::mcuboot::get_image_info(std::io::Cursor::new(image_data.as_ref()))?;

            structured_print(Some(file), args.json, |s| {
                s.key_value("version", image_info.version.to_string());
                s.key_value("hash", hex::encode(image_info.hash));
            })?;
        }
    }

    Ok(())
}
