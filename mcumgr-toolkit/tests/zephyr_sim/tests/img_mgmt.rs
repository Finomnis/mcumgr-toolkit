//! Application/software image management group (group 1)

use mcumgr_toolkit::client::MCUmgrClientError;
use mcumgr_toolkit::commands::image::{ImageState, SlotInfoImage, SlotInfoImageSlot};
use sha2::{Digest, Sha256};

use super::{device_error, group_error};
use crate::sim::image::{IMAGE_TLV_SEC_CNT, ImageBuilder};
use crate::sim::img_mgmt::{Magic, img_mgmt_err};
use crate::sim::smp::group_id;
use crate::sim::{Config, SimDevice};

const IMAGE_UPLOAD: u8 = 1;

fn v1() -> ImageBuilder {
    ImageBuilder::new((1, 2, 3, 0))
}

fn v2() -> ImageBuilder {
    ImageBuilder::new((2, 0, 0, 0)).body(vec![0x5a; 10_000])
}

fn state(slot: u32, image: &ImageBuilder, version: &str) -> ImageState {
    ImageState {
        image: 0,
        slot,
        version: version.into(),
        hash: Some(image.hash().to_vec()),
        bootable: true,
        pending: false,
        confirmed: false,
        active: false,
        permanent: false,
    }
}

#[track_caller]
fn assert_image_error(err: MCUmgrClientError, rc: u16) {
    assert_eq!(device_error(err), group_error(group_id::IMAGE, rc));
}

#[test]
fn state_of_a_freshly_programmed_device() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(
        client.image_get_state().unwrap(),
        [ImageState {
            confirmed: true,
            active: true,
            ..state(0, &v1(), "1.2.3")
        }]
    );
}

#[test]
fn state_reports_build_number_and_bootable_flag() {
    let device = SimDevice::new(Config::default());
    let image = ImageBuilder::new((3, 14, 15926, 535)).non_bootable();
    device.lock().flash_image(0, &image.build());
    let client = device.client();

    let state = client.image_get_state().unwrap();
    assert_eq!(state[0].version, "3.14.15926.535");
    assert!(!state[0].bootable);
    assert_eq!(state[0].hash.as_deref(), Some(&image.hash()[..]));
}

#[test]
fn state_of_an_image_with_protected_tlvs() {
    let device = SimDevice::new(Config::default());
    let image = v1().protected_tlv(IMAGE_TLV_SEC_CNT, 7u32.to_le_bytes().to_vec());
    device.lock().flash_image(0, &image.build());
    let client = device.client();

    let state = client.image_get_state().unwrap();
    assert_eq!(state[0].hash.as_deref(), Some(&image.hash()[..]));
}

#[test]
fn state_with_frugal_list() {
    let device = SimDevice::new(Config {
        img_frugal_list: true,
        ..Default::default()
    });
    device.lock().flash_image(0, &v1().build());
    let client = device.client();

    // Only flags that are set are sent, the client defaults the rest
    assert_eq!(
        client.image_get_state().unwrap(),
        [ImageState {
            confirmed: true,
            active: true,
            ..state(0, &v1(), "1.2.3")
        }]
    );
    let image_v2 = v2().build();
    client
        .image_upload(&image_v2, None, None, false, None)
        .unwrap();
    assert_eq!(
        client.image_get_state().unwrap()[1],
        state(1, &v2(), "2.0.0")
    );
}

#[test]
fn state_of_an_empty_device() {
    let device = SimDevice::new(Config::default());
    let client = device.client();

    assert_eq!(client.image_get_state().unwrap(), []);
}

