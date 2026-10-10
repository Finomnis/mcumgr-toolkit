use crate::{
    assert_group_error,
    zephyr_sim::{
        Config, Fault, client, client_with,
        firmware::{firmware, image_hash, sha256},
        wire::{map, number, uint},
    },
};
use mcumgr_toolkit::client::MCUmgrClientError;

#[test]
fn image_state_supports_omitted_image_and_false_flags() {
    for compact_images in [false, true] {
        let (client, handle) = client_with(Config {
            compact_images,
            ..Config::default()
        });
        let states = client.image_get_state().unwrap();
        assert_eq!(states.len(), 1);
        let state = &states[0];
        assert_eq!(
            (state.image, state.slot, state.version.as_str()),
            (0, 0, "1.2.3.4")
        );
        assert_eq!(state.hash, handle.inspect(|d| image_hash(&d.slots[0].data)));
        assert!(state.bootable && state.active && state.confirmed);
        assert!(!state.pending && !state.permanent);
    }
}

#[test]
fn image_upload_checks_all_bytes_hash_metadata_offsets_and_progress() {
    for supplied_checksum in [false, true] {
        let (client, handle) = client();
        client.set_frame_size(192);
        let data = firmware(2, 1100);
        let mut progress = Vec::new();
        client
            .image_upload(
                &data,
                Some(0),
                supplied_checksum.then(|| sha256(&data)),
                true,
                Some(&mut |done, total| {
                    progress.push((done, total));
                    true
                }),
            )
            .unwrap();
        let uploaded = handle.inspect(|d| d.last_upload.clone().unwrap());
        assert_eq!(uploaded.data, data);
        assert_eq!(uploaded.sha, sha256(&data));
        assert_eq!((uploaded.image, uploaded.size), (0, data.len()));
        assert!(
            progress
                .iter()
                .all(|(done, total)| *done <= *total && *total == data.len() as u64)
        );
        assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert_eq!(
            progress.last(),
            Some(&(data.len() as u64, data.len() as u64))
        );
        let requests = handle.requests();
        assert!(requests.len() > 2);
        assert!(
            requests
                .iter()
                .all(|r| (r.group(), r.id(), r.op()) == (1, 1, 2) && r.frame_len() <= 192)
        );
        assert_eq!(
            requests[0].get("len").and_then(number),
            Some(data.len() as u64)
        );
        assert_eq!(requests[0].get("image").and_then(number), Some(0));
        assert_eq!(
            requests[0].get("sha").unwrap().as_bytes().unwrap(),
            &sha256(&data)
        );
        assert_eq!(requests[0].get("upgrade").unwrap().as_bool(), Some(true));
        let mut off = 0;
        for request in &requests {
            assert_eq!(request.get("off").and_then(number), Some(off));
            let chunk = request.get("data").unwrap().as_bytes().unwrap();
            assert_eq!(chunk, &data[off as usize..off as usize + chunk.len()]);
            off += chunk.len() as u64;
        }
        assert_eq!(off, data.len() as u64);
        let states = client.image_get_state().unwrap();
        let new = states.iter().find(|s| s.slot == 1).unwrap();
        assert_eq!(new.version, "2.2.3.4");
        assert_eq!(new.hash, image_hash(&data));
        assert!(!new.pending && !new.active && !new.confirmed && !new.permanent);
    }
}

#[test]
fn image_upload_without_device_hash_verification() {
    let (client, handle) = client_with(Config {
        image_check: false,
        ..Config::default()
    });
    let data = firmware(2, 500);
    client.image_upload(&data, None, None, false, None).unwrap();
    assert_eq!(
        handle.inspect(|d| d.last_upload.as_ref().unwrap().data.clone()),
        data
    );
}

#[test]
fn image_upload_rejects_incorrect_local_checksum_before_sending() {
    let (client, handle) = client();
    assert!(matches!(
        client.image_upload(firmware(2, 400), None, Some([0; 32]), false, None),
        Err(MCUmgrClientError::ChecksumMismatch)
    ));
    assert!(handle.requests().is_empty());
}

