mod common;

use common::firmware_update_helpers::{
    ACTIVE, CONFIRMED, Event, OLD_HASH, OTHER_HASH, PENDING, RC_BAD_STATE, RC_NOT_SUPPORTED,
    STABLE, SetStateCall, TARGET_HASH, image_state, scripted_client, test_firmware,
};
use mcumgr_toolkit::{
    MCUmgrClient,
    bootloader::BootloaderType,
    client::{FirmwareUpdateError, FirmwareUpdateParams, FirmwareUpdateStep},
};

/// Runs a firmware update and records all reported progress steps.
///
/// Intermediate upload progress reports (`Some(..)`) are omitted.
fn update_with_progress(
    client: &MCUmgrClient,
    params: FirmwareUpdateParams,
) -> (Result<(), FirmwareUpdateError>, Vec<FirmwareUpdateStep>) {
    let mut steps = Vec::new();
    let mut cb = |step: FirmwareUpdateStep, prog: Option<(u64, u64)>| {
        if prog.is_none() {
            steps.push(step);
        }
        true
    };
    let result = client.firmware_update(test_firmware(), None, params, Some(&mut cb));
    (result, steps)
}

/// Extracts `(current_version, new_version)` from the reported `UpdateInfo` step.
#[allow(clippy::type_complexity)]
fn update_info(
    steps: &[FirmwareUpdateStep],
) -> (Option<(String, Option<Vec<u8>>)>, (String, Vec<u8>)) {
    steps
        .iter()
        .find_map(|step| match step {
            FirmwareUpdateStep::UpdateInfo {
                current_version,
                new_version,
            } => Some((current_version.clone(), new_version.clone())),
            _ => None,
        })
        .expect("no update info reported")
}

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

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::ImageAlreadyPending)
    ));
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

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::ImageCurrentlyTested)
    ));
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

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::ImageAlreadyPending)
    ));
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

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::ImageCurrentlyTested)
    ));
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

// Device interaction

#[test]
fn normal_update_activates_uploaded_image_before_reboot() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    // Only the order that matters for the outcome is checked:
    // set-state needs the uploaded image, and the reboot must come last
    // or the device boots without the activated image.
    let writes = device.writes();
    let last_upload = writes
        .iter()
        .rposition(|event| matches!(event, Event::Upload { .. }))
        .expect("image was not uploaded");
    let set_state = writes
        .iter()
        .position(|event| matches!(event, Event::SetState(_)))
        .expect("image was not activated");
    assert!(last_upload < set_state);
    assert_eq!(writes.last(), Some(&Event::Reset));
}

#[test]
fn upgrade_only_is_forwarded_to_upload() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);
    let mut update_params = params();
    update_params.upgrade_only = true;

    client
        .firmware_update(test_firmware(), None, update_params, None)
        .unwrap();

    assert!(matches!(
        device
            .events()
            .iter()
            .find(|event| matches!(event, Event::Upload { .. })),
        Some(Event::Upload {
            upgrade: Some(true),
            ..
        })
    ));
}

// Progress reporting

#[test]
fn progress_reports_update_info_activation_and_reboot() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, _device) = scripted_client(before.clone(), after);

    let (result, steps) = update_with_progress(&client, params());
    result.unwrap();

    let (current_version, new_version) = update_info(&steps);
    assert_eq!(
        current_version,
        Some((before[0].version.clone(), Some(OLD_HASH.to_vec())))
    );
    assert_eq!(new_version.1, TARGET_HASH.to_vec());

    let activating = steps
        .iter()
        .position(|step| *step == FirmwareUpdateStep::ActivatingFirmware)
        .expect("activation was not reported");
    let rebooting = steps
        .iter()
        .position(|step| *step == FirmwareUpdateStep::TriggeringReboot)
        .expect("reboot was not reported");
    assert!(activating < rebooting);
}

#[test]
fn progress_with_skip_reboot_reports_no_reboot() {
    let before = vec![image_state(0, 0, None, Default::default())];
    let after = vec![image_state(0, 0, Some(&TARGET_HASH), Default::default())];
    let (client, _device) = scripted_client(before.clone(), after);
    let mut update_params = params();
    update_params.skip_reboot = true;

    let (result, steps) = update_with_progress(&client, update_params);
    result.unwrap();

    assert!(matches!(update_info(&steps).0, Some((_, None))));
    assert!(!steps.contains(&FirmwareUpdateStep::TriggeringReboot));
}