#[test]
fn upload_writes_the_image_to_the_secondary_slot() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();

    client
        .image_upload(&image, None, None, false, None)
        .unwrap();

    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);
    assert_eq!(
        client.image_get_state().unwrap()[1],
        state(1, &v2(), "2.0.0")
    );

    let requests = device.requests_for(group_id::IMAGE, IMAGE_UPLOAD);
    assert!(requests.len() > 10);
    assert!(requests.iter().all(|r| r.frame_len <= 384));
    assert_eq!(device.lock().link.dropped_oversized, 0);

    // The first chunk announces the upload, the following ones only carry data
    let first = &requests[0];
    assert_eq!(first.field("off").unwrap().as_integer(), Some(0.into()));
    assert_eq!(
        first.field("len").unwrap().as_integer(),
        Some(image.len().into())
    );
    assert_eq!(
        first.field("sha").unwrap().as_bytes().unwrap()[..],
        Sha256::digest(&image)[..]
    );
    assert_eq!(first.field("upgrade").unwrap().as_bool(), Some(false));
    assert!(first.field("image").is_none());

    let mut expected_off = first.field("data").unwrap().as_bytes().unwrap().len();
    for request in &requests[1..] {
        assert!(request.field("len").is_none());
        assert!(request.field("sha").is_none());
        assert!(request.field("upgrade").is_none());
        assert_eq!(
            request.field("off").unwrap().as_integer(),
            Some(expected_off.into())
        );
        expected_off += request.field("data").unwrap().as_bytes().unwrap().len();
    }
    assert_eq!(expected_off, image.len());
}

#[test]
fn upload_reports_progress() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();

    let mut progress = vec![];
    let mut callback = |current: u64, total: u64| {
        progress.push((current, total));
        true
    };
    client
        .image_upload(&image, None, None, false, Some(&mut callback))
        .unwrap();

    let total = image.len() as u64;
    assert!(progress.len() > 10);
    assert!(progress.iter().all(|(_, t)| *t == total));
    assert!(progress.windows(2).all(|p| p[0].0 < p[1].0));
    assert_eq!(progress.last(), Some(&(total, total)));
}

#[test]
fn an_aborted_upload_is_resumed() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();

    let mut abort_after = |current: u64, _: u64| current < 5000;
    let err = client
        .image_upload(&image, None, None, false, Some(&mut abort_after))
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::ProgressCallbackError));
    device.clear_requests();

    // The device recognizes the SHA of the interrupted upload and answers the
    // first chunk with the offset it already got.
    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    let requests = device.requests_for(group_id::IMAGE, IMAGE_UPLOAD);
    let resumed_at = requests[1].field("off").unwrap().as_integer().unwrap();
    assert!(u64::try_from(resumed_at).unwrap() >= 5000);

    let reference = SimDevice::with_firmware();
    reference
        .client()
        .image_upload(&image, None, None, false, None)
        .unwrap();
    let full_upload = reference.requests_for(group_id::IMAGE, IMAGE_UPLOAD).len();
    assert!(requests.len() < full_upload);

    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);
}

#[test]
fn uploading_data_that_is_already_in_the_slot_finishes_immediately() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();

    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    device.clear_requests();

    // CONFIG_IMG_ENABLE_IMAGE_CHECK: the device compares the SHA of the slot
    let mut progress = vec![];
    let mut callback = |current: u64, total: u64| {
        progress.push((current, total));
        true
    };
    client
        .image_upload(&image, None, None, false, Some(&mut callback))
        .unwrap();
    assert_eq!(device.requests_for(group_id::IMAGE, IMAGE_UPLOAD).len(), 1);
    assert_eq!(progress, [(image.len() as u64, image.len() as u64)]);
}

#[test]
fn upload_checksum_is_verified_before_uploading() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();

    let err = client
        .image_upload(&image, None, Some([0; 32]), false, None)
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::ChecksumMismatch));
    assert!(device.requests().is_empty());

    let checksum = Sha256::digest(&image).into();
    client
        .image_upload(&image, None, Some(checksum), false, None)
        .unwrap();
}

#[test]
fn upload_fails_if_the_device_reads_back_different_data() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    device.lock().img.corrupt_flash_writes = true;

    let err = client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::ChecksumMismatchOnDevice));
}

