use std::collections::HashSet;

use indicatif::MultiProgress;
use zephyr_mcumgr::{
    bootloader::{BootloaderInfo, MCUbootMode},
    commands::os::ThreadStateFlags,
    connection::ExecuteError,
};

use crate::{args::CommonArgs, client::Client, errors::CliError, formatting::structured_print};

#[derive(Debug, clap::Subcommand)]
pub enum OsCommand {
    /// Executes an echo command on the device
    Echo {
        /// The message to echo
        msg: String,
    },
    /// Queries live task statistics
    TaskStatistics,
    /// Set the device's RTC datetime
    SetDatetime {
        /// The datetime value, as RFC3339; host time if omitted
        value: Option<String>,
        /// Use UTC time instead of local time
        #[arg(long)]
        utc: bool,
    },
    /// Retrieve the device's RTC datetime
    GetDatetime,
    /// Issue a system reset
    SystemReset {
        /// Issue a force reset
        #[arg(short, long)]
        force: bool,
        /// Overwrite the boot mode
        ///
        /// - 0: Normal system boot
        /// - 1: Bootloader recovery mode
        ///
        /// Requires `CONFIG_MCUMGR_GRP_OS_RESET_BOOT_MODE`
        #[arg(verbatim_doc_comment)]
        #[arg(long)]
        bootmode: Option<u8>,
    },
    /// Fetch parameters from the MCUmgr library
    McumgrParameters,
    /// Fetch information on the running image, similar to `uname`
    ///
    /// Run this without flags to get a structured overview
    /// of all available information.
    ///
    /// Specify one or more flags to get the raw output
    /// without any post-processing.
    ApplicationInfo(#[command(flatten)] ApplicationInfoFlags),
    /// Fetch information on the running bootloader
    BootloaderInfo,
}

#[derive(Debug, clap::Args)]
pub struct ApplicationInfoFlags {
    /// Kernel name
    #[arg(short = 's', long)]
    kernel_name: bool,
    /// Node name
    #[arg(short = 'n', long)]
    node_name: bool,
    /// Kernel release
    #[arg(short = 'r', long)]
    kernel_release: bool,
    /// Kernel version
    #[arg(short = 'v', long)]
    kernel_version: bool,
    /// Build date and time (requires CONFIG_MCUMGR_GRP_OS_INFO_BUILD_DATE_TIME)
    #[arg(short = 'b', long)]
    build_time: bool,
    /// Machine
    #[arg(short = 'm', long)]
    machine: bool,
    /// Processor
    #[arg(short = 'p', long)]
    processor: bool,
    /// Hardware platform
    #[arg(short = 'i', long)]
    hardware_platform: bool,
    /// Operating system
    #[arg(short = 'o', long)]
    operating_system: bool,
    /// All fields (shorthand for all above options)
    #[arg(short = 'a', long)]
    all: bool,
}

impl ApplicationInfoFlags {
    pub fn accumulate(&self) -> HashSet<char> {
        let mut flags = HashSet::new();

        if self.kernel_name {
            flags.insert('s');
        }
        if self.node_name {
            flags.insert('n');
        }
        if self.kernel_release {
            flags.insert('r');
        }
        if self.kernel_version {
            flags.insert('v');
        }
        if self.build_time {
            flags.insert('b');
        }
        if self.machine {
            flags.insert('m');
        }
        if self.processor {
            flags.insert('p');
        }
        if self.hardware_platform {
            flags.insert('i');
        }
        if self.operating_system {
            flags.insert('o');
        }
        if self.all {
            flags.insert('a');
        }

        flags
    }
}

pub fn run(
    client: &Client,
    _multiprogress: &MultiProgress,
    args: CommonArgs,
    command: OsCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    match command {
        OsCommand::Echo { msg } => println!("{}", client.os_echo(msg)?),
        OsCommand::TaskStatistics => {
            let tasks_map = client.os_task_statistics()?;

            let mut tasks = tasks_map.iter().collect::<Vec<_>>();
            tasks.sort_by_key(|(name, stats)| (stats.prio, (*name).clone()));

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&tasks_map).map_err(CliError::JsonEncodeError)?
                );
            } else {
                structured_print(None, args.json, |s| {
                    for (name, stats) in tasks {
                        s.sublist(name, |s| {
                            s.key_value("Priority", stats.prio);
                            s.key_value("Task ID", stats.tid);
                            s.key_value("State", {
                                let pretty_state =
                                    ThreadStateFlags::pretty_print(stats.state as u8);
                                if pretty_state.is_empty() {
                                    format!("{}", stats.state)
                                } else {
                                    format!("{} ({})", stats.state, pretty_state)
                                }
                            });
                            if let (Some(stkuse), Some(stksiz)) = (stats.stkuse, stats.stksiz) {
                                s.key_value(
                                    "Stack Usage",
                                    if stksiz != 0 {
                                        let pct = stkuse * 100 / stksiz;
                                        format!("{stkuse} / {stksiz} bytes ({pct} %)")
                                    } else {
                                        format!("{stkuse} / {stksiz} bytes")
                                    },
                                );
                            }
                            if let Some(cswcnt) = stats.cswcnt {
                                s.key_value("Context Switches", cswcnt);
                            }
                            if let Some(runtime) = stats.runtime {
                                s.key_value("Runtime", format!("{} ticks", runtime));
                            }
                        });
                    }
                })?;
            }
        }
        OsCommand::SetDatetime { value, utc } => {
            use chrono::{DateTime, FixedOffset, NaiveDateTime};

            let datetime_value = if let Some(value) = value {
                value
                    .parse::<DateTime<FixedOffset>>()
                    .map(|datetime| {
                        if utc {
                            datetime.naive_utc()
                        } else {
                            datetime.naive_local()
                        }
                    })
                    .or_else(|_| value.parse::<NaiveDateTime>())?
            } else {
                let now = chrono::Local::now();
                if utc {
                    now.naive_utc()
                } else {
                    now.naive_local()
                }
            };

            client.os_set_datetime(datetime_value)?;

            if args.verbose {
                println!("Set device time to: {}", datetime_value.format("%F %T"));
            }
        }
        OsCommand::GetDatetime => {
            let datetime = client.os_get_datetime()?;
            if args.verbose {
                println!("Device time: {}", datetime);
            } else {
                println!("{:?}", datetime);
            }
        }
        OsCommand::SystemReset { force, bootmode } => {
            client.os_system_reset(force, bootmode)?;
        }
        OsCommand::McumgrParameters => {
            let params = client.os_mcumgr_parameters()?;

            structured_print(Some("MCUmgr Parameters".to_string()), args.json, |s| {
                s.key_value("buf_size", params.buf_size);
                s.key_value("buf_count", params.buf_count);
            })?;
        }
        OsCommand::ApplicationInfo(flags) => {
            let flags = flags.accumulate();

            if flags.is_empty() {
                // Fetch everything and do a detailed print

                let kernel_name = client.os_application_info(Some("s"))?;
                let node_name = client.os_application_info(Some("n"))?;
                let kernel_release = client.os_application_info(Some("r"))?;
                let kernel_version = client.os_application_info(Some("v"))?;
                let build_time = match client.os_application_info(Some("b")) {
                    Ok(val) => Some(val),
                    Err(ExecuteError::ErrorResponse(e)) => {
                        log::debug!("Failed to fetch build time: {e}");
                        None
                    }
                    Err(e) => Err(e)?,
                };
                let machine = client.os_application_info(Some("m"))?;
                let processor = client.os_application_info(Some("p"))?;
                let hardware_platform = client.os_application_info(Some("i"))?;
                let operating_system = client.os_application_info(Some("o"))?;

                structured_print(Some("OS/Application Info".to_string()), args.json, |s| {
                    s.key_value("Kernel name", kernel_name);
                    s.key_value("Node name", node_name);
                    s.key_value("Kernel release", kernel_release);
                    s.key_value("Kernel version", kernel_version);
                    if let Some(build_time) = build_time {
                        s.key_value("Build time", build_time);
                    }
                    s.key_value("Machine", machine);
                    s.key_value("Processor", processor);
                    s.key_value("Hardware platform", hardware_platform);
                    s.key_value("Operating system", operating_system);
                })?;
            } else {
                let output = client.os_application_info(Some(&flags.iter().collect::<String>()))?;
                println!("{output}");
            }
        }
        OsCommand::BootloaderInfo => {
            let info = client.os_bootloader_info()?;

            structured_print(Some("Bootloader Info".to_string()), args.json, |s| {
                match info {
                    BootloaderInfo::MCUboot { mode, no_downgrade } => {
                        s.key_value("Name", "MCUboot");

                        let mode_name = if args.json {
                            None
                        } else {
                            MCUbootMode::from_repr(mode)
                        };

                        if let Some(mode_name) = mode_name {
                            s.key_value("Mode", format!("{mode} ({mode_name})"));
                        } else {
                            s.key_value("Mode", mode);
                        }
                        s.key_value("Downgrade Prevention", no_downgrade);
                    }
                    BootloaderInfo::Unknown { name } => {
                        s.key_value("Name", name);
                    }
                };
            })?;
        }
    }

    Ok(())
}
