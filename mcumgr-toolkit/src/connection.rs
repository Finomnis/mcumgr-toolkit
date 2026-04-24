use std::{io::Cursor, sync::Mutex, time::Duration};

use crate::{
    DEFAULT_RETRIES,
    commands::{ErrResponse, ErrResponseV2, McuMgrCommand},
    smp_errors::{DeviceError, MCUmgrErr},
    transport::{ReceiveError, SendError, Transport},
};

use miette::{Diagnostic, IntoDiagnostic};
use polonius_the_crab::prelude::*;
use thiserror::Error;

struct Transceiver {
    transport: Box<dyn Transport + Send>,
    next_seqnum: u8,
    receive_buffer: Box<[u8; u16::MAX as usize]>,
}

struct Inner {
    transceiver: Transceiver,
    send_buffer: Box<[u8; u16::MAX as usize]>,
    retries: u8,
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
    #[diagnostic(code(mcumgr_toolkit::connection::execute::send))]
    SendFailed(#[from] SendError),
    /// An error happened on SMP transport level while receiving a response
    #[error("Receiving failed")]
    #[diagnostic(code(mcumgr_toolkit::connection::execute::receive))]
    ReceiveFailed(#[from] ReceiveError),
    /// An error happened while CBOR encoding the request payload
    #[error("CBOR encoding failed")]
    #[diagnostic(code(mcumgr_toolkit::connection::execute::encode))]
    EncodeFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync>),
    /// An error happened while CBOR decoding the response payload
    #[error("CBOR decoding failed")]
    #[diagnostic(code(mcumgr_toolkit::connection::execute::decode))]
    DecodeFailed(#[source] Box<dyn miette::Diagnostic + Send + Sync>),
    /// The device returned an SMP error
    #[error("Device returned error code: {0}")]
    #[diagnostic(code(mcumgr_toolkit::connection::execute::device_error))]
    ErrorResponse(DeviceError),
}

impl ExecuteError {
    /// Checks if the device reported the command as unsupported
    pub fn command_not_supported(&self) -> bool {
        if let Self::ErrorResponse(DeviceError::V1 { rc, .. }) = self {
            *rc == MCUmgrErr::MGMT_ERR_ENOTSUP as i32
        } else {
            false
        }
    }
}

impl Transceiver {
    fn transceive_command(
        &mut self,
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        data: &[u8],
    ) -> Result<&'_ [u8], ExecuteError> {
        let sequence_num = self.next_seqnum;
        self.next_seqnum = self.next_seqnum.wrapping_add(1);

        self.transport
            .send_frame(write_operation, sequence_num, group_id, command_id, data)?;

        self.transport
            .receive_frame(
                &mut self.receive_buffer,
                write_operation,
                sequence_num,
                group_id,
                command_id,
            )
            .map_err(Into::into)
    }

    fn transceive_command_with_retries(
        &mut self,
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        data: &[u8],
        num_retries: u8,
    ) -> Result<&'_ [u8], ExecuteError> {
        let mut this = self;

        let mut counter = 0;

        polonius_loop!(|this| -> Result<&'polonius [u8], ExecuteError> {
            let result = this.transceive_command(write_operation, group_id, command_id, data);

            if counter >= num_retries {
                polonius_return!(result)
            }
            counter += 1;

            match result {
                Ok(_) => polonius_return!(result),
                Err(e) => {
                    let mut lowest_err: &dyn std::error::Error = &e;
                    while let Some(lower_err) = lowest_err.source() {
                        lowest_err = lower_err;
                    }
                    log::warn!("Retry transmission, error occurred: {lowest_err}");
                }
            }
        })
    }
}

impl Connection {
    /// Creates a new SMP connection
    pub fn new<T: Transport + Send + 'static>(transport: T) -> Self {
        Self {
            inner: Mutex::new(Inner {
                transceiver: Transceiver {
                    transport: Box::new(transport),
                    next_seqnum: rand::random(),
                    receive_buffer: Box::new([0; u16::MAX as usize]),
                },
                send_buffer: Box::new([0; u16::MAX as usize]),
                retries: DEFAULT_RETRIES,
            }),
        }
    }

    /// Returns the maximum SMP frame size the underlying transport can
    /// deliver reliably.
    pub fn max_transport_frame_size(&self) -> usize {
        self.inner
            .lock()
            .unwrap()
            .transceiver
            .transport
            .max_smp_frame_size()
    }

    /// Changes the communication timeout.
    ///
    /// When the device does not respond to packets within the set
    /// duration, an error will be raised.
    pub fn set_timeout(
        &self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner
            .lock()
            .unwrap()
            .transceiver
            .transport
            .set_timeout(timeout)
    }

    /// Changes the retry amount.
    ///
    /// When the device encounters a transport error, it will retry
    /// this many times until giving up.
    pub fn set_retries(&self, retries: u8) {
        self.inner.lock().unwrap().retries = retries;
    }

    /// Executes a given CBOR based SMP command.
    pub fn execute_command<R: McuMgrCommand>(
        &self,
        request: &R,
    ) -> Result<R::Response, ExecuteError> {
        self.execute_command_impl(request, true)
    }

    /// Executes a given CBOR based SMP command.
    ///
    /// Does not use retries.
    pub fn execute_command_without_retries<R: McuMgrCommand>(
        &self,
        request: &R,
    ) -> Result<R::Response, ExecuteError> {
        self.execute_command_impl(request, false)
    }

    fn execute_command_impl<R: McuMgrCommand>(
        &self,
        request: &R,
        use_retries: bool,
    ) -> Result<R::Response, ExecuteError> {
        let mut lock_guard = self.inner.lock().unwrap();
        let locked_self: &mut Inner = &mut lock_guard;

        let mut cursor = Cursor::new(locked_self.send_buffer.as_mut_slice());
        ciborium::into_writer(request.data(), &mut cursor)
            .into_diagnostic()
            .map_err(Into::into)
            .map_err(ExecuteError::EncodeFailed)?;
        let data_size = cursor.position() as usize;
        let data = &locked_self.send_buffer[..data_size];

        log::debug!("TX data: {}", hex::encode(data));

        let write_operation = request.is_write_operation();
        let group_id = request.group_id();
        let command_id = request.command_id();

        let response = locked_self.transceiver.transceive_command_with_retries(
            write_operation,
            group_id,
            command_id,
            data,
            if use_retries { locked_self.retries } else { 0 },
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
            if rc != MCUmgrErr::MGMT_ERR_EOK as i32 {
                return Err(ExecuteError::ErrorResponse(DeviceError::V1 {
                    rc,
                    rsn: err.rsn,
                }));
            }
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
        use_retries: bool,
    ) -> Result<Box<[u8]>, ExecuteError> {
        let mut lock_guard = self.inner.lock().unwrap();
        let locked_self: &mut Inner = &mut lock_guard;

        locked_self
            .transceiver
            .transceive_command_with_retries(
                write_operation,
                group_id,
                command_id,
                data,
                if use_retries { locked_self.retries } else { 0 },
            )
            .map(|val| val.into())
    }
}
