use crate::{args::CommonArgs, client::Client, errors::CliError, formatting::structured_print};

#[derive(Debug, clap::Subcommand)]
pub enum ImageCommand {
    /// Obtain a list of images with their current state
    GetState,
    /// Obtain a list of available image slots
    SlotInfo,
}

pub fn run(client: &Client, args: CommonArgs, command: ImageCommand) -> Result<(), CliError> {
    let client = client.get()?;
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
        ImageCommand::SlotInfo => {
            let images = client.image_slot_info()?;

            if args.json {
                let json_str =
                    serde_json::to_string_pretty(&images).map_err(CliError::JsonEncodeError)?;
                println!("{json_str}");
            } else {
                structured_print(None, args.json, |s| {
                    for image in images {
                        s.sublist(format!("Image {}", image.image), |s| {
                            for slot in image.slots {
                                s.sublist(format!("Slot {}", slot.slot), |s| {
                                    s.key_value("size", slot.size);
                                    s.key_value_maybe("upload_image_id", slot.upload_image_id);
                                });
                            }
                            s.key_value_maybe("max_image_size", image.max_image_size);
                        });
                    }
                })?;
            }
        }
    }

    Ok(())
}
