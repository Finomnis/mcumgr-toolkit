use std::io::{self, Read, Write};

use miette::Diagnostic;
use thiserror::Error;

use crate::{
    commands::{self, fs::file_upload_max_data_chunk_size},
    connection::{Connection, ExecuteError},
    transport::SerialTransport,
};

/// The default SMP frame size of Zephyr.
///
/// Matches Zephyr default value of [MCUMGR_TRANSPORT_NETBUF_SIZE](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40).
pub const ZEPHYR_DEFAULT_SMP_FRAME_SIZE: usize = 384;

pub struct MCUmgrClient {
    connection: Connection,
    smp_frame_size: usize,
}

#[derive(Error, Debug, Diagnostic)]
pub enum FileDownloadError {
    #[error("command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::execute))]
    ExecuteError(#[from] ExecuteError),
    #[error("received offset does not match requested offset")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::offset_mismatch))]
    UnexpectedOffset,
    #[error("writer returned an error")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::writer))]
    WriterError(#[from] io::Error),
    #[error("received data does not match reported size")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::size_mismatch))]
    SizeMismatch,
    #[error("received data is missing file size information")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::missing_size))]
    MissingSize,
}

#[derive(Error, Debug, Diagnostic)]
pub enum FileUploadError {
    #[error("command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::client::file_upload::execute))]
    ExecuteError(#[from] ExecuteError),
    #[error("writer returned an error")]
    #[diagnostic(code(zephyr_mcumgr::client::file_upload::reader))]
    ReaderError(#[from] io::Error),
}

impl MCUmgrClient {
    pub fn from_serial<T: Send + Read + Write + 'static>(serial: T) -> Self {
        Self {
            connection: Connection::new(SerialTransport::new(serial)),
            smp_frame_size: ZEPHYR_DEFAULT_SMP_FRAME_SIZE,
        }
    }

    pub fn with_frame_size(mut self, smp_frame_size: usize) -> Self {
        self.smp_frame_size = smp_frame_size;
        self
    }

    pub fn use_auto_frame_size(&mut self) -> Result<(), ExecuteError> {
        let mcumgr_params = self
            .connection
            .execute_cbor(&commands::os::MCUmgrParameters)?;

        self.smp_frame_size = mcumgr_params.buf_size as usize;

        log::debug!("Using frame size {}.", self.smp_frame_size);

        Ok(())
    }

    pub fn os_echo(&mut self, msg: impl AsRef<str>) -> Result<String, ExecuteError> {
        self.connection
            .execute_cbor(&commands::os::Echo { d: msg.as_ref() })
            .map(|resp| resp.r)
    }

    /// Load a file from the device.
    ///
    ///  # Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `writer` - A [`Write`] object that the file content will be written to.
    ///
    /// # Performance
    ///
    /// Downloading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` or larger.
    pub fn fs_file_download<T: Write>(
        &mut self,
        name: impl AsRef<str>,
        mut writer: T,
    ) -> Result<(), FileDownloadError> {
        let name = name.as_ref();
        let response = self
            .connection
            .execute_cbor(&commands::fs::FileDownload { name, off: 0 })?;

        let file_len = response.len.ok_or(FileDownloadError::MissingSize)?;
        if response.off != 0 {
            return Err(FileDownloadError::UnexpectedOffset);
        }

        let mut offset = 0;

        writer.write_all(&response.data)?;
        offset += response.data.len() as u64;

        while offset < file_len {
            let response = self
                .connection
                .execute_cbor(&commands::fs::FileDownload { name, off: offset })?;

            if response.off != offset {
                return Err(FileDownloadError::UnexpectedOffset);
            }

            writer.write_all(&response.data)?;
            offset += response.data.len() as u64;
        }

        if offset != file_len {
            return Err(FileDownloadError::SizeMismatch);
        }

        Ok(())
    }

    /// Write a file to the device.
    ///
    ///  # Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `reader` - A [`Read`] object that contains the file content.
    /// * `size` - The file size.
    ///
    /// # Performance
    ///
    /// Uploading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` and then enable larger chunking through either [`MCUmgrClient::with_frame_size`]
    /// or [`MCUmgrClient::use_auto_frame_size`].
    pub fn fs_file_upload<T: Read>(
        &mut self,
        name: impl AsRef<str>,
        mut reader: T,
        size: u64,
    ) -> Result<(), FileUploadError> {
        let name = name.as_ref();

        let chunk_size_max = file_upload_max_data_chunk_size(self.smp_frame_size);
        let mut data_buffer = vec![0u8; chunk_size_max].into_boxed_slice();

        let mut offset = 0;

        while offset < size {
            let current_chunk_size = (size - offset).min(data_buffer.len() as u64) as usize;

            let chunk_buffer = &mut data_buffer[..current_chunk_size];
            reader.read_exact(chunk_buffer)?;

            self.connection.execute_cbor(&commands::fs::FileUpload {
                off: offset,
                data: chunk_buffer,
                name,
                len: if offset == 0 { Some(size) } else { None },
            })?;

            offset += chunk_buffer.len() as u64;
        }

        Ok(())
    }
}
