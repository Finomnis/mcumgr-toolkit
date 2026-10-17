//! Zephyr basic management group (group 63)

use super::{device_error, group_error};
use crate::sim::basic_mgmt::ZEPHYRBASIC_MGMT_ERR_FLASH_OPEN_FAILED;
use crate::sim::smp::group_id;
use crate::sim::{Config, SimDevice};

#[test]
fn erase_storage() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert!(
        device
            .lock()
            .storage_partition
            .as_ref()
            .unwrap()
            .iter()
            .all(|b| *b == 0x5a)
    );
    client.zephyr_erase_storage().unwrap();
    assert!(
        device
            .lock()
            .storage_partition
            .as_ref()
            .unwrap()
            .iter()
            .all(|b| *b == 0xff)
    );
}

#[test]
fn erase_storage_without_storage_partition() {
    let device = SimDevice::new(Config {
        storage_partition_size: None,
        ..Default::default()
    });
    let client = device.client();

    let err = client.zephyr_erase_storage().unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(
            group_id::ZEPHYR_BASIC,
            ZEPHYRBASIC_MGMT_ERR_FLASH_OPEN_FAILED
        )
    );
}
