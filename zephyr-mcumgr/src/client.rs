use std::{
    collections::HashMap,
    io::{self, Read, Write},
    sync::atomic::AtomicUsize,
    time::Duration,
};

use miette::Diagnostic;
use thiserror::Error;

use crate::{
    bootloader::BootloaderInfo,
    commands::{self, fs::file_upload_max_data_chunk_size},
    connection::{Connection, ExecuteError},
    transport::serial::{ConfigurableTimeout, SerialTransport},
};

/// The default SMP frame size of Zephyr.
///
/// Matches Zephyr default value of [MCUMGR_TRANSPORT_NETBUF_SIZE](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40).
const ZEPHYR_DEFAULT_SMP_FRAME_SIZE: usize = 384;

/// A high level client for Zephyr's MCUmgr SMP protocol.
///
/// This struct is the central entry point of this crate.
pub struct MCUmgrClient {
    connection: Connection,
    smp_frame_size: AtomicUsize,
}

/// Possible error values of [`MCUmgrClient::fs_file_download`].
#[derive(Error, Debug, Diagnostic)]
pub enum FileDownloadError {
    /// The command failed in the SMP protocol layer.
    #[error("Command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::execute))]
    ExecuteError(#[from] ExecuteError),
    /// A device response contained an unexpected offset value.
    #[error("Received offset does not match requested offset")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::offset_mismatch))]
    UnexpectedOffset,
    /// The writer returned an error.
    #[error("Writer returned an error")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::writer))]
    WriterError(#[from] io::Error),
    /// The received data does not match the reported file size.
    #[error("Received data does not match reported size")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::size_mismatch))]
    SizeMismatch,
    /// The received data unexpectedly did not report the file size.
    #[error("Received data is missing file size information")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::missing_size))]
    MissingSize,
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(zephyr_mcumgr::client::file_download::progress_cb_error))]
    ProgressCallbackError,
}

/// Possible error values of [`MCUmgrClient::fs_file_upload`].
#[derive(Error, Debug, Diagnostic)]
pub enum FileUploadError {
    /// The command failed in the SMP protocol layer.
    #[error("Command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::client::file_upload::execute))]
    ExecuteError(#[from] ExecuteError),
    /// The reader returned an error.
    #[error("Reader returned an error")]
    #[diagnostic(code(zephyr_mcumgr::client::file_upload::reader))]
    ReaderError(#[from] io::Error),
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(zephyr_mcumgr::client::file_upload::progress_cb_error))]
    ProgressCallbackError,
    /// The current SMP frame size is too small for this command.
    #[error("SMP frame size too small for this command")]
    #[diagnostic(code(zephyr_mcumgr::client::file_upload::framesize_too_small))]
    FrameSizeTooSmall(#[source] io::Error),
}

impl MCUmgrClient {
    /// Creates a Zephyr MCUmgr SMP client based on a configured and opened serial port.
    ///
    /// ```no_run
    /// # use zephyr_mcumgr::MCUmgrClient;
    /// # fn main() {
    /// let serial = serialport::new("COM42", 115200)
    ///     .timeout(std::time::Duration::from_millis(500))
    ///     .open()
    ///     .unwrap();
    ///
    /// let mut client = MCUmgrClient::new_from_serial(serial);
    /// # }
    /// ```
    pub fn new_from_serial<T: Send + Read + Write + ConfigurableTimeout + 'static>(
        serial: T,
    ) -> Self {
        Self {
            connection: Connection::new(SerialTransport::new(serial)),
            smp_frame_size: ZEPHYR_DEFAULT_SMP_FRAME_SIZE.into(),
        }
    }

    /// Configures the maximum SMP frame size that we can send to the device.
    ///
    /// Must not exceed [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40),
    /// otherwise we might crash the device.
    pub fn set_frame_size(&self, smp_frame_size: usize) {
        self.smp_frame_size
            .store(smp_frame_size, std::sync::atomic::Ordering::SeqCst);
    }