#[test]
fn upload_without_image_check_on_the_device() {
    let device = SimDevice::new(Config {
        img_enable_image_check: false,
        ..Default::default()
    });
    device.lock().flash_image(0, &v1().build());
    device.lock().img.corrupt_flash_writes = true;
    let client = device.client();

    // Without CONFIG_IMG_ENABLE_IMAGE_CHECK there is no "match" to check
    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
}

#[test]
fn upload_only_newer_versions() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    for version in [(1, 2, 3, 0), (1, 2, 3, 99), (1, 2, 2, 0), (0, 9, 0, 0)] {
        let image = ImageBuilder::new(version).build();
        let err = client
            .image_upload(&image, None, None, true, None)
            .unwrap_err();
        assert_image_error(err, img_mgmt_err::CURRENT_VERSION_IS_NEWER);
    }

    let image = ImageBuilder::new((1, 2, 4, 0)).build();
    client.image_upload(&image, None, None, true, None).unwrap();

    let requests = device.requests_for(group_id::IMAGE, IMAGE_UPLOAD);
    assert_eq!(requests[0].field("upgrade").unwrap().as_bool(), Some(true));
}

#[test]
fn upload_rejects_images_too_large_for_the_slot() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let image = ImageBuilder::new((2, 0, 0, 0))
        .body(vec![0; 64 * 1024])
        .build();
    let err = client
        .image_upload(&image, None, None, false, None)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_IMAGE_TOO_LARGE);
}

#[test]
fn upload_rejects_data_that_is_no_mcuboot_image() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client
        .image_upload(vec![0x12; 1000], None, None, false, None)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_IMAGE_HEADER_MAGIC);

    let err = client
        .image_upload(vec![0x12; 20], None, None, false, None)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_IMAGE_HEADER);
}

#[test]
fn test_reboot_and_confirm_a_new_image() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();
    let hash = v2().hash();

    client
        .image_upload(&image, None, None, false, None)
        .unwrap();

    let new_state = client.image_set_state(Some(&hash), false).unwrap();
    assert_eq!(
        new_state,
        [
            ImageState {
                confirmed: true,
                active: true,
                ..state(0, &v1(), "1.2.3")
            },
            ImageState {
                pending: true,
                ..state(1, &v2(), "2.0.0")
            }
        ]
    );
    assert_eq!(client.image_get_state().unwrap(), new_state);

    // Setting the same image for test again is fine
    client.image_set_state(Some(&hash), false).unwrap();

    // MCUboot swaps the images and boots the new one in test mode
    client.os_system_reset(false, None).unwrap();
    assert_eq!(
        client.image_get_state().unwrap(),
        [
            ImageState {
                active: true,
                ..state(0, &v2(), "2.0.0")
            },
            ImageState {
                confirmed: true,
                ..state(1, &v1(), "1.2.3")
            }
        ]
    );

    // Confirm the running image
    let confirmed_state = client.image_set_state(None, true).unwrap();
    assert_eq!(
        confirmed_state,
        [
            ImageState {
                confirmed: true,
                active: true,
                ..state(0, &v2(), "2.0.0")
            },
            state(1, &v1(), "1.2.3")
        ]
    );

    client.os_system_reset(false, None).unwrap();
    assert_eq!(client.image_get_state().unwrap(), confirmed_state);
}

#[test]
fn an_unconfirmed_image_is_reverted() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
    client.image_set_state(Some(&v2().hash()), false).unwrap();
    client.os_system_reset(false, None).unwrap();
    assert_eq!(client.image_get_state().unwrap()[0].version, "2.0.0");

    // While the test boot is not confirmed, the old image must stay around
    let err = client
        .image_upload(
            ImageBuilder::new((3, 0, 0, 0)).build(),
            None,
            None,
            false,
            None,
        )
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::NO_FREE_SLOT);
    let err = client.image_erase(None).unwrap_err();
    assert_image_error(err, img_mgmt_err::NO_FREE_SLOT);

    client.os_system_reset(false, None).unwrap();
    assert_eq!(
        client.image_get_state().unwrap(),
        [
            ImageState {
                confirmed: true,
                active: true,
                ..state(0, &v1(), "1.2.3")
            },
            state(1, &v2(), "2.0.0")
        ]
    );
}

