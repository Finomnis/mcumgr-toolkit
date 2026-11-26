use miette::IntoDiagnostic;
use serde::{Deserialize, Serialize, ser::SerializeMap};

use crate::commands::{McuMgrCommand, max_data_chunk_size};

/// One chunk of an upload to a user-defined command
///
/// Works like [`ImageUpload`](super::image::ImageUpload): every chunk carries
/// the user-defined `fields`, `off` and `data`, the first one also `len`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawUpload<'a, 'b> {
    /// the group ID of the command
    pub(crate) group_id: u16,
    /// the command ID
    pub(crate) command_id: u8,
    /// user-defined map entries, sent with every chunk
    pub(crate) fields: &'a [(ciborium::Value, ciborium::Value)],
    /// offset to start/continue upload at
    pub(crate) off: u64,
    /// data to write at provided offset
    pub(crate) data: &'b [u8],
    /// length of the whole upload; only in the first chunk
    pub(crate) len: Option<u64>,
}

impl Serialize for RawUpload<'_, '_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let entries = self.fields.len() + 2 + usize::from(self.len.is_some());
        let mut map = serializer.serialize_map(Some(entries))?;
        for (key, value) in self.fields {
            map.serialize_entry(key, value)?;
        }
        map.serialize_entry("off", &self.off)?;
        map.serialize_entry("data", serde_bytes::Bytes::new(self.data))?;
        if let Some(len) = self.len {
            map.serialize_entry("len", &len)?;
        }
        map.end()
    }
}

/// Response for [`RawUpload`] command
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct RawUploadResponse {
    /// offset the device expects next
    pub(crate) off: u64,
}

impl McuMgrCommand for RawUpload<'_, '_> {
    type Payload = Self;
    type Response = RawUploadResponse;
    fn is_write_operation(&self) -> bool {
        true
    }
    fn group_id(&self) -> u16 {
        self.group_id
    }
    fn command_id(&self) -> u8 {
        self.command_id
    }
    fn data(&self) -> &Self {
        self
    }
}

/// Converts user-defined fields into the entries of a CBOR map.
///
/// Fails if `fields` does not serialize to a map, or if it contains
/// `off`, `data` or `len`, which are set by the upload itself.
pub(crate) fn raw_upload_fields(
    fields: &impl Serialize,
) -> miette::Result<Vec<(ciborium::Value, ciborium::Value)>> {
    let ciborium::Value::Map(entries) = ciborium::Value::serialized(fields).into_diagnostic()?
    else {
        miette::bail!("Raw upload fields must serialize to a map");
    };

    for (key, _) in &entries {
        if let Some(key @ ("off" | "data" | "len")) = key.as_text() {
            miette::bail!("Raw upload fields must not contain '{key}'");
        }
    }

    Ok(entries)
}

/// Computes how large [`RawUpload::data`] is allowed to be.
///
/// # Arguments
///
/// * `smp_frame_size`  - The max allowed size of an SMP frame.
/// * `fields`          - The user-defined map entries of every chunk.
/// * `first_chunk`     - Whether this is the first chunk, which also carries `len`.
///
pub(crate) fn raw_upload_max_data_chunk_size(
    smp_frame_size: usize,
    fields: &[(ciborium::Value, ciborium::Value)],
    first_chunk: bool,
) -> std::io::Result<usize> {
    max_data_chunk_size(
        smp_frame_size,
        &RawUpload {
            group_id: 0,
            command_id: 0,
            fields,
            off: u64::MAX,
            data: &[0u8],
            len: first_chunk.then_some(u64::MAX),
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn slot_fields() -> Vec<(ciborium::Value, ciborium::Value)> {
        raw_upload_fields(&BTreeMap::from([("slot", 1)])).unwrap()
    }

    #[test]
    fn raw_upload_fields_must_be_a_map() {
        assert_eq!(slot_fields(), vec![("slot".into(), 1.into())]);
        assert!(raw_upload_fields(&[1, 2]).is_err());
    }

    #[test]
    fn raw_upload_fields_must_not_contain_upload_keys() {
        for key in ["off", "data", "len"] {
            assert!(raw_upload_fields(&BTreeMap::from([(key, 1)])).is_err());
        }
    }

    #[test]
    fn raw_upload_encoding() {
        let fields = slot_fields();
        let request = RawUpload {
            group_id: 64,
            command_id: 0,
            fields: &fields,
            off: 0,
            data: &[1, 2],
            len: Some(2),
        };

        let mut encoded = vec![];
        ciborium::into_writer(&request, &mut encoded).unwrap();

        let expected_map = ciborium::Value::Map(vec![
            ("slot".into(), 1.into()),
            ("off".into(), 0.into()),
            ("data".into(), ciborium::Value::Bytes(vec![1, 2])),
            ("len".into(), 2.into()),
        ]);
        let mut expected = vec![];
        ciborium::into_writer(&expected_map, &mut expected).unwrap();

        assert_eq!(hex::encode(encoded), hex::encode(expected));
    }

    #[test]
    fn raw_upload_max_data_chunk_size_fits_frame() {
        let fields = slot_fields();
        for smp_frame_size in 64..5000 {
            for first_chunk in [true, false] {
                let max_data_size =
                    raw_upload_max_data_chunk_size(smp_frame_size, &fields, first_chunk).unwrap();

                let mut encoded = vec![];
                ciborium::into_writer(
                    &RawUpload {
                        group_id: 64,
                        command_id: 0,
                        fields: &fields,
                        off: u64::MAX,
                        data: &vec![0; max_data_size],
                        len: first_chunk.then_some(u64::MAX),
                    },
                    &mut encoded,
                )
                .unwrap();

                assert!(encoded.len() <= smp_frame_size - 8);
            }
        }
    }
}
