//! The high level firmware update routine

use mcumgr_toolkit::bootloader::BootloaderType;
use mcumgr_toolkit::client::{
    FirmwareUpdateError, FirmwareUpdateParams, FirmwareUpdateStep, MCUmgrClientError,
};
use mcumgr_toolkit::mcuboot::{ImageVersion, get_image_info};
use sha2::{Digest, Sha256};

use super::{device_error, group_error};
use crate::sim::image::ImageBuilder;
use crate::sim::img_mgmt::img_mgmt_err;
use crate::sim::smp::group_id;
use crate::sim::{Bootloader, Config, SimDevice};

fn new_firmware() -> ImageBuilder {
    ImageBuilder::new((2, 1, 0, 7)).body((0..12_345u32).map(|i| (i % 256) as u8).collect())
}

/// Records the steps reported to the progress callback
fn record_update(
    device: &SimDevice,
    firmware: &[u8],
    params: FirmwareUpdateParams,
) -> (Result<(), FirmwareUpdateError>, Vec<FirmwareUpdateStep>) {
    let client = device.client();
    let mut steps = vec![];
    let mut progress = |step: FirmwareUpdateStep, _: Option<(u64, u64)>| {
        if steps.last() != Some(&step) {
            steps.push(step);
        }
        true
    };
    let result = client.firmware_update(firmware, None, params, Some(&mut progress));
    (result, steps)
}

#[test]
fn image_info_of_the_simulated_images() {
    let firmware = new_firmware();
    let info = get_image_info(std::io::Cursor::new(firmware.build())).unwrap();

    assert_eq!(
        info.version,
        ImageVersion {
            major: 2,
            minor: 1,
            revision: 0,
            build_num: 7
        }
    );
    assert_eq!(info.hash.as_ref(), firmware.hash());
}

#[test]
fn update_with_test_boot() {
    let device = SimDevice::with_firmware();
    let firmware = new_firmware();

    let (result, steps) = record_update(&device, &firmware.build(), Default::default());
    result.unwrap();

    assert_eq!(
        steps,
        [
            FirmwareUpdateStep::DetectingBootloader,
            FirmwareUpdateStep::BootloaderFound(BootloaderType::MCUboot),
            FirmwareUpdateStep::ParsingFirmwareImage,
            FirmwareUpdateStep::QueryingDeviceState,
            FirmwareUpdateStep::UpdateInfo {
                current_version: Some((
                    "1.2.3".into(),
                    Some(ImageBuilder::new((1, 2, 3, 0)).hash().to_vec())
                )),
                new_version: ("2.1.0.7".into(), firmware.hash().to_vec()),
            },
            FirmwareUpdateStep::UploadingFirmware,
            FirmwareUpdateStep::QueryingDeviceState,
            FirmwareUpdateStep::ActivatingFirmware,
            FirmwareUpdateStep::TriggeringReboot,
        ]
    );

    // The device rebooted into the new firmware, which still has to confirm
    // itself.
    let client = device.client();
    let state = client.image_get_state().unwrap();
    assert_eq!(state[0].version, "2.1.0.7");
    assert!(state[0].active);
    assert!(!state[0].confirmed);
    assert_eq!(device.lock().boot_count, 2);

    // Without confirmation, MCUboot reverts on the next reboot
    client.os_system_reset(false, None).unwrap();
    assert_eq!(client.image_get_state().unwrap()[0].version, "1.2.3");
}

#[test]
fn update_with_force_confirm() {
    let device = SimDevice::with_firmware();
    let firmware = new_firmware();

    let params = FirmwareUpdateParams {
        force_confirm: true,
        ..Default::default()
    };
    let (result, _) = record_update(&device, &firmware.build(), params);
    result.unwrap();

    let client = device.client();
    client.os_system_reset(false, None).unwrap();
    let state = client.image_get_state().unwrap();
    assert_eq!(state[0].version, "2.1.0.7");
    assert!(state[0].confirmed);
}

