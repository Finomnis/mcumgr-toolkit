use clap::ValueEnum;
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use zephyr_mcumgr::client::FirmwareUpdateParams;

use crate::{
    args::CommonArgs, client::Client, errors::CliError, file_read_write::read_input_file,
    formatting::structured_print, groups::parse_sha256,
};

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum BootloaderType {
    Mcuboot,
}

impl From<BootloaderType> for zephyr_mcumgr::bootloader::BootloaderType {
    fn from(value: BootloaderType) -> Self {
        match value {
            BootloaderType::Mcuboot => Self::MCUboot,
        }
    }
}

#[derive(Debug, clap::Subcommand)]
pub enum FirmwareCommand {
    /// Shows information about an MCUboot image file
    GetImageInfo {
        /// The image type
        #[arg(value_enum)]
        r#type: BootloaderType,
        /// The image file to analyze. '-' for stdin.
        file: String,
    },
    /// Perform a device firmware update
    Update {
        /// The firmware image file to update to. '-' for stdin.
        firmware_file: String,
        /// Specify the bootloader type
        ///
        /// Auto-detect if not specified
        #[arg(short, long)]
        bootloader: Option<BootloaderType>,
        /// Do not reboot after the update
        #[arg(long)]
        skip_reboot: bool,
        /// Skip test boot and confirm directly
        #[arg(long)]
        force_confirm: bool,
        /// Prevent firmware downgrades
        #[arg(long)]
        upgrade_only: bool,
        /// SHA-256 checksum of the image file
        #[arg(long, value_parser=parse_sha256)]
        checksum: Option<[u8; 32]>,
    },
}

struct FirmwareUpgradeProgressHandler<'a> {
    previous_message: String,
    multiprogress: &'a MultiProgress,
    progressbar: Option<ProgressBar>,
}

impl<'a> FirmwareUpgradeProgressHandler<'a> {
    fn new(multiprogress: &'a MultiProgress) -> Self {
        Self {
            previous_message: "".to_string(),
            multiprogress,
            progressbar: None,
        }
    }
    fn update(&mut self, msg: &str, progress: Option<(u64, u64)>) -> bool {
        if msg != self.previous_message {
            self.previous_message = msg.to_string();
            self.multiprogress.println(msg).ok();
        }

        if let Some((current, total)) = progress {
            let progressbar = self.progressbar.get_or_insert_with(||{
                let progressbar = self.multiprogress.add(ProgressBar::new(total)).with_finish(ProgressFinish::AndClear);
                progressbar.set_style(
                ProgressStyle::with_template(
                    "{wide_bar} {decimal_bytes:>9} / {decimal_total_bytes:9} ({decimal_bytes_per_sec:9})",
                )
                .unwrap());
                progressbar
            });

            progressbar.set_length(total);
            progressbar.set_position(current);
        } else if let Some(progressbar) = self.progressbar.take() {
            progressbar.finish_and_clear();
            self.multiprogress.remove(&progressbar);
        }

        true
    }
}

impl Drop for FirmwareUpgradeProgressHandler<'_> {
    fn drop(&mut self) {
        if let Some(progressbar) = self.progressbar.take() {
            progressbar.finish_and_clear();
            self.multiprogress.remove(&progressbar);
        }
    }
}

pub fn run(
    client: &Client,
    multiprogress: &MultiProgress,
    args: CommonArgs,
    command: FirmwareCommand,
) -> Result<(), CliError> {
    match command {
        FirmwareCommand::GetImageInfo { file, r#type } => {
            let (image_data, _source_filename) = read_input_file(&file)?;

            match r#type {
                BootloaderType::Mcuboot => {
                    let image_info = zephyr_mcumgr::mcuboot::get_image_info(std::io::Cursor::new(
                        image_data.as_ref(),
                    ))?;

                    structured_print(Some(file), args.json, |s| {
                        s.key_value("version", image_info.version.to_string());
                        s.key_value("hash", hex::encode(image_info.hash));
                    })?;
                }
            }
        }
        FirmwareCommand::Update {
            firmware_file,
            bootloader,
            skip_reboot,
            force_confirm,
            upgrade_only,
            checksum,
        } => {
            let (firmware, _source_filename) = read_input_file(&firmware_file)?;

            let client = client.get()?;

            let params = FirmwareUpdateParams {
                bootloader_type: bootloader.map(Into::into),
                skip_reboot,
                force_confirm,
                upgrade_only,
            };

            if args.quiet {
                client.firmware_update(firmware, checksum, params, None)
            } else {
                let mut progress_handler = FirmwareUpgradeProgressHandler::new(multiprogress);
                client.firmware_update(
                    firmware,
                    checksum,
                    params,
                    Some(&mut move |msg, progress| progress_handler.update(msg, progress)),
                )
            }?;

            multiprogress.println("Success.").ok();

            if !skip_reboot {
                multiprogress
                    .println("Device should reboot with new firmware.")
                    .ok();
            }
        }
    }

    Ok(())
}
