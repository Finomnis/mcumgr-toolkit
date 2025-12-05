use pyo3::{prelude::*, types::PyBytes};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};

use ::zephyr_mcumgr::commands;
use serde::Serialize;

use crate::repr_macro::generate_repr_from_serialize;

/// Return value of `MCUmgrClient.fs_file_status`.
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct FileStatus {
    /// length of file (in bytes)
    #[pyo3(get)]
    pub length: u64,
}
generate_repr_from_serialize!(FileStatus);
impl From<commands::fs::FileStatusResponse> for FileStatus {
    fn from(value: commands::fs::FileStatusResponse) -> Self {
        Self { length: value.len }
    }
}

/// Return value of `MCUmgrClient.os_mcumgr_parameters`.
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct MCUmgrParametersResponse {
    /// Single SMP buffer size, this includes SMP header and CBOR payload
    #[pyo3(get)]
    pub buf_size: u32,
    /// Number of SMP buffers supported
    #[pyo3(get)]
    pub buf_count: u32,
}
generate_repr_from_serialize!(MCUmgrParametersResponse);
impl From<commands::os::MCUmgrParametersResponse> for MCUmgrParametersResponse {
    fn from(value: commands::os::MCUmgrParametersResponse) -> Self {
        Self {
            buf_size: value.buf_size,
            buf_count: value.buf_count,
        }
    }
}

/// Return value of `MCUmgrClient.fs_file_checksum`.
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct FileChecksum {
    /// type of hash/checksum that was performed
    #[pyo3(name = "type", get)]
    pub r#type: String,
    /// offset that hash/checksum calculation started at
    #[pyo3(get)]
    pub offset: u64,
    /// length of input data used for hash/checksum generation (in bytes)
    #[pyo3(get)]
    pub length: u64,
    /// output hash/checksum
    #[pyo3(get)]
    #[serde(serialize_with = "crate::repr_macro::serialize_pybytes_as_hex")]
    pub output: Py<PyBytes>,
}
generate_repr_from_serialize!(FileChecksum);

impl FileChecksum {
    pub(crate) fn from_response<'py>(
        py: Python<'py>,
        value: commands::fs::FileChecksumResponse,
    ) -> Self {
        let output = match value.output {
            commands::fs::FileChecksumData::Hash(data) => PyBytes::new(py, &data).unbind(),
            commands::fs::FileChecksumData::Checksum(data) => {
                PyBytes::new(py, &data.to_be_bytes()).unbind()
            }
        };
        Self {
            r#type: value.r#type,
            offset: value.off,
            length: value.len,
            output,
        }
    }
}

/// Data format of the hash/checksum type
#[gen_stub_pyclass_enum]
#[pyclass(frozen, eq, eq_int)]
#[derive(Copy, Clone, Eq, PartialEq, Serialize)]
pub enum FileChecksumDataFormat {
    /// Data is a number
    Numerical = 0,
    /// Data is a bytes array
    ByteArray = 1,
}

/// Properties of a hash/checksum algorithm
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct FileChecksumProperties {
    /// format that the hash/checksum returns
    #[pyo3(get)]
    pub format: FileChecksumDataFormat,
    /// size (in bytes) of output hash/checksum response
    #[pyo3(get)]
    pub size: u32,
}
generate_repr_from_serialize!(FileChecksumProperties);

impl From<commands::fs::FileChecksumProperties> for FileChecksumProperties {
    fn from(value: commands::fs::FileChecksumProperties) -> Self {
        Self {
            format: match value.format {
                commands::fs::FileChecksumDataFormat::Numerical => {
                    FileChecksumDataFormat::Numerical
                }
                commands::fs::FileChecksumDataFormat::ByteArray => {
                    FileChecksumDataFormat::ByteArray
                }
            },
            size: value.size,
        }
    }
}

/// Statistics of an MCU task/thread
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct TaskStatistics {
    /// task priority
    #[pyo3(get)]
    pub prio: i32,
    /// numeric task ID
    #[pyo3(get)]
    pub tid: u32,
    /// numeric task state
    #[pyo3(get)]
    pub state: u32,
    /// task’s/thread’s stack usage
    #[pyo3(get)]
    pub stkuse: Option<u64>,
    /// task’s/thread’s stack size
    #[pyo3(get)]
    pub stksiz: Option<u64>,
    /// task’s/thread’s context switches
    #[pyo3(get)]
    pub cswcnt: Option<u64>,
    /// task’s/thread’s runtime in “ticks”
    #[pyo3(get)]
    pub runtime: Option<u64>,
}
generate_repr_from_serialize!(TaskStatistics);

impl From<commands::os::TaskStatisticsEntry> for TaskStatistics {
    fn from(value: commands::os::TaskStatisticsEntry) -> Self {
        Self {
            prio: value.prio,
            tid: value.tid,
            state: value.state,
            stkuse: value.stkuse,
            stksiz: value.stksiz,
            cswcnt: value.cswcnt,
            runtime: value.runtime,
        }
    }
}

/// The state of an image slot
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct ImageState {
    /// image number
    #[pyo3(get)]
    pub image: u64,
    /// slot number within “image”
    #[pyo3(get)]
    pub slot: u64,
    /// string representing image version, as set with `imgtool`
    #[pyo3(get)]
    pub version: String,
    /// SHA256 hash of the image header and body
    ///
    /// Note that this will not be the same as the SHA256 of the whole file, it is the field in the
    /// MCUboot TLV section that contains a hash of the data which is used for signature
    /// verification purposes.
    #[pyo3(get)]
    #[serde(serialize_with = "crate::repr_macro::serialize_option_pybytes_as_hex")]
    pub hash: Option<Py<PyBytes>>,
    /// true if image has bootable flag set
    #[pyo3(get)]
    pub bootable: bool,
    /// true if image is set for next swap
    #[pyo3(get)]
    pub pending: bool,
    /// true if image has been confirmed
    #[pyo3(get)]
    pub confirmed: bool,
    /// true if image is currently active application
    #[pyo3(get)]
    pub active: bool,
    /// true if image is to stay in primary slot after the next boot
    #[pyo3(get)]
    pub permanent: bool,
}
generate_repr_from_serialize!(ImageState);

impl ImageState {
    pub(crate) fn from_response<'py>(py: Python<'py>, value: commands::image::ImageState) -> Self {
        Self {
            image: value.image,
            slot: value.slot,
            version: value.version,
            hash: value.hash.map(|val| PyBytes::new(py, &val).unbind()),
            bootable: value.bootable,
            pending: value.pending,
            confirmed: value.confirmed,
            active: value.active,
            permanent: value.permanent,
        }
    }
}
