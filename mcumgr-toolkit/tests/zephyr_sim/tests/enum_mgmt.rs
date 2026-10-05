//! Enumeration management group (group 10)

use mcumgr_toolkit::MCUmgrGroup;
use mcumgr_toolkit::commands::r#enum::GroupDetailsEntry;

use super::{device_error, group_error};
use crate::sim::enum_mgmt::ENUM_MGMT_ERR_INDEX_TOO_LARGE;
use crate::sim::smp::group_id;
use crate::sim::{Config, SimDevice};

/// Groups in the order Zephyr registers them (sorted by handler name)
const ALL_GROUPS: [u16; 8] = [10, 8, 1, 0, 3, 9, 2, 63];

#[test]
fn group_count_and_ids() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(client.enum_get_group_count().unwrap(), 8);
    assert_eq!(client.enum_get_group_ids().unwrap(), ALL_GROUPS);
    for (index, group) in ALL_GROUPS.iter().enumerate() {
        assert_eq!(client.enum_get_group_id(index as u16).unwrap(), *group);
    }

    let names: Vec<_> = ALL_GROUPS
        .iter()
        .map(|g| MCUmgrGroup::group_id_to_string(*g))
        .collect();
    assert_eq!(
        names,
        [
            "MGMT_GROUP_ID_ENUM",
            "MGMT_GROUP_ID_FS",
            "MGMT_GROUP_ID_IMAGE",
            "MGMT_GROUP_ID_OS",
            "MGMT_GROUP_ID_SETTINGS",
            "MGMT_GROUP_ID_SHELL",
            "MGMT_GROUP_ID_STAT",
            "ZEPHYR_MGMT_GRP_BASIC",
        ]
    );
}

#[test]
fn group_id_out_of_range() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client.enum_get_group_id(8).unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::ENUM, ENUM_MGMT_ERR_INDEX_TOO_LARGE)
    );
}

#[test]
fn iterate_group_ids() {
    let device = SimDevice::new(Config {
        mcumgr_grp_fs: false,
        mcumgr_grp_shell: false,
        ..Default::default()
    });
    let client = device.client();

    let ids: Vec<u16> = client
        .enum_iter_group_ids()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(ids, [10, 1, 0, 3, 2, 63]);

    // One request for the count, one per group
    assert_eq!(device.requests().len(), 7);
}

#[test]
fn iterate_group_ids_reports_errors() {
    let device = SimDevice::new(Config {
        mcumgr_grp_enum: false,
        ..Default::default()
    });
    let client = device.client();

    let results: Vec<_> = client.enum_iter_group_ids().collect();
    assert_eq!(results.len(), 1);
    assert!(results[0].as_ref().unwrap_err().command_not_supported());
}

#[test]
fn group_details() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let details = client.enum_get_group_details(None).unwrap();
    let expected = [
        (10, "enum mgmt", 4),
        (8, "fs mgmt", 5),
        (1, "img mgmt", 7),
        (0, "os mgmt", 9),
        (3, "settings mgmt", 4),
        (9, "shell mgmt", 1),
        (2, "stat mgmt", 2),
        (63, "zephyr basic mgmt", 1),
    ]
    .map(|(group, name, handlers)| GroupDetailsEntry {
        group,
        name: Some(name.into()),
        handlers: Some(handlers),
    });
    assert_eq!(details, expected);

    let details = client.enum_get_group_details(Some(&[63, 0, 42])).unwrap();
    assert_eq!(details, [expected[3].clone(), expected[7].clone()]);

    let request = device.requests_for(group_id::ENUM, 3).remove(1);
    let groups: Vec<_> = request
        .field("groups")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|g| u16::try_from(g.as_integer().unwrap()).unwrap())
        .collect();
    assert_eq!(groups, [63, 0, 42]);
}

#[test]
fn group_details_without_optional_fields() {
    let device = SimDevice::new(Config {
        enum_details_name: false,
        enum_details_handlers: false,
        ..Default::default()
    });
    let client = device.client();

    let details = client.enum_get_group_details(Some(&[1])).unwrap();
    assert_eq!(
        details,
        [GroupDetailsEntry {
            group: 1,
            name: None,
            handlers: None
        }]
    );
}

#[test]
fn group_details_not_enabled() {
    let device = SimDevice::new(Config {
        enum_details: false,
        ..Default::default()
    });
    let client = device.client();

    let err = client.enum_get_group_details(None).unwrap_err();
    assert!(err.command_not_supported());
    assert_eq!(client.enum_get_group_count().unwrap(), 8);
}
