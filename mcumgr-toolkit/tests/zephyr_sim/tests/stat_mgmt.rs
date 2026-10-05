//! Statistics management group (group 2)

use std::collections::HashMap;

use super::{device_error, group_error, smp_error};
use crate::sim::SimDevice;
use crate::sim::smp::{group_id, mgmt_err};
use crate::sim::stat_mgmt::{STAT_MGMT_ERR_INVALID_STAT_NAME, StatGroup};

#[test]
fn list_groups() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(
        client.stats_list_groups().unwrap(),
        ["smp_svr_stats", "ble_ll"]
    );

    device.lock().stat.groups.clear();
    assert!(client.stats_list_groups().unwrap().is_empty());
}

#[test]
fn group_data() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(
        client.stats_get_group_data("ble_ll").unwrap(),
        HashMap::from([
            ("rx_pdu".to_string(), 17),
            ("tx_pdu".to_string(), 19),
            ("crc_err".to_string(), 0),
        ])
    );

    device.lock().stat.groups.push(StatGroup {
        name: "empty".into(),
        fields: vec![],
    });
    assert!(client.stats_get_group_data("empty").unwrap().is_empty());

    device.lock().stat.groups[0].fields[0].1 = u32::MAX.into();
    assert_eq!(
        client.stats_get_group_data("smp_svr_stats").unwrap()["ticks"],
        u64::from(u32::MAX)
    );
}

#[test]
fn group_data_errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client.stats_get_group_data("nonexistent").unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::STAT, STAT_MGMT_ERR_INVALID_STAT_NAME)
    );

    // Names must fit into CONFIG_MCUMGR_GRP_STAT_MAX_NAME_LEN including the
    // terminating NUL character
    let err = client.stats_get_group_data("x".repeat(32)).unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));

    let err = client.stats_get_group_data("").unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));
}

#[test]
fn group_name_length_limit() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // 31 bytes is the longest name that is looked up
    let name = "g".repeat(31);
    device.lock().stat.groups.push(StatGroup {
        name: name.clone(),
        fields: vec![("f".into(), 1)],
    });
    assert_eq!(client.stats_get_group_data(&name).unwrap()["f"], 1);
}
