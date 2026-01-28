use indicatif::MultiProgress;
use zephyr_mcumgr::commands::image::ImageState;

use crate::{
    args::CommonArgs, client::Client, errors::CliError, file_read_write::read_input_file,
    formatting::structured_print, groups::parse_sha256, progress::with_progress_bar,
};

#[derive(Debug, clap::Subcommand)]
pub enum ImageCommand {
    /// Obtain a list of images with their current state
    GetState,
    /// Changes the image state
    SetState {
        /// Boot to the image with the given hash ID
        #[arg(long, value_parser=parse_sha256, required_unless_present = "confirm")]
        hash: Option<[u8; 32]>,
        /// Mark the given image as confirmed
        ///
        /// If no hash is specified, confirm the currently running image
        #[arg(long)]
        confirm: bool,
    },
    /// Upload a firmware image to the device
    Upload {
        /// The firmware image file to upload. '-' for stdin.
        image_file: String,
        /// Selects target image on the device. Default: 0
        #[arg(long)]
        image_id: Option<u32>,
        /// Prevent firmware downgrades
        #[arg(long)]
        upgrade_only: bool,
        /// SHA-256 checksum of the image file
        #[arg(long, value_parser=parse_sha256)]
        checksum: Option<[u8; 32]>,
    },
    /// Erase image slot on target device
    Erase {
        /// The slot ID of the image to erase. Default: 1
        slot: Option<u32>,
    },
    /// Obtain a list of available image slots
    SlotInfo,
}

fn print_current_image_state(images: &[ImageState], args: CommonArgs) -> Result<(), CliError> {
    if args.json {
        let json_str = serde_json::to_string_pretty(images).map_err(CliError::JsonEncodeError)?;
        println!("{json_str}");
    } else {
        structured_print(None, args.json, |s| {
            for image in images {
                s.sublist(format!("Image {}, Slot {}", image.image, image.slot), |s| {
                    s.key_value("version", image.version.as_str());
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

    Ok(())
}

pub fn run(
    client: &Client,
    multiprogress: &MultiProgress,
    args: CommonArgs,
    command: ImageCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    match command {
        ImageCommand::GetState => {
            let images = client.image_get_state()?;
            print_current_image_state(&images, args)?;
        }
        ImageCommand::SetState { hash, confirm } => {
            let images = client.image_set_state(hash, confirm)?;

            if !(args.quiet || args.json) {
                println!();
                println!("Success. New state:");
            }

            print_current_image_state(&images, args)?;
        }
        ImageCommand::Upload {
            image_file,
            image_id,
            upgrade_only,
            checksum,
        } => {
            let (data, source_filename) = read_input_file(&image_file)?;

            with_progress_bar(
                multiprogress,
                !args.quiet,
                source_filename.as_deref(),
                |progress| client.image_upload(&data, image_id, checksum, upgrade_only, progress),
            )?;
        }
        ImageCommand::Erase { slot } => client.image_erase(slot)?,
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
                                    s.unaligned();
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