#[test]
fn progress_reports_empty_device_as_no_current_version() {
    let after = vec![image_state(0, 0, Some(&TARGET_HASH), Default::default())];
    let (client, _device) = scripted_client(Vec::new(), after);

    let (result, steps) = update_with_progress(&client, params());
    result.unwrap();

    assert_eq!(update_info(&steps).0, None);
}

#[test]
fn progress_callback_abort_before_upload_does_not_touch_device() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let (client, device) = scripted_client(before.clone(), before);

    let mut cb = |step: FirmwareUpdateStep, _| step != FirmwareUpdateStep::UploadingFirmware;
    let result = client.firmware_update(test_firmware(), None, params(), Some(&mut cb));

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::ProgressCallbackError)
    ));
    assert!(device.writes().is_empty());
}

#[test]
fn progress_callback_abort_during_upload_stops_update() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);

    let mut cb = |_, prog: Option<(u64, u64)>| prog.is_none();
    let result = client.firmware_update(test_firmware(), None, params(), Some(&mut cb));

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::ProgressCallbackError)
    ));
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

// Device errors

#[test]
fn upload_error_aborts_update() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let (client, device) = scripted_client(before.clone(), before);
    device.fail_upload_with(RC_BAD_STATE);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::ImageUploadFailed(_))
    ));
    assert_eq!(device.upload_count(), 1);
    assert!(device.set_state_calls().is_empty());
    assert_eq!(device.reset_count(), 0);
}

#[test]
fn set_state_error_is_reported_and_does_not_reboot() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), Default::default()),
    ];
    let (client, device) = scripted_client(before, after);
    device.fail_set_state_with(RC_NOT_SUPPORTED);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    match result {
        Err(FirmwareUpdateError::SetStateFailed(err)) => assert!(err.command_not_supported()),
        other => panic!("expected SetStateFailed, got {other:?}"),
    }
    assert_eq!(device.set_state_calls().len(), 1);
    assert_eq!(device.reset_count(), 0);
}

// Inconsistent device state

#[test]
fn inconsistent_state_before_upload_does_not_touch_device() {
    let before = vec![
        image_state(0, 0, Some(&OLD_HASH), ACTIVE),
        image_state(0, 1, Some(&OTHER_HASH), ACTIVE),
    ];
    let (client, device) = scripted_client(before.clone(), before);

    let result = client.firmware_update(test_firmware(), None, params(), None);

    assert!(matches!(
        result,
        Err(FirmwareUpdateError::InconsistentDeviceState)
    ));
    assert!(device.writes().is_empty());
}

#[test]
fn inconsistent_state_after_upload_fails_without_reset() {
    let before = vec![image_state(0, 0, Some(&OLD_HASH), STABLE)];
    let after = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(0, 1, Some(&TARGET_HASH), CONFIRMED),
    ];
    let (client, device) = scripted_client(before, after);

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
fn inconsistent_other_image_does_not_block_update() {
    let before = vec![
        image_state(0, 0, Some(&OLD_HASH), STABLE),
        image_state(1, 0, Some(&OLD_HASH), ACTIVE),
        image_state(1, 1, Some(&OTHER_HASH), ACTIVE),
    ];
    let mut after = before.clone();
    after.push(image_state(0, 1, Some(&TARGET_HASH), Default::default()));
    let (client, device) = scripted_client(before, after);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.set_state_calls().len(), 1);
    assert_eq!(device.reset_count(), 1);
}

#[test]
fn recovery_with_target_already_in_slot_zero_reuploads() {
    // Without state flags we cannot know whether the image in slot 0 actually
    // boots. Recovery mode exists to re-flash broken devices, so the user's
    // request to upload must be respected instead of reporting AlreadyInstalled.
    let before = vec![image_state(0, 0, Some(&TARGET_HASH), Default::default())];
    let (client, device) = scripted_client(before.clone(), before);

    client
        .firmware_update(test_firmware(), None, params(), None)
        .unwrap();

    assert_eq!(device.upload_count(), 1);
    assert_eq!(device.reset_count(), 1);
}
