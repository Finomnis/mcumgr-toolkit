use std::io;

use miette::Diagnostic;
use thiserror::Error;

mod serial;
pub use serial::SerialTransport;

#[derive(Debug, PartialEq, Clone, Copy)]
struct SmpHeader {
    ver: u8,
    op: u8,
    flags: u8,
    data_length: u16,
    group_id: u16,
    sequence_num: u8,
    command_id: u8,
}

impl SmpHeader {
    fn from_bytes(data: [u8; SMP_HEADER_SIZE]) -> Self {
        Self {
            ver: (data[0] >> 3) & 0b11,
            op: data[0] & 0b111,
            flags: data[1],
            data_length: u16::from_be_bytes([data[2], data[3]]),
            group_id: u16::from_be_bytes([data[4], data[5]]),
            sequence_num: data[6],
            command_id: data[7],
        }
    }
    fn to_bytes(self) -> [u8; SMP_HEADER_SIZE] {
        let [length_0, length_1] = self.data_length.to_be_bytes();
        let [group_id_0, group_id_1] = self.group_id.to_be_bytes();
        [
            ((self.ver & 0b11) << 3) | (self.op & 0b111),
            self.flags,
            length_0,
            length_1,
            group_id_0,
            group_id_1,
            self.sequence_num,
            self.command_id,
        ]
    }
}

const SMP_HEADER_SIZE: usize = 8;
const SMP_TRANSFER_BUFFER_SIZE: usize = u16::MAX as usize;

mod smp_op {
    pub(super) const READ: u8 = 0;
    pub(super) const READ_RSP: u8 = 1;
    pub(super) const WRITE: u8 = 2;
    pub(super) const WRITE_RSP: u8 = 3;
}

/// Error while sending a command request
#[derive(Error, Debug, Diagnostic)]
pub enum SendError {
    /// An error occurred in the underlying transport
    #[error("transport error")]
    #[diagnostic(code(zephyr_mcumgr::transport::send::transport))]
    TransportError(#[from] io::Error),
    /// Unable to send data because it is too big
    #[error("given data slice was too big")]
    #[diagnostic(code(zephyr_mcumgr::transport::send::too_big))]
    DataTooBig,
}

/// Error while receiving a command response
#[derive(Error, Debug, Diagnostic)]
pub enum ReceiveError {
    /// An error occurred in the underlying transport
    #[error("transport error")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::transport))]
    TransportError(#[from] io::Error),
    /// We received a response that did not fit to our request
    #[error("received unexpected response")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::unexpected))]
    UnexpectedResponse,
    /// The response we received is bigger than the configured MTU
    #[error("received frame that exceeds configured MTU")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::too_big))]
    FrameTooBig,
    /// The response we received is not base64 encoded
    #[error("failed to decode base64 data")]
    #[diagnostic(code(zephyr_mcumgr::transport::recv::too_big))]
    Base64DecodeError(#[from] base64::DecodeSliceError),
}

/// Defines the API of the SMP transport layer
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

        let header_data = header.to_bytes();

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

            let header = SmpHeader::from_bytes(*header_data);

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
