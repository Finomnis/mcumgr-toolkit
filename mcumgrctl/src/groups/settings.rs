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
    /// Delete a setting from the device
    Delete {
        /// Name of the setting
        name: String,
    },
    /// Commit modified settings on the device
    Commit,
    /// Load settings from persistent storage
    Load,
    /// Save settings to persistent storage
    Save {
        /// Only store the subtree with the given name
        #[arg(long)]
        name: Option<String>,
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
        SettingsCommand::Delete { name } => {
            client.settings_delete(name)?;
        }
        SettingsCommand::Commit => {
            client.settings_commit()?;
        }
        SettingsCommand::Load => {
            client.settings_load()?;
        }
        SettingsCommand::Save { name } => {
            client.settings_save(name)?;
        }
    }

    Ok(())
}
