use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::macros::impl_serialize_as_empty_map;

/// [Statistics: group data](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_2.html#statistics-group-data) command
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct GroupData<'a> {
    /// group name
    pub name: &'a str,
}

/// Response for [`GroupData`] command
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct GroupDataResponse {
    /// name of group the response contains data for
    pub name: String,
    /// map of entries within given group
    pub fields: HashMap<String, u64>,
}

/// [Statistics: list of groups](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_2.html#statistics-list-of-groups) command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListGroups;
impl_serialize_as_empty_map!(ListGroups);

/// Response for [`ListGroups`] command
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ListGroupsResponse {
    /// array of strings representing group names
    pub stat_list: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use ciborium::cbor;

    command_encode_decode_test! {
        group_data,
        (0, 2, 0),
        GroupData{name: "foo"},
        cbor!({"name" => "foo"}),
        cbor!({
            "name" => "bar",
            "fields" => {
                "abc" => 10,
                "foo50" => 0xFFFFFFFFFFFFFFFFu64,
            }
        }),
        GroupDataResponse{
            name: "bar".to_string(),
            fields: HashMap::from([
                ("abc".to_string(), 10),
                ("foo50".to_string(), u64::MAX),
            ])
        },
    }

    command_encode_decode_test! {
        list_groups,
        (0, 2, 1),
        ListGroups,
        cbor!({}),
        cbor!({"stat_list" => ["foo", "bar"]}),
        ListGroupsResponse{ stat_list: vec!["foo".to_string(), "bar".to_string()] },
    }
}
