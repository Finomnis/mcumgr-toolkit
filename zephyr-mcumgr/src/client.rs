/// High level firmware update routine
mod firmware_update;

pub use firmware_update::{
    FirmwareUpdateError, FirmwareUpdateParams, FirmwareUpdateProgressCallback,
};

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    sync::atomic::AtomicUsize,
    time::Duration,
};

use miette::Diagnostic;
use rand::distr::SampleString;
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    bootloader::BootloaderInfo,
    commands::{
        self, fs::file_upload_max_data_chunk_size, image::image_upload_max_data_chunk_size,
    },
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

/// Possible error values of [`MCUmgrClient::image_upload`].
#[derive(Error, Debug, Diagnostic)]
pub enum ImageUploadError {
    /// The command failed in the SMP protocol layer.
    #[error("Command execution failed")]
    #[diagnostic(code(zephyr_mcumgr::client::image_upload::execute))]
    ExecuteError(#[from] ExecuteError),
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(zephyr_mcumgr::client::image_upload::progress_cb_error))]
    ProgressCallbackError,
    /// The current SMP frame size is too small for this command.
    #[error("SMP frame size too small for this command")]
    #[diagnostic(code(zephyr_mcumgr::client::image_upload::framesize_too_small))]
    FrameSizeTooSmall(#[source] io::Error),
    /// A device response contained an unexpected offset value.
    #[error("Received offset out of expected range")]
    #[diagnostic(code(zephyr_mcumgr::client::image_upload::invalid_offset))]
    UnexpectedOffset,
    /// The device reported a checksum mismatch
    #[error("Device reported checksum mismatch")]
    #[diagnostic(code(zephyr_mcumgr::client::image_upload::checksum_mismatch_on_device))]
    ChecksumMismatchOnDevice,
    /// The firmware image does not match the given checksum
    #[error("Firmware image does not match given checksum")]
    #[diagnostic(code(zephyr_mcumgr::client::image_upload::checksum_mismatch))]
    ChecksumMismatch,
}

/// Information about a serial port
#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
pub struct UsbSerialPortInfo {
    /// The identifier that the regex will match against
    pub identifier: String,
    /// The name of the port
    pub port_name: String,
    /// Information about the port
    pub port_info: serialport::UsbPortInfo,
}

/// A list of available serial ports
///
/// Used for pretty error messages.
#[derive(Serialize, Clone, Eq, PartialEq)]
#[serde(transparent)]
pub struct UsbSerialPorts(pub Vec<UsbSerialPortInfo>);
impl std::fmt::Display for UsbSerialPorts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            writeln!(f)?;
            write!(f, " - None -")?;
            return Ok(());
        }

        for UsbSerialPortInfo {
            identifier,
            port_name,
            port_info,
        } in &self.0
        {
            writeln!(f)?;
            write!(f, " - {identifier}")?;

            let mut print_port_string = true;
            let port_string = format!("({port_name})");

            if port_info.manufacturer.is_some() || port_info.product.is_some() {
                write!(f, " -")?;
                if let Some(manufacturer) = &port_info.manufacturer {
                    let mut print_manufacturer = true;

                    if let Some(product) = &port_info.product {
                        if product.starts_with(manufacturer) {
                            print_manufacturer = false;
                        }
                    }

                    if print_manufacturer {
                        write!(f, " {manufacturer}")?;
                    }
                }
                if let Some(product) = &port_info.product {
                    write!(f, " {product}")?;

                    if product.ends_with(&port_string) {
                        print_port_string = false;
                    }
                }
            }

            if print_port_string {
                write!(f, " {port_string}")?;
            }
        }
        Ok(())
    }
}
impl std::fmt::Debug for UsbSerialPorts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

