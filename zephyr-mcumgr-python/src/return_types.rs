use pyo3::{prelude::*, types::PyBytes};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};

use ::zephyr_mcumgr::commands;

/// Return value of [`MCUmgrClient::fs_file_status`].
#[gen_stub_pyclass]
#[pyclass(frozen)]
pub struct FileStatus {
    /// length of file (in bytes)
    #[pyo3(get)]
    length: u64,
}
impl From<commands::fs::FileStatusResponse> for FileStatus {
    fn from(value: commands::fs::FileStatusResponse) -> Self {
        Self { length: value.len }
    }
}

/// Return value of [`MCUmgrClient::fs_file_hash_checksum`].
#[gen_stub_pyclass]
#[pyclass(frozen)]
pub struct FileHashChecksum {
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
    pub output: Py<PyBytes>,
}
impl FileHashChecksum {
    pub(crate) fn from_response<'py>(
        py: Python<'py>,
        value: commands::fs::FileHashChecksumResponse,
    ) -> Self {
        let output = match value.output {
            commands::fs::FileHashChecksumData::Hash(data) => PyBytes::new(py, &data).unbind(),
            commands::fs::FileHashChecksumData::Checksum(data) => {
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
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum HashChecksumDataFormat {
    /// Data is a number
    Numerical = 0,
    /// Data is a bytes array
    ByteArray = 1,
}

/// Properties of a hash/checksum algorithm
#[gen_stub_pyclass]
#[pyclass(frozen)]
pub struct HashChecksumProperties {
    /// format that the hash/checksum returns
    #[pyo3(get)]
    pub format: HashChecksumDataFormat,
    /// size (in bytes) of output hash/checksum response
    #[pyo3(get)]
    pub size: u32,
}
impl From<commands::fs::SupportedFileHashChecksumTypesEntry> for HashChecksumProperties {
    fn from(value: commands::fs::SupportedFileHashChecksumTypesEntry) -> Self {
        Self {
            format: match value.format {
                commands::fs::SupportedFileHashChecksumDataFormat::Numerical => {
                    HashChecksumDataFormat::Numerical
                }
                commands::fs::SupportedFileHashChecksumDataFormat::ByteArray => {
                    HashChecksumDataFormat::ByteArray
                }
            },
            size: value.size,
        }
    }
}
