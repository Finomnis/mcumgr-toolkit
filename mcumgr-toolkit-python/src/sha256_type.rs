use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sha256(pub [u8; 32]);

impl FromPyObject<'_, '_> for Sha256 {
    type Error = PyErr;

    fn extract(obj: Borrowed<'_, '_, PyAny>) -> PyResult<Self> {
        // raw bytes checksum
        if let Ok(b) = obj.cast::<PyBytes>() {
            let bytes = b.as_bytes();

            let out = bytes.try_into().map_err(|_| {
                PyValueError::new_err(format!(
                    "sha256 bytes must be length 32, got {}",
                    bytes.len()
                ))
            })?;

            return Ok(Sha256(out));
        }

        // hex encoded string checksum
        if let Ok(s) = obj.cast::<PyString>() {
            let txt = s.to_str()?;
            let mut out = [0u8; 32];
            hex::decode_to_slice(txt, &mut out)
                .map_err(|e| PyValueError::new_err(format!("invalid sha256 hex string: {e}")))?;

            return Ok(Sha256(out));
        }

        Err(PyValueError::new_err(
            "sha256 must be a 64-char hex string or a 32-byte bytes object",
        ))
    }
}

impl pyo3_stub_gen::PyStubType for Sha256 {
    fn type_input() -> pyo3_stub_gen::TypeInfo {
        pyo3_stub_gen::TypeInfo::builtin("str") | pyo3_stub_gen::TypeInfo::builtin("bytes")
    }

    fn type_output() -> pyo3_stub_gen::TypeInfo {
        panic!("Sha256 is only an input type")
    }
}