/// Possible error values of [`MCUmgrClient::new_from_usb_serial`].
#[derive(Error, Debug, Diagnostic)]
pub enum UsbSerialError {
    /// Serialport error
    #[error("Serialport returned an error")]
    #[diagnostic(code(zephyr_mcumgr::usb_serial::serialport_error))]
    SerialPortError(#[from] serialport::Error),
    /// No port matched the given identifier
    #[error("No serial port matched the identifier '{identifier}'\nAvailable ports:\n{available}")]
    #[diagnostic(code(zephyr_mcumgr::usb_serial::no_matches))]
    NoMatchingPort {
        /// The original identifier provided by the user
        identifier: String,
        /// A list of available ports
        available: UsbSerialPorts,
    },
    /// More than one port matched the given identifier
    #[error("Multiple serial ports matched the identifier '{identifier}'\n{ports}")]
    #[diagnostic(code(zephyr_mcumgr::usb_serial::multiple_matches))]
    MultipleMatchingPorts {
        /// The original identifier provided by the user
        identifier: String,
        /// The matching ports
        ports: UsbSerialPorts,
    },
    /// Returned when the identifier was empty;
    /// can be used to query all available ports
    #[error("An empty identifier was provided")]
    #[diagnostic(code(zephyr_mcumgr::usb_serial::empty_identifier))]
    IdentifierEmpty {
        /// A list of available ports
        ports: UsbSerialPorts,
    },
    /// The given identifier was not a valid RegEx
    #[error("The given identifier was not a valid RegEx")]
    #[diagnostic(code(zephyr_mcumgr::usb_serial::regex_error))]
    RegexError(#[from] regex::Error),
}

impl MCUmgrClient {
    /// Creates a Zephyr MCUmgr SMP client based on a configured and opened serial port.
    ///
    /// ```no_run
    /// # use zephyr_mcumgr::MCUmgrClient;
    /// # fn main() {
    /// let serial = serialport::new("COM42", 115200)
    ///     .timeout(std::time::Duration::from_millis(10000))
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

    /// Creates a Zephyr MCUmgr SMP client based on a USB serial port identified by VID:PID.
    ///
    /// Useful for programming many devices in rapid succession, as Windows usually
    /// gives each one a different COMxx identifier.
    ///
    /// # Arguments
    ///
    /// * `identifier` - A regex that identifies the device.
    /// * `baud_rate` - The baud rate the port should operate at.
    /// * `timeout` - The communication timeout.
    ///
    /// # Identifier examples
    ///
    /// - `1234:89AB` - Vendor ID 1234, Product ID 89AB. Will fail if product has multiple serial ports.
    /// - `1234:89AB:12` - Vendor ID 1234, Product ID 89AB, Interface 12.
    /// - `1234:.*:[2-3]` - Vendor ID 1234, any Product Id, Interface 2 or 3.
    ///
    pub fn new_from_usb_serial(
        identifier: impl AsRef<str>,
        baud_rate: u32,
        timeout: Duration,
    ) -> Result<Self, UsbSerialError> {
        let identifier = identifier.as_ref();

        let ports = serialport::available_ports()?
            .into_iter()
            .filter_map(|port| {
                if let serialport::SerialPortType::UsbPort(port_info) = port.port_type {
                    if let Some(interface) = port_info.interface {
                        Some(UsbSerialPortInfo {
                            identifier: format!(
                                "{:04x}:{:04x}:{}",
                                port_info.vid, port_info.pid, interface
                            ),
                            port_name: port.port_name,
                            port_info,
                        })
                    } else {
                        Some(UsbSerialPortInfo {
                            identifier: format!("{:04x}:{:04x}", port_info.vid, port_info.pid),
                            port_name: port.port_name,
                            port_info,
                        })
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if identifier.is_empty() {
            return Err(UsbSerialError::IdentifierEmpty {
                ports: UsbSerialPorts(ports),
            });
        }

        let port_regex = regex::RegexBuilder::new(identifier)
            .case_insensitive(true)
            .unicode(true)
            .build()?;

        let matches = ports
            .iter()
            .filter(|port| {
                if let Some(m) = port_regex.find(&port.identifier) {
                    // Only accept if the regex matches at the beginning of the string
                    m.start() == 0
                } else {
                    false
                }
            })
            .cloned()
            .collect::<Vec<_>>();

        if matches.len() > 1 {
            return Err(UsbSerialError::MultipleMatchingPorts {
                identifier: identifier.to_string(),
                ports: UsbSerialPorts(matches),
            });
        }

        let port_name = match matches.into_iter().next() {
            Some(port) => port.port_name,
            None => {
                return Err(UsbSerialError::NoMatchingPort {
                    identifier: identifier.to_string(),
                    available: UsbSerialPorts(ports),
                });
            }
        };

        let serial = serialport::new(port_name, baud_rate)
            .timeout(timeout)
            .open()?;

        Ok(Self::new_from_serial(serial))
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

    /// Checks if the device is alive and responding.
    ///
    /// Runs a simple echo with random data and checks if the response matches.
    ///
    /// # Return
    ///
    /// An error if the device is not alive and responding.
    pub fn check_connection(&self) -> Result<(), ExecuteError> {
        let random_message = rand::distr::Alphanumeric.sample_string(&mut rand::rng(), 16);
        let response = self.os_echo(&random_message)?;
        if random_message == response {
            Ok(())
        } else {
            Err(ExecuteError::ReceiveFailed(
                crate::transport::ReceiveError::UnexpectedResponse,
            ))
        }
    }

    /// High level firmware update routine.
    ///
    /// # Arguments
    ///
    /// * `firmware` - The firmware image data.
    /// * `checksum` - SHA256 of the firmware image. Optional.
    /// * `params` - Configurable parameters.
    /// * `progress` - A callback that receives progress updates.
    ///
    pub fn firmware_update(
        &self,
        firmware: impl AsRef<[u8]>,
        checksum: Option<[u8; 32]>,
        params: FirmwareUpdateParams,
        progress: Option<&mut FirmwareUpdateProgressCallback>,
    ) -> Result<(), FirmwareUpdateError> {
        firmware_update::firmware_update(self, firmware, checksum, params, progress)
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

    /// Modify the current image state
    ///
    /// # Arguments
    ///
    /// * `hash` - the SHA256 id of the image.
    /// * `confirm` - mark the given image as 'confirmed'
    ///
    /// If `confirm` is `false`, perform a test boot with the given image and revert upon hard reset.
    ///
    /// If `confirm` is `true`, boot to the given image and mark it as `confirmed`. If `hash` is omitted,
    /// confirm the currently running image.
    ///
    /// Note that `hash` will not be the same as the SHA256 of the whole firmware image,
    /// it is the field in the MCUboot TLV section that contains a hash of the data
    /// which is used for signature verification purposes.
    pub fn image_set_state(
        &self,
        hash: Option<[u8; 32]>,
        confirm: bool,
    ) -> Result<Vec<commands::image::ImageState>, ExecuteError> {
        self.connection
            .execute_command(&commands::image::SetImageState {
                hash: hash.as_ref(),
                confirm,
            })
            .map(|val| val.images)
    }

    /// Upload a firmware image to an image slot.
    ///
    /// # Arguments
    ///
    /// * `data` - The firmware image data
    /// * `image` - Selects target image on the device. Defaults to `0`.
    /// * `checksum` - The SHA256 checksum of the image. If missing, will be computed from the image data.
    /// * `upgrade_only` - If true, allow firmware upgrades only and reject downgrades.
    /// * `progress` - A callback that receives a pair of (transferred, total) bytes and returns false on error.
    ///
    pub fn image_upload(
        &self,
        data: impl AsRef<[u8]>,
        image: Option<u32>,
        checksum: Option<[u8; 32]>,
        upgrade_only: bool,
        mut progress: Option<&mut dyn FnMut(u64, u64) -> bool>,
    ) -> Result<(), ImageUploadError> {
        let chunk_size_max = image_upload_max_data_chunk_size(
            self.smp_frame_size
                .load(std::sync::atomic::Ordering::SeqCst),
        )
        .map_err(ImageUploadError::FrameSizeTooSmall)?;

        let data = data.as_ref();

        let actual_checksum: [u8; 32] = Sha256::digest(data).into();
        if let Some(checksum) = checksum {
            if actual_checksum != checksum {
                return Err(ImageUploadError::ChecksumMismatch);
            }
        }

        let mut offset = 0;
        let size = data.len();

        let mut checksum_matched = None;

        while offset < size {
            let current_chunk_size = (size - offset).min(chunk_size_max);
            let chunk_data = &data[offset..offset + current_chunk_size];

            let upload_response = if offset == 0 {
                self.connection
                    .execute_command(&commands::image::ImageUpload {
                        image,
                        len: Some(size as u64),
                        off: offset as u64,
                        sha: Some(&actual_checksum),
                        data: chunk_data,
                        upgrade: Some(upgrade_only),
                    })?
            } else {
                self.connection
                    .execute_command(&commands::image::ImageUpload {
                        image: None,
                        len: None,
                        off: offset as u64,
                        sha: None,
                        data: chunk_data,
                        upgrade: None,
                    })?
            };

            offset = upload_response
                .off
                .try_into()
                .map_err(|_| ImageUploadError::UnexpectedOffset)?;

            if offset > size {
                return Err(ImageUploadError::UnexpectedOffset);
            }

            if let Some(progress) = &mut progress {
                if !progress(offset as u64, size as u64) {
                    return Err(ImageUploadError::ProgressCallbackError);
                };
            }

            if let Some(is_match) = upload_response.r#match {
                checksum_matched = Some(is_match);
            }
        }

        if let Some(checksum_matched) = checksum_matched {
            if !checksum_matched {
                return Err(ImageUploadError::ChecksumMismatchOnDevice);
            }
        } else {
            log::warn!("Device did not perform image checksum verification");
        }

        Ok(())
    }

    /// Erase image slot on target device.
    ///
    /// # Arguments
    ///
    /// * `slot` - The slot ID of the image to erase. Slot `1` if omitted.
    ///
    pub fn image_erase(&self, slot: Option<u32>) -> Result<(), ExecuteError> {
        self.connection
            .execute_command(&commands::image::ImageErase { slot })
            .map(Into::into)
    }

    /// Obtain a list of available image slots.
    pub fn image_slot_info(&self) -> Result<Vec<commands::image::SlotInfoImage>, ExecuteError> {
        self.connection
            .execute_command(&commands::image::SlotInfo)
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
