use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use strum::Display;

use crate::commands::{
    CountingWriter, data_too_large_error,
    macros::{impl_deserialize_from_empty_map_and_into_unit, impl_serialize_as_empty_map},
};

use super::is_default;

/// [File Download](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-download) command
#[derive(Debug, Serialize, Eq, PartialEq)]
pub struct FileDownload<'a> {
    /// offset to start download at
    pub off: u64,
    /// absolute path to a file
    pub name: &'a str,
}

/// Response for [`FileDownload`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
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
/// # Arguments
///
/// * `smp_frame_size`  - The max allowed size of an SMP frame.
/// * `filename`        - The filename we transfer to.
pub fn file_upload_max_data_chunk_size(
    smp_frame_size: usize,
    filename: &str,
) -> std::io::Result<usize> {
    const MGMT_HDR_SIZE: usize = 8; // Size of SMP header

    let mut size_counter = CountingWriter::new();
    ciborium::into_writer(
        &FileUpload {
            off: u64::MAX,
            name: filename,
            data: &[0u8],
            len: Some(u64::MAX),
        },
        &mut size_counter,
    )
    .map_err(|_| data_too_large_error())?;

    let size_with_one_byte = size_counter.bytes_written;
    let size_without_data = size_with_one_byte - 1;

    let estimated_data_size = smp_frame_size
        .checked_sub(MGMT_HDR_SIZE)
        .ok_or_else(data_too_large_error)?
        .checked_sub(size_without_data)
        .ok_or_else(data_too_large_error)?;

    let data_length_bytes = if estimated_data_size == 0 {
        return Err(data_too_large_error());
    } else if estimated_data_size <= u8::MAX as usize {
        1
    } else if estimated_data_size <= u16::MAX as usize {
        2
    } else if estimated_data_size <= u32::MAX as usize {
        4
    } else {
        8
    };

    // Remove data length entry from estimated data size
    let actual_data_size = estimated_data_size
        .checked_sub(data_length_bytes as usize)
        .ok_or_else(data_too_large_error)?;

    if actual_data_size == 0 {
        return Err(data_too_large_error());
    }

    Ok(actual_data_size)
}

/// [File Upload](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-upload) command
#[derive(Debug, Serialize, Eq, PartialEq)]
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
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct FileUploadResponse {
    /// offset of last successfully written data
    pub off: u64,
}

/// [File Status](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-status) command
#[derive(Debug, Serialize, Eq, PartialEq)]
pub struct FileStatus<'a> {
    /// absolute path to a file
    pub name: &'a str,
}

/// Response for [`FileStatus`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct FileStatusResponse {
    /// length of file (in bytes)
    pub len: u64,
}

/// [File Hash/Checksum](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-hash-checksum) command
#[derive(Debug, Serialize, Eq, PartialEq)]
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
#[derive(Debug, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Deserialize, Eq, PartialEq)]
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
            FileChecksumData::Hash(data) => hex::encode(data),
            FileChecksumData::Checksum(value) => format!("{value:08x}"),
        }
    }
}

/// [Supported file hash/checksum types](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#supported-file-hash-checksum-types) command
#[derive(Debug, Eq, PartialEq)]
pub struct SupportedFileChecksumTypes;
impl_serialize_as_empty_map!(SupportedFileChecksumTypes);

/// Response for [`SupportedFileChecksumTypes`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
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
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct FileChecksumProperties {
    /// format that the hash/checksum returns
    pub format: FileChecksumDataFormat,
    /// size (in bytes) of output hash/checksum response
    pub size: u32,
}

/// [File Close](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html#file-close) command
#[derive(Debug, Eq, PartialEq)]
pub struct FileClose;
impl_serialize_as_empty_map!(FileClose);