    /// Configures the maximum SMP frame size that we can send to the device automatically
    /// by reading the value of [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// from the device.
    pub fn use_auto_frame_size(&self) -> Result<(), ExecuteError> {
        let mcumgr_params = self
            .connection
            .execute_command(&commands::os::MCUmgrParameters)?;

        log::debug!("Using frame size {}.", mcumgr_params.buf_size);

        self.smp_frame_size.store(
            mcumgr_params.buf_size as usize,
            std::sync::atomic::Ordering::SeqCst,
        );

        Ok(())
    }

    /// Changes the communication timeout.
    ///
    /// When the device does not respond to packets within the set
    /// duration, an error will be raised.
    pub fn set_timeout(&self, timeout: Duration) -> Result<(), miette::Report> {
        self.connection.set_timeout(timeout)
    }

    /// Sends a message to the device and expects the same message back as response.
    ///
    /// This can be used as a sanity check for whether the device is connected and responsive.
    pub fn os_echo(&self, msg: impl AsRef<str>) -> Result<String, ExecuteError> {
        self.connection
            .execute_command(&commands::os::Echo { d: msg.as_ref() })
            .map(|resp| resp.r)
    }

    /// Queries live task statistics
    ///
    /// # Note
    ///
    /// Converts `stkuse` and `stksiz` to bytes.
    /// Zephyr originally reports them as number of 4 byte words.
    ///
    /// # Return
    ///
    /// A map of task names with their respective statistics
    pub fn os_task_statistics(
        &self,
    ) -> Result<HashMap<String, commands::os::TaskStatisticsEntry>, ExecuteError> {
        self.connection
            .execute_command(&commands::os::TaskStatistics)
            .map(|resp| {
                let mut tasks = resp.tasks;
                for (_, stats) in tasks.iter_mut() {
                    stats.stkuse = stats.stkuse.map(|val| val * 4);
                    stats.stksiz = stats.stksiz.map(|val| val * 4);
                }
                tasks
            })
    }

    /// Sets the RTC of the device to the given datetime.
    pub fn os_set_datetime(&self, datetime: chrono::NaiveDateTime) -> Result<(), ExecuteError> {
        self.connection
            .execute_command(&commands::os::DateTimeSet { datetime })
            .map(Into::into)
    }

    /// Retrieves the device RTC's datetime.
    pub fn os_get_datetime(&self) -> Result<chrono::NaiveDateTime, ExecuteError> {
        self.connection
            .execute_command(&commands::os::DateTimeGet)
            .map(|val| val.datetime)
    }

    /// Issues a system reset.
    ///
    /// # Arguments
    ///
    /// * `force` - Issues a force reset.
    /// * `boot_mode` - Overwrites the boot mode.
    ///
    /// Known `boot_mode` values:
    /// * `0` - Normal system boot
    /// * `1` - Bootloader recovery mode
    ///
    /// Note that `boot_mode` only works if [`MCUMGR_GRP_OS_RESET_BOOT_MODE`](https://docs.zephyrproject.org/latest/kconfig.html#CONFIG_MCUMGR_GRP_OS_RESET_BOOT_MODE) is enabled.
    ///
    pub fn os_system_reset(&self, force: bool, boot_mode: Option<u8>) -> Result<(), ExecuteError> {
        self.connection
            .execute_command(&commands::os::SystemReset { force, boot_mode })
            .map(Into::into)
    }

    /// Fetch parameters from the MCUmgr library
    pub fn os_mcumgr_parameters(
        &self,
    ) -> Result<commands::os::MCUmgrParametersResponse, ExecuteError> {
        self.connection
            .execute_command(&commands::os::MCUmgrParameters)
    }

