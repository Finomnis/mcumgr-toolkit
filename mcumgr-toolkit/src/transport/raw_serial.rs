use crate::transport::{SmpHeader, serial::ConfigurableTimeout};

use super::{ReceiveError, SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport};

/// A transport layer implementation for serial ports, without any framing,
/// encoding or error detection.
///
/// Only suitable for stable serial connections like USB CDC ACM.
pub struct RawSerialTransport<T> {
    serial: T,
}

impl<T> RawSerialTransport<T> {
    /// Create a new [`RawSerialTransport`].
    ///
    /// # Arguments
    ///
    /// * `serial` - A serial port object, like [`serialport::SerialPort`].
    ///
    pub fn new(serial: T) -> Self {
        Self { serial }
    }
}

impl<T> Transport for RawSerialTransport<T>
where
    T: std::io::Write + std::io::Read + ConfigurableTimeout,
{
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), SendError> {
        self.serial.write_all(&header)?;
        self.serial.write_all(data)?;
        log::debug!("Sent Raw SMP Frame ({} bytes)", data.len());
        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], ReceiveError> {
        let (header, data) = buffer.split_first_chunk_mut::<SMP_HEADER_SIZE>().unwrap();

        self.serial.read_exact(header)?;
        let header = SmpHeader::from_bytes(*header);

        let len = header.data_length as usize;

        let data = &mut data[..len];
        self.serial.read_exact(data)?;

        log::debug!("Received UDP SMP Frame ({} bytes)", len);
        Ok(&buffer[..SMP_HEADER_SIZE + len])
    }

    fn set_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        ConfigurableTimeout::set_timeout(&mut self.serial, timeout)
    }
}