#[test]
fn image_upload_detects_corruption_reported_by_device() {
    let (client, handle) = client();
    handle.edit(|d| d.corrupt_upload = true);
    assert!(matches!(
        client.image_upload(firmware(2, 400), None, None, false, None),
        Err(MCUmgrClientError::ChecksumMismatchOnDevice)
    ));
}

#[test]
fn image_upload_resumes_a_cancelled_session_using_full_file_hash() {
    let (client, handle) = client();
    client.set_frame_size(192);
    let data = firmware(2, 900);
    let mut cancel_after_data = |done, _| done == 0;
    assert!(matches!(
        client.image_upload(&data, None, None, false, Some(&mut cancel_after_data)),
        Err(MCUmgrClientError::ProgressCallbackError)
    ));
    let saved_offset = handle.inspect(|d| d.upload.as_ref().unwrap().data.len());
    assert!(saved_offset > 0 && saved_offset < data.len());
    let before = handle.requests().len();
    client.image_upload(&data, None, None, false, None).unwrap();
    let requests = handle.requests();
    assert_eq!(requests[before].get("off").and_then(number), Some(0));
    assert_eq!(
        requests[before + 1].get("off").and_then(number),
        Some(saved_offset as u64)
    );
    assert_eq!(
        handle.inspect(|d| d.last_upload.as_ref().unwrap().data.clone()),
        data
    );
}

#[test]
fn image_upload_recovers_from_lost_first_and_middle_replies() {
    for skip in [0, 2] {
        let (client, handle) = client();
        client.set_frame_size(192);
        client.set_retries(1);
        handle.fault_on(1, 1, skip, Fault::LoseReply);
        let data = firmware(2, 1500);
        client.image_upload(&data, None, None, false, None).unwrap();
        assert_eq!(
            handle.inspect(|d| d.last_upload.as_ref().unwrap().data.clone()),
            data
        );
        handle.assert_faults_consumed();
    }
}

#[test]
fn changed_hash_starts_new_image_session() {
    let (client, handle) = client();
    client.set_frame_size(192);
    let first = firmware(2, 900);
    let second = firmware(3, 600);
    assert!(
        client
            .image_upload(&first, None, None, false, Some(&mut |done, _| done == 0))
            .is_err()
    );
    client
        .image_upload(&second, None, None, false, None)
        .unwrap();
    assert_eq!(
        handle.inspect(|d| d.last_upload.as_ref().unwrap().data.clone()),
        second
    );
}

#[test]
fn image_upload_rejects_invalid_header_slot_and_downgrade() {
    let (client, _) = client();
    assert_group_error(
        client
            .image_upload([0u8; 100], None, None, false, None)
            .unwrap_err(),
        1,
        23,
    );
    assert_group_error(
        client
            .image_upload(firmware(2, 500), Some(99), None, false, None)
            .unwrap_err(),
        1,
        9,
    );
    assert_group_error(
        client
            .image_upload(firmware(1, 500), None, None, true, None)
            .unwrap_err(),
        1,
        27,
    );
    client
        .image_upload(firmware(1, 500), None, None, false, None)
        .unwrap();
}

#[test]
fn image_upload_rejects_out_of_bounds_acknowledgement() {
    let (client, handle) = client();
    handle.fault(Fault::Reply(map([("off", uint(999_999u32))])));
    assert!(matches!(
        client.image_upload(firmware(2, 400), None, None, false, None),
        Err(MCUmgrClientError::UnexpectedOffset)
    ));
}

#[test]
fn image_frame_too_small_fails_without_sending() {
    let (client, handle) = client();
    client.set_frame_size(8);
    assert!(matches!(
        client.image_upload(firmware(2, 300), None, None, false, None),
        Err(MCUmgrClientError::FrameSizeTooSmall(_))
    ));
    assert!(handle.requests().is_empty());
}

