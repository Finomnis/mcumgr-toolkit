use pyo3::{
    Py, Python,
    types::{PyBytes, PyBytesMethods},
};
use serde::Serializer;

/// Makes the struct `print`able by converting it
/// to a python dict and then printing that
macro_rules! generate_repr_from_serialize {
    ($type:ty) => {
        #[pyo3::pymethods]
        impl $type {
            fn __repr__(&self, py: pyo3::Python<'_>) -> pyo3::PyResult<String> {
                let py_obj = serde_pyobject::to_pyobject(py, self)?;

                let repr_str = py
                    .import("builtins")?
                    .getattr("repr")?
                    .call1((py_obj,))?
                    .extract::<String>()?;

                Ok(repr_str)
            }
        }
    };
}

pub fn serialize_pybytes_as_hex<S>(pybytes: &Py<PyBytes>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    Python::attach(|py| {
        let bytes = pybytes.bind(py).as_bytes();
        serializer.serialize_str(&hex::encode(bytes))
    })
}

pub fn serialize_option_pybytes_as_hex<S>(
    pybytes: &Option<Py<PyBytes>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(pybytes) = &pybytes {
        serialize_pybytes_as_hex(pybytes, serializer)
    } else {
        serializer.serialize_none()
    }
}

pub(crate) use generate_repr_from_serialize;
