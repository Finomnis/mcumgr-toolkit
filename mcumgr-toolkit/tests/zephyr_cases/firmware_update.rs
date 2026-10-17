use crate::zephyr_sim::{
    Config, Fault, client, client_with,
    firmware::{firmware, image_hash, sha256},
    wire::RemoteError,
};
use mcumgr_toolkit::{
    bootloader::BootloaderType,
    client::{FirmwareUpdateError, FirmwareUpdateParams, FirmwareUpdateStep},
};

#[test]
fn complete_firmware_update_uploads_activates_and_reboots() {
    let (client, handle) = client();
    client.set_frame_size(192);
    let data = firmware(2, 1500);
    let mut events = Vec::new();
    client
        .firmware_update(
            &data,
            None,
            FirmwareUpdateParams::default(),
            Some(&mut |step, progress| {
                events.push((step, progress));
                true
            }),
        )
        .unwrap();
    assert_eq!(
        handle.inspect(|d| d.last_upload.as_ref().unwrap().data.clone()),
        data
    );
    assert_eq!(handle.inspect(|d| d.resets.len()), 1);
    let states = client.image_get_state().unwrap();
    let active = states.iter().find(|s| s.active).unwrap();
    assert_eq!(active.version, "2.2.3.4");
    assert_eq!(active.hash, image_hash(&data));
    assert!(!active.confirmed, "default update requests a test boot");
    for step in [
        FirmwareUpdateStep::DetectingBootloader,
        FirmwareUpdateStep::BootloaderFound(BootloaderType::MCUboot),
        FirmwareUpdateStep::ParsingFirmwareImage,
        FirmwareUpdateStep::QueryingDeviceState,
        FirmwareUpdateStep::UploadingFirmware,
        FirmwareUpdateStep::ActivatingFirmware,
        FirmwareUpdateStep::TriggeringReboot,
    ] {
        assert!(
            events.iter().any(|e| e.0 == step),
            "missing progress step {step:?}"
        );
    }
    let summary = events
        .iter()
        .find_map(|(step, _)| match step {
            FirmwareUpdateStep::UpdateInfo {
                current_version,
                new_version,
            } => Some((current_version, new_version)),
            _ => None,
        })
        .unwrap();
    assert_eq!(summary.0.as_ref().unwrap().0, "1.2.3.4");
    assert_eq!(summary.1, &("2.2.3.4".into(), image_hash(&data).unwrap()));
    assert!(
        events
            .iter()
            .any(|(_, p)| *p == Some((data.len() as u64, data.len() as u64)))
    );
    let routes: Vec<_> = handle
        .requests()
        .iter()
        .map(|r| (r.group(), r.id(), r.op()))
        .collect();
    let last_upload = routes.iter().rposition(|r| *r == (1, 1, 2)).unwrap();
    let activate = routes.iter().position(|r| *r == (1, 0, 2)).unwrap();
    let reboot = routes.iter().position(|r| *r == (0, 5, 2)).unwrap();
    assert!(last_upload < activate && activate < reboot);
}

#[test]
fn firmware_update_explicit_bootloader_skip_reboot_and_force_confirm() {
    let (client, handle) = client();
    let data = firmware(2, 800);
    client
        .firmware_update(
            &data,
            Some(sha256(&data)),
            FirmwareUpdateParams {
                bootloader_type: Some(BootloaderType::MCUboot),
                skip_reboot: true,
                force_confirm: true,
                upgrade_only: true,
            },
            None,
        )
        .unwrap();
    assert!(
        !handle
            .requests()
            .iter()
            .any(|r| r.group() == 0 && r.id() == 8)
    );
    assert!(handle.inspect(|d| d.resets.is_empty()));
    let pending = client
        .image_get_state()
        .unwrap()
        .into_iter()
        .find(|s| s.pending)
        .unwrap();
    assert!(pending.permanent);
    assert_eq!(pending.hash, image_hash(&data));
    let request = handle
        .requests()
        .into_iter()
        .find(|r| r.group() == 1 && r.id() == 1)
        .unwrap();
    assert_eq!(request.get("upgrade").unwrap().as_bool(), Some(true));
    client.os_system_reset(false, None).unwrap();
    assert!(
        client
            .image_get_state()
            .unwrap()
            .iter()
            .find(|s| s.active)
            .unwrap()
            .confirmed
    );
}

#[test]
fn firmware_update_rejects_already_running_image() {
    let (client, handle) = client();
    let image = handle.inspect(|d| d.slots[0].data.clone());
    assert!(matches!(
        client.firmware_update(image, None, FirmwareUpdateParams::default(), None),
        Err(FirmwareUpdateError::AlreadyInstalled)
    ));
    assert!(handle.inspect(|d| d.last_upload.is_none() && d.resets.is_empty()));
    assert!(
        !handle
            .requests()
            .iter()
            .any(|r| r.group() == 1 && r.id() == 1)
    );
}

