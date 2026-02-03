use pyo3::{Bound, PyAny, PyResult, Python};
use serde_pyobject::{from_pyobject, to_pyobject};
use zephyr_mcumgr::commands::McuMgrCommand;

pub struct RawPyAnyCommand {
    write_operation: bool,
    group_id: u16,
    command_id: u8,
    data: ciborium::Value,
}

impl RawPyAnyCommand {
    pub fn new<'py>(
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        data: &Bound<'py, PyAny>,
    ) -> PyResult<Self> {
        let data = from_pyobject(data.clone())?;
        Ok(Self {
            write_operation,
            group_id,
            command_id,
            data,
        })
    }

    pub fn convert_result<'py>(
        py: Python<'py>,
        result: ciborium::Value,
    ) -> PyResult<Bound<'py, PyAny>> {
        to_pyobject(py, &result).map_err(Into::into)
    }
}

impl McuMgrCommand for RawPyAnyCommand {
    type Payload = ciborium::Value;
    type Response = ciborium::Value;

    fn is_write_operation(&self) -> bool {
        self.write_operation
    }

    fn group_id(&self) -> u16 {
        self.group_id
    }

    fn command_id(&self) -> u8 {
        self.command_id
    }

    fn data(&self) -> &Self::Payload {
        &self.data
    }
}
