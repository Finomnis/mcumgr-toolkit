use zephyr_mcumgr::MCUmgrClient;

use crate::{args::CommonArgs, errors::CliError, formatting::structured_print};

#[derive(Debug, clap::Subcommand)]
pub enum ImageCommand {
    /// Obtain a list of images with their current state
    GetState,
}

pub fn run(client: &MCUmgrClient, args: CommonArgs, command: ImageCommand) -> Result<(), CliError> {
    match command {
        ImageCommand::GetState => {
            let images = client.image_get_state()?;

            if args.json {
                let json_str =
                    serde_json::to_string_pretty(&images).map_err(CliError::JsonEncodeError)?;
                println!("{json_str}");
            } else {
                structured_print(None, args.json, |s| {
                    for image in images {
                        s.sublist(format!("Image {}, Slot {}", image.image, image.slot), |s| {
                            s.key_value("version", image.version);
                            s.key_value_maybe("hash", image.hash.map(hex::encode));
                            s.key_value("bootable", image.bootable);
                            s.key_value("pending", image.pending);
                            s.key_value("confirmed", image.confirmed);
                            s.key_value("active", image.active);
                            s.key_value("permanent", image.permanent);
                        });
                    }
                })?;
            }
        }
    }

    Ok(())
}
