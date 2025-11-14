use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use strum_macros::Display;

use super::is_default;

/// [File Download](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-download) command
#[derive(Debug, Serialize)]
pub struct FileDownload<'a> {
    /// offset to start download at
    pub off: u64,
    /// absolute path to a file
    pub name: &'a str,
}

/// Response for [`FileDownload`] command
#[derive(Debug, Deserialize)]
pub struct FileDownloadResponse {
    /// offset the response is for
    pub off: u64,
    /// chunk of data read from file
    pub data: Vec<u8>,
    /// length of file, this field is only mandatory when “off” is 0
    pub len: Option<u64>,
}

/// Computes how large [`FileUpload::data`] is allowed to be.
///
/// Taken from Zephyr's [MCUMGR_GRP_FS_DL_CHUNK_SIZE](https://github.com/zephyrproject-rtos/zephyr/blob/v4.2.1/subsys/mgmt/mcumgr/grp/fs_mgmt/include/mgmt/mcumgr/grp/fs_mgmt/fs_mgmt_config.h#L45).
///
/// # Arguments
///
/// * `smp_frame_size` - The max allowed size of an SMP frame.
pub const fn file_upload_max_data_chunk_size(smp_frame_size: usize) -> usize {
    const MCUMGR_GRP_FS_MAX_OFFSET_LEN: usize = std::mem::size_of::<u64>();
    const MGMT_HDR_SIZE: usize = 8; // Size of SMP header
    const CBOR_AND_OTHER_HDR: usize = MGMT_HDR_SIZE
        + (9 + 1)
        + (1 + 3 + MCUMGR_GRP_FS_MAX_OFFSET_LEN)
        + (1 + 4 + MCUMGR_GRP_FS_MAX_OFFSET_LEN)
        + (1 + 2 + 1)
        + (1 + 3 + MCUMGR_GRP_FS_MAX_OFFSET_LEN);

    smp_frame_size - CBOR_AND_OTHER_HDR
}

/// [File Upload](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-upload) command
#[derive(Debug, Serialize)]
pub struct FileUpload<'a, 'b> {
    /// offset to start/continue upload at
    pub off: u64,
    /// chunk of data to write to the file
    #[serde(with = "serde_bytes")]
    pub data: &'a [u8],
    /// absolute path to a file
    pub name: &'b str,
    /// length of file, this field is only mandatory when “off” is 0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<u64>,
}

/// Response for [`FileUpload`] command
#[derive(Debug, Deserialize)]
pub struct FileUploadResponse {
    /// offset of last successfully written data
    pub off: u64,
}

/// [File Status](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-status) command
#[derive(Debug, Serialize)]
pub struct FileStatus<'a> {
    /// absolute path to a file
    pub name: &'a str,
}

/// Response for [`FileStatus`] command
#[derive(Debug, Deserialize)]
pub struct FileStatusResponse {
    /// length of file (in bytes)
    pub len: u64,
}

/// [File Hash/Checksum](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-hash-checksum) command
#[derive(Debug, Serialize)]
pub struct FileChecksum<'a, 'b> {
    /// absolute path to a file
    pub name: &'a str,
    /// type of hash/checksum to perform or None to use default
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<&'b str>,
    /// offset to start hash/checksum calculation at
    #[serde(default, skip_serializing_if = "is_default")]
    pub off: u64,
    /// maximum length of data to read from file to generate hash/checksum with (optional, full file size if None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<u64>,
}

/// Response for [`FileChecksum`] command
#[derive(Debug, Deserialize)]
pub struct FileChecksumResponse {
    /// type of hash/checksum that was performed
    pub r#type: String,
    /// offset that hash/checksum calculation started at
    #[serde(default, skip_serializing_if = "is_default")]
    pub off: u64,
    /// length of input data used for hash/checksum generation (in bytes)
    pub len: u64,
    /// output hash/checksum
    pub output: FileChecksumData,
}

/// Hash data of [`FileChecksumResponse`]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FileChecksumData {
    /// hash bytes
    #[serde(with = "serde_bytes")]
    Hash(Box<[u8]>),
    /// checksum integer
    Checksum(u32),
}

impl FileChecksumData {
    /// Convert to hex string
    pub fn hex(&self) -> String {
        match self {
            FileChecksumData::Hash(data) => data.iter().map(|val| format!("{val:02x}")).collect(),
            FileChecksumData::Checksum(value) => format!("{value:08x}"),
        }
    }
}

/// [Supported file hash/checksum types](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#supported-file-hash-checksum-types) command
#[derive(Debug, Serialize)]
pub struct SupportedFileChecksumTypes;

/// Response for [`SupportedFileChecksumTypes`] command
#[derive(Debug, Deserialize)]
pub struct SupportedFileChecksumTypesResponse {
    /// names and properties of the hash/checksum types
    pub r#types: HashMap<String, FileChecksumProperties>,
}

/// Data format of the hash/checksum type
#[derive(Display, Deserialize_repr, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum FileChecksumDataFormat {
    /// Data is a number
    Numerical = 0,
    /// Data is a bytes array
    ByteArray = 1,
}

/// Properties of a hash/checksum algorithm
#[derive(Debug, Deserialize)]
pub struct FileChecksumProperties {
    /// format that the hash/checksum returns
    pub format: FileChecksumDataFormat,
    /// size (in bytes) of output hash/checksum response
    pub size: u32,
}

/// [File Close](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-close) command
#[derive(Debug, Serialize)]
pub struct FileClose;