    /// Fetch information on the running image
    ///
    /// Similar to Linux's `uname` command.
    ///
    /// # Arguments
    ///
    /// * `format` - Format specifier for the returned response
    ///
    /// For more information about the format specifier fields, see
    /// the [SMP documentation](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#os-application-info-request).
    ///
    pub fn os_application_info(&self, format: Option<&str>) -> Result<String, ExecuteError> {
        self.connection
            .execute_command(&commands::os::ApplicationInfo { format })
            .map(|resp| resp.output)
    }

    /// Fetch information on the device's bootloader
    pub fn os_bootloader_info(&self) -> Result<BootloaderInfo, ExecuteError> {
        Ok(
            match self
                .connection
                .execute_command(&commands::os::BootloaderInfo)?
                .bootloader
                .as_str()
            {
                "MCUboot" => {
                    let mode_data = self
                        .connection
                        .execute_command(&commands::os::BootloaderInfoMcubootMode {})?;
                    BootloaderInfo::MCUboot {
                        mode: mode_data.mode,
                        no_downgrade: mode_data.no_downgrade,
                    }
                }
                name => BootloaderInfo::Unknown {
                    name: name.to_string(),
                },
            },
        )
    }

    /// Obtain a list of images with their current state.
    pub fn image_get_state(&self) -> Result<Vec<commands::image::ImageState>, ExecuteError> {
        self.connection
            .execute_command(&commands::image::GetImageState)
            .map(|val| val.images)
    }

    /// Load a file from the device.
    ///
    /// # Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `writer` - A [`Write`] object that the file content will be written to.
    /// * `progress` - A callback that receives a pair of (transferred, total) bytes.
    ///
    /// # Performance
    ///
    /// Downloading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` or larger.
    pub fn fs_file_download<T: Write>(
        &self,
        name: impl AsRef<str>,
        mut writer: T,
        mut progress: Option<&mut dyn FnMut(u64, u64) -> bool>,
    ) -> Result<(), FileDownloadError> {
        let name = name.as_ref();
        let response = self
            .connection
            .execute_command(&commands::fs::FileDownload { name, off: 0 })?;

        let file_len = response.len.ok_or(FileDownloadError::MissingSize)?;
        if response.off != 0 {
            return Err(FileDownloadError::UnexpectedOffset);
        }

        let mut offset = 0;

        if let Some(progress) = &mut progress {
            if !progress(offset, file_len) {
                return Err(FileDownloadError::ProgressCallbackError);
            };
        }

        writer.write_all(&response.data)?;
        offset += response.data.len() as u64;

        if let Some(progress) = &mut progress {
            if !progress(offset, file_len) {
                return Err(FileDownloadError::ProgressCallbackError);
            };
        }

        while offset < file_len {
            let response = self
                .connection
                .execute_command(&commands::fs::FileDownload { name, off: offset })?;

            if response.off != offset {
                return Err(FileDownloadError::UnexpectedOffset);
            }

            writer.write_all(&response.data)?;
            offset += response.data.len() as u64;

            if let Some(progress) = &mut progress {
                if !progress(offset, file_len) {
                    return Err(FileDownloadError::ProgressCallbackError);
                };
            }
        }

        if offset != file_len {
            return Err(FileDownloadError::SizeMismatch);
        }

        Ok(())
    }

