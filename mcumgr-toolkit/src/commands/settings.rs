use serde::{Deserialize, Serialize};

use crate::commands::macros::{
    impl_deserialize_from_empty_map_and_into_unit, impl_serialize_as_empty_map,
};

/// [Read Setting](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_3.html#read-setting-request) command
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct ReadSetting<'a> {
    /// string of the setting to retrieve
    pub name: &'a str,
    /// optional maximum size of data to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u32>,
}

/// Response for [`ReadSetting`] command
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ReadSettingResponse {
    /// The returned data.
    ///
    /// Note that the underlying data type cannot be specified through this and must be known by the client.
    pub val: Vec<u8>,
    /// Will be set if the maximum supported data size is smaller than the
    /// maximum requested data size, and contains the maximum data size
    /// which the device supports
    pub max_size: Option<u32>,
}

/// [Write Setting](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_3.html#write-setting-request) command
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct WriteSetting<'a, 'b> {
    /// string of the setting to update/set
    pub name: &'a str,
    /// value to set the setting to
    #[serde(with = "serde_bytes")]
    pub val: &'b [u8],
}

/// Response for [`WriteSetting`] command
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct WriteSettingResponse;
impl_deserialize_from_empty_map_and_into_unit!(WriteSettingResponse);

/// [Delete Setting](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_3.html#delete-setting-command) command
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct DeleteSetting<'a> {
    /// string of the setting to delete
    pub name: &'a str,
}

/// Response for [`DeleteSetting`] command
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct DeleteSettingResponse;
impl_deserialize_from_empty_map_and_into_unit!(DeleteSettingResponse);

/// [Commit settings](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_3.html#commit-settings-command) command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitSettings;
impl_serialize_as_empty_map!(CommitSettings);

/// Response for [`CommitSettings`] command
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct CommitSettingsResponse;
impl_deserialize_from_empty_map_and_into_unit!(CommitSettingsResponse);

/// [Load settings](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_3.html#load-settings-request) command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadSettings;
impl_serialize_as_empty_map!(LoadSettings);

/// Response for [`LoadSettings`] command
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct LoadSettingsResponse;
impl_deserialize_from_empty_map_and_into_unit!(LoadSettingsResponse);

/// [Save Settings](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_3.html#save-settings-request) command
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct SaveSettings<'a> {
    /// if provided, contains the settings subtree name to save, if not then will save all settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
}

/// Response for [`SaveSettings`] command
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct SaveSettingsResponse;
impl_deserialize_from_empty_map_and_into_unit!(SaveSettingsResponse);

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use ciborium::cbor;

    command_encode_decode_test! {
        read_setting,
        (0, 3, 0),
        ReadSetting {
            name: "foo/bar",
            max_size: None
        },
        cbor!({
            "name" => "foo/bar",
        }),
        cbor!({
            "val" => ciborium::Value::Bytes(vec![1,2,3,4,5]),
        }),
        ReadSettingResponse{
            val: vec![1,2,3,4,5],
            max_size: None,
        },
    }
    command_encode_decode_test! {
        read_setting_with_max_size,
        (0, 3, 0),
        ReadSetting {
            name: "foo/bar",
            max_size: Some(42),
        },
        cbor!({
            "name" => "foo/bar",
            "max_size" => 42,
        }),
        cbor!({
            "val" => ciborium::Value::Bytes(vec![1,2,3,4,5]),
            "max_size" => 36,
        }),
        ReadSettingResponse{
            val: vec![1,2,3,4,5],
            max_size: Some(36),
        },
    }

    command_encode_decode_test! {
        write_setting,
        (2, 3, 0),
        WriteSetting {
            name: "foo",
            val: &[1,2,3,4,5],
        },
        cbor!({
            "name" => "foo",
            "val" => ciborium::Value::Bytes(vec![1,2,3,4,5]),
        }),
        cbor!({}),
        WriteSettingResponse,
    }

    command_encode_decode_test! {
        delete_setting,
        (2, 3, 1),
        DeleteSetting {
            name: "bar",
        },
        cbor!({
            "name" => "bar",
        }),
        cbor!({}),
        DeleteSettingResponse,
    }

    command_encode_decode_test! {
        commit_settings,
        (2, 3, 2),
        CommitSettings,
        cbor!({}),
        cbor!({}),
        CommitSettingsResponse,
    }

    command_encode_decode_test! {
        load_settings,
        (0, 3, 3),
        LoadSettings,
        cbor!({}),
        cbor!({}),
        LoadSettingsResponse,
    }

    command_encode_decode_test! {
        save_settings,
        (2, 3, 3),
        SaveSettings{
            name: None,
        },
        cbor!({}),
        cbor!({}),
        SaveSettingsResponse,
    }

    command_encode_decode_test! {
        save_settings_with_name,
        (2, 3, 3),
        SaveSettings{
            name: Some("foo"),
        },
        cbor!({
            "name" => "foo",
        }),
        cbor!({}),
        SaveSettingsResponse,
    }
}