#[test]
fn image_test_boot_confirm_and_revert() {
    for confirm_after_boot in [false, true] {
        let (client, _) = client();
        let data = firmware(2, 500);
        client.image_upload(&data, None, None, false, None).unwrap();
        let hash = image_hash(&data).unwrap();
        let states = client.image_set_state(Some(&hash), false).unwrap();
        let pending = states
            .iter()
            .find(|s| s.hash.as_deref() == Some(&hash))
            .unwrap();
        assert!(pending.pending && !pending.active && !pending.permanent);
        client.os_system_reset(false, None).unwrap();
        let running = client
            .image_get_state()
            .unwrap()
            .into_iter()
            .find(|s| s.active)
            .unwrap();
        assert_eq!(running.hash.as_deref(), Some(hash.as_slice()));
        assert!(!running.confirmed);
        if confirm_after_boot {
            let states = client.image_set_state(None, true).unwrap();
            assert!(states.iter().find(|s| s.active).unwrap().confirmed);
        }
        client.os_system_reset(false, None).unwrap();
        let running = client
            .image_get_state()
            .unwrap()
            .into_iter()
            .find(|s| s.active)
            .unwrap();
        assert_eq!(
            running.version,
            if confirm_after_boot {
                "2.2.3.4"
            } else {
                "1.2.3.4"
            }
        );
    }
}

#[test]
fn image_permanent_activation() {
    let (client, _) = client();
    let data = firmware(2, 500);
    client.image_upload(&data, None, None, false, None).unwrap();
    let states = client
        .image_set_state(image_hash(&data).as_deref(), true)
        .unwrap();
    let pending = states.iter().find(|s| s.pending).unwrap();
    assert!(pending.permanent);
    client.os_system_reset(false, None).unwrap();
    let states = client.image_get_state().unwrap();
    assert!(states.iter().find(|s| s.active).unwrap().confirmed);
}

#[test]
fn image_state_errors_preserve_group_error_codes() {
    let (client, _) = client();
    assert_group_error(client.image_set_state(None, false).unwrap_err(), 1, 24);
    assert_group_error(
        client.image_set_state(Some(&[1, 2]), false).unwrap_err(),
        1,
        24,
    );
    assert_group_error(
        client.image_set_state(Some(&[0; 32]), true).unwrap_err(),
        1,
        8,
    );
    let state = client.image_get_state().unwrap().remove(0);
    assert_group_error(
        client
            .image_set_state(state.hash.as_deref(), false)
            .unwrap_err(),
        1,
        33,
    );
}

#[test]
fn image_erase_default_explicit_and_protected_slot() {
    let (client, _) = client();
    assert_group_error(client.image_erase(Some(0)).unwrap_err(), 1, 9);
    assert_group_error(client.image_erase(Some(100)).unwrap_err(), 1, 14);
    for slot in [None, Some(1)] {
        client
            .image_upload(firmware(2, 300), None, None, false, None)
            .unwrap();
        assert_eq!(client.image_get_state().unwrap().len(), 2);
        client.image_erase(slot).unwrap();
        assert_eq!(client.image_get_state().unwrap().len(), 1);
    }
}

#[test]
fn image_slot_info_and_multi_image_upload_selection() {
    let (client, handle) = client_with(Config {
        image_count: 2,
        ..Config::default()
    });
    let info = client.image_slot_info().unwrap();
    assert_eq!(info.len(), 2);
    for (index, image) in info.iter().enumerate() {
        assert_eq!(image.image, index as u32);
        assert_eq!(image.max_image_size, Some(65_024));
        assert_eq!(image.slots.len(), 2);
        assert_eq!(
            (
                image.slots[0].slot,
                image.slots[0].size,
                image.slots[0].upload_image_id
            ),
            (0, 65_536, None)
        );
        assert_eq!(
            (
                image.slots[1].slot,
                image.slots[1].size,
                image.slots[1].upload_image_id
            ),
            (1, 65_536, Some(index as u32))
        );
    }
    let data = firmware(3, 512);
    client
        .image_upload(&data, Some(1), None, false, None)
        .unwrap();
    assert_eq!(handle.inspect(|d| d.last_upload.as_ref().unwrap().image), 1);
    let states = client.image_get_state().unwrap();
    assert!(
        states
            .iter()
            .any(|s| s.image == 1 && s.slot == 1 && s.version == "3.2.3.4")
    );
    assert!(!states.iter().any(|s| s.image == 0 && s.slot == 1));
}
