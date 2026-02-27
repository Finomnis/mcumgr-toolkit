use indicatif::MultiProgress;
use mcumgr_toolkit::MCUmgrClient;

use crate::{args::CommonArgs, client::Client, errors::CliError, formatting::structured_print};

#[derive(Debug, clap::Subcommand)]
pub enum StatsCommand {
    /// Retrieve device statistics
    Get {
        /// Group name
        ///
        /// Query all groups if omitted
        group: Option<String>,
    },
    /// List all available groups
    ListGroups,
}

fn get_group_list_sorted(client: &MCUmgrClient) -> Result<Vec<String>, CliError> {
    let mut groups = client.stats_list_groups()?;
    groups.sort();
    Ok(groups)
}

pub fn run(
    client: &Client,
    _multiprogress: &MultiProgress,
    args: CommonArgs,
    command: StatsCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    match command {
        StatsCommand::ListGroups => {
            let groups = get_group_list_sorted(client)?;

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&groups).map_err(CliError::JsonEncodeError)?
                );
            } else {
                println!();
                println!("Available statistics groups:");
                println!();
                for group in groups {
                    println!(" - {group}");
                }
                println!();
            }
        }
        StatsCommand::Get { group } => {
            let groups = if let Some(group) = group {
                vec![group]
            } else {
                get_group_list_sorted(client)?
            };

            let stats = groups
                .iter()
                .map(|name| client.stats_get_group_data(name))
                .collect::<Result<Vec<_>, _>>()?;

            structured_print(None, args.json, |s| {
                for (name, stats) in groups.into_iter().zip(stats) {
                    s.sublist(name, |s| {
                        let mut keys = stats.keys().collect::<Vec<_>>();
                        keys.sort();
                        for key in keys {
                            s.key_value(key, stats[key]);
                        }
                    });
                }
            })?;
        }
    }

    Ok(())
}
