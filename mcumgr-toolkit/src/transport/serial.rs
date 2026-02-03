use std::time::Duration;

use base64::prelude::*;
use miette::IntoDiagnostic;
use ringbuf::{
    LocalRb,
    storage::Heap,
    traits::{Consumer, Observer, Producer},
};
use serialport::SerialPort;

use super::{ReceiveError, SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport};

/// A transport layer implementation for serial ports.
pub struct SerialTransport<T> {
    transfer_buffer: Box<[u8]>,
    body_buffer: Box<[u8]>,
    serial: T,
    crc_algo: crc::Crc<u16>,
    read_buffer: LocalRb<Heap<u8>>,
}

fn fill_buffer_with_data<'a, I: Iterator<Item = u8>>(
    buffer: &'a mut [u8],
    data_iter: &mut I,
) -> &'a [u8] {
    for (pos, val) in buffer.iter_mut().enumerate() {
        if let Some(next) = data_iter.next() {
            *val = next;
        } else {
            return &buffer[..pos];
        }
    }

    buffer
}

/// See Zephyr's [`MCUMGR_SERIAL_MAX_FRAME`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/include/zephyr/mgmt/mcumgr/transport/serial.h#L18).
const SERIAL_TRANSPORT_ZEPHYR_MTU: usize = 127;

impl<T> SerialTransport<T>
where
    T: std::io::Write + std::io::Read,
{
    /// Create a new [`SerialTransport`].
    ///
    /// # Arguments
    ///
    /// * `serial` - A serial port object, like [`serialport::SerialPort`].
    ///
    pub fn new(serial: T) -> Self {
        let mtu = SERIAL_TRANSPORT_ZEPHYR_MTU;
        Self {
            serial,
            transfer_buffer: vec![0u8; mtu].into_boxed_slice(),
            body_buffer: vec![0u8; ((mtu - 3) / 4) * 3].into_boxed_slice(),
            crc_algo: crc::Crc::<u16>::new(&crc::CRC_16_XMODEM),
            read_buffer: LocalRb::new(4096),
        }
    }

    /// Take a raw data stream, split it into SMP transport frames and transmit them.
    ///
    /// # Arguments
    ///
    /// * `data_iter` - An iterator that produces the binary data of the message to send.
    ///
    fn send_chunked<I: Iterator<Item = u8>>(&mut self, mut data_iter: I) -> Result<(), SendError> {
        self.transfer_buffer[0] = 6;
        self.transfer_buffer[1] = 9;

        loop {
            let body = fill_buffer_with_data(&mut self.body_buffer, &mut data_iter);

            if body.is_empty() {
                break Ok(());
            }

            let base64_len = BASE64_STANDARD
                .encode_slice(body, &mut self.transfer_buffer[2..])
                .expect("Transfer buffer overflow; this is a bug. Please report.");

            self.transfer_buffer[base64_len + 2] = 0x0a;

            self.serial
                .write_all(&self.transfer_buffer[..base64_len + 3])?;

            log::debug!(
                "Sent Chunk ({}, {} bytes raw, {} bytes encoded)",
                if self.transfer_buffer[0] == 6 {
                    "initial"
                } else {
                    "partial"
                },
                body.len(),
                base64_len,
            );

            self.transfer_buffer[0] = 4;
            self.transfer_buffer[1] = 20;
        }
    }

    /// Receive an SMP transport frame and decode it.
    ///
    /// # Arguments
    ///
    /// * `first` - whether this is the first first frame of the message.
    ///
    /// # Return
    ///
    /// The received data
    ///
    fn recv_chunk(&mut self, first: bool) -> Result<&[u8], ReceiveError> {
        let expected_header_0 = if first { 6 } else { 4 };
        let expected_header_1 = if first { 9 } else { 20 };

        loop {
            while self.read_buffer.occupied_len() < 2 {
                let num_read = self
                    .read_buffer
                    .read_from(&mut self.serial, None)
                    .unwrap()?;

                if num_read == 0 {
                    return Err(ReceiveError::TransportError(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Serial port unexpectedly returned end-of-file",
                    )));
                }
            }

            let current = self.read_buffer.try_pop().unwrap();
            let next = self.read_buffer.try_peek().unwrap();
            if current == expected_header_0 && *next == expected_header_1 {
                self.read_buffer.try_pop().unwrap();
                break;
            }
        }

        let mut base64_data = None;
        for (pos, elem) in self.transfer_buffer.iter_mut().enumerate() {
            let data = loop {
                if let Some(e) = self.read_buffer.try_pop() {
                    break e;
                } else {
                    let num_read = self
                        .read_buffer
                        .read_from(&mut self.serial, None)
                        .unwrap()?;

                    if num_read == 0 {
                        return Err(ReceiveError::TransportError(std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "Serial port unexpectedly returned end-of-file",
                        )));
                    }
                }
            };

            if data == 0x0a {
                base64_data = Some(&self.transfer_buffer[..pos]);
                break;
            }

            *elem = data;
        }

        if let Some(0x0a) = self.read_buffer.try_peek() {
            base64_data = Some(&self.transfer_buffer);
        }

        if let Some(base64_data) = base64_data {
            let len = BASE64_STANDARD.decode_slice(base64_data, &mut self.body_buffer)?;

            log::debug!(
                "Received Chunk ({}, {} bytes raw, {} bytes decoded)",
                if first { "initial" } else { "partial" },
                base64_data.len(),
                len
            );
            Ok(&self.body_buffer[..len])
        } else {
            Err(ReceiveError::FrameTooBig)
        }
    }
}

