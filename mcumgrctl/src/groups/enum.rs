use indicatif::MultiProgress;
use mcumgr_toolkit::MCUmgrGroup;

use crate::{args::CommonArgs, client::Client, errors::CliError, formatting::structured_print};

#[derive(Debug, clap::Subcommand)]
pub enum EnumCommand {
    /// List all supported MCUmgr groups
    ListGroups {
        /// Use slow iterative algorithm
        #[clap(long)]
        iter: bool,
    },
    /// Show details for groups
    ShowGroupDetails {
        /// The group IDs to load details for
        ///
        /// If omitted, load details for all groups
        groups: Option<Vec<u16>>,
    },
}

pub fn run(
    client: &Client,
    _multiprogress: &MultiProgress,
    args: CommonArgs,
    command: EnumCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    match command {
        EnumCommand::ListGroups { iter } => {
            let mut groups = if iter {
                client.enum_iter_group_ids().collect::<Result<_, _>>()?
            } else {
                client.enum_get_group_ids()?
            };

            groups.sort();

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&groups).map_err(CliError::JsonEncodeError)?
                );
            } else {
                structured_print(Some("Available MCUmgr groups".into()), args.json, |s| {
                    for group in groups {
                        s.key_value(group, MCUmgrGroup::group_id_to_string(group));
                    }
                })?;
            }
        }
        EnumCommand::ShowGroupDetails { groups } => {
            let mut details = client.enum_get_group_details(groups.as_deref())?;

            details.sort_by_key(|val| val.group);

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&details).map_err(CliError::JsonEncodeError)?
                );
            } else {
                structured_print(None, args.json, |s| {
                    for entry in details {
                        s.sublist(
                            format!(
                                "{} - {}",
                                entry.group,
                                entry
                                    .name
                                    .unwrap_or_else(|| MCUmgrGroup::group_id_to_string(
                                        entry.group
                                    ))
                            ),
                            |s| {
                                s.key_value("handlers", entry.handlers);
                            },
                        );
                    }
                })?;
            }
        }
    }

    Ok(())
}
