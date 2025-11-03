use std::io;

use deku::prelude::*;
use miette::Diagnostic;
use thiserror::Error;

mod serial;
pub use serial::{SERIAL_TRANSPORT_DEFAULT_MTU, SerialTransport};

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
pub const SMP_TRANSFER_BUFFER_SIZE: usize = u16::MAX as usize;

mod smp_op {
    pub const READ: u8 = 0;
    pub const READ_RSP: u8 = 1;
    pub const WRITE: u8 = 2;
    pub const WRITE_RSP: u8 = 3;
}

#[derive(Error, Debug, Diagnostic)]
pub enum SendError {
    #[error("transport error")]
    #[diagnostic(code(zephyr_mcumgr::transport::send::transport))]
    TransportError(#[from] io::Error),
    #[error("given data slice was too big")]
    #[diagnostic(code(zephyr_mcumgr::transport::send::too_big))]
    DataTooBig,
}

#[derive(Error, Debug, Diagnostic)]
pub enum ReceiveError {
    #[error("transport error")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::transport))]
    TransportError(#[from] io::Error),
    #[error("received unexpected response")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::unexpected))]
    UnexpectedResponse,
    #[error("received frame that exceeds configured MTU")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::too_big))]
    FrameTooBig,
    #[error("received frame that exceeds configured MTU")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::too_big))]
    Base64DecodeError(#[from] base64::DecodeSliceError),
}

pub trait Transport {
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), SendError>;

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], ReceiveError>;

    fn send_frame(
        &mut self,
        write_operation: bool,
        sequence_num: u8,
        group_id: u16,
        command_id: u8,
        data: &[u8],
    ) -> Result<(), SendError> {
        let header = SmpHeader {
            res: 0,
            ver: 0b01,
            op: if write_operation {
                smp_op::WRITE
            } else {
                smp_op::READ
            },
            flags: 0,
            data_length: data.len().try_into().map_err(|_| SendError::DataTooBig)?,
            group_id,
            sequence_num,
            command_id,
        };

        let mut header_data = [0u8; SMP_HEADER_SIZE];
        header.to_slice(&mut header_data).unwrap();

        self.send_raw_frame(header_data, data)
    }

    fn receive_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
        write_operation: bool,
        sequence_num: u8,
        group_id: u16,
        command_id: u8,
    ) -> Result<&'a [u8], ReceiveError> {
        let data_size = loop {
            let frame = self.recv_raw_frame(buffer)?;

            let (header_data, data) = frame
                .split_first_chunk::<SMP_HEADER_SIZE>()
                .ok_or(ReceiveError::UnexpectedResponse)?;

            let header = SmpHeader::from_bytes((header_data, 0)).unwrap().1;

            let expected_op = if write_operation {
                smp_op::WRITE_RSP
            } else {
                smp_op::READ_RSP
            };

            // Receiving packets with the wrong sequence number is not an error,
            // they should simply be silently ignored.
            if header.sequence_num != sequence_num {
                continue;
            }

            if (header.group_id != group_id)
                || (header.command_id != command_id)
                || (header.op != expected_op)
                || (usize::from(header.data_length) != data.len())
            {
                return Err(ReceiveError::UnexpectedResponse);
            }

            break data.len();
        };

        Ok(&buffer[SMP_HEADER_SIZE..SMP_HEADER_SIZE + data_size])
    }
}