#[test]
fn firmware_update_rejects_invalid_mcuboot_data() {
    let (client, handle) = client();
    assert!(matches!(
        client.firmware_update(b"not firmware", None, FirmwareUpdateParams::default(), None),
        Err(FirmwareUpdateError::InvalidMcuBootFirmwareImage(_))
    ));
    assert!(handle.inspect(|d| d.last_upload.is_none() && d.resets.is_empty()));
}

#[test]
fn firmware_update_rejects_unknown_bootloader() {
    let (client, handle) = client_with(Config {
        bootloader: "custom".into(),
        ..Config::default()
    });
    assert!(
        matches!(client.firmware_update(firmware(2, 400), None, FirmwareUpdateParams::default(), None), Err(FirmwareUpdateError::BootloaderNotSupported(name)) if name == "custom")
    );
    assert!(handle.inspect(|d| d.last_upload.is_none() && d.resets.is_empty()));
}

#[test]
fn firmware_update_wraps_failures_at_each_remote_stage() {
    for stage in 0..5 {
        let (client, handle) = client();
        let (group, id, skip) = match stage {
            0 => (0, 8, 0),
            1 => (1, 0, 0),
            2 => (1, 1, 0),
            3 => (1, 0, 1),
            _ => (0, 5, 0),
        };
        handle.fault_on(group, id, skip, Fault::Reply(RemoteError::Smp(11).body()));
        let error = client
            .firmware_update(
                firmware(2, 400),
                None,
                FirmwareUpdateParams::default(),
                None,
            )
            .unwrap_err();
        let correct = match stage {
            0 => matches!(error, FirmwareUpdateError::BootloaderDetectionFailed(_)),
            1 => matches!(error, FirmwareUpdateError::GetStateFailed(_)),
            2 => matches!(error, FirmwareUpdateError::ImageUploadFailed(_)),
            3 => matches!(error, FirmwareUpdateError::SetStateFailed(_)),
            _ => matches!(error, FirmwareUpdateError::RebootFailed(_)),
        };
        assert!(correct, "stage {stage}: {error:?}");
        assert!(handle.inspect(|d| d.resets.is_empty()));
        handle.assert_faults_consumed();
    }
}

#[test]
fn firmware_update_can_cancel_at_every_progress_stage() {
    for stop in [
        FirmwareUpdateStep::DetectingBootloader,
        FirmwareUpdateStep::ParsingFirmwareImage,
        FirmwareUpdateStep::QueryingDeviceState,
        FirmwareUpdateStep::UploadingFirmware,
        FirmwareUpdateStep::ActivatingFirmware,
        FirmwareUpdateStep::TriggeringReboot,
    ] {
        let (client, handle) = client();
        let mut cancelled = false;
        let result = client.firmware_update(
            firmware(2, 600),
            None,
            FirmwareUpdateParams::default(),
            Some(&mut |step, _| {
                if step == stop {
                    cancelled = true;
                    false
                } else {
                    true
                }
            }),
        );
        assert!(cancelled, "stage never reached: {stop:?}");
        assert!(result.is_err(), "cancel ignored at {stop:?}");
        assert!(
            handle.inspect(|d| d.resets.is_empty()),
            "reboot after cancellation at {stop:?}"
        );
    }
}

#[test]
fn firmware_update_upgrade_only_failure_does_not_activate() {
    let (client, handle) = client();
    assert!(matches!(
        client.firmware_update(
            firmware(0, 300),
            None,
            FirmwareUpdateParams {
                upgrade_only: true,
                ..FirmwareUpdateParams::default()
            },
            None
        ),
        Err(FirmwareUpdateError::ImageUploadFailed(_))
    ));
    assert!(
        !handle
            .requests()
            .iter()
            .any(|r| (r.group(), r.id(), r.op()) == (1, 0, 2))
    );
    assert!(handle.inspect(|d| d.resets.is_empty()));
}

#[test]
fn firmware_update_propagates_image_checksum_failure() {
    let (client, handle) = client();
    handle.edit(|d| d.corrupt_upload = true);
    assert!(matches!(
        client.firmware_update(
            firmware(2, 400),
            None,
            FirmwareUpdateParams::default(),
            None
        ),
        Err(FirmwareUpdateError::ImageUploadFailed(_))
    ));
    assert!(handle.inspect(|d| d.resets.is_empty()));
}