#[test]
fn a_permanent_update_needs_no_confirmation() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
    let new_state = client.image_set_state(Some(&v2().hash()), true).unwrap();
    assert_eq!(
        new_state[1],
        ImageState {
            pending: true,
            permanent: true,
            ..state(1, &v2(), "2.0.0")
        }
    );

    client.os_system_reset(false, None).unwrap();
    client.os_system_reset(false, None).unwrap();
    assert_eq!(
        client.image_get_state().unwrap(),
        [
            ImageState {
                confirmed: true,
                active: true,
                ..state(0, &v2(), "2.0.0")
            },
            state(1, &v1(), "1.2.3")
        ]
    );
}

#[test]
fn set_state_errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client
        .image_set_state(Some(&[0x11; 32]), false)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::HASH_NOT_FOUND);

    let err = client.image_set_state(Some(&[0x11; 16]), true).unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_HASH);

    let err = client.image_set_state(None, false).unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_HASH);

    let err = client
        .image_set_state(Some(&v1().hash()), false)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::IMAGE_SETTING_TEST_TO_ACTIVE_DENIED);

    // Confirming what already runs confirmed is fine
    client.image_set_state(Some(&v1().hash()), true).unwrap();
    client.image_set_state(None, true).unwrap();

    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
    client.image_set_state(Some(&v2().hash()), false).unwrap();
    // A test is pending, the running image can not be confirmed now
    let err = client.image_set_state(None, true).unwrap_err();
    assert_image_error(err, img_mgmt_err::IMAGE_ALREADY_PENDING);
}

#[test]
fn set_state_fails_for_a_slot_with_a_corrupted_trailer() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
    device.lock().img.slots[1].trailer.magic = Magic::Bad;

    let err = client
        .image_set_state(Some(&v2().hash()), false)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_IMAGE_HEADER_MAGIC);
}

#[test]
fn erase() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
    assert_eq!(client.image_get_state().unwrap().len(), 2);

    client.image_erase(None).unwrap();
    assert_eq!(client.image_get_state().unwrap().len(), 1);
    assert!(device.lock().img.slots[1].flash.iter().all(|b| *b == 0xff));

    // Erasing an empty slot is fine
    client.image_erase(Some(1)).unwrap();

    let err = client.image_erase(Some(0)).unwrap_err();
    assert_image_error(err, img_mgmt_err::NO_FREE_SLOT);

    let err = client.image_erase(Some(7)).unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_SLOT);

    let requests = device.requests_for(group_id::IMAGE, 5);
    assert!(requests[0].field("slot").is_none());
    assert_eq!(
        requests[1].field("slot").unwrap().as_integer(),
        Some(1.into())
    );
}

#[test]
fn erase_a_pending_image() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
    client.image_set_state(Some(&v2().hash()), false).unwrap();

    // CONFIG_MCUMGR_GRP_IMG_ALLOW_ERASE_PENDING
    client.image_erase(None).unwrap();
    client.os_system_reset(false, None).unwrap();
    assert_eq!(client.image_get_state().unwrap()[0].version, "1.2.3");
}

#[test]
fn erase_a_pending_image_when_not_allowed() {
    let device = SimDevice::new(Config {
        img_allow_erase_pending: false,
        ..Default::default()
    });
    device.lock().flash_image(0, &v1().build());
    let client = device.client();

    client
        .image_upload(v2().build(), None, None, false, None)
        .unwrap();
    client.image_set_state(Some(&v2().hash()), false).unwrap();

    let err = client.image_erase(None).unwrap_err();
    assert_image_error(err, img_mgmt_err::NO_FREE_SLOT);
}