#[test]
fn update_without_reboot() {
    let device = SimDevice::with_firmware();
    let firmware = new_firmware();

    let params = FirmwareUpdateParams {
        skip_reboot: true,
        ..Default::default()
    };
    let (result, steps) = record_update(&device, &firmware.build(), params);
    result.unwrap();
    assert!(!steps.contains(&FirmwareUpdateStep::TriggeringReboot));
    assert_eq!(device.lock().boot_count, 1);

    let state = device.client().image_get_state().unwrap();
    assert_eq!(state[1].version, "2.1.0.7");
    assert!(state[1].pending);
}

#[test]
fn update_with_known_bootloader_skips_detection() {
    let device = SimDevice::new(Config {
        // Detection would fail on this device
        bootloader: Bootloader::Other,
        ..Default::default()
    });
    device
        .lock()
        .flash_image(0, &ImageBuilder::new((1, 0, 0, 0)).build());

    let params = FirmwareUpdateParams {
        bootloader_type: Some(BootloaderType::MCUboot),
        ..Default::default()
    };
    let (result, steps) = record_update(&device, &new_firmware().build(), params);
    result.unwrap();
    assert_eq!(steps[0], FirmwareUpdateStep::ParsingFirmwareImage);
    assert!(device.requests_for(group_id::OS, 8).is_empty());
}

#[test]
fn update_fails_when_the_bootloader_can_not_be_detected() {
    let device = SimDevice::new(Config {
        bootloader: Bootloader::Other,
        ..Default::default()
    });

    let (result, _) = record_update(&device, &new_firmware().build(), Default::default());
    assert!(matches!(
        result.unwrap_err(),
        FirmwareUpdateError::BootloaderDetectionFailed(_)
    ));
}

#[test]
fn update_detects_already_installed_firmware() {
    let device = SimDevice::new(Config::default());
    device.lock().flash_image(0, &new_firmware().build());

    let (result, _) = record_update(&device, &new_firmware().build(), Default::default());
    assert!(matches!(
        result.unwrap_err(),
        FirmwareUpdateError::AlreadyInstalled
    ));
    assert!(device.requests_for(group_id::IMAGE, 1).is_empty());
}

#[test]
fn update_rejects_invalid_images() {
    let device = SimDevice::with_firmware();

    let (result, _) = record_update(&device, &[0x42; 4096], Default::default());
    assert!(matches!(
        result.unwrap_err(),
        FirmwareUpdateError::InvalidMcuBootFirmwareImage(_)
    ));
    assert!(device.requests_for(group_id::IMAGE, 1).is_empty());
}

#[test]
fn update_refuses_downgrades_when_asked_to() {
    let device = SimDevice::new(Config::default());
    device.lock().flash_image(0, &new_firmware().build());

    let params = FirmwareUpdateParams {
        upgrade_only: true,
        ..Default::default()
    };
    let old = ImageBuilder::new((1, 0, 0, 0));
    let (result, _) = record_update(&device, &old.build(), params);

    let FirmwareUpdateError::ImageUploadFailed(err) = result.unwrap_err() else {
        panic!("expected an upload error");
    };
    assert_eq!(
        device_error(err),
        group_error(group_id::IMAGE, img_mgmt_err::CURRENT_VERSION_IS_NEWER)
    );
}

#[test]
fn update_verifies_the_given_checksum() {
    let device = SimDevice::with_firmware();
    let firmware = new_firmware().build();
    let client = device.client();

    let err = client
        .firmware_update(&firmware, Some([1; 32]), Default::default(), None)
        .unwrap_err();
    assert!(matches!(
        err,
        FirmwareUpdateError::ImageUploadFailed(MCUmgrClientError::ChecksumMismatch)
    ));

    let checksum = Sha256::digest(&firmware).into();
    client
        .firmware_update(&firmware, Some(checksum), Default::default(), None)
        .unwrap();
}

