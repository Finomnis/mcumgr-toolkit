use std::{
    io::{self, Read, Write},
    time::Duration,
};

use deku::prelude::*;
use thiserror::Error;

#[derive(Debug, PartialEq, DekuRead, DekuWrite)]
#[deku(endian = "big")]
struct SmpHeader {
    #[deku(bits = 3)]
    res: u8,
    #[deku(bits = 2)]
    ver: u8,
    #[deku(bits = 3)]
    op: u8,
    flags: u8,
    data_length: u16,
    group_id: u16,
    sequence_num: u8,
    command_id: u8,
}

const SMP_HEADER_SIZE: usize = 8;

mod SmpOp {
    pub const READ: u8 = 0;
    pub const READ_RSP: u8 = 1;
    pub const WRITE: u8 = 2;
    pub const WRITE_RSP: u8 = 3;
}

#[derive(Error, Debug)]
pub enum SendError {
    #[error("given data slice was too big")]
    DataTooBig,
    #[error("transport error")]
    Disconnect(#[from] io::Error),
    #[error("received unexpected response")]
    UnexpectedResponse,
}

pub trait Transport: Send + Read + Write {
    fn set_timeout(&mut self, timeout: Duration);

    fn send_frame(
        &mut self,
        write: bool,
        sequence_num: u8,
        group_id: u16,
        command_id: u8,
        data: &[u8],
    ) -> Result<(), SendError> {
        let header = SmpHeader {
            res: 0,
            ver: 0b01,
            op: if write { SmpOp::WRITE } else { SmpOp::READ },
            flags: 0,
            data_length: data.len().try_into().map_err(|_| SendError::DataTooBig)?,
            group_id,
            sequence_num,
            command_id,
        };

        let header_data = header.to_bytes().unwrap();

        self.write_all(&header_data)?;
        self.write_all(data)?;

        Ok(())
    }

    fn receive_frame(&mut self) -> Result<(), SendError> {
        let mut header_data = [0u8; SMP_HEADER_SIZE];

        loop {
            self.read_exact(&mut header_data)?;
            let header = SmpHeader::from_bytes((&header_data, 0)).unwrap().1;

            let mut data = vec![0u8; header.data_length.into()];
            self.read_exact(&mut data)?;

            // TODO check
        }
    }
}
