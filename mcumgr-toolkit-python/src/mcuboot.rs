use pyo3::{prelude::*, types::PyBytes};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction};
use serde::Serialize;

use crate::repr_macro::generate_repr_from_serialize;

/// Information about an MCUboot firmware image
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize, Debug)]
pub struct McubootImageInfo {
    /// Firmware version
    #[pyo3(get)]
    pub version: String,
    /// The identifying hash for the firmware
    ///
    /// Note that this will not be the same as the SHA256 of the whole file, it is the field in the
    /// MCUboot TLV section that contains a hash of the data which is used for signature
    /// verification purposes.
    #[serde(serialize_with = "crate::repr_macro::serialize_pybytes_as_hex")]
    #[pyo3(get)]
    pub hash: Py<PyBytes>,
}
generate_repr_from_serialize!(McubootImageInfo);

/// Extract information from an MCUboot image file
#[pyfunction]
#[gen_stub_pyfunction]
pub fn mcuboot_get_image_info<'py>(
    py: Python<'py>,
    image_data: Bound<'py, PyBytes>,
) -> PyResult<McubootImageInfo> {
    let data = image_data.as_bytes();
    let image_info = zephyr_mcumgr::mcuboot::get_image_info(std::io::Cursor::new(data))
        .map_err(super::err_to_pyerr)?;

    Ok(McubootImageInfo {
        version: image_info.version.to_string(),
        hash: PyBytes::new(py, &image_info.hash).unbind(),
    })
}
