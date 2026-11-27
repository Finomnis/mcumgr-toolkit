use mcumgr_toolkit::commands::McuMgrCommand;
use pyo3::{Bound, PyAny, PyResult, Python};
use pythonize::{depythonize, pythonize};

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
        let data = depythonize(data)?;
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
        pythonize(py, &result).map_err(Into::into)
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
    use pyo3::prelude::*;
    use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString};
    use std::ffi::CStr;

    fn eval<'py>(py: Python<'py>, expr: &CStr) -> Bound<'py, PyAny> {
        py.eval(expr, None, None).unwrap()
    }

    fn try_encode(expr: &CStr) -> PyResult<Value> {
        Python::initialize();
        Python::attach(|py| {
            RawPyAnyCommand::new(true, 100, 0, &eval(py, expr)).map(|cmd| cmd.data().clone())
        })
    }

    fn encode(expr: &CStr) -> Value {
        try_encode(expr).unwrap()
    }

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    fn int(i: i64) -> Value {
        Value::Integer(i.into())
    }

    #[test]
    fn command_header() {
        Python::initialize();
        Python::attach(|py| {
            let cmd = RawPyAnyCommand::new(false, 100, 3, &eval(py, c"{}")).unwrap();
            assert!(!cmd.is_write_operation());
            assert_eq!(cmd.group_id(), 100);
            assert_eq!(cmd.command_id(), 3);
        });
    }

    #[test]
    fn bytes_become_byte_strings() {
        assert_eq!(encode(c"b'\\x01\\x02\\x03'"), Value::Bytes(vec![1, 2, 3]));
        assert_eq!(encode(c"b''"), Value::Bytes(vec![]));
        assert_eq!(
            encode(c"bytes(range(256))"),
            Value::Bytes((0..=255).collect())
        );
    }

    #[test]
    fn bytearray_is_rejected() {
        // pythonize doesn't support bytearray yet, see
        // https://github.com/davidhewitt/pythonize/issues/130
        // Once fixed, this should become `Value::Bytes`.
        assert!(try_encode(c"bytearray(b'\\x01\\x02')").is_err());
    }

    #[test]
    fn bytes_in_flat_payload() {
        assert_eq!(
            encode(c"{'name': 'foo', 'data': b'\\x01\\x02\\x03'}"),
            Value::Map(vec![
                (text("name"), text("foo")),
                (text("data"), Value::Bytes(vec![1, 2, 3])),
            ])
        );
    }

    #[test]
    fn bytes_nested_in_containers() {
        assert_eq!(
            encode(c"{'outer': {'inner': b'\\xaa'}, 'list': [b'\\x01', (b'\\x02', 3)]}"),
            Value::Map(vec![
                (
                    text("outer"),
                    Value::Map(vec![(text("inner"), Value::Bytes(vec![0xaa]))])
                ),
                (
                    text("list"),
                    Value::Array(vec![
                        Value::Bytes(vec![1]),
                        Value::Array(vec![Value::Bytes(vec![2]), int(3)]),
                    ])
                ),
            ])
        );
    }

    #[test]
    fn scalar_types() {
        assert_eq!(
            encode(c"{'s': 'Hello!', 'n': -1, 'u': 2**64 - 1, 'f': 1.5, 'b': True, 'x': None}"),
            Value::Map(vec![
                (text("s"), text("Hello!")),
                (text("n"), int(-1)),
                (text("u"), Value::Integer(u64::MAX.into())),
                (text("f"), Value::Float(1.5)),
                (text("b"), Value::Bool(true)),
                (text("x"), Value::Null),
            ])
        );
    }

    #[test]
    fn non_string_keys() {
        assert_eq!(
            encode(c"{1: 'a', b'k': 2}"),
            Value::Map(vec![
                (int(1), text("a")),
                (Value::Bytes(b"k".to_vec()), int(2)),
            ])
        );
    }

    #[test]
    fn empty_containers() {
        assert_eq!(encode(c"{}"), Value::Map(vec![]));
        assert_eq!(encode(c"[]"), Value::Array(vec![]));
        assert_eq!(encode(c"()"), Value::Array(vec![]));
        assert_eq!(encode(c"''"), text(""));
    }

    #[test]
    fn big_integers_become_bignums() {
        // outside of the u64/i64 range, integers are encoded as CBOR bignums (tag 2/3)
        let mut magnitude = vec![0u8; 9];
        magnitude[0] = 0x40;
        assert_eq!(
            encode(c"2**70"),
            Value::Tag(2, Box::new(Value::Bytes(magnitude)))
        );
        // negative bignums store -1 - n
        let mut magnitude = vec![0xffu8; 9];
        magnitude[0] = 0x3f;
        assert_eq!(
            encode(c"-(2**70)"),
            Value::Tag(3, Box::new(Value::Bytes(magnitude)))
        );
    }

    #[test]
    fn unsupported_values_are_errors() {
        // too large even for 128 bit integers, also when nested
        assert!(try_encode(c"2**200").is_err());
        assert!(try_encode(c"{'data': b'\\x01', 'n': 2**200}").is_err());
        assert!(try_encode(c"[b'\\x01', (2**200,)]").is_err());
        assert!(try_encode(c"{2**200: 1}").is_err());
        // not serializable at all
        assert!(try_encode(c"object()").is_err());
        assert!(try_encode(c"{'f': lambda: 0}").is_err());
    }

    #[test]
    fn response_types() {
        Python::initialize();
        Python::attach(|py| {
            let response = Value::Map(vec![
                (text("s"), text("Hello!")),
                (text("n"), int(-1)),
                (text("f"), Value::Float(1.5)),
                (text("b"), Value::Bool(true)),
                (text("x"), Value::Null),
                (text("data"), Value::Bytes(vec![1, 2, 3])),
                (
                    text("list"),
                    Value::Array(vec![Value::Bytes(vec![4]), int(5)]),
                ),
            ]);
            let obj = RawPyAnyCommand::convert_result(py, response).unwrap();
            let dict = obj.cast::<PyDict>().unwrap();
            let get = |key: &str| dict.get_item(key).unwrap().unwrap();

            assert!(get("s").is_instance_of::<PyString>());
            assert!(get("n").is_instance_of::<PyInt>());
            assert!(get("f").is_instance_of::<PyFloat>());
            assert!(get("b").is_instance_of::<PyBool>());
            assert!(get("x").is_none());

            let data = get("data");
            assert!(data.is_instance_of::<PyBytes>());
            assert_eq!(data.cast::<PyBytes>().unwrap().as_bytes(), &[1, 2, 3]);

            let list = get("list");
            let list = list.cast::<PyList>().unwrap();
            assert!(list.get_item(0).unwrap().is_instance_of::<PyBytes>());
            assert_eq!(list.get_item(1).unwrap().extract::<i64>().unwrap(), 5);
        });
    }

    #[test]
    fn round_trip() {
        Python::initialize();
        Python::attach(|py| {
            for expr in [
                c"{'name': 'foo', 'data': b'\\x01\\x02\\x03'}",
                c"{'outer': {'inner': [b'\\xaa', 1, -2, 1.5, True, None, '']}}",
                c"{'empty': b'', 'list': [], 'dict': {}}",
                c"{1: b'\\x01', b'k': 'v'}",
            ] {
                let original = eval(py, expr);
                let cmd = RawPyAnyCommand::new(true, 100, 0, &original).unwrap();
                let back = RawPyAnyCommand::convert_result(py, cmd.data().clone()).unwrap();
                assert!(back.eq(&original).unwrap(), "{expr:?} came back as {back}");
            }
        });
    }
}
