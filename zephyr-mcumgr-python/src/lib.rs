#![forbid(unsafe_code)]

use pyo3::prelude::*;

use pyo3::exceptions::PyRuntimeError;
use std::error::Error;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

#[pyclass(frozen)]
struct MCUmgrClient {
    client: Mutex<::zephyr_mcumgr::MCUmgrClient>,
}

fn convert_error<T, E: Error>(res: Result<T, E>) -> PyResult<T> {
    res.map_err(|e| PyRuntimeError::new_err(format!("{e}")))
}

impl MCUmgrClient {
    fn lock(&self) -> PyResult<MutexGuard<'_, ::zephyr_mcumgr::MCUmgrClient>> {
        let res = self.client.lock();
        convert_error(res)
    }
}

#[pymethods]
impl MCUmgrClient {
    #[staticmethod]
    #[pyo3(signature = (serial, baud_rate=115200, timeout_ms=500))]
    fn new_from_serial(serial: &str, baud_rate: u32, timeout_ms: u64) -> PyResult<Self> {
        let serial = serialport::new(serial, baud_rate)
            .timeout(Duration::from_millis(timeout_ms))
            .open();
        let serial = convert_error(serial)?;
        let client = ::zephyr_mcumgr::MCUmgrClient::new_from_serial(serial);
        Ok(MCUmgrClient {
            client: Mutex::new(client),
        })
    }

    fn os_echo(&self, msg: &str) -> PyResult<String> {
        let res = self.lock()?.os_echo(msg);
        convert_error(res)
    }
}

#[pymodule]
mod zephyr_mcumgr {
    #[pymodule_export]
    use super::MCUmgrClient;
}
