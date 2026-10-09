//! Settings (config) management group (group 3)

use super::{device_error, group_error, smp_error};
use crate::sim::SimDevice;
use crate::sim::settings_mgmt::settings_mgmt_err;
use crate::sim::smp::{group_id, mgmt_err};

#[track_caller]
fn assert_settings_error(err: mcumgr_toolkit::client::MCUmgrClientError, rc: u16) {
    assert_eq!(device_error(err), group_error(group_id::SETTINGS, rc));
}

#[test]
fn read_and_write() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(client.settings_read("app/name").unwrap(), b"zephyr-sim");
    assert_eq!(
        client.settings_read("app/counter").unwrap(),
        7u32.to_le_bytes()
    );

    client
        .settings_write("app/counter", &1234u32.to_le_bytes())
        .unwrap();
    assert_eq!(
        client.settings_read("app/counter").unwrap(),
        1234u32.to_le_bytes()
    );

    client.settings_write("app/name", b"").unwrap();
    assert_eq!(client.settings_read("app/name").unwrap(), b"");

    let request = device.requests_for(group_id::SETTINGS, 0).remove(2);
    assert_eq!(
        request.field("name").unwrap().as_text(),
        Some("app/counter")
    );
    assert_eq!(
        request.field("val").unwrap().as_bytes(),
        Some(&1234u32.to_le_bytes().to_vec())
    );
}

#[test]
fn read_with_size_limit() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let response = client.settings_read_ext("app/name", Some(6)).unwrap();
    assert_eq!(response.val, b"zephyr");
    assert_eq!(response.max_size, None);

    // Values are truncated to CONFIG_MCUMGR_GRP_SETTINGS_VALUE_LEN, which the
    // device reports if more was requested
    let response = client.settings_read_ext("app/blob", Some(1000)).unwrap();
    assert_eq!(response.val, [0; 32]);
    assert_eq!(response.max_size, Some(32));

    let response = client.settings_read_ext("app/blob", None).unwrap();
    assert_eq!(response.val, [0; 32]);
    assert_eq!(response.max_size, None);
}

#[test]
fn errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client.settings_read("nothing/here").unwrap_err();
    assert_settings_error(err, settings_mgmt_err::ROOT_KEY_NOT_FOUND);

    let err = client.settings_read("app/missing").unwrap_err();
    assert_settings_error(err, settings_mgmt_err::KEY_NOT_FOUND);

    let err = client.settings_write("app/missing", b"1").unwrap_err();
    assert_settings_error(err, settings_mgmt_err::KEY_NOT_FOUND);

    let err = client.settings_write("factory/serial", b"1").unwrap_err();
    assert_settings_error(err, settings_mgmt_err::WRITE_NOT_SUPPORTED);

    let err = client.settings_read("secret/key").unwrap_err();
    assert_settings_error(err, settings_mgmt_err::READ_NOT_SUPPORTED);

    let long_name = format!("app/{}", "x".repeat(28));
    let err = client.settings_read(&long_name).unwrap_err();
    assert_settings_error(err, settings_mgmt_err::KEY_TOO_LONG);
    let err = client.settings_write(&long_name, b"").unwrap_err();
    assert_settings_error(err, settings_mgmt_err::KEY_TOO_LONG);
    let err = client.settings_delete(&long_name).unwrap_err();
    assert_settings_error(err, settings_mgmt_err::KEY_TOO_LONG);
    let err = client.settings_save(Some(&long_name)).unwrap_err();
    assert_settings_error(err, settings_mgmt_err::KEY_TOO_LONG);

    let err = client.settings_read("").unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));
    let err = client.settings_save(Some("")).unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));

    // The secret can be written, though
    client.settings_write("secret/key", b"hunter2").unwrap();
}

#[test]
fn save_load_and_persistence_across_reboots() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client.settings_write("app/name", b"persisted").unwrap();
    client.settings_save(None::<&str>).unwrap();
    assert_eq!(
        device.lock().settings.storage.get("app/name"),
        Some(&b"persisted".to_vec())
    );

    client.settings_write("app/name", b"volatile").unwrap();
    client.os_system_reset(false, None).unwrap();
    assert_eq!(client.settings_read("app/name").unwrap(), b"persisted");

    // Load restores the stored values over unsaved changes
    client.settings_write("app/name", b"volatile").unwrap();
    client.settings_load().unwrap();
    assert_eq!(client.settings_read("app/name").unwrap(), b"persisted");
}

#[test]
fn save_a_subtree() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client.settings_write("secret/key", b"hunter2").unwrap();
    client.settings_write("app/name", b"saved?").unwrap();
    client.settings_save(Some("secret")).unwrap();

    let device = device.lock();
    assert_eq!(
        device.settings.storage.get("secret/key"),
        Some(&b"hunter2".to_vec())
    );
    assert_eq!(device.settings.storage.get("app/name"), None);
    assert_eq!(device.settings.loads, 0);
}

#[test]
fn delete() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client.settings_write("app/name", b"stored").unwrap();
    client.settings_save(None::<&str>).unwrap();

    client.settings_delete("app/name").unwrap();
    assert!(!device.lock().settings.storage.contains_key("app/name"));

    // Deletion removes the stored value, not the runtime value
    assert_eq!(client.settings_read("app/name").unwrap(), b"stored");
    client.os_system_reset(false, None).unwrap();
    assert_eq!(client.settings_read("app/name").unwrap(), b"zephyr-sim");

    // Deleting something that does not exist is not an error
    client.settings_delete("does/not/exist").unwrap();
}

#[test]
fn commit() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client.settings_commit().unwrap();
    client.settings_commit().unwrap();
    assert!(
        device
            .lock()
            .settings
            .handlers
            .iter()
            .all(|h| h.commits == 2)
    );
}

#[test]
fn name_and_value_length_limits() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // 31 bytes is the longest name; it gets looked up
    let longest = format!("app/{}", "k".repeat(27));
    assert_eq!(longest.len(), 31);
    let err = client.settings_read(&longest).unwrap_err();
    assert_settings_error(err, settings_mgmt_err::KEY_NOT_FOUND);

    // A value of exactly CONFIG_MCUMGR_GRP_SETTINGS_VALUE_LEN bytes
    client.settings_write("app/name", &[0x61; 32]).unwrap();
    let response = client.settings_read_ext("app/name", None).unwrap();
    assert_eq!(response.val, [0x61; 32]);
    assert_eq!(response.max_size, None);

    let response = client.settings_read_ext("app/name", Some(32)).unwrap();
    assert_eq!(response.max_size, None);
    let response = client.settings_read_ext("app/name", Some(33)).unwrap();
    assert_eq!(response.max_size, Some(32));

    let response = client.settings_read_ext("app/name", Some(0)).unwrap();
    assert_eq!(response.val, b"");
}

#[test]
fn load_commits_the_loaded_settings() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // settings_load() ends with settings_commit()
    client.settings_load().unwrap();
    assert!(
        device
            .lock()
            .settings
            .handlers
            .iter()
            .all(|h| h.commits == 1)
    );
}
