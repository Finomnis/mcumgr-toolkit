use crate::commands::macros::{
    impl_deserialize_from_empty_map_and_into_unit, impl_serialize_as_empty_map,
};

/// [Erase Storage](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_63.html#erase-storage-command) command
#[derive(Debug, Eq, PartialEq)]
pub struct EraseStorage;
impl_serialize_as_empty_map!(EraseStorage);

/// Response for [`EraseStorage`] command
#[derive(Default, Debug, Eq, PartialEq)]
pub struct EraseStorageResponse;
impl_deserialize_from_empty_map_and_into_unit!(EraseStorageResponse);

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use ciborium::cbor;

    command_encode_decode_test! {
        erase_storage,
        (2, 63, 0),
        EraseStorage,
        cbor!({}),
        cbor!({}),
        EraseStorageResponse,
    }
}