impl<T> Transport for SerialTransport<T>
where
    T: std::io::Write + std::io::Read + ConfigurableTimeout,
{
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), SendError> {
        log::debug!("Sending SMP Frame ({} bytes)", data.len());

        let checksum = {
            let mut digest = self.crc_algo.digest();
            digest.update(&header);
            digest.update(data);
            digest.finalize().to_be_bytes()
        };

        let size = u16::try_from(header.len() + data.len() + checksum.len())
            .map_err(|_| SendError::DataTooBig)?
            .to_be_bytes();

        self.send_chunked(
            size.into_iter()
                .chain(header)
                .chain(data.iter().copied())
                .chain(checksum),
        )
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], ReceiveError> {
        let first_chunk = self.recv_chunk(true)?;

        let (len, first_data) =
            if let Some((len_data, first_data)) = first_chunk.split_first_chunk::<2>() {
                (u16::from_be_bytes(*len_data), first_data)
            } else {
                return Err(ReceiveError::UnexpectedResponse);
            };

        let result_buffer = buffer
            .split_at_mut_checked(len.into())
            .ok_or(ReceiveError::FrameTooBig)?
            .0;

        let (first_result_buffer, mut leftover_result_buffer) = result_buffer
            .split_at_mut_checked(first_data.len())
            .ok_or(ReceiveError::UnexpectedResponse)?;

        first_result_buffer.copy_from_slice(first_data);

        while !leftover_result_buffer.is_empty() {
            let next_chunk = self.recv_chunk(false)?;

            let current_result_buffer;
            (current_result_buffer, leftover_result_buffer) = leftover_result_buffer
                .split_at_mut_checked(next_chunk.len())
                .ok_or(ReceiveError::UnexpectedResponse)?;

            current_result_buffer.copy_from_slice(next_chunk);
        }

        let (data, checksum_data) = result_buffer
            .split_last_chunk::<2>()
            .ok_or(ReceiveError::UnexpectedResponse)?;

        let expected_checksum = u16::from_be_bytes(*checksum_data);

        let actual_checksum = self.crc_algo.checksum(data);

        if expected_checksum != actual_checksum {
            return Err(ReceiveError::UnexpectedResponse);
        }

        log::debug!("Received SMP Frame ({} bytes)", data.len());

        Ok(data)
    }

    fn set_timeout(&mut self, timeout: std::time::Duration) -> Result<(), miette::Report> {
        ConfigurableTimeout::set_timeout(&mut self.serial, timeout)
    }
}

/// Specifies that the serial transport has a configurable timeout
pub trait ConfigurableTimeout {
    /// Changes the communication timeout.
    ///
    /// When the device does not respond within the set duration,
    /// an error will be returned.
    fn set_timeout(&mut self, duration: Duration) -> Result<(), miette::Report>;
}

impl<T: AsMut<dyn SerialPort> + ?Sized> ConfigurableTimeout for T {
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), miette::Report> {
        SerialPort::set_timeout(self.as_mut(), timeout).into_diagnostic()
    }
}