#[test]
fn update_can_be_cancelled_through_the_progress_callback() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let firmware = new_firmware().build();

    let mut cancel_during_upload = |step: FirmwareUpdateStep, progress: Option<(u64, u64)>| {
        !(step == FirmwareUpdateStep::UploadingFirmware
            && progress.is_some_and(|(current, _)| current > 3000))
    };
    let err = client
        .firmware_update(
            &firmware,
            None,
            Default::default(),
            Some(&mut cancel_during_upload),
        )
        .unwrap_err();
    assert!(matches!(err, FirmwareUpdateError::ProgressCallbackError));

    let mut cancel_before_activation = |step: FirmwareUpdateStep, _: Option<(u64, u64)>| {
        step != FirmwareUpdateStep::ActivatingFirmware
    };
    let err = client
        .firmware_update(
            &firmware,
            None,
            Default::default(),
            Some(&mut cancel_before_activation),
        )
        .unwrap_err();
    assert!(matches!(err, FirmwareUpdateError::ProgressCallbackError));
    assert!(
        device
            .requests_for(group_id::IMAGE, 0)
            .iter()
            .all(|r| r.hdr.op == 0)
    );
}

#[test]
fn update_fails_while_a_test_boot_is_unconfirmed() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // Leave the device in test mode of another image; the previous image has
    // to stay in the secondary slot until the test boot is confirmed.
    let other = ImageBuilder::new((1, 5, 0, 0));
    client
        .image_upload(other.build(), None, None, false, None)
        .unwrap();
    client.image_set_state(Some(&other.hash()), false).unwrap();
    client.os_system_reset(false, None).unwrap();
    device.clear_requests();

    let (result, _) = record_update(&device, &new_firmware().build(), Default::default());
    assert!(matches!(
        result.unwrap_err(),
        FirmwareUpdateError::ImageCurrentlyTested
    ));
    assert!(device.requests_for(group_id::IMAGE, 1).is_empty());
}

#[test]
fn update_fails_while_another_image_is_pending() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let other = ImageBuilder::new((1, 5, 0, 0));
    client
        .image_upload(other.build(), None, None, false, None)
        .unwrap();
    client.image_set_state(Some(&other.hash()), false).unwrap();
    device.clear_requests();

    let (result, _) = record_update(&device, &new_firmware().build(), Default::default());
    assert!(matches!(
        result.unwrap_err(),
        FirmwareUpdateError::ImageAlreadyPending
    ));
    assert!(device.requests_for(group_id::IMAGE, 1).is_empty());
}

#[test]
fn update_over_a_serial_port() {
    let device = SimDevice::with_firmware();
    let client = device.serial_client();
    let firmware = new_firmware();

    client
        .firmware_update(firmware.build(), None, Default::default(), None)
        .unwrap();

    let state = client.image_get_state().unwrap();
    assert_eq!(state[0].version, "2.1.0.7");
    assert_eq!(state[0].hash.as_deref(), Some(&firmware.hash()[..]));
}

#[test]
fn update_with_an_image_that_was_already_uploaded() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let firmware = new_firmware();

    client
        .image_upload(firmware.build(), None, None, false, None)
        .unwrap();
    device.clear_requests();

    client
        .firmware_update(firmware.build(), None, Default::default(), None)
        .unwrap();
    // The device recognizes the data in the slot and skips the transfer
    assert_eq!(device.requests_for(group_id::IMAGE, 1).len(), 1);
    assert_eq!(client.image_get_state().unwrap()[0].version, "2.1.0.7");
}

#[test]
fn update_of_an_empty_firmware_file() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client
        .firmware_update(Vec::<u8>::new(), None, Default::default(), None)
        .unwrap_err();
    assert!(matches!(
        err,
        FirmwareUpdateError::InvalidMcuBootFirmwareImage(_)
    ));
}
