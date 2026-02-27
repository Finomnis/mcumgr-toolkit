use indicatif::MultiProgress;

use crate::{args::CommonArgs, client::Client, errors::CliError, formatting::structured_print};

#[derive(Debug, clap::Subcommand)]
pub enum StatsCommand {
    /// Retrieves device statistics
    Get {
        /// Group name
        ///
        /// Query all groups if omitted
        group: Option<String>,
    },
    /// List all available groups
    ListGroups,
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
            let groups = client.stats_list_groups()?;

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
            let groups;

            if let Some(group) = group {
                groups = vec![group];
            } else {
                groups = client.stats_list_groups()?;
            }

            let stats = groups
                .iter()
                .map(|name| client.stats_group_data(name))
                .collect::<Result<Vec<_>, _>>()?;

            structured_print(None, args.json, |s| {
                for (name, stats) in groups.into_iter().zip(stats.into_iter()) {
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