#[test]
fn slot_info() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(
        client.image_slot_info().unwrap(),
        [SlotInfoImage {
            image: 0,
            slots: vec![
                SlotInfoImageSlot {
                    slot: 0,
                    size: 65536,
                    upload_image_id: None
                },
                SlotInfoImageSlot {
                    slot: 1,
                    size: 65536,
                    upload_image_id: Some(0)
                }
            ],
            max_image_size: None
        }]
    );
}

#[test]
fn slot_info_with_max_image_size() {
    let device = SimDevice::new(Config {
        img_too_large_sysbuild_footer: Some(0x2000),
        ..Default::default()
    });
    let client = device.client();

    let info = client.image_slot_info().unwrap();
    assert_eq!(info[0].max_image_size, Some(65536 - 0x2000));
}

#[test]
fn slot_info_not_enabled() {
    let device = SimDevice::new(Config {
        img_slot_info: false,
        ..Default::default()
    });
    let client = device.client();

    assert!(
        client
            .image_slot_info()
            .unwrap_err()
            .command_not_supported()
    );
}

#[test]
fn multiple_images() {
    let device = SimDevice::new(Config {
        img_updatable_image_number: 2,
        ..Default::default()
    });
    let net_core = ImageBuilder::new((0, 9, 1, 0)).body(vec![0x77; 2000]);
    device.lock().flash_image(0, &v1().build());
    device.lock().flash_image(1, &net_core.build());
    let client = device.client();

    let initial = client.image_get_state().unwrap();
    assert_eq!(initial.len(), 2);
    assert_eq!((initial[1].image, initial[1].slot), (1, 0));
    assert_eq!(initial[1].version, "0.9.1");
    assert!(initial[1].active);

    let update = ImageBuilder::new((0, 9, 2, 0)).body(vec![0x78; 2000]);
    client
        .image_upload(update.build(), Some(1), None, false, None)
        .unwrap();
    let requests = device.requests_for(group_id::IMAGE, IMAGE_UPLOAD);
    assert_eq!(
        requests[0].field("image").unwrap().as_integer(),
        Some(1.into())
    );

    let new_state = client.image_set_state(Some(&update.hash()), false).unwrap();
    assert_eq!(new_state.len(), 3);
    assert_eq!(
        new_state[2],
        ImageState {
            image: 1,
            pending: true,
            ..state(1, &update, "0.9.2")
        }
    );

    client.os_system_reset(false, None).unwrap();
    let new_state = client.image_get_state().unwrap();
    assert_eq!(new_state[1].version, "0.9.2");
    assert_eq!((new_state[1].image, new_state[1].slot), (1, 0));
    assert!(new_state[1].active && !new_state[1].confirmed);
    assert_eq!(new_state[0].version, "1.2.3");

    let info = client.image_slot_info().unwrap();
    assert_eq!(info.len(), 2);
    assert_eq!(info[1].image, 1);
    assert_eq!(info[1].slots[1].upload_image_id, Some(1));

    let err = client
        .image_upload(update.build(), Some(2), None, false, None)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::NO_FREE_SLOT);
}

#[test]
fn uploading_an_empty_image_fails() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // An empty image can not be valid; Zephyr rejects anything shorter than
    // an image header with IMG_MGMT_ERR_INVALID_IMAGE_HEADER.
    let result = client.image_upload(Vec::<u8>::new(), None, None, false, None);
    assert!(result.is_err(), "uploading 0 bytes reported success");
}

/// Data starting with a valid image header, of exactly `len` bytes
fn upload_data(len: usize) -> Vec<u8> {
    let mut data = ImageBuilder::new((2, 0, 0, 0))
        .body(vec![0x3c; len])
        .build();
    data.truncate(len);
    data.iter_mut()
        .enumerate()
        .skip(32)
        .for_each(|(i, b)| *b = (i % 253) as u8);
    data
}

