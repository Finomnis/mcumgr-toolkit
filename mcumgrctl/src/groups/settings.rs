use indicatif::MultiProgress;
use pretty_hex::{HexConfig, PrettyHex};

use crate::{args::CommonArgs, client::Client, errors::CliError, groups::parse_hex_str};

#[derive(Debug, clap::Subcommand)]
pub enum SettingsCommand {
    /// Read a setting from the device
    ///
    /// Note: The value is printed in hex.
    Read {
        /// Name of the setting
        name: String,
    },
    /// Write a setting to the device
    Write {
        /// Name of the setting
        name: String,
        /// Value of the setting (in hex)
        #[arg(value_parser=parse_hex_str)]
        value: Box<[u8]>,
    },
}

pub fn run(
    client: &Client,
    _multiprogress: &MultiProgress,
    args: CommonArgs,
    command: SettingsCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    match command {
        SettingsCommand::Read { name } => {
            let value = client.settings_read(name, None)?.val;

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).map_err(CliError::JsonEncodeError)?
                );
            } else if args.quiet {
                println!("{}", hex::encode(value));
            } else {
                let cfg = HexConfig {
                    width: 8,
                    ..HexConfig::default()
                };

                println!("{:?}", value.hex_conf(cfg));
            }
        }
        SettingsCommand::Write { name, value } => {
            client.settings_write(name, &value)?;
        }
    }

    Ok(())
}
