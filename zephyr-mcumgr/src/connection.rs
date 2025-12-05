use std::{io::Cursor, sync::Mutex, time::Duration};

use crate::{
    commands::{ErrResponse, ErrResponseV2, McuMgrCommand},
    smp_errors::DeviceError,
    transport::{ReceiveError, SendError, Transport},
};

use miette::{Diagnostic, IntoDiagnostic};
use thiserror::Error;

struct Inner {
    transport: Box<dyn Transport + Send>,
    next_seqnum: u8,
    transport_buffer: Box<[u8; u16::MAX as usize]>,
}

/// An SMP protocol layer connection to a device.
///
/// In most cases this struct will not be used directly by the user,
/// but instead it is used indirectly through [`MCUmgrClient`](crate::MCUmgrClient).
pub struct Connection {
    inner: Mutex<Inner>,
}

/// Errors that can happen on SMP protocol level
#[derive(Error, Debug, Diagnostic)]
pub enum ExecuteError {
    /// An error happened on SMP transport level while sending a request
    #[error("Sending failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::send))]
    SendFailed(#[from] SendError),
    /// An error happened on SMP transport level while receiving a response
    #[error("Receiving failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::receive))]
    ReceiveFailed(#[from] ReceiveError),
    /// An error happened while CBOR encoding the request payload
    #[error("CBOR encoding failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::encode))]
    EncodeFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync>),
    /// An error happened while CBOR decoding the response payload
    #[error("CBOR decoding failed")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::decode))]
    DecodeFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync>),
    /// The device returned an SMP error
    #[error("Device returned error code: {0}")]
    #[diagnostic(code(zephyr_mcumgr::connection::execute::device_error))]
    ErrorResponse(DeviceError),
}

impl Connection {
    /// Creates a new SMP
    pub fn new<T: Transport + Send + 'static>(transport: T) -> Self {
        Self {
            inner: Mutex::new(Inner {
                transport: Box::new(transport),
                next_seqnum: rand::random(),
                transport_buffer: Box::new([0; u16::MAX as usize]),
            }),
        }
    }

    /// Changes the communication timeout.
    ///
    /// When the device does not respond to packets within the set
    /// duration, an error will be raised.
    pub fn set_timeout(&self, timeout: Duration) -> Result<(), miette::Report> {
        self.inner.lock().unwrap().transport.set_timeout(timeout)
    }

    /// Executes a given CBOR based SMP command.
    pub fn execute_command<R: McuMgrCommand>(
        &self,
        request: &R,
    ) -> Result<R::Response, ExecuteError> {
        let mut lock_guard = self.inner.lock().unwrap();
        let locked_self: &mut Inner = &mut lock_guard;

        let mut cursor = Cursor::new(locked_self.transport_buffer.as_mut_slice());
        ciborium::into_writer(request.data(), &mut cursor)
            .into_diagnostic()
            .map_err(Into::into)
            .map_err(ExecuteError::EncodeFailed)?;
        let data_size = cursor.position() as usize;
        let data = &locked_self.transport_buffer[..data_size];

        log::debug!("TX data: {}", hex::encode(data));

        let sequence_num = locked_self.next_seqnum;
        locked_self.next_seqnum = locked_self.next_seqnum.wrapping_add(1);

        let write_operation = request.is_write_operation();
        let group_id = request.group_id();
        let command_id = request.command_id();

        locked_self.transport.send_frame(
            write_operation,
            sequence_num,
            group_id,
            command_id,
            data,
        )?;

        let response = locked_self.transport.receive_frame(
            &mut locked_self.transport_buffer,
            write_operation,
            sequence_num,
            group_id,
            command_id,
        )?;

        log::debug!("RX data: {}", hex::encode(response));

        let err: ErrResponse = ciborium::from_reader(Cursor::new(response))
            .into_diagnostic()
            .map_err(Into::into)
            .map_err(ExecuteError::DecodeFailed)?;

        if let Some(ErrResponseV2 { rc, group }) = err.err {
            return Err(ExecuteError::ErrorResponse(DeviceError::V2 { group, rc }));
        }

        if let Some(rc) = err.rc {
            return Err(ExecuteError::ErrorResponse(DeviceError::V1 { rc }));
        }

        let decoded_response: R::Response = ciborium::from_reader(Cursor::new(response))
            .into_diagnostic()
            .map_err(Into::into)
            .map_err(ExecuteError::DecodeFailed)?;

        Ok(decoded_response)
    }

    /// Executes a raw SMP command.
    ///
    /// Same as [`Connection::execute_command`], but the payload can be anything and must not
    /// necessarily be CBOR encoded.
    ///
    /// Errors are also not decoded but instead will be returned as raw CBOR data.
    ///
    /// Read Zephyr's [SMP Protocol Specification](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html)
    /// for more information.
    pub fn execute_raw_command(
        &self,
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        data: &[u8],
    ) -> Result<Box<[u8]>, ExecuteError> {
        let mut lock_guard = self.inner.lock().unwrap();
        let locked_self: &mut Inner = &mut lock_guard;

        let sequence_num = locked_self.next_seqnum;
        locked_self.next_seqnum = locked_self.next_seqnum.wrapping_add(1);

        locked_self.transport.send_frame(
            write_operation,
            sequence_num,
            group_id,
            command_id,
            data,
        )?;

        locked_self
            .transport
            .receive_frame(
                &mut locked_self.transport_buffer,
                write_operation,
                sequence_num,
                group_id,
                command_id,
            )
            .map_err(Into::into)
            .map(|val| val.into())
    }
}
