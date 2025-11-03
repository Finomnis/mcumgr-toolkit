use std::io::Cursor;

use crate::{
    commands::{ErrResponse, ErrResponseV2, McuMgrRequest},
    transport::{ReceiveError, SendError, Transport},
};

use miette::Diagnostic;
use thiserror::Error;

pub struct Connection {
    transport: Box<dyn Transport>,
    next_seqnum: u8,
    transport_buffer: [u8; u16::MAX as usize],
}

#[derive(Debug)]
pub enum DeviceError {
    V1 { rc: i32 },
    V2 { group: u32, rc: u32 },
}

#[derive(Error, Debug, Diagnostic)]
pub enum ExecuteError {
    #[error("sending failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::send))]
    SendFailed(#[from] SendError),
    #[error("receiving failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::receive))]
    ReceiveFailed(#[from] ReceiveError),
    #[error("cbor encoding failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::encode))]
    EncodeFailed,
    #[error("cbor decoding failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::decode))]
    DecodeFailed,
    #[error("device returned an error")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::device_error))]
    ErrorResponse(DeviceError),
}

impl Connection {
    pub fn new<T: Transport + 'static>(transport: T) -> Self {
        Self {
            transport: Box::new(transport),
            next_seqnum: rand::random(),
            transport_buffer: [0; u16::MAX as usize],
        }
    }

    pub fn execute_cbor<R: McuMgrRequest>(
        &mut self,
        request: &R,
    ) -> Result<R::Response, ExecuteError> {
        let mut cursor = Cursor::new(self.transport_buffer.as_mut_slice());
        ciborium::into_writer(request, &mut cursor).map_err(|_| ExecuteError::EncodeFailed)?;
        let data_size = cursor.position() as usize;
        let data = &self.transport_buffer[..data_size];

        let sequence_num = self.next_seqnum;
        self.next_seqnum = self.next_seqnum.wrapping_add(1);

        self.transport.send_frame(
            R::WRITE_OPERATION,
            sequence_num,
            R::GROUP_ID,
            R::COMMAND_ID,
            data,
        )?;

        let response = self.transport.receive_frame(
            &mut self.transport_buffer,
            R::WRITE_OPERATION,
            sequence_num,
            R::GROUP_ID,
            R::COMMAND_ID,
        )?;

        let err: ErrResponse =
            ciborium::from_reader(Cursor::new(response)).map_err(|_| ExecuteError::DecodeFailed)?;

        if let Some(ErrResponseV2 { rc, group }) = err.err {
            return Err(ExecuteError::ErrorResponse(DeviceError::V2 { group, rc }));
        }

        if let Some(rc) = err.rc {
            return Err(ExecuteError::ErrorResponse(DeviceError::V1 { rc }));
        }

        let decoded_response: R::Response =
            ciborium::from_reader(Cursor::new(response)).map_err(|_| ExecuteError::DecodeFailed)?;

        Ok(decoded_response)
    }

    pub fn execute_raw(
        &mut self,
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        data: &[u8],
    ) -> Result<&[u8], ExecuteError> {
        let sequence_num = self.next_seqnum;
        self.next_seqnum = self.next_seqnum.wrapping_add(1);

        self.transport
            .send_frame(write_operation, sequence_num, group_id, command_id, data)?;

        self.transport
            .receive_frame(
                &mut self.transport_buffer,
                write_operation,
                sequence_num,
                group_id,
                command_id,
            )
            .map_err(Into::into)
    }
}
