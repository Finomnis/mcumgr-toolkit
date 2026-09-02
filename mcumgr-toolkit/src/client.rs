/// High-level firmware update routine
mod firmware_update;

pub use firmware_update::{
    FirmwareUpdateError, FirmwareUpdateParams, FirmwareUpdateProgressCallback, FirmwareUpdateStep,
};

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::SocketAddr,
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
    transport::{
        ReceiveError, Transport,
        serial::{ConfigurableTimeout, SerialTransport},
        udp::UdpTransport,
    },
};

#[cfg(feature = "ble")]
use crate::transport::ble::{BleIdentifier, BleRuntimeError};

/// The default SMP frame size of Zephyr.
///
/// Matches Zephyr default value of [MCUMGR_TRANSPORT_NETBUF_SIZE](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40).
const ZEPHYR_DEFAULT_SMP_FRAME_SIZE: usize = 384;

/// A high-level client for Zephyr's MCUmgr SMP protocol.
///
/// This struct is the central entry point of this crate.
pub struct MCUmgrClient {
    connection: Connection,
    smp_frame_size: AtomicUsize,
}

/// Possible error values of [`MCUmgrClient`].
#[derive(Error, Debug, Diagnostic)]
pub enum MCUmgrClientError {
    /// The command failed in the SMP protocol layer.
    #[error("Command execution failed")]
    #[diagnostic(code(mcumgr_toolkit::client::execute))]
    ExecuteError(#[from] ExecuteError),
    /// A device response contained an unexpected offset value.
    #[error("Received an unexpected offset value")]
    #[diagnostic(code(mcumgr_toolkit::client::unexpected_offset))]
    UnexpectedOffset,
    /// The writer returned an error.
    #[error("Writer returned an error")]
    #[diagnostic(code(mcumgr_toolkit::client::writer))]
    WriterError(#[source] io::Error),
    /// The reader returned an error.
    #[error("Reader returned an error")]
    #[diagnostic(code(mcumgr_toolkit::client::reader))]
    ReaderError(#[source] io::Error),
    /// The received data does not match the reported file size.
    #[error("Received data does not match reported size")]
    #[diagnostic(code(mcumgr_toolkit::client::size_mismatch))]
    SizeMismatch,
    /// The received data unexpectedly did not report the file size.
    #[error("Received data is missing file size information")]
    #[diagnostic(code(mcumgr_toolkit::client::missing_size))]
    MissingSize,
    /// The progress callback returned an error.
    #[error("Progress callback returned an error")]
    #[diagnostic(code(mcumgr_toolkit::client::progress_cb_error))]
    ProgressCallbackError,
    /// The current SMP frame size is too small for this command.
    #[error("SMP frame size too small for this command")]
    #[diagnostic(code(mcumgr_toolkit::client::framesize_too_small))]
    FrameSizeTooSmall(#[source] io::Error),
    /// The device reported a checksum mismatch
    #[error("Device reported checksum mismatch")]
    #[diagnostic(code(mcumgr_toolkit::client::checksum_mismatch_on_device))]
    ChecksumMismatchOnDevice,
    /// The firmware image does not match the given checksum
    #[error("Firmware image does not match given checksum")]
    #[diagnostic(code(mcumgr_toolkit::client::checksum_mismatch))]
    ChecksumMismatch,
    /// Setting the device timeout failed
    #[error("Failed to set the device timeout")]
    #[diagnostic(code(mcumgr_toolkit::client::set_timeout))]
    SetTimeoutFailed(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl MCUmgrClientError {
    /// Checks if the device reported the command as unsupported
    pub fn command_not_supported(&self) -> bool {
        if let Self::ExecuteError(err) = self {
            err.command_not_supported()
        } else {
            false
        }
    }
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

#[cfg(feature = "ble")]
fn ble_identifier_to_str<S>(id: &BleIdentifier, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    ser.collect_str(id)
}

/// Information about a BLE device
#[cfg(feature = "ble")]
#[derive(Debug, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct BleDeviceInfo {
    /// An device identifier
    #[serde(serialize_with = "ble_identifier_to_str")]
    pub id: BleIdentifier,
    /// The device name
    pub name: Option<String>,
    /// RSSI, in dBm
    pub rssi: Option<i16>,
}

/// A list of available BLE devices
///
/// Used for pretty error messages.
#[cfg(feature = "ble")]
#[derive(Serialize, Clone, Eq, PartialEq)]
#[serde(transparent)]
pub struct BleDevices(pub Vec<BleDeviceInfo>);

#[cfg(feature = "ble")]
impl std::fmt::Display for BleDevices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            writeln!(f)?;
            write!(f, " - None -")?;
            return Ok(());
        }

        for BleDeviceInfo { id, name, rssi } in &self.0 {
            writeln!(f)?;

            if let Some(name) = name {
                write!(f, " - {id} - {name:?}")?;
            } else {
                write!(f, " - {id} - <unknown>")?;
            }

            if let Some(rssi) = rssi {
                write!(f, " ({rssi} dBm)")?;
            }
        }
        Ok(())
    }
}

#[cfg(feature = "ble")]
impl std::fmt::Debug for BleDevices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

/// Possible error values of [`MCUmgrClient::new_from_udp`].
#[derive(Error, Debug, Diagnostic)]
pub enum UdpError {
    /// An I/O error occurred while opening the UDP socket
    #[error("Failed to open UDP socket")]
    #[diagnostic(code(mcumgr_toolkit::udp::io_error))]
    Io(#[from] io::Error),
}

/// Possible error values of [`MCUmgrClient::new_from_usb_serial`].
#[derive(Error, Debug, Diagnostic)]
pub enum UsbSerialError {
    /// Serialport error
    #[error("Serialport returned an error")]
    #[diagnostic(code(mcumgr_toolkit::usb_serial::serialport_error))]
    SerialPortError(#[from] serialport::Error),
    /// No port matched the given identifier
    #[error("No serial port matched the identifier '{identifier}'\nAvailable ports:\n{available}")]
    #[diagnostic(code(mcumgr_toolkit::usb_serial::no_matches))]
    NoMatchingPort {
        /// The original identifier provided by the user
        identifier: String,
        /// A list of available ports
        available: UsbSerialPorts,
    },
    /// More than one port matched the given identifier
    #[error("Multiple serial ports matched the identifier '{identifier}'\n{ports}")]
    #[diagnostic(code(mcumgr_toolkit::usb_serial::multiple_matches))]
    MultipleMatchingPorts {
        /// The original identifier provided by the user
        identifier: String,
        /// The matching ports
        ports: UsbSerialPorts,
    },
    /// Returned when the identifier was empty;
    /// can be used to query all available ports
    #[error("An empty identifier was provided")]
    #[diagnostic(code(mcumgr_toolkit::usb_serial::empty_identifier))]
    IdentifierEmpty {
        /// A list of available ports
        ports: UsbSerialPorts,
    },
    /// The given identifier was not a valid RegEx
    #[error("The given identifier was not a valid RegEx")]
    #[diagnostic(code(mcumgr_toolkit::usb_serial::regex_error))]
    RegexError(#[from] regex::Error),
}

/// Possible error values of [`MCUmgrClient::new_from_ble`].
#[cfg(feature = "ble")]
#[derive(Error, Debug, Diagnostic)]
pub enum BleError {
    /// BLE Runtime error
    #[error("BLE runtime layer returned an error")]
    #[diagnostic(code(mcumgr_toolkit::ble::runtime))]
    BleRuntime(#[from] BleRuntimeError),
    /// BLE Scanning stopped unexpectedly
    #[error("BLE scanning unexpectedly stopped")]
    #[diagnostic(code(mcumgr_toolkit::ble::scan_stopped))]
    ScanStopped,
    /// No matching BLE device was discovered
    #[error("Device not found\nAvailable devices:\n{available}")]
    #[diagnostic(code(mcumgr_toolkit::ble::device_not_found))]
    DeviceNotFound {
        /// A list of available devices
        available: BleDevices,
    },
    /// Returned when the identifier was empty;
    /// can be used to query all available devices
    #[error("An empty identifier was provided")]
    #[diagnostic(code(mcumgr_toolkit::ble::empty_identifier))]
    IdentifierEmpty {
        /// A list of available devices
        devices: BleDevices,
    },
}

impl MCUmgrClient {
    /// Creates a Zephyr MCUmgr SMP client based on a configured and opened serial port.
    ///
    /// ```no_run
    /// # use mcumgr_toolkit::MCUmgrClient;
    /// # fn main() {
    /// let serial = serialport::new("COM42", 115200)
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

    /// Creates a Zephyr MCUmgr SMP client based on a BLE connection.
    ///
    /// # Arguments
    ///
    /// * `identifier` - An OS dependent identifier for BLE devices.
    /// * `timeout` - The communication timeout.
    ///
    #[cfg(feature = "ble")]
    pub fn new_from_ble(
        identifier: Option<BleIdentifier>,
        timeout: Duration,
    ) -> Result<Self, BleError> {
        use btleplug::api::{Central, Peripheral};
        use futures::StreamExt;
        use tokio::time::error::Elapsed;

        let mut runtime = crate::transport::ble::BleRuntime::new()?;

        let scan_timeout = Duration::from_secs(3);

        let mut devices = HashMap::new();

        let device = runtime.scan(
            async |mut events, central| -> Result<btleplug::platform::Peripheral, BleError> {
                tokio::time::timeout(scan_timeout, async {
                    loop {
                        match events.next().await.ok_or(BleError::ScanStopped)? {
                            btleplug::api::CentralEvent::DeviceDiscovered(id)
                            | btleplug::api::CentralEvent::DeviceConnected(id)
                            | btleplug::api::CentralEvent::DeviceUpdated(id)
                            | btleplug::api::CentralEvent::DeviceServicesModified(id)
                            | btleplug::api::CentralEvent::ServiceDataAdvertisement {
                                id,
                                service_data: _,
                            }
                            | btleplug::api::CentralEvent::ServicesAdvertisement {
                                id,
                                services: _,
                            }
                            | btleplug::api::CentralEvent::ManufacturerDataAdvertisement {
                                id,
                                manufacturer_data: _,
                            } => {
                                if let Ok(device) = central.peripheral(&id).await {
                                    // println!("{id} {device:?} {properties:?}");

                                    #[allow(irrefutable_let_patterns)]
                                    #[allow(clippy::unnecessary_fallible_conversions)]
                                    if let Ok(current_identifier) = BleIdentifier::try_from(&device)
                                    {
                                        if let Some(identifier) = &identifier
                                            && identifier == &current_identifier
                                        {
                                            break Ok(device);
                                        }

                                        if let Ok(Some(properties)) = device.properties().await
                                            && properties
                                                .services
                                                .contains(&crate::transport::ble::SMP_UUID)
                                        {
                                            devices.entry(id).insert_entry(BleDeviceInfo {
                                                id: current_identifier,
                                                name: properties.local_name,
                                                rssi: properties.rssi,
                                            });
                                        }
                                    }
                                }
                            }
                            btleplug::api::CentralEvent::RssiUpdate { id, rssi } => {
                                if let Some(device) = devices.get_mut(&id) {
                                    device.rssi = Some(rssi);
                                }
                            }
                            _ => (),
                        }
                    }
                })
                .await
                .map_err(|_: Elapsed| {
                    let devices = BleDevices({
                        let mut device_list = devices.into_values().collect::<Vec<_>>();
                        device_list.sort();
                        device_list
                    });
                    if identifier.is_none() {
                        BleError::IdentifierEmpty { devices }
                    } else {
                        BleError::DeviceNotFound { available: devices }
                    }
                })?
            },
        )??;

        let transport = runtime.into_transport(device, timeout)?;
        Ok(Self {
            connection: Connection::new(transport),
            smp_frame_size: ZEPHYR_DEFAULT_SMP_FRAME_SIZE.into(),
        })
    }

    /// Creates a client over any transport implementing [`Transport`].
    ///
    /// The built-in constructors cover serial, USB serial, BLE and UDP. This one
    /// exists for transports living outside the crate: SMP over ISO-TP on a CAN
    /// bus, over a test harness, or over anything else that can carry a raw SMP
    /// frame. Without it a third-party [`Transport`] implementation cannot be
    /// used, because [`MCUmgrClient`]'s connection field is private and every
    /// other constructor is tied to a concrete transport type.
    ///
    /// ```no_run
    /// # use mcumgr_toolkit::{MCUmgrClient, transport::Transport};
    /// # fn example<T: Transport + Send + 'static>(my_transport: T) {
    /// let client = MCUmgrClient::new_from_transport(my_transport);
    /// # }
    /// ```
    pub fn new_from_transport<T: Transport + Send + 'static>(transport: T) -> Self {
        Self {
            connection: Connection::new(transport),
            smp_frame_size: ZEPHYR_DEFAULT_SMP_FRAME_SIZE.into(),
        }
    }

    /// Creates a Zephyr MCUmgr SMP client based on a UDP socket.
    ///
    /// # Arguments
    ///
    /// * `addr` - The remote UDP endpoint.
    /// * `timeout` - The communication timeout.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mcumgr_toolkit::MCUmgrClient;
    /// # use std::time::Duration;
    /// # use std::net::SocketAddr;
    /// # fn main() {
    /// let addr: SocketAddr = "192.168.1.1:1337".parse().unwrap();
    /// let mut client = MCUmgrClient::new_from_udp(addr, Duration::from_millis(1000)).unwrap();
    /// # }
    /// ```
    ///
    /// Alternatively, you can use [`to_socket_addrs`](https://doc.rust-lang.org/std/net/trait.ToSocketAddrs.html#tymethod.to_socket_addrs)
    /// to resolve hostnames:
    ///
    /// ```no_run
    /// # use mcumgr_toolkit::MCUmgrClient;
    /// # use std::time::Duration;
    /// # use std::net::ToSocketAddrs;
    /// # fn main() {
    /// let addr = "mydevice.local:1337".to_socket_addrs().unwrap().next().unwrap();
    /// let mut client = MCUmgrClient::new_from_udp(addr, Duration::from_millis(1000)).unwrap();
    /// # }
    /// ```
    pub fn new_from_udp(addr: impl Into<SocketAddr>, timeout: Duration) -> Result<Self, UdpError> {
        let addr = addr.into();
        log::debug!("Connecting to {addr} ...");
        Ok(Self {
            connection: Connection::new(UdpTransport::new(addr, timeout)?),
            smp_frame_size: ZEPHYR_DEFAULT_SMP_FRAME_SIZE.into(),
        })
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
    pub fn use_auto_frame_size(&self) -> Result<(), MCUmgrClientError> {
        let mcumgr_params = self
            .connection
            .execute_command(&commands::os::MCUmgrParameters)?;

        let frame_size =
            (mcumgr_params.buf_size as usize).min(self.connection.max_transport_frame_size());

        log::debug!("Using frame size {}.", frame_size);

        self.smp_frame_size
            .store(frame_size, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    /// Changes the communication timeout.
    ///
    /// When the device does not respond to packets within the set
    /// duration, an error will be raised.
    pub fn set_timeout(&self, timeout: Duration) -> Result<(), MCUmgrClientError> {
        self.connection
            .set_timeout(timeout)
            .map_err(MCUmgrClientError::SetTimeoutFailed)
    }

    /// Changes the retry amount.
    ///
    /// When the device encounters a transport error, it will retry
    /// this many times until giving up.
    pub fn set_retries(&self, retries: u8) {
        self.connection.set_retries(retries)
    }

    /// Checks if the device is alive and responding.
    ///
    /// Runs a simple echo with random data and checks if the response matches.
    ///
    /// # Return
    ///
    /// An error if the device is not alive and responding.
    pub fn check_connection(&self) -> Result<(), MCUmgrClientError> {
        let random_message = rand::distr::Alphanumeric.sample_string(&mut rand::rng(), 16);
        let response = self.os_echo(&random_message)?;
        if random_message == response {
            Ok(())
        } else {
            Err(
                ExecuteError::ReceiveFailed(crate::transport::ReceiveError::UnexpectedResponse)
                    .into(),
            )
        }
    }

    /// High-level firmware update routine.
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
    pub fn os_echo(&self, msg: impl AsRef<str>) -> Result<String, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::Echo { d: msg.as_ref() })
            .map(|resp| resp.r)
            .map_err(Into::into)
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
    ) -> Result<HashMap<String, commands::os::TaskStatisticsEntry>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::TaskStatistics)
            .map(|resp| {
                let mut tasks = resp.tasks;
                for stats in tasks.values_mut() {
                    stats.stkuse = stats.stkuse.map(|val| val * 4);
                    stats.stksiz = stats.stksiz.map(|val| val * 4);
                }
                tasks
            })
            .map_err(Into::into)
    }

    /// Queries live memory pool statistics
    ///
    /// # Return
    ///
    /// A map of memory pool names with their respective statistics
    pub fn os_memory_pool_statistics(
        &self,
    ) -> Result<HashMap<String, commands::os::MemoryPoolStatisticsEntry>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::MemoryPoolStatistics)
            .map(|resp| resp.pools)
            .map_err(Into::into)
    }

    /// Sets the RTC of the device to the given datetime.
    pub fn os_set_datetime(
        &self,
        datetime: chrono::NaiveDateTime,
    ) -> Result<(), MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::DateTimeSet { datetime })
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Retrieves the device RTC's datetime.
    pub fn os_get_datetime(&self) -> Result<chrono::NaiveDateTime, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::DateTimeGet)
            .map(|val| val.datetime)
            .map_err(Into::into)
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
    pub fn os_system_reset(
        &self,
        force: bool,
        boot_mode: Option<u8>,
    ) -> Result<(), MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::SystemReset { force, boot_mode })
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Fetch parameters from the MCUmgr library
    pub fn os_mcumgr_parameters(
        &self,
    ) -> Result<commands::os::MCUmgrParametersResponse, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::MCUmgrParameters)
            .map_err(Into::into)
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
    pub fn os_application_info(&self, format: Option<&str>) -> Result<String, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::os::ApplicationInfo { format })
            .map(|resp| resp.output)
            .map_err(Into::into)
    }

    /// Fetch information on the device's bootloader
    pub fn os_bootloader_info(&self) -> Result<BootloaderInfo, MCUmgrClientError> {
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
    pub fn image_get_state(&self) -> Result<Vec<commands::image::ImageState>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::image::GetImageState)
            .map(|val| val.images)
            .map_err(Into::into)
    }

    /// Modify the current image state
    ///
    /// # Arguments
    ///
    /// * `hash` - the hash id of the image. See [`mcuboot::get_image_info`](crate::mcuboot::get_image_info).
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
        hash: Option<&[u8]>,
        confirm: bool,
    ) -> Result<Vec<commands::image::ImageState>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::image::SetImageState { hash, confirm })
            .map(|val| val.images)
            .map_err(Into::into)
    }

    /// Upload a firmware image to an image slot.
    ///
    /// # Note
    ///
    /// This only uploads the image to a slot on the device, it has to be activated
    /// through [`image_set_state`](Self::image_set_state) for an actual update to happen.
    ///
    /// For a full firmware update algorithm in a single step, see [`firmware_update`](Self::firmware_update).
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
    ) -> Result<(), MCUmgrClientError> {
        let first_chunk_size_max = image_upload_max_data_chunk_size(
            self.smp_frame_size
                .load(std::sync::atomic::Ordering::SeqCst),
            true,
        )
        .map_err(MCUmgrClientError::FrameSizeTooSmall)?;
        let other_chunk_size_max = image_upload_max_data_chunk_size(
            self.smp_frame_size
                .load(std::sync::atomic::Ordering::SeqCst),
            false,
        )
        .map_err(MCUmgrClientError::FrameSizeTooSmall)?;
        log::debug!("Max chunk size: {first_chunk_size_max}, {other_chunk_size_max}");

        let data = data.as_ref();

        let actual_checksum: [u8; 32] = Sha256::digest(data).into();
        if let Some(checksum) = checksum {
            if actual_checksum != checksum {
                return Err(MCUmgrClientError::ChecksumMismatch);
            }
        }

        let mut offset = 0;
        let size = data.len();

        let mut checksum_matched = None;

        while offset < size {
            let upload_response = if offset == 0 {
                let current_chunk_size = (size - offset).min(first_chunk_size_max);
                let chunk_data = &data[offset..offset + current_chunk_size];

                let result = self
                    .connection
                    .execute_command(&commands::image::ImageUpload {
                        image,
                        len: Some(size as u64),
                        off: offset as u64,
                        sha: Some(&actual_checksum),
                        data: chunk_data,
                        upgrade: Some(upgrade_only),
                    });

                if let Err(ExecuteError::ReceiveFailed(ReceiveError::Timeout)) = &result {
                    log::warn!(
                        "Timed out during transfer of first chunk. Consider enabling CONFIG_IMG_ERASE_PROGRESSIVELY."
                    )
                }

                result?
            } else {
                let current_chunk_size = (size - offset).min(other_chunk_size_max);
                let chunk_data = &data[offset..offset + current_chunk_size];

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
                .map_err(|_| MCUmgrClientError::UnexpectedOffset)?;

            if offset > size {
                return Err(MCUmgrClientError::UnexpectedOffset);
            }

            if let Some(progress) = &mut progress {
                if !progress(offset as u64, size as u64) {
                    return Err(MCUmgrClientError::ProgressCallbackError);
                };
            }

            if let Some(is_match) = upload_response.r#match {
                checksum_matched = Some(is_match);
            }
        }

        if let Some(checksum_matched) = checksum_matched {
            if !checksum_matched {
                return Err(MCUmgrClientError::ChecksumMismatchOnDevice);
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
    pub fn image_erase(&self, slot: Option<u32>) -> Result<(), MCUmgrClientError> {
        self.connection
            .execute_command(&commands::image::ImageErase { slot })
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Obtain a list of available image slots.
    pub fn image_slot_info(
        &self,
    ) -> Result<Vec<commands::image::SlotInfoImage>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::image::SlotInfo)
            .map(|val| val.images)
            .map_err(Into::into)
    }

    /// Query the current values of a given stats group
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the group. See [`stats_list_groups`](Self::stats_list_groups).
    ///
    pub fn stats_get_group_data(
        &self,
        name: impl AsRef<str>,
    ) -> Result<HashMap<String, u64>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::stats::GroupData {
                name: name.as_ref(),
            })
            .map(|val| val.fields)
            .map_err(Into::into)
    }

    /// Query the list of available stats groups
    pub fn stats_list_groups(&self) -> Result<Vec<String>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::stats::ListGroups)
            .map(|val| val.stat_list)
            .map_err(Into::into)
    }

    /// Read a setting from the device.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the setting.
    ///
    /// # Return
    ///
    /// The value of the setting, as raw bytes.
    ///
    /// Note that the underlying data type cannot be specified through this and must be known by the client.
    ///
    pub fn settings_read(&self, name: impl AsRef<str>) -> Result<Vec<u8>, MCUmgrClientError> {
        let name = name.as_ref();

        self.settings_read_ext(name, None).map(|val| val.val)
    }

    /// Read a setting from the device.
    ///
    /// Extended version.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the setting.
    /// * `max_size` - Optional maximum size of data to return.
    ///
    pub fn settings_read_ext(
        &self,
        name: impl AsRef<str>,
        max_size: Option<u32>,
    ) -> Result<commands::settings::ReadSettingResponse, MCUmgrClientError> {
        let name = name.as_ref();

        self.connection
            .execute_command(&commands::settings::ReadSetting { name, max_size })
            .map_err(Into::into)
    }

    /// Write a setting to the device.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the setting.
    /// * `value` - The value of the setting.
    ///
    pub fn settings_write(
        &self,
        name: impl AsRef<str>,
        value: &[u8],
    ) -> Result<(), MCUmgrClientError> {
        let name = name.as_ref();

        self.connection
            .execute_command(&commands::settings::WriteSetting { name, val: value })
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Delete a setting from the device.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the setting.
    ///
    pub fn settings_delete(&self, name: impl AsRef<str>) -> Result<(), MCUmgrClientError> {
        let name = name.as_ref();

        self.connection
            .execute_command(&commands::settings::DeleteSetting { name })
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Commit all modified settings on the device.
    ///
    pub fn settings_commit(&self) -> Result<(), MCUmgrClientError> {
        self.connection
            .execute_command(&commands::settings::CommitSettings)
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Load settings from persistent storage.
    ///
    pub fn settings_load(&self) -> Result<(), MCUmgrClientError> {
        self.connection
            .execute_command(&commands::settings::LoadSettings)
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Save settings to persistent storage.
    ///
    /// # Arguments
    ///
    /// * `name` - Only persist the subtree with the given name.
    ///
    pub fn settings_save(&self, name: Option<impl AsRef<str>>) -> Result<(), MCUmgrClientError> {
        let name = name.as_ref().map(|val| val.as_ref());

        self.connection
            .execute_command(&commands::settings::SaveSettings { name })
            .map(Into::into)
            .map_err(Into::into)
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
    ) -> Result<(), MCUmgrClientError> {
        let name = name.as_ref();
        let response = self
            .connection
            .execute_command(&commands::fs::FileDownload { name, off: 0 })?;

        let file_len = response.len.ok_or(MCUmgrClientError::MissingSize)?;
        if response.off != 0 {
            return Err(MCUmgrClientError::UnexpectedOffset);
        }

        let mut offset = 0;

        if let Some(progress) = &mut progress {
            if !progress(offset, file_len) {
                return Err(MCUmgrClientError::ProgressCallbackError);
            };
        }

        writer
            .write_all(&response.data)
            .map_err(MCUmgrClientError::WriterError)?;
        offset += response.data.len() as u64;

        if let Some(progress) = &mut progress {
            if !progress(offset, file_len) {
                return Err(MCUmgrClientError::ProgressCallbackError);
            };
        }

        while offset < file_len {
            let response = self
                .connection
                .execute_command(&commands::fs::FileDownload { name, off: offset })?;

            if response.off != offset {
                return Err(MCUmgrClientError::UnexpectedOffset);
            }

            writer
                .write_all(&response.data)
                .map_err(MCUmgrClientError::WriterError)?;
            offset += response.data.len() as u64;

            if let Some(progress) = &mut progress {
                if !progress(offset, file_len) {
                    return Err(MCUmgrClientError::ProgressCallbackError);
                };
            }
        }

        if offset != file_len {
            return Err(MCUmgrClientError::SizeMismatch);
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
    ) -> Result<(), MCUmgrClientError> {
        let name = name.as_ref();

        let chunk_size_max = file_upload_max_data_chunk_size(
            self.smp_frame_size
                .load(std::sync::atomic::Ordering::SeqCst),
            name,
        )
        .map_err(MCUmgrClientError::FrameSizeTooSmall)?;
        let mut data_buffer = vec![0u8; chunk_size_max].into_boxed_slice();

        let mut offset = 0;

        while offset < size {
            let current_chunk_size = (size - offset).min(data_buffer.len() as u64) as usize;

            let chunk_buffer = &mut data_buffer[..current_chunk_size];
            reader
                .read_exact(chunk_buffer)
                .map_err(MCUmgrClientError::ReaderError)?;

            self.connection.execute_command(&commands::fs::FileUpload {
                off: offset,
                data: chunk_buffer,
                name,
                len: if offset == 0 { Some(size) } else { None },
            })?;

            offset += chunk_buffer.len() as u64;

            if let Some(progress) = &mut progress {
                if !progress(offset, size) {
                    return Err(MCUmgrClientError::ProgressCallbackError);
                };
            }
        }

        Ok(())
    }

    /// Queries the file status
    pub fn fs_file_status(
        &self,
        name: impl AsRef<str>,
    ) -> Result<commands::fs::FileStatusResponse, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::fs::FileStatus {
                name: name.as_ref(),
            })
            .map_err(Into::into)
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
    ) -> Result<commands::fs::FileChecksumResponse, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::fs::FileChecksum {
                name: name.as_ref(),
                r#type: algorithm.as_ref().map(AsRef::as_ref),
                off: offset,
                len: length,
            })
            .map_err(Into::into)
    }

    /// Queries which hash/checksum algorithms are available on the target
    pub fn fs_supported_checksum_types(
        &self,
    ) -> Result<HashMap<String, commands::fs::FileChecksumProperties>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::fs::SupportedFileChecksumTypes)
            .map(|val| val.types)
            .map_err(Into::into)
    }

    /// Close all device files MCUmgr has currently open
    pub fn fs_file_close(&self) -> Result<(), MCUmgrClientError> {
        self.connection
            .execute_command(&commands::fs::FileClose)
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Run a shell command.
    ///
    /// # Arguments
    ///
    /// * `argv` - The shell command to be executed.
    /// * `use_retries` - Retry request a certain amount of times if a transport error occurs.
    ///   Be aware that this might cause the command to be executed multiple times.
    ///
    /// # Return
    ///
    /// A tuple of (returncode, stdout) produced by the command execution.
    pub fn shell_execute(
        &self,
        argv: &[String],
        use_retries: bool,
    ) -> Result<(i32, String), MCUmgrClientError> {
        let command = commands::shell::ShellCommandLineExecute { argv };

        if use_retries {
            self.connection.execute_command(&command)
        } else {
            self.connection.execute_command_without_retries(&command)
        }
        .map(|ret| (ret.ret, ret.o))
        .map_err(Into::into)
    }

    /// Query how many MCUmgr groups are supported by the device.
    ///
    /// # Return
    ///
    /// The number of MCUmgr groups the device supports.
    ///
    pub fn enum_get_group_count(&self) -> Result<u16, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::r#enum::GroupCount)
            .map(|ret| ret.count)
            .map_err(Into::into)
    }

    /// Query all available group IDs in a single command.
    ///
    /// Note that this might fail if the amount of groups is too large for the
    /// SMP frame.
    /// But given that the Zephyr implementation contains less than 10 groups,
    /// this is currently highly unlikely.
    ///
    /// If it does fail, use [`enum_iter_group_ids`](Self::enum_iter_group_ids) to iterate
    /// through the available group IDs one by one.
    ///
    /// # Return
    ///
    /// A list of all MCUmgr group IDs the device supports.
    ///
    pub fn enum_get_group_ids(&self) -> Result<Vec<u16>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::r#enum::ListGroups)
            .map(|ret| ret.groups)
            .map_err(Into::into)
    }

    /// Query a single group ID from the device.
    ///
    /// # Arguments
    ///
    /// * `index` - The index in the list of group IDs.
    ///   Must be smaller than [`enum_get_group_count`](Self::enum_get_group_count).
    ///
    /// # Return
    ///
    /// The group ID of the group with the given index
    ///
    pub fn enum_get_group_id(&self, index: u16) -> Result<u16, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::r#enum::GroupId { index: Some(index) })
            .map(|ret| ret.group)
            .map_err(Into::into)
    }

    /// Iterate through all supported MCUmgr Groups.
    ///
    /// Same as [`enum_get_group_ids`](Self::enum_get_group_ids), but does not
    /// require large message sizes if the number of groups is large. The tradeoff is
    /// that this function is much slower.
    pub fn enum_iter_group_ids(&self) -> impl Iterator<Item = Result<u16, MCUmgrClientError>> {
        let mut i = 0;
        let mut num_elements = None;

        std::iter::from_fn(move || -> Option<Result<u16, MCUmgrClientError>> {
            let mut num_elements_err = None;
            let num_elements =
                *num_elements.get_or_insert_with(|| match self.enum_get_group_count() {
                    Ok(n) => n,
                    Err(e) => {
                        num_elements_err = Some(e);
                        0
                    }
                });
            if let Some(err) = num_elements_err {
                return Some(Err(err));
            }

            if i >= num_elements {
                None
            } else {
                Some(match self.enum_get_group_id(i) {
                    Ok(group_id) => {
                        i += 1;
                        Ok(group_id)
                    }
                    Err(e) => {
                        i = num_elements;
                        Err(e)
                    }
                })
            }
        })
    }

    /// Query details from all available groups.
    ///
    /// # Arguments
    ///
    /// * `groups` - The group IDs to fetch details for. If omitted, fetch all groups.
    ///
    /// # Return
    ///
    /// A list of details about all MCUmgr group IDs the device supports.
    ///
    pub fn enum_get_group_details(
        &self,
        groups: Option<&[u16]>,
    ) -> Result<Vec<commands::r#enum::GroupDetailsEntry>, MCUmgrClientError> {
        self.connection
            .execute_command(&commands::r#enum::GroupDetails { groups })
            .map(|ret| ret.groups)
            .map_err(Into::into)
    }

    /// Erase the `storage_partition` flash partition.
    pub fn zephyr_erase_storage(&self) -> Result<(), MCUmgrClientError> {
        self.connection
            .execute_command(&commands::zephyr::EraseStorage)
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Execute a raw [`commands::McuMgrCommand`].
    ///
    /// Only returns if no error happened, so the
    /// user does not need to check for an `rc` or `err`
    /// field in the response.
    pub fn raw_command<T: commands::McuMgrCommand>(
        &self,
        command: &T,
    ) -> Result<T::Response, MCUmgrClientError> {
        self.connection.execute_command(command).map_err(Into::into)
    }
}
