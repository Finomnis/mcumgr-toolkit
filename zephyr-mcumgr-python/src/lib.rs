#![forbid(unsafe_code)]

use miette::IntoDiagnostic;
use pyo3::{prelude::*, types::PyBytes};

use pyo3::exceptions::PyRuntimeError;
use pyo3_stub_gen::{
    define_stub_info_gatherer,
    derive::{gen_stub_pyclass, gen_stub_pymethods},
};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crate::raw_py_any_command::RawPyAnyCommand;

mod raw_py_any_command;

/// A high level client for Zephyr's MCUmgr SMP functionality
#[gen_stub_pyclass]
#[pyclass(frozen)]
struct MCUmgrClient {
    client: Mutex<::zephyr_mcumgr::MCUmgrClient>,
}

fn err_to_pyerr<E: Into<miette::Report>>(err: E) -> PyErr {
    let e: miette::Report = err.into();
    PyRuntimeError::new_err(format!("{e:?}"))
}

impl MCUmgrClient {
    fn lock(&self) -> PyResult<MutexGuard<'_, ::zephyr_mcumgr::MCUmgrClient>> {
        self.client
            .lock()
            .map_err(|e| PyRuntimeError::new_err(format!("{e}")))
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl MCUmgrClient {
    /// Creates a new serial port based Zephyr MCUmgr SMP client.
    ///
    ///  # Arguments
    ///
    /// * `serial` - The identifier of the serial device. (Windows: `COMxx`, Linux: `/dev/ttyXX`)
    /// * `baud_rate` - The baud rate of the serial port.
    /// * `timeout_ms` - The communication timeout, in ms.
    ///
    #[staticmethod]
    #[pyo3(signature = (serial, baud_rate=115200, timeout_ms=500))]
    fn new_from_serial(serial: &str, baud_rate: u32, timeout_ms: u64) -> PyResult<Self> {
        let serial = serialport::new(serial, baud_rate)
            .timeout(Duration::from_millis(timeout_ms))
            .open()
            .into_diagnostic()
            .map_err(err_to_pyerr)?;
        let client = ::zephyr_mcumgr::MCUmgrClient::new_from_serial(serial);
        Ok(MCUmgrClient {
            client: Mutex::new(client),
        })
    }

    /// Configures the maximum SMP frame size that we can send to the device.
    ///
    /// Must not exceed [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40),
    /// otherwise we might crash the device.
    fn set_frame_size(&self, smp_frame_size: usize) -> PyResult<()> {
        self.lock()?.set_frame_size(smp_frame_size);
        Ok(())
    }

    /// Configures the maximum SMP frame size that we can send to the device automatically
    /// by reading the value of [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// from the device.
    pub fn use_auto_frame_size(&self) -> PyResult<()> {
        self.lock()?.use_auto_frame_size().map_err(err_to_pyerr)
    }

    /// Changes the communication timeout.
    ///
    /// When the device does not respond to packets within the set
    /// duration, an error will be raised.
    pub fn set_timeout_ms(&self, timeout_ms: u64) -> PyResult<()> {
        self.lock()?
            .set_timeout(Duration::from_millis(timeout_ms))
            .map_err(err_to_pyerr)
    }

    /// Sends a message to the device and expects the same message back as response.
    ///
    /// This can be used as a sanity check for whether the device is connected and responsive.
    fn os_echo(&self, msg: &str) -> PyResult<String> {
        self.lock()?.os_echo(msg).map_err(err_to_pyerr)
    }

    /// Load a file from the device.
    ///
    ///  # Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `progress` - A callable object that takes (transmitted, total) values as parameters.
    ///                Any return value is ignored. Raising an exception aborts the operation.
    ///
    /// # Return
    ///
    /// The file content
    ///
    /// # Performance
    ///
    /// Downloading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` or larger.
    #[pyo3(signature = (name, progress=None))]
    pub fn fs_file_download<'py>(
        &self,
        py: Python<'py>,
        name: &str,
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
            self.lock()?
                .fs_file_download(name, &mut data, Some(&mut cb))
        } else {
            self.lock()?.fs_file_download(name, &mut data, None)
        };

        if let Some(cb_error) = cb_error {
            return Err(cb_error);
        }

        res.map_err(err_to_pyerr)?;
        Ok(PyBytes::new(py, &data))
    }

    /// Write a file to the device.
    ///
    ///  # Arguments
    ///
    /// * `name` - The full path of the file on the device.
    /// * `data` - The file content.
    /// * `progress` - A callable object that takes (transmitted, total) values as parameters.
    ///                Any return value is ignored. Raising an exception aborts the operation.
    ///
    /// # Performance
    ///
    /// Uploading files with Zephyr's default parameters is slow.
    /// You want to increase [`MCUMGR_TRANSPORT_NETBUF_SIZE`](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/transport/Kconfig#L40)
    /// to maybe `4096` and then enable larger chunking through either [`MCUmgrClient::set_frame_size`]
    /// or [`MCUmgrClient::use_auto_frame_size`].
    #[pyo3(signature = (name, data, progress=None))]
    pub fn fs_file_upload<'py>(
        &self,
        name: &str,
        data: &Bound<'py, PyBytes>,
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
            self.lock()?
                .fs_file_upload(name, bytes, bytes.len() as u64, Some(&mut cb))
        } else {
            self.lock()?
                .fs_file_upload(name, bytes, bytes.len() as u64, None)
        };

        if let Some(cb_error) = cb_error {
            return Err(cb_error);
        }

        res.map_err(err_to_pyerr)
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
    pub fn shell_execute(&self, argv: Vec<String>) -> PyResult<(i32, String)> {
        self.lock()?.shell_execute(&argv).map_err(err_to_pyerr)
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
    /// # Arguments
    ///
    /// * `write_operation` - Whether the command is a read or write operation.
    /// * `group_id` - The group ID of the command
    /// * `command_id` - The command ID
    /// * `data` - Anything that can be serialized as a proper packet payload.
    ///
    /// # Example
    ///
    /// ```python
    /// client.raw_command(True, 0, 0, {"d": "Hello!"})
    /// ```
    ///
    /// Response:
    /// ```none
    /// {'r': 'Hello!'}
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
        let result = self.lock()?.raw_command(&command).map_err(err_to_pyerr)?;
        RawPyAnyCommand::convert_result(py, result)
    }
}

#[pymodule]
mod zephyr_mcumgr {
    #[pymodule_export]
    use super::MCUmgrClient;
}

// Define a function to gather stub information.
define_stub_info_gatherer!(stub_info);
