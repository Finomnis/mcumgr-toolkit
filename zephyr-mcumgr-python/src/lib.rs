#![forbid(unsafe_code)]

use miette::IntoDiagnostic;
use pyo3::types::PyDateTime;
use pyo3::{prelude::*, types::PyBytes};

use pyo3::exceptions::PyRuntimeError;
use pyo3_stub_gen::{
    define_stub_info_gatherer,
    derive::{gen_stub_pyclass, gen_stub_pymethods},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::raw_py_any_command::RawPyAnyCommand;

mod return_types;
pub use return_types::*;

mod raw_py_any_command;
mod repr_macro;

/// A high level client for Zephyr's MCUmgr SMP functionality
#[gen_stub_pyclass]
#[pyclass(frozen)]
struct MCUmgrClient {
    // Mutex<Option<Arc<>>> so we can delete the client in __exit__()
    client: Mutex<Option<Arc<::zephyr_mcumgr::MCUmgrClient>>>,
}

fn err_to_pyerr<E: Into<miette::Report>>(err: E) -> PyErr {
    let e: miette::Report = err.into();
    PyRuntimeError::new_err(format!("{e:?}"))
}

impl MCUmgrClient {
    fn get_client(&self) -> PyResult<Arc<::zephyr_mcumgr::MCUmgrClient>> {
        let locked_client = self.client.lock().unwrap();
        locked_client
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| PyRuntimeError::new_err("Client already closed"))
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl MCUmgrClient {
    /// Creates a new serial port based Zephyr MCUmgr SMP client.
    ///
    /// ### Arguments
    ///
    /// * `serial` - The identifier of the serial device. (Windows: `COMxx`, Linux: `/dev/ttyXX`)
    /// * `baud_rate` - The baud rate of the serial port.
    /// * `timeout_ms` - The communication timeout, in ms.
    ///
    #[staticmethod]
    #[pyo3(signature = (serial, baud_rate=115200, timeout_ms=500))]
    fn serial(serial: &str, baud_rate: u32, timeout_ms: u64) -> PyResult<Self> {
        let serial = serialport::new(serial, baud_rate)
            .timeout(Duration::from_millis(timeout_ms))
            .open()
            .into_diagnostic()
            .map_err(err_to_pyerr)?;
        let client = ::zephyr_mcumgr::MCUmgrClient::new_from_serial(serial);
        Ok(MCUmgrClient {
            client: Mutex::new(Some(Arc::new(client))),
        })
    }

    /// Configures the maximum SMP frame size that we can send to the device.
    ///
    /// Must not exceed [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40),
    /// otherwise we might crash the device.
    fn set_frame_size(&self, smp_frame_size: usize) -> PyResult<()> {
        self.get_client()?.set_frame_size(smp_frame_size);
        Ok(())
    }

    /// Configures the maximum SMP frame size that we can send to the device automatically
    /// by reading the value of [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// from the device.
    pub fn use_auto_frame_size(&self) -> PyResult<()> {
        self.get_client()?
            .use_auto_frame_size()
            .map_err(err_to_pyerr)
    }

    /// Changes the communication timeout.
    ///
    /// When the device does not respond to packets within the set
    /// duration, an error will be raised.
    pub fn set_timeout_ms(&self, timeout_ms: u64) -> PyResult<()> {
        self.get_client()?
            .set_timeout(Duration::from_millis(timeout_ms))
            .map_err(err_to_pyerr)
    }

    /// Sends a message to the device and expects the same message back as response.
    ///
    /// This can be used as a sanity check for whether the device is connected and responsive.
    fn os_echo(&self, msg: &str) -> PyResult<String> {
        self.get_client()?.os_echo(msg).map_err(err_to_pyerr)
    }

    /// Queries live task statistics
    ///
    /// ### Note
    ///
    /// Converts `stkuse` and `stksiz` to bytes.
    /// Zephyr originally reports them as number of 4 byte words.
    ///
    /// ### Return
    ///
    /// A map of task names with their respective statistics
    fn os_task_statistics(&self) -> PyResult<HashMap<String, TaskStatistics>> {
        self.get_client()?
            .os_task_statistics()
            .map(|tasks| {
                tasks
                    .into_iter()
                    .map(|(name, stats)| (name, stats.into()))
                    .collect()
            })
            .map_err(err_to_pyerr)
    }

    /// Sets the RTC of the device to the given datetime.
    ///
    /// Uses the contained local time and discards timezone information.
    ///
    pub fn os_set_datetime<'py>(&self, datetime: Bound<'py, PyDateTime>) -> PyResult<()> {
        self.get_client()?
            .os_set_datetime(datetime.extract()?)
            .map_err(err_to_pyerr)
    }

    /// Retrieves the device RTC's datetime.
    ///
    /// Will not contain timezone information.
    ///
    pub fn os_get_datetime<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDateTime>> {
        self.get_client()?
            .os_get_datetime()
            .map_err(err_to_pyerr)
            .and_then(|datetime| datetime.into_pyobject(py))
    }

    /// Issues a system reset.
    ///
    /// ### Arguments
    ///
    /// * `force` - Issues a force reset.
    /// * `boot_mode` - Overwrites the default boot mode.
    ///
    /// Known `boot_mode` values:
    /// * `0` - Normal system boot
    /// * `1` - Bootloader recovery mode
    ///
    /// Note that `boot_mode` only works if [`MCUMGR_GRP_OS_RESET_BOOT_MODE`](https://docs.zephyrproject.org/latest/kconfig.html#CONFIG_MCUMGR_GRP_OS_RESET_BOOT_MODE) is enabled.
    ///
    #[pyo3(signature = (force=false, boot_mode=None))]
    pub fn os_system_reset(&self, force: bool, boot_mode: Option<u8>) -> PyResult<()> {
        self.get_client()?
            .os_system_reset(force, boot_mode)
            .map_err(err_to_pyerr)
    }

    /// Fetch parameters from the MCUmgr library
    pub fn os_mcumgr_parameters(&self) -> PyResult<MCUmgrParametersResponse> {
        self.get_client()?
            .os_mcumgr_parameters()
            .map(Into::into)
            .map_err(err_to_pyerr)
    }

    /// Fetch information on the running image
    ///
    /// Similar to Linux's `uname` command.
    ///
    /// ### Arguments
    ///
    /// * `format` - Format specifier for the returned response
    ///
    /// For more information about the format specifier fields, see
    /// the [SMP documentation](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#os-application-info-request).
    ///
    #[pyo3(signature = (format=None))]
    pub fn os_application_info(&self, format: Option<&str>) -> PyResult<String> {
        self.get_client()?
            .os_application_info(format)
            .map_err(err_to_pyerr)
    }

    /// Fetch information on the device's bootloader
    pub fn os_bootloader_info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.get_client()?
            .os_bootloader_info()
            .map_err(err_to_pyerr)
            .map(|info| serde_pyobject::to_pyobject(py, &info))?
            .map_err(Into::into)
    }

    /// Load a file from the device.
    ///
    /// ### Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `progress` - A callable object that takes (transmitted, total) values as parameters.
    ///                Any return value is ignored. Raising an exception aborts the operation.
    ///
    /// ### Return
    ///
    /// The file content
    ///
    /// ### Performance
    ///
    /// Downloading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` or larger.
    #[pyo3(signature = (name, progress=None))]
    pub fn fs_file_download<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        #[gen_stub(override_type(type_repr="typing.Optional[collections.abc.Callable[[builtins.int, builtins.int], None]]", imports=("builtins", "collections.abc", "typing")))]
        progress: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let mut data = vec![];

        let mut cb_error = None;

        let res = if let Some(progress) = progress {
            let mut cb = |current, total| match progress.call((current, total), None) {
                Ok(_) => true,
                Err(e) => {
                    cb_error = Some(e);
                    false
                }
            };
            self.get_client()?
                .fs_file_download(name, &mut data, Some(&mut cb))
        } else {
            self.get_client()?.fs_file_download(name, &mut data, None)
        };

        if let Some(cb_error) = cb_error {
            return Err(cb_error);
        }

        res.map_err(err_to_pyerr)?;
        Ok(PyBytes::new(py, &data))
    }

    /// Write a file to the device.
    ///
    /// ### Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `data` - The file content.
    /// * `progress` - A callable object that takes (transmitted, total) values as parameters.
    ///                Any return value is ignored. Raising an exception aborts the operation.
    ///
    /// ### Performance
    ///
    /// Uploading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` and then enable larger chunking through either `set_frame_size`
    /// or `use_auto_frame_size`.
    #[pyo3(signature = (name, data, progress=None))]
    pub fn fs_file_upload<'py>(
        &self,
        name: &str,
        data: &Bound<'py, PyBytes>,
        #[gen_stub(override_type(type_repr="typing.Optional[collections.abc.Callable[[builtins.int, builtins.int], None]]", imports=("builtins", "collections.abc", "typing")))]
        progress: Option<Bound<'py, PyAny>>,
    ) -> PyResult<()> {
        let bytes: &[u8] = data.extract()?;

        let mut cb_error = None;

        let res = if let Some(progress) = progress {
            let mut cb = |current, total| match progress.call((current, total), None) {
                Ok(_) => true,
                Err(e) => {
                    cb_error = Some(e);
                    false
                }
            };
            self.get_client()?
                .fs_file_upload(name, bytes, bytes.len() as u64, Some(&mut cb))
        } else {
            self.get_client()?
                .fs_file_upload(name, bytes, bytes.len() as u64, None)
        };

        if let Some(cb_error) = cb_error {
            return Err(cb_error);
        }

        res.map_err(err_to_pyerr)
    }

    /// Queries the file status
    pub fn fs_file_status(&self, name: &str) -> PyResult<FileStatus> {
        self.get_client()?
            .fs_file_status(name)
            .map(Into::into)
            .map_err(err_to_pyerr)
    }

    /// Computes the hash/checksum of a file
    ///
    /// For available algorithms, see `fs_supported_checksum_types`.
    ///
    /// ### Arguments
    ///
    /// * `name` - The absolute path of the file on the device
    /// * `algorithm` - The hash/checksum algorithm to use, or default if None
    /// * `offset` - How many bytes of the file to skip
    /// * `length` - How many bytes to read after `offset`. None for the entire file.
    ///
    #[pyo3(signature = (name, algorithm=None, offset=0, length=None))]
    pub fn fs_file_checksum<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        algorithm: Option<&str>,
        offset: u64,
        length: Option<u64>,
    ) -> PyResult<FileChecksum> {
        self.get_client()?
            .fs_file_checksum(name, algorithm, offset, length)
            .map(|val| FileChecksum::from_response(py, val))
            .map_err(err_to_pyerr)
    }

    /// Queries which hash/checksum algorithms are available on the target
    pub fn fs_supported_checksum_types(&self) -> PyResult<HashMap<String, FileChecksumProperties>> {
        self.get_client()?
            .fs_supported_checksum_types()
            .map(|val| {
                let iter = val
                    .into_iter()
                    .map(|(key, value)| (key, FileChecksumProperties::from(value)));
                iter.collect()
            })
            .map_err(err_to_pyerr)
    }

    /// Close all device files MCUmgr has currently open
    pub fn fs_file_close(&self) -> PyResult<()> {
        self.get_client()?.fs_file_close().map_err(err_to_pyerr)
    }

    /// Run a shell command.
    ///
    /// ### Arguments
    ///
    /// * `argv` - The shell command to be executed.
    ///
    /// ### Return
    ///
    /// The command output
    ///
    pub fn shell_execute(&self, argv: Vec<String>) -> PyResult<String> {
        let (exitcode, data) = self
            .get_client()?
            .shell_execute(&argv)
            .map_err(err_to_pyerr)?;

        if exitcode < 0 {
            return Err(PyRuntimeError::new_err(format!(
                "Shell command returned error exit code: {}\n{}",
                ::zephyr_mcumgr::Errno::errno_to_string(exitcode),
                data
            )));
        }

        Ok(data)
    }

    /// Erase the `storage_partition` flash partition.
    pub fn zephyr_erase_storage(&self) -> PyResult<()> {
        self.get_client()?
            .zephyr_erase_storage()
            .map_err(err_to_pyerr)
    }

    /// Execute a raw MCUmgrCommand.
    ///
    /// Only returns if no error happened, so the
    /// user does not need to check for an `rc` or `err`
    /// field in the response.
    ///
    /// Read Zephyr's [SMP Protocol Specification](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html)
    /// for more information.
    ///
    /// ### Arguments
    ///
    /// * `write_operation` - Whether the command is a read or write operation.
    /// * `group_id` - The group ID of the command
    /// * `command_id` - The command ID
    /// * `data` - Anything that can be serialized as a proper packet payload.
    ///
    /// ### Example
    ///
    /// ```python
    /// client.raw_command(True, 0, 0, {"d": "Hello!"})
    /// # Returns: {'r': 'Hello!'}
    /// ```
    ///
    pub fn raw_command<'py>(
        &self,
        py: Python<'py>,
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        data: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let command = RawPyAnyCommand::new(write_operation, group_id, command_id, data)?;
        let result = self
            .get_client()?
            .raw_command(&command)
            .map_err(err_to_pyerr)?;
        RawPyAnyCommand::convert_result(py, result)
    }

    fn __enter__(slf: PyRef<Self>) -> PyResult<PyRef<Self>> {
        Ok(slf)
    }

    /// Closes the connection
    fn __exit__(
        &self,
        _exc_type: Py<PyAny>,
        _exc_value: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<bool> {
        self.client.lock().unwrap().take();
        Ok(false)
    }
}

/// ### Example
///
/// ```python
/// from zephyr_mcumgr import MCUmgrClient
///
/// with MCUmgrClient.serial("/dev/ttyACM0") as client:
///     client.set_timeout_ms(500)
///     client.use_auto_frame_size()
///
///     print(client.os_echo("Hello world!"))
///     # Hello world!
/// ```
///
#[pymodule]
mod zephyr_mcumgr {
    use pyo3::prelude::*;

    #[pymodule_export]
    use super::MCUmgrClient;
    #[pymodule_export]
    use super::return_types::FileChecksum;
    #[pymodule_export]
    use super::return_types::FileChecksumDataFormat;
    #[pymodule_export]
    use super::return_types::FileChecksumProperties;
    #[pymodule_export]
    use super::return_types::FileStatus;
    #[pymodule_export]
    use super::return_types::MCUmgrParametersResponse;
    #[pymodule_export]
    use super::return_types::TaskStatistics;

    #[pymodule_init]
    fn init(_m: &Bound<'_, PyModule>) -> PyResult<()> {
        pyo3_log::init();
        Ok(())
    }
}

// Define a function to gather stub information.
define_stub_info_gatherer!(stub_info);
