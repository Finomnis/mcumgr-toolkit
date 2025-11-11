use std::{
    fs::File,
    io::{Read, Write},
};

use crate::CliError;

pub fn read_input_file(filename: &str) -> Result<Box<[u8]>, CliError> {
    if filename == "-" {
        let mut data = Vec::new();
        std::io::stdin()
            .lock()
            .read_to_end(&mut data)
            .map_err(CliError::InputReadFailed)?;
        Ok(data.into_boxed_slice())
    } else {
        let mut f = File::open(filename).map_err(CliError::InputReadFailed)?;
        let mut data = vec![];
        f.read_to_end(&mut data)
            .map_err(CliError::InputReadFailed)?;
        Ok(data.into_boxed_slice())
    }
}

pub fn write_output_file(filename: &str, data: &[u8]) -> Result<(), CliError> {
    if filename == "-" {
        std::io::stdout()
            .lock()
            .write_all(&data)
            .map_err(CliError::OutputWriteFailed)
    } else {
        File::create(filename)
            .map_err(CliError::OutputWriteFailed)?
            .write_all(&data)
            .map_err(CliError::OutputWriteFailed)
    }
}
