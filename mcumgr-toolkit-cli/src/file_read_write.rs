use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::errors::CliError;

/// Reads the input file, or stdin if '-'.
///
/// # Return
///
/// A tuple of (file_content, file_basename).
///
pub fn read_input_file(filename: &str) -> Result<(Box<[u8]>, Option<String>), CliError> {
    if filename == "-" {
        let mut data = Vec::new();

        std::io::stdin()
            .lock()
            .read_to_end(&mut data)
            .map_err(CliError::InputReadFailed)?;

        Ok((data.into_boxed_slice(), None))
    } else {
        let filename: &Path = filename.as_ref();
        let mut file = File::open(filename).map_err(CliError::InputReadFailed)?;

        let mut data = if let Ok(file_size) = file.metadata().map(|m| m.len() as usize) {
            Vec::with_capacity(file_size)
        } else {
            Vec::new()
        };

        file.read_to_end(&mut data)
            .map_err(CliError::InputReadFailed)?;

        Ok((
            data.into_boxed_slice(),
            filename
                .file_name()
                .map(|val| val.to_string_lossy().into_owned()),
        ))
    }
}

pub fn write_output_file(
    output_path: &str,
    source_filename: Option<&str>,
    data: &[u8],
) -> Result<(), CliError> {
    if output_path == "-" {
        return std::io::stdout()
            .lock()
            .write_all(data)
            .map_err(CliError::OutputWriteFailed);
    }

    let mut output_path = PathBuf::from(output_path);
    if output_path.is_dir() {
        let filename = source_filename.ok_or_else(|| CliError::DestinationFilenameUnknown)?;
        output_path.push(filename);
    }
    File::create(output_path)
        .map_err(CliError::OutputWriteFailed)?
        .write_all(data)
        .map_err(CliError::OutputWriteFailed)
}
