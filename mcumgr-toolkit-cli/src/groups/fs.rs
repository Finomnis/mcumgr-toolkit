use std::collections::BTreeMap;

use indicatif::MultiProgress;

use crate::{
    args::CommonArgs,
    client::Client,
    errors::CliError,
    file_read_write::{read_input_file, write_output_file},
    formatting::structured_print,
    progress::with_progress_bar,
};

#[derive(Debug, clap::Subcommand)]
pub enum FsCommand {
    /// Downloads a file from the device
    Download {
        /// The file path on the device.
        remote: String,
        /// The target path. '-' for stdout.
        local: String,
    },
    /// Uploads a file to the device
    Upload {
        /// The file to copy. '-' for stdin.
        local: String,
        /// The target path on the device.
        remote: String,
    },
    /// Shows status details about a file
    Status {
        /// The path of the file on the device
        name: String,
    },
    /// Computes the checksum of a file
    Checksum {
        /// The path of the file on the device
        name: String,
        /// The checksum algorithm to use
        /// For more info, see `fs supported-checksums`
        #[arg(verbatim_doc_comment)]
        algo: Option<String>,
        /// How many bytes in the file to skip
        #[arg(long, default_value_t = 0)]
        offset: u64,
        /// How many bytes to read from the file; if not specified, read all
        #[arg(long)]
        length: Option<u64>,
    },
    /// Shows supported checksum algorithms
    SupportedChecksums,
    /// Closes all files currently opened by MCUmgr
    Close,
}

pub fn run(
    client: &Client,
    multiprogress: &MultiProgress,
    args: CommonArgs,
    command: FsCommand,
) -> Result<(), CliError> {
    let client = client.get()?;
    match command {
        FsCommand::Download { remote, local } => {
            let mut data = vec![];
            with_progress_bar(multiprogress, !args.quiet, Some(&remote), |progress| {
                client.fs_file_download(remote.as_str(), &mut data, progress)
            })?;

            let filename = remote.rsplit('/').next().filter(|s| !s.is_empty());

            write_output_file(&local, filename, &data)?;
        }
        FsCommand::Upload { local, mut remote } => {
            let (data, source_filename) = read_input_file(&local)?;

            if remote.ends_with("/") {
                let filename =
                    source_filename.ok_or_else(|| CliError::DestinationFilenameUnknown)?;
                remote.push_str(&filename);
            }

            with_progress_bar(multiprogress, !args.quiet, Some(&remote), |progress| {
                client.fs_file_upload(remote.as_str(), &*data, data.len() as u64, progress)
            })?;
        }
        FsCommand::Status { name } => {
            let status = client.fs_file_status(&name)?;
            structured_print(Some(name), args.json, |s| {
                s.key_value("length", status.len);
            })?;
        }
        FsCommand::Checksum {
            name,
            algo,
            offset,
            length,
        } => {
            let checksum = client.fs_file_checksum(&name, algo, offset, length)?;

            if args.json || args.verbose {
                structured_print(Some(name), args.json, |s| {
                    s.key_value("checksum", checksum.output.hex());
                    s.key_value("type", checksum.r#type);
                    s.key_value("data offset", checksum.off);
                    s.key_value("data length", checksum.len);
                })?;
            } else {
                println!("{}  {}", checksum.output.hex(), name);
            }
        }
        FsCommand::SupportedChecksums => {
            let checksums = client
                .fs_supported_checksum_types()?
                .into_iter()
                // Sort
                .collect::<BTreeMap<_, _>>();

            if args.json || args.verbose {
                structured_print(None, args.json, |s| {
                    for (algo, properties) in checksums {
                        s.sublist(algo, |s| {
                            s.key_value("format", properties.format.to_string());
                            s.key_value("size", properties.size);
                        });
                    }
                })?;
            } else {
                println!(
                    "{}",
                    checksums.keys().cloned().collect::<Vec<_>>().join(",")
                );
            }
        }
        FsCommand::Close => client.fs_file_close()?,
    }

    Ok(())
}