/// Response for [`FileClose`] command
#[derive(Default, Debug, Eq, PartialEq)]
pub struct FileCloseResponse;
impl_deserialize_from_empty_map_and_into_unit!(FileCloseResponse);

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use ciborium::cbor;

    #[test]
    fn file_upload_max_data_chunk_size() {
        for smp_frame_size in 57..100000 {
            let smp_payload_size = smp_frame_size - 8 /* SMP frame header */;

            let filename = "test.txt";
            let max_data_size =
                super::file_upload_max_data_chunk_size(smp_frame_size, filename).unwrap();

            let cmd = FileUpload {
                off: u64::MAX,
                data: &vec![0; max_data_size],
                name: filename,
                len: Some(u64::MAX),
            };

            let mut cbor_data = vec![];
            ciborium::into_writer(&cmd, &mut cbor_data).unwrap();

            assert!(
                smp_payload_size - 2 <= cbor_data.len() && cbor_data.len() <= smp_payload_size,
                "Failed at frame size {}: actual={}, max={}",
                smp_frame_size,
                cbor_data.len(),
                smp_payload_size,
            );
        }
    }

    #[test]
    fn file_upload_max_data_chunk_size_too_small() {
        for smp_frame_size in 0..57 {
            let filename = "test.txt";
            let max_data_size = super::file_upload_max_data_chunk_size(smp_frame_size, filename);

            assert!(max_data_size.is_err());
        }
    }

    command_encode_decode_test! {
        file_download_with_len,
        (0, 8, 0),
        FileDownload{
            off: 42,
            name: "foo.txt",
        },
        cbor!({
            "off" => 42,
            "name" => "foo.txt",
        }),
        cbor!({
            "off" => 42,
            "data" => ciborium::Value::Bytes(vec![1,2,3,4,5]),
            "len" => 100,
        }),
        FileDownloadResponse{
            off: 42,
            data: vec![1,2,3,4,5],
            len: Some(100),
        },
    }

    command_encode_decode_test! {
        file_download_without_len,
        (0, 8, 0),
        FileDownload{
            off: 69,
            name: "bla.txt",
        },
        cbor!({
            "off" => 69,
            "name" => "bla.txt",
        }),
        cbor!({
            "off" => 50,
            "data" => ciborium::Value::Bytes(vec![10]),
        }),
        FileDownloadResponse{
            off: 50,
            data: vec![10],
            len: None,
        },
    }

    command_encode_decode_test! {
        file_upload_with_len,
        (2, 8, 0),
        FileUpload{off: 0, data: &[1,2,3,4,5], name: "foo.bar", len: Some(123)},
        cbor!({
            "off" => 0,
            "data" => ciborium::Value::Bytes(vec![1,2,3,4,5]),
            "name" => "foo.bar",
            "len" => 123,
        }),
        cbor!({
            "off" => 58,
        }),
        FileUploadResponse{
            off: 58
        }
    }

    command_encode_decode_test! {
        file_upload_without_len,
        (2, 8, 0),
        FileUpload{off: 10, data: &[40], name: "a.xy", len: None},
        cbor!({
            "off" => 10,
            "data" => ciborium::Value::Bytes(vec![40]),
            "name" => "a.xy",
        }),
        cbor!({
            "off" => 0,
        }),
        FileUploadResponse{
            off: 0
        }
    }

    command_encode_decode_test! {
        file_status,
        (0, 8, 1),
        FileStatus{name: "a.xy"},
        cbor!({
            "name" => "a.xy",
        }),
        cbor!({
            "len" => 123,
        }),
        FileStatusResponse{
            len: 123,
        }
    }

    command_encode_decode_test! {
        file_checksum_full_with_checksum,
        (0, 8, 2),
        FileChecksum{
            name: "file.txt",
            r#type: Some("sha256"),
            off: 42,
            len: Some(16),
        },
        cbor!({
            "name" => "file.txt",
            "type" => "sha256",
            "off"  => 42,
            "len"  => 16,
        }),
        cbor!({
            "type"   => "foo",
            "off"    => 69,
            "len"    => 42,
            "output" => 100000,
        }),
        FileChecksumResponse{
            r#type: "foo".to_string(),
            off: 69,
            len: 42,
            output: FileChecksumData::Checksum(100000),
        }
    }

    command_encode_decode_test! {
        file_checksum_empty_with_hash,
        (0, 8, 2),
        FileChecksum{
            name: "file.txt",
            r#type: None,
            off: 0,
            len: None,
        },
        cbor!({
            "name" => "file.txt",
        }),
        cbor!({
            "type"   => "foo",
            "len"    => 42,
            "output" => ciborium::Value::Bytes(vec![1,2,3,4]),
        }),
        FileChecksumResponse{
            r#type: "foo".to_string(),
            off: 0,
            len: 42,
            output: FileChecksumData::Hash(vec![1,2,3,4].into_boxed_slice()),
        }
    }

    command_encode_decode_test! {
        supported_checksum_types,
        (0, 8, 3),
        SupportedFileChecksumTypes,
        cbor!({}),
        cbor!({
            "types" => {
                "sha256" => {
                    "format" => 1,
                    "size" => 32,
                },
                "crc32" => {
                    "format" => 0,
                    "size" => 4
                },
            },
        }),
        SupportedFileChecksumTypesResponse{
            types: HashMap::from([
                (
                    "crc32".to_string(),
                    FileChecksumProperties{
                        format: FileChecksumDataFormat::Numerical,
                        size: 4,
                    }
                ),
                (
                    "sha256".to_string(),
                    FileChecksumProperties{
                        format: FileChecksumDataFormat::ByteArray,
                        size: 32,
                    }
                ),
            ])
        }
    }

    command_encode_decode_test! {
        file_close,
        (2, 8, 4),
        FileClose,
        cbor!({}),
        cbor!({}),
        FileCloseResponse,
    }
}
