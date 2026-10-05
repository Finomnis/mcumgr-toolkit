//! The same API usage against devices configured with different encoding
//! related Kconfig options.

use mcumgr_toolkit::MCUmgrClient;

use super::{device_error, group_error};
use crate::sim::image::ImageBuilder;
use crate::sim::smp::group_id;
use crate::sim::{Config, SimDevice};

/// Uses every read-only command plus a few transfers
fn tour(device: &SimDevice, client: &MCUmgrClient) {
    assert_eq!(client.os_echo("tour").unwrap(), "tour");
    assert_eq!(client.os_task_statistics().unwrap().len(), 3);
    assert_eq!(client.os_memory_pool_statistics().unwrap().len(), 2);
    client.os_mcumgr_parameters().unwrap();
    client.os_application_info(Some("a")).unwrap();
    client.os_bootloader_info().unwrap();

    let image = ImageBuilder::new((9, 0, 0, 0));
    client
        .image_upload(image.build(), None, None, false, None)
        .unwrap();
    let state = client.image_get_state().unwrap();
    assert_eq!(state.len(), 2);
    assert_eq!(state[1].hash.as_deref(), Some(&image.hash()[..]));
    client.image_slot_info().unwrap();
    client.image_erase(None).unwrap();

    assert_eq!(client.stats_list_groups().unwrap().len(), 2);
    assert_eq!(client.stats_get_group_data("ble_ll").unwrap().len(), 3);

    client.settings_write("app/name", b"tour").unwrap();
    assert_eq!(client.settings_read("app/name").unwrap(), b"tour");

    let data = vec![0xa5; 1500];
    client
        .fs_file_upload("/lfs1/tour", &data[..], 1500, None)
        .unwrap();
    let mut downloaded = vec![];
    client
        .fs_file_download("/lfs1/tour", &mut downloaded, None)
        .unwrap();
    assert_eq!(downloaded, data);
    assert_eq!(client.fs_file_status("/lfs1/tour").unwrap().len, 1500);
    client
        .fs_file_checksum("/lfs1/tour", None::<&str>, 0, None)
        .unwrap();
    client.fs_supported_checksum_types().unwrap();

    client
        .shell_execute(&["kernel".into(), "version".into()], true)
        .unwrap();
    assert_eq!(client.enum_get_group_ids().unwrap().len(), 8);
    assert_eq!(client.enum_get_group_details(None).unwrap().len(), 8);
    client.zephyr_erase_storage().unwrap();

    // Errors are still recognized
    let err = client.stats_get_group_data("unknown").unwrap_err();
    assert_eq!(device_error(err), group_error(group_id::STAT, 3));

    assert_eq!(device.lock().link.dropped_oversized, 0);
}

fn device_with(config: Config) -> SimDevice {
    let device = SimDevice::new(config);
    device
        .lock()
        .flash_image(0, &ImageBuilder::new((1, 0, 0, 0)).build());
    device
}

#[test]
fn default_configuration() {
    let device = device_with(Config::default());
    tour(&device, &device.client());
}

#[test]
fn legacy_rc_behaviour() {
    // CONFIG_MCUMGR_SMP_LEGACY_RC_BEHAVIOUR: successful responses carry "rc": 0
    let device = device_with(Config {
        smp_legacy_rc_behaviour: true,
        ..Default::default()
    });
    tour(&device, &device.client());
}

#[test]
fn without_original_protocol_support() {
    let device = device_with(Config {
        smp_support_original_protocol: false,
        ..Default::default()
    });
    tour(&device, &device.client());
}

#[test]
fn large_buffers() {
    let device = device_with(Config {
        mcumgr_transport_netbuf_size: 4096,
        ..Default::default()
    });
    let client = device.client();
    client.use_auto_frame_size().unwrap();
    tour(&device, &client);
}

#[test]
fn over_serial() {
    let device = device_with(Config::default());
    tour(&device, &device.serial_client());
}
