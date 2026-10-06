mod common;

use common::firmware_update_helpers::{
    ACTIVE, CONFIRMED, Event, OLD_HASH, OTHER_HASH, PENDING, STABLE, SetStateCall, TARGET_HASH,
    image_state, scripted_client, test_firmware,
};
use mcumgr_toolkit::{
    bootloader::BootloaderType,
    client::{FirmwareUpdateError, FirmwareUpdateParams},
};

fn params() -> FirmwareUpdateParams {
    FirmwareUpdateParams {
        bootloader_type: Some(BootloaderType::MCUboot),
        skip_reboot: false,
        force_confirm: false,
        upgrade_only: false,
    }
}

#[test]
fn already_installed_stable_image_does_not_touch_device() {
    let before = vec![image_state(0, 0, Some(&TARGET_HASH), STABLE)];
    let (client, device) = scripted_client(before.clone(), before);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(result, Err(FirmwareUpdateError::AlreadyInstalled)));
    assert_eq!(device.upload_count(), 0);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn preexisting_pending_state_blocks_update_before_upload() {
    let before = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
    ];
    let (client, device) = scripted_client(before.clone(), before);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(result, Err(FirmwareUpdateError::SystemNotReady)));
    assert_eq!(device.upload_count(), 0);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn preexisting_testing_state_blocks_update_before_upload() {
    let before = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
    ];
    let (client, device) = scripted_client(before.clone(), before);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(result, Err(FirmwareUpdateError::SystemNotReady)));
    assert_eq!(device.upload_count(), 0);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn normal_zephyr_update_uploads_sets_state_and_reboots() {
    let before = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&OTHER_HASH), Default::default()),
    ];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.upload_count(), 1);
    assert_eq!(
        device.set_state_calls(),
        vec![SetStateCall {
            hash: Some(TARGET_HASH.to_vec()),
            confirm: false,
        }]
    );
    assert_eq!(device.reset_count(), 1);

    assert!(matches!(
        device
            .events()
            .iter()
            .find(|event| matches!(event, Event::Upload { .. })),
        Some(Event::Upload { image: None, .. })
    ));
}

#[test]
fn force_confirm_is_forwarded_when_activation_is_required() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);
    let mut update_params = params();
    update_params.force_confirm = true;

    client
        .firmware_update(test_firmware(), None, update_params, None)
        .unwrap();

    assert_eq!(
        device.set_state_calls(),
        vec![SetStateCall {
            hash: Some(TARGET_HASH.to_vec()),
            confirm: true,
        }]
    );
}

#[test]
fn skip_reboot_still_activates_but_does_not_reset() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);
    let mut update_params = params();
    update_params.skip_reboot = true;

    client
        .firmware_update(test_firmware(), None, update_params, None)
        .unwrap();

    assert_eq!(device.set_state_calls().len(), 1);
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn recovery_upload_that_becomes_active_does_not_call_set_state() {
    let before = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&OTHER_HASH), Default::default()),
    ];
    let after = vec![
        image_state(0, 0, Some(&TARGET_HASH), STABLE),
        image_state(0, 1, Some(&OLD_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.upload_count(), 1);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn recovery_stable_without_hash_does_not_call_set_state() {
    let before = vec![image_state(0, 0, None, Default::default())];
    let after = vec![
        image_state(0, 0, None, STABLE),
        image_state(0, 1, None, Default::default()),
    ];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.upload_count(), 1);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn recovery_without_state_flags_allows_redundant_upload() {
    let before = vec![image_state(0, 0, None, Default::default())];
    let after = vec![image_state(0, 0, None, Default::default())];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.upload_count(), 1);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn empty_recovery_target_can_be_uploaded() {
    let after = vec![image_state(0, 0, None, Default::default())];
    let (client, device) = scripted_client(Vec::new(), after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.upload_count(), 1);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn post_upload_pending_target_is_accepted_without_set_state() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), PENDING),
    ];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn post_upload_pending_without_hash_is_accepted_conservatively() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, None, PENDING),
    ];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn post_upload_pending_other_image_fails_without_reset() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&OTHER_HASH), PENDING),
    ];
    let (client, device) = scripted_client(before, after);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(result, Err(FirmwareUpdateError::SystemNotReady)));
    assert_eq!(device.upload_count(), 1);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn post_upload_testing_target_is_accepted_without_confirmation() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&TARGET_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
    ];
    let (client, device) = scripted_client(before, after);
    let mut update_params = params();
    update_params.force_confirm = true;

    client
        .firmware_update(test_firmware(), None, update_params, None)
        .unwrap();

    // Even with force_confirm, do not issue set-state for an already-active test image.
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn post_upload_testing_wrong_current_image_fails_without_reset() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OTHER_HASH), ACTIVE),
        image_state(0, 1, Some(&OLD_HASH), CONFIRMED),
    ];
    let (client, device) = scripted_client(before, after);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(result, Err(FirmwareUpdateError::SystemNotReady)));
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn post_upload_unknown_guess_with_target_hash_is_accepted() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![image_state(0, 0, Some(&TARGET_HASH), Default::default())];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn post_upload_unknown_guess_with_wrong_hash_is_inconsistent() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![image_state(0, 0, Some(&OTHER_HASH), Default::default())];
    let (client, device) = scripted_client(before, after);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::InconsistentDeviceState)
    ));
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn post_upload_unknown_without_any_guess_is_inconsistent() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let (client, device) = scripted_client(before, Vec::new());

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::InconsistentDeviceState)
    ));
    assert_eq!(device.upload_count(), 1);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn other_image_pair_pending_does_not_block_image_zero_update() {
    let before = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(1, 0, Some(&OTHER_HASH), STABLE),
        image_state(1, 1, Some(&TARGET_HASH), PENDING),
    ];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
        image_state(1, 0, Some(&OTHER_HASH), STABLE),
        image_state(1, 1, Some(&TARGET_HASH), PENDING),
    ];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.upload_count(), 1);
    assert_eq!(device.set_state_calls().len(), 1);
    assert_eq!(device.reset_count(), 1);
}
