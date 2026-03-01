use serde::{Deserialize, Serialize};

use super::macros::impl_serialize_as_empty_map;

/// [Count of supported groups](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_10.html#count-of-supported-groups-command) command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupCount;
impl_serialize_as_empty_map!(GroupCount);

/// Response for [`GroupCount`] command
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct GroupCountResponse {
    /// the total number of supported MCUmgr groups on the device
    pub count: u16,
}

/// [List supported groups](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_10.html#list-supported-groups-command) command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListGroups;
impl_serialize_as_empty_map!(ListGroups);

/// Response for [`ListGroups`] command
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ListGroupsResponse {
    /// list of the supported MCUmgr group IDs on the device
    pub groups: Vec<u16>,
}

/// [Fetch single group ID](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_10.html#fetch-single-group-id-command) command
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct GroupId {
    /// contains the (0-based) index of the group to return information on, can be omitted to return the first group’s details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u16>,
}

/// Response for [`GroupId`] command
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct GroupIdResponse {
    /// the group ID
    pub group: u16,
    /// true if the listed group is the final supported group on the device
    #[serde(default)]
    pub end: bool,
}

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use ciborium::cbor;

    command_encode_decode_test! {
        group_count,
        (0, 10, 0),
        GroupCount,
        cbor!({}),
        cbor!({
            "count" => 42,
        }),
        GroupCountResponse{
            count: 42,
        },
    }

    command_encode_decode_test! {
        list_groups,
        (0, 10, 1),
        ListGroups,
        cbor!({}),
        cbor!({"groups" => [5,69,65535]}),
        ListGroupsResponse{ groups: vec![5,69,u16::MAX] },
    }

    command_encode_decode_test! {
        get_group_id_empty,
        (0, 10, 2),
        GroupId{index: None},
        cbor!({}),
        cbor!({
            "group" => 42,
        }),
        GroupIdResponse{
            group: 42,
            end: false,
        },
    }

    command_encode_decode_test! {
        get_group_id_full,
        (0, 10, 2),
        GroupId{index: Some(69)},
        cbor!({
            "index" => 69,
        }),
        cbor!({
            "group" => 65535,
            "end" => true,
        }),
        GroupIdResponse{
            group: u16::MAX,
            end: true,
        },
    }
}
