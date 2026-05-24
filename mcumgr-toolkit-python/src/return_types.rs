use std::collections::{HashMap, HashSet};

use pyo3::{PyClass, prelude::*, types::PyBytes};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};

use ::mcumgr_toolkit::commands;
use serde::{Serialize, ser::SerializeSeq};

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
pub struct MCUmgrParameters {
    /// Single SMP buffer size, this includes SMP header and CBOR payload
    #[pyo3(get)]
    pub buf_size: u32,
    /// Number of SMP buffers supported
    #[pyo3(get)]
    pub buf_count: u32,
}
generate_repr_from_serialize!(MCUmgrParameters);
impl From<commands::os::MCUmgrParametersResponse> for MCUmgrParameters {
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

/// Return value of `MCUmgrClient.settings_read_ext`.
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct SettingData {
    /// The returned data.
    ///
    /// Note that the underlying data type cannot be specified through this and
    /// must be known and parsed by the client.
    #[pyo3(get)]
    #[serde(serialize_with = "crate::repr_macro::serialize_pybytes_as_hex")]
    pub value: Py<PyBytes>,
    /// Will be set if the maximum supported data size is smaller than the
    /// maximum requested data size, and contains the maximum data size
    /// which the device supports
    #[pyo3(get)]
    pub max_size: Option<u32>,
}
generate_repr_from_serialize!(SettingData);

impl SettingData {
    pub(crate) fn from_response<'py>(
        py: Python<'py>,
        response: commands::settings::ReadSettingResponse,
    ) -> Self {
        Self {
            value: PyBytes::new(py, &response.val).unbind(),
            max_size: response.max_size,
        }
    }
}

/// Data format of the hash/checksum type
#[gen_stub_pyclass_enum]
#[pyclass(frozen, eq, eq_int, hash, skip_from_py_object)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Serialize)]
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

/// Statistics of an MCU memory pool
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct MemoryPoolStatistics {
    /// size of a memory block in the pool
    #[pyo3(get)]
    pub blksiz: u64,
    /// number of blocks in the pool
    #[pyo3(get)]
    pub nblks: u64,
    /// number of free blocks
    #[pyo3(get)]
    pub nfree: u64,
    /// lowest number of free blocks the pool reached during run-time
    #[pyo3(get)]
    pub min: u64,
}
generate_repr_from_serialize!(MemoryPoolStatistics);

impl From<commands::os::MemoryPoolStatisticsEntry> for MemoryPoolStatistics {
    fn from(value: commands::os::MemoryPoolStatisticsEntry) -> Self {
        Self {
            blksiz: value.blksiz,
            nblks: value.nblks,
            nfree: value.nfree,
            min: value.min,
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
    pub image: u32,
    /// slot number within “image”
    #[pyo3(get)]
    pub slot: u32,
    /// string representing image version, as set with `imgtool`
    #[pyo3(get)]
    pub version: String,
    /// Hash of the image header and body
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

/// Details about an MCUmgr group
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct GroupDetails {
    /// the group ID of the MCUmgr command group
    #[pyo3(get)]
    pub group: u16,
    /// the name of the MCUmgr command group
    #[pyo3(get)]
    pub name: Option<String>,
    /// the number of handlers that the MCUmgr command group supports
    #[pyo3(get)]
    pub handlers: Option<u8>,
}
generate_repr_from_serialize!(GroupDetails);

impl GroupDetails {
    pub(crate) fn from_response(value: commands::r#enum::GroupDetailsEntry) -> Self {
        Self {
            group: value.group,
            name: value.name,
            handlers: value.handlers,
        }
    }
}

fn serialize_pyvec<S, T>(slots: &[Py<T>], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: PyClass + serde::Serialize,
{
    Python::attach(|py| {
        let mut seq = serializer.serialize_seq(Some(slots.len()))?;
        for obj in slots {
            let cell = obj.borrow(py);
            seq.serialize_element(&*cell)?;
        }
        seq.end()
    })
}

/// Information about a firmware image type returned by `MCUmgrClient.image_slot_info`
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct SlotInfoImage {
    /// number of the image
    #[pyo3(get)]
    pub image: u32,
    /// slots available for the image
    #[pyo3(get)]
    #[serde(serialize_with = "serialize_pyvec")]
    pub slots: Vec<Py<SlotInfoImageSlot>>,
    /// maximum size of an application that can be uploaded to that image number
    #[pyo3(get)]
    pub max_image_size: Option<u64>,
}
generate_repr_from_serialize!(SlotInfoImage);

/// Information about a slot that can hold a firmware image
#[gen_stub_pyclass]
#[pyclass(frozen)]
#[derive(Serialize)]
pub struct SlotInfoImageSlot {
    /// slot inside the image being enumerated
    #[pyo3(get)]
    pub slot: u32,
    /// size of the slot
    #[pyo3(get)]
    pub size: u64,
    /// specifies the image ID that can be used by external tools to upload an image to that slot
    #[pyo3(get)]
    pub upload_image_id: Option<u32>,
}
generate_repr_from_serialize!(SlotInfoImageSlot);

impl SlotInfoImage {
    pub(crate) fn from_response<'py>(
        py: Python<'py>,
        value: commands::image::SlotInfoImage,
    ) -> PyResult<Self> {
        Ok(Self {
            image: value.image,
            slots: value
                .slots
                .into_iter()
                .map(|slot| {
                    Py::new(
                        py,
                        SlotInfoImageSlot {
                            slot: slot.slot,
                            size: slot.size,
                            upload_image_id: slot.upload_image_id,
                        },
                    )
                })
                .collect::<PyResult<_>>()?,
            max_image_size: value.max_image_size,
        })
    }
}

/// An iterator over enum group IDs.
///
/// Returned from `MCUmgrClient::enum_iter_group_ids`.
#[pyclass]
pub(crate) struct GroupIdIter {
    client: Py<crate::MCUmgrClient>,
    next_index: u16,
    num_elements: Option<u16>,
}

impl GroupIdIter {
    pub(crate) fn new(client: Py<crate::MCUmgrClient>) -> Self {
        Self {
            client,
            next_index: 0,
            num_elements: None,
        }
    }

    fn get_num_elements(slf: &mut PyRefMut<'_, Self>) -> PyResult<u16> {
        match slf.num_elements {
            Some(num_elements) => Ok(num_elements),
            None => {
                let res = slf.client.bind(slf.py()).get().enum_get_group_count();

                slf.num_elements = Some(*res.as_ref().unwrap_or(&0));

                res
            }
        }
    }
}

#[pymethods]
impl GroupIdIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(mut slf: PyRefMut<'_, Self>) -> PyResult<Option<u16>> {
        let num_elements = GroupIdIter::get_num_elements(&mut slf)?;

        if slf.next_index >= num_elements {
            return Ok(None);
        }

        match slf
            .client
            .bind(slf.py())
            .get()
            .enum_get_group_id(slf.next_index)
        {
            Ok(group_id) => {
                slf.next_index += 1;
                Ok(Some(group_id))
            }
            Err(e) => {
                slf.next_index = num_elements;
                Err(e)
            }
        }
    }
}

impl pyo3_stub_gen::PyStubType for GroupIdIter {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        pyo3_stub_gen::TypeInfo {
            name: "collections.abc.Iterator[builtins.int]".to_string(),
            source_module: None,
            import: HashSet::from([
                pyo3_stub_gen::ImportRef::Module(pyo3_stub_gen::ModuleRef::from("collections.abc")),
                pyo3_stub_gen::ImportRef::Module(pyo3_stub_gen::ModuleRef::from("builtins")),
            ]),
            type_refs: HashMap::new(),
        }
    }
}