#[test]
fn upload_sizes_around_the_chunk_sizes() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .image_upload(upload_data(5000), None, None, false, None)
        .unwrap();
    let requests = device.requests_for(group_id::IMAGE, IMAGE_UPLOAD);
    let chunk_len = |i: usize| requests[i].field("data").unwrap().as_bytes().unwrap().len();
    let (first, other) = (chunk_len(0), chunk_len(1));
    assert!(first < other);

    for len in [
        32,
        first - 1,
        first,
        first + 1,
        first + other - 1,
        first + other,
        first + other + 1,
        first + 5 * other,
    ] {
        client.image_erase(None).unwrap();
        device.clear_requests();
        let data = upload_data(len);

        let mut progress = vec![];
        let mut callback = |current: u64, _: u64| {
            progress.push(current);
            true
        };
        client
            .image_upload(&data, None, None, false, Some(&mut callback))
            .unwrap();

        assert_eq!(
            device.lock().img.slots[1].flash[..len],
            data[..],
            "length {len}"
        );
        let expected_requests = 1 + (len.saturating_sub(first)).div_ceil(other);
        assert_eq!(
            device.requests_for(group_id::IMAGE, IMAGE_UPLOAD).len(),
            expected_requests,
            "length {len}"
        );
        assert_eq!(progress.last(), Some(&(len as u64)), "length {len}");
    }
}

#[test]
fn upload_an_image_that_fills_the_slot_exactly() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .image_upload(upload_data(65536), None, None, false, None)
        .unwrap();
    client.image_erase(None).unwrap();

    let err = client
        .image_upload(upload_data(65537), None, None, false, None)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_IMAGE_TOO_LARGE);
}

#[test]
fn upload_restarts_when_the_device_reboots_in_between() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();

    // The device loses its upload state in the middle of the transfer and
    // answers the next chunk with offset 0.
    let mut rebooted = false;
    let mut reboot_midway = |current: u64, _: u64| {
        if current > 4000 && !rebooted {
            device.lock().reboot();
            rebooted = true;
        }
        true
    };
    client
        .image_upload(&image, None, None, false, Some(&mut reboot_midway))
        .unwrap();

    assert!(rebooted);
    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);
    let first_chunks = device
        .requests_for(group_id::IMAGE, IMAGE_UPLOAD)
        .iter()
        .filter(|r| r.field("len").is_some())
        .count();
    assert_eq!(first_chunks, 2);
}

#[test]
fn set_state_with_an_empty_hash() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // Zephyr treats an empty hash like a missing one
    client.image_set_state(Some(&[]), true).unwrap();
    let err = client.image_set_state(Some(&[]), false).unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_HASH);
}

#[test]
fn erase_while_an_upload_is_in_progress() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = v2().build();

    let mut abort = |current: u64, _: u64| current < 3000;
    client
        .image_upload(&image, None, None, false, Some(&mut abort))
        .unwrap_err();

    // Erasing also forgets the upload, so the next upload starts over
    client.image_erase(None).unwrap();
    device.clear_requests();
    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    let requests = device.requests_for(group_id::IMAGE, IMAGE_UPLOAD);
    assert!(
        requests[1..]
            .iter()
            .all(|r| r.field("off").unwrap().as_integer() != Some(0.into()))
    );
    assert_eq!(
        requests[1].field("off").unwrap().as_integer(),
        Some(
            requests[0]
                .field("data")
                .unwrap()
                .as_bytes()
                .unwrap()
                .len()
                .into()
        )
    );
    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);
}

#[test]
fn upload_leaves_room_for_the_update_footer() {
    // CONFIG_MCUMGR_GRP_IMG_TOO_LARGE_SYSBUILD with equally sized slots
    let device = SimDevice::new(Config {
        img_too_large_sysbuild_footer: Some(0x1000),
        ..Default::default()
    });
    device.lock().flash_image(0, &v1().build());
    let client = device.client();

    client
        .image_upload(upload_data(65536 - 0x1000), None, None, false, None)
        .unwrap();
    client.image_erase(None).unwrap();

    let err = client
        .image_upload(upload_data(65536 - 0x1000 + 1), None, None, false, None)
        .unwrap_err();
    assert_image_error(err, img_mgmt_err::INVALID_IMAGE_TOO_LARGE);
}