    /// Write a file to the device.
    ///
    /// # Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `reader` - A [`Read`] object that contains the file content.
    /// * `size` - The file size.
    /// * `progress` - A callback that receives a pair of (transferred, total) bytes and returns false on error.
    ///
    /// # Performance
    ///
    /// Uploading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` and then enable larger chunking through either [`MCUmgrClient::set_frame_size`]
    /// or [`MCUmgrClient::use_auto_frame_size`].
    pub fn fs_file_upload<T: Read>(
        &self,
        name: impl AsRef<str>,
        mut reader: T,
        size: u64,
        mut progress: Option<&mut dyn FnMut(u64, u64) -> bool>,
    ) -> Result<(), FileUploadError> {
        let name = name.as_ref();

        let chunk_size_max = file_upload_max_data_chunk_size(
            self.smp_frame_size
                .load(std::sync::atomic::Ordering::SeqCst),
            name,
        )
        .map_err(FileUploadError::FrameSizeTooSmall)?;
        let mut data_buffer = vec![0u8; chunk_size_max].into_boxed_slice();

        let mut offset = 0;

        while offset < size {
            let current_chunk_size = (size - offset).min(data_buffer.len() as u64) as usize;

            let chunk_buffer = &mut data_buffer[..current_chunk_size];
            reader.read_exact(chunk_buffer)?;

            self.connection.execute_command(&commands::fs::FileUpload {
                off: offset,
                data: chunk_buffer,
                name,
                len: if offset == 0 { Some(size) } else { None },
            })?;

            offset += chunk_buffer.len() as u64;

            if let Some(progress) = &mut progress {
                if !progress(offset, size) {
                    return Err(FileUploadError::ProgressCallbackError);
                };
            }
        }

        Ok(())
    }

    /// Queries the file status
    pub fn fs_file_status(
        &self,
        name: impl AsRef<str>,
    ) -> Result<commands::fs::FileStatusResponse, ExecuteError> {
        self.connection.execute_command(&commands::fs::FileStatus {
            name: name.as_ref(),
        })
    }

    /// Computes the hash/checksum of a file
    ///
    /// For available algorithms, see [`fs_supported_checksum_types()`](MCUmgrClient::fs_supported_checksum_types).
    ///
    /// # Arguments
    ///
    /// * `name` - The absolute path of the file on the device
    /// * `algorithm` - The hash/checksum algorithm to use, or default if None
    /// * `offset` - How many bytes of the file to skip
    /// * `length` - How many bytes to read after `offset`. None for the entire file.
    ///
    pub fn fs_file_checksum(
        &self,
        name: impl AsRef<str>,
        algorithm: Option<impl AsRef<str>>,
        offset: u64,
        length: Option<u64>,
    ) -> Result<commands::fs::FileChecksumResponse, ExecuteError> {
        self.connection
            .execute_command(&commands::fs::FileChecksum {
                name: name.as_ref(),
                r#type: algorithm.as_ref().map(AsRef::as_ref),
                off: offset,
                len: length,
            })
    }

    /// Queries which hash/checksum algorithms are available on the target
    pub fn fs_supported_checksum_types(
        &self,
    ) -> Result<HashMap<String, commands::fs::FileChecksumProperties>, ExecuteError> {
        self.connection
            .execute_command(&commands::fs::SupportedFileChecksumTypes)
            .map(|val| val.types)
    }

    /// Close all device files MCUmgr has currently open
    pub fn fs_file_close(&self) -> Result<(), ExecuteError> {
        self.connection
            .execute_command(&commands::fs::FileClose)
            .map(Into::into)
    }

    /// Run a shell command.
    ///
    /// # Arguments
    ///
    /// * `argv` - The shell command to be executed.
    ///
    /// # Return
    ///
    /// A tuple of (returncode, stdout) produced by the command execution.
    pub fn shell_execute(&self, argv: &[String]) -> Result<(i32, String), ExecuteError> {
        self.connection
            .execute_command(&commands::shell::ShellCommandLineExecute { argv })
            .map(|ret| (ret.ret, ret.o))
    }

    /// Erase the `storage_partition` flash partition.
    pub fn zephyr_erase_storage(&self) -> Result<(), ExecuteError> {
        self.connection
            .execute_command(&commands::zephyr::EraseStorage)
            .map(Into::into)
    }

    /// Execute a raw [`commands::McuMgrCommand`].
    ///
    /// Only returns if no error happened, so the
    /// user does not need to check for an `rc` or `err`
    /// field in the response.
    pub fn raw_command<T: commands::McuMgrCommand>(
        &self,
        command: &T,
    ) -> Result<T::Response, ExecuteError> {
        self.connection.execute_command(command)
    }
}
