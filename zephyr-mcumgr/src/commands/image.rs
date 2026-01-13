use serde::{Deserialize, Serialize};

use crate::commands::macros::impl_serialize_as_empty_map;

fn serialize_option_hex<S, T>(data: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: hex::ToHex,
{
    data.as_ref()
        .map(|val| val.encode_hex::<String>())
        .serialize(serializer)
}

/// The state of an image slot
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ImageState {
    /// image number
    #[serde(default)]
    pub image: u64,
    /// slot number within “image”
    pub slot: u64,
    /// string representing image version, as set with `imgtool`
    pub version: String,
    /// SHA256 hash of the image header and body
    ///
    /// Note that this will not be the same as the SHA256 of the whole file, it is the field in the
    /// MCUboot TLV section that contains a hash of the data which is used for signature
    /// verification purposes.
    #[serde(serialize_with = "serialize_option_hex")] // For JSON (cli)
    pub hash: Option<[u8; 32]>,
    /// true if image has bootable flag set
    #[serde(default)]
    pub bootable: bool,
    /// true if image is set for next swap
    #[serde(default)]
    pub pending: bool,
    /// true if image has been confirmed
    #[serde(default)]
    pub confirmed: bool,
    /// true if image is currently active application
    #[serde(default)]
    pub active: bool,
    /// true if image is to stay in primary slot after the next boot
    #[serde(default)]
    pub permanent: bool,
}

/// [Get Image State](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_1.html#get-state-of-images-request) command
#[derive(Debug, Eq, PartialEq)]
pub struct GetImageState;
impl_serialize_as_empty_map!(GetImageState);

/// Response for [`GetImageState`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct GetImageStateResponse {
    /// List of all images and their state
    pub images: Vec<ImageState>,
    // splitStatus field is missing
    // because it is unused by Zephyr
}

/// [Slot Info](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_1.html#slot-info) command
#[derive(Debug, Eq, PartialEq)]
pub struct SlotInfo;
impl_serialize_as_empty_map!(SlotInfo);

/// Information about a firmware image type returned by [`SlotInfo`]
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct SlotInfoImage {
    /// The number of the image
    pub image: u32,
    /// Slots available for the image
    pub slots: Vec<SlotInfoImageSlot>,
    /// Maximum size of an application that can be uploaded to that image number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_image_size: Option<u64>,
}

/// Information about a slot that can hold a firmware image
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct SlotInfoImageSlot {
    /// The slot inside the image being enumerated
    pub slot: u32,
    /// The size of the slot
    pub size: u64,
    /// Specifies the image ID that can be used by external tools to upload an image to that slot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_image_id: Option<u32>,
}

/// Response for [`SlotInfo`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct SlotInfoResponse {
    /// List of all image slot collections on the device
    pub images: Vec<SlotInfoImage>,
}

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use ciborium::cbor;

    command_encode_decode_test! {
        get_image_state,
        (0, 1, 0),
        GetImageState,
        cbor!({}),
        cbor!({
            "images" => [
                {
                    "image" => 3,
                    "slot" => 5,
                    "version" => "v1.2.3",
                    "hash" => ciborium::Value::Bytes(vec![1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32]),
                    "bootable" => true,
                    "pending" => true,
                    "confirmed" => true,
                    "active" => true,
                    "permanent" => true,
                },
                {
                    "image" => 4,
                    "slot" => 6,
                    "version" => "v5.5.5",
                    "bootable" => false,
                    "pending" => false,
                    "confirmed" => false,
                    "active" => false,
                    "permanent" => false,
                },
                {
                    "slot" => 9,
                    "version" => "8.6.4",
                },
            ],
            "splitStatus" => 42,
        }),
        GetImageStateResponse{
            images: vec![
                ImageState{
                    image: 3,
                    slot: 5,
                    version: "v1.2.3".to_string(),
                    hash: Some([1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32]),
                    bootable: true,
                    pending: true,
                    confirmed: true,
                    active: true,
                    permanent: true,
                },
                ImageState{
                    image: 4,
                    slot: 6,
                    version: "v5.5.5".to_string(),
                    hash: None,
                    bootable: false,
                    pending: false,
                    confirmed: false,
                    active: false,
                    permanent: false,
                },
                ImageState{
                    image: 0,
                    slot: 9,
                    version: "8.6.4".to_string(),
                    hash: None,
                    bootable: false,
                    pending: false,
                    confirmed: false,
                    active: false,
                    permanent: false,
                }
            ],
        },
    }

    command_encode_decode_test! {
        slot_info,
        (0, 1, 6),
        SlotInfo,
        cbor!({}),
        cbor!({
            "images" => [
                {
                    "image" => 0,
                    "slots" => [
                        {
                            "slot" => 0,
                            "size" => 42,
                            "upload_image_id" => 2,
                        },
                        {
                            "slot" => 1,
                            "size" => 123456789012u64,
                        },
                    ],
                    "max_image_size" => 123456789987u64,
                },
                {
                    "image" => 1,
                    "slots" => [
                    ],
                },
            ],
        }),
        SlotInfoResponse{
            images: vec![
                SlotInfoImage {
                    image: 0,
                    slots: vec![
                        SlotInfoImageSlot {
                            slot: 0,
                            size: 42,
                            upload_image_id: Some(2),
                        },
                        SlotInfoImageSlot {
                            slot: 1,
                            size: 123456789012,
                            upload_image_id: None,
                        }
                    ],
                    max_image_size: Some(123456789987)
                },
                SlotInfoImage {
                    image: 1,
                    slots: vec![],
                    max_image_size: None,
                }
            ],
        },
    }
}
