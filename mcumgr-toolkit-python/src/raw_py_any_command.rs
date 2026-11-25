use mcumgr_toolkit::commands::McuMgrCommand;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyTuple};
use pyo3::{Bound, PyAny, PyResult, Python, prelude::*};
use serde_pyobject::{from_pyobject, to_pyobject};

pub struct RawPyAnyCommand {
    write_operation: bool,
    group_id: u16,
    command_id: u8,
    data: ciborium::Value,
}

/// Converts a Python object to a CBOR value.
///
/// `serde_pyobject` has no notion of byte strings, so containers are walked here
/// and `bytes`/`bytearray` are converted to CBOR byte strings. Everything else is
/// left to `serde_pyobject`.
fn py_to_cbor(obj: &Bound<'_, PyAny>) -> PyResult<ciborium::Value> {
    if let Ok(bytes) = obj.cast::<PyBytes>() {
        return Ok(ciborium::Value::Bytes(bytes.as_bytes().to_vec()));
    }
    if let Ok(bytes) = obj.cast::<PyByteArray>() {
        return Ok(ciborium::Value::Bytes(bytes.to_vec()));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        return dict
            .iter()
            .map(|(key, value)| Ok((py_to_cbor(&key)?, py_to_cbor(&value)?)))
            .collect::<PyResult<_>>()
            .map(ciborium::Value::Map);
    }
    if let Ok(list) = obj.cast::<PyList>() {
        return list
            .iter()
            .map(|item| py_to_cbor(&item))
            .collect::<PyResult<_>>()
            .map(ciborium::Value::Array);
    }
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        return tuple
            .iter()
            .map(|item| py_to_cbor(&item))
            .collect::<PyResult<_>>()
            .map(ciborium::Value::Array);
    }

    Ok(from_pyobject(obj.clone())?)
}

impl RawPyAnyCommand {
    pub fn new<'py>(
        write_operation: bool,
        group_id: u16,
        command_id: u8,
        data: &Bound<'py, PyAny>,
    ) -> PyResult<Self> {
        let data = py_to_cbor(data)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::Value;
    use pyo3::ffi::c_str;

    fn try_convert(expr: &std::ffi::CStr) -> PyResult<Value> {
        Python::initialize();
        Python::attach(|py| {
            let obj = py.eval(expr, None, None).unwrap();
            py_to_cbor(&obj)
        })
    }

    fn convert(expr: &std::ffi::CStr) -> Value {
        try_convert(expr).unwrap()
    }

    #[test]
    fn bytes_become_byte_strings() {
        assert_eq!(convert(c_str!("b'\\x01\\x02'")), Value::Bytes(vec![1, 2]));
        assert_eq!(convert(c_str!("bytearray(b'\\x03')")), Value::Bytes(vec![3]));
    }

    #[test]
    fn bytes_nested_in_containers() {
        assert_eq!(
            convert(c_str!("{'entry': 'wlan0', 'val': b'\\xaa\\xbb'}")),
            Value::Map(vec![
                (Value::Text("entry".into()), Value::Text("wlan0".into())),
                (Value::Text("val".into()), Value::Bytes(vec![0xaa, 0xbb])),
            ])
        );
        assert_eq!(
            convert(c_str!("[b'\\x01', (b'\\x02', 3)]")),
            Value::Array(vec![
                Value::Bytes(vec![1]),
                Value::Array(vec![Value::Bytes(vec![2]), Value::Integer(3.into())]),
            ])
        );
    }

    #[test]
    fn other_types_are_unchanged() {
        assert_eq!(
            convert(c_str!("{'d': 'Hello!', 'n': -1, 'f': 1.5, 'b': True, 'x': None}")),
            Value::Map(vec![
                (Value::Text("d".into()), Value::Text("Hello!".into())),
                (Value::Text("n".into()), Value::Integer((-1).into())),
                (Value::Text("f".into()), Value::Float(1.5)),
                (Value::Text("b".into()), Value::Bool(true)),
                (Value::Text("x".into()), Value::Null),
            ])
        );
    }

    #[test]
    fn non_string_keys() {
        assert_eq!(
            convert(c_str!("{1: b'\\x01', b'k': 2}")),
            Value::Map(vec![
                (Value::Integer(1.into()), Value::Bytes(vec![1])),
                (Value::Bytes(b"k".to_vec()), Value::Integer(2.into())),
            ])
        );
    }

    #[test]
    fn empty_containers() {
        assert_eq!(convert(c_str!("{}")), Value::Map(vec![]));
        assert_eq!(convert(c_str!("[]")), Value::Array(vec![]));
        assert_eq!(convert(c_str!("()")), Value::Array(vec![]));
        assert_eq!(convert(c_str!("b''")), Value::Bytes(vec![]));
    }

    #[test]
    fn nested_errors_are_returned() {
        // too large for serde_pyobject's i64
        assert!(try_convert(c_str!("2**70")).is_err());
        assert!(try_convert(c_str!("{'val': b'\\x01', 'n': 2**70}")).is_err());
        assert!(try_convert(c_str!("[b'\\x01', (2**70,)]")).is_err());
        assert!(try_convert(c_str!("{2**70: 1}")).is_err());
    }
}
