use pyo3::{prelude::*, types::PyBytes};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction};
use serde::Serialize;

use crate::repr_macro::generate_repr_from_serialize;

/// Information about an MCUboot firmware image
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize, Debug)]
pub struct ImageInfo {
    /// Firmware version
    #[pyo3(get)]
    pub version: String,
    /// The identifying hash for the firmware
    #[serde(serialize_with = "crate::repr_macro::serialize_pybytes_as_hex")]
    #[pyo3(get)]
    pub hash: Py<PyBytes>,
}
generate_repr_from_serialize!(ImageInfo);

/// Parse an MCUboot firmware image
#[pyfunction]
#[gen_stub_pyfunction]
pub fn mcuboot_parse_image<'py>(
    py: Python<'py>,
    image_data: Bound<'py, PyBytes>,
) -> PyResult<ImageInfo> {
    let data = image_data.as_bytes();
    let image_info = zephyr_mcumgr::mcuboot::image::parse(std::io::Cursor::new(data))
        .map_err(super::err_to_pyerr)?;

    Ok(ImageInfo {
        version: image_info.version.to_string(),
        hash: PyBytes::new(py, &image_info.hash).unbind(),
    })
}
