//! Behavior on an unreliable link: retries, timeouts and unexpected frames.

use mcumgr_toolkit::client::MCUmgrClientError;
use mcumgr_toolkit::connection::ExecuteError;
use mcumgr_toolkit::transport::{ReceiveError, SendError};

use super::{assert_timeout, device_error, group_error};
use crate::sim::fs_mgmt::fs_mgmt_err;
use crate::sim::image::ImageBuilder;
use crate::sim::smp::group_id;
use crate::sim::{Fault, SimDevice};

#[test]
fn lost_responses_are_retried_with_a_new_sequence_number() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    device.inject_faults([Fault::DropResponse, Fault::DropResponse]);
    assert_eq!(client.os_echo("retry me").unwrap(), "retry me");

    let requests = device.requests();
    assert_eq!(requests.len(), 3);
    assert_ne!(requests[0].hdr.seq, requests[1].hdr.seq);
    assert_ne!(requests[1].hdr.seq, requests[2].hdr.seq);
}

#[test]
fn lost_requests_are_retried() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    device.inject_faults([Fault::DropRequest]);
    assert_eq!(client.os_echo("again").unwrap(), "again");
    assert_eq!(device.requests().len(), 1);
}

#[test]
fn a_timeout_is_reported_once_all_retries_are_used_up() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(3);

    device.inject_faults([Fault::DropResponse; 4]);
    assert_timeout(client.os_echo("lost").unwrap_err());
    assert_eq!(device.requests().len(), 4);

    // The link works again afterwards
    assert_eq!(client.os_echo("back").unwrap(), "back");
}

#[test]
fn retries_can_be_disabled() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(0);

    device.inject_faults([Fault::DropRequest]);
    assert_timeout(client.os_echo("once").unwrap_err());
    assert!(device.requests().is_empty());
}

#[test]
fn responses_with_other_sequence_numbers_are_skipped() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(0);

    device.inject_faults([Fault::StaleResponse]);
    assert_eq!(client.os_echo("fresh").unwrap(), "fresh");
    assert_eq!(device.requests().len(), 1);
}

#[test]
fn a_late_response_to_an_earlier_attempt_is_skipped() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    device.inject_faults([Fault::LateResponse]);
    assert_eq!(client.os_echo("late").unwrap(), "late");

    // The first attempt timed out; its response arrived together with the
    // response to the retry and was discarded.
    assert_eq!(device.requests().len(), 2);
    assert_eq!(client.os_echo("next").unwrap(), "next");
}

#[test]
fn a_response_for_another_command_is_an_unexpected_response() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(0);

    device.inject_faults([Fault::WrongGroup]);
    let err = client.os_echo("wrong").unwrap_err();
    assert!(matches!(
        err,
        MCUmgrClientError::ExecuteError(ExecuteError::ReceiveFailed(
            ReceiveError::UnexpectedResponse
        ))
    ));

    // ... which is retried
    client.set_retries(1);
    device.inject_faults([Fault::WrongGroup]);
    assert_eq!(client.os_echo("right").unwrap(), "right");
}

#[test]
fn a_truncated_response_is_an_unexpected_response() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(0);

    device.inject_faults([Fault::TruncatedResponse]);
    let err = client.os_echo("short").unwrap_err();
    assert!(matches!(
        err,
        MCUmgrClientError::ExecuteError(ExecuteError::ReceiveFailed(
            ReceiveError::UnexpectedResponse
        ))
    ));
}

#[test]
fn transport_send_errors_are_reported_and_retried() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(0);

    device.inject_faults([Fault::SendError]);
    let err = client.os_echo("fail").unwrap_err();
    assert!(matches!(
        err,
        MCUmgrClientError::ExecuteError(ExecuteError::SendFailed(SendError::TransportError(_)))
    ));

    client.set_retries(1);
    device.inject_faults([Fault::SendError]);
    assert_eq!(client.os_echo("works").unwrap(), "works");
}

#[test]
fn transfers_survive_an_unreliable_link() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let image = ImageBuilder::new((2, 0, 0, 0))
        .body((0..20_000u32).map(|i| (i * 7 % 251) as u8).collect())
        .build();
    let file: Vec<u8> = (0..5_000u32).map(|i| (i * 13 % 241) as u8).collect();
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/data.bin".into(), file.clone());

    // Every other request something goes wrong
    let faults = [
        Fault::DropResponse,
        Fault::LateResponse,
        Fault::StaleResponse,
        Fault::DropRequest,
        Fault::SendError,
    ];
    let mut schedule = vec![];
    for fault in faults.iter().cycle().take(100) {
        schedule.push(*fault);
        schedule.push(Fault::StaleResponse);
    }
    device.lock().link.faults = schedule.into();

    // A lost image upload response makes the client repeat the chunk; the
    // device answers with the offset it expects and the upload continues.
    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);

    let mut downloaded = vec![];
    client
        .fs_file_download("/lfs1/data.bin", &mut downloaded, None)
        .unwrap();
    assert_eq!(downloaded, file);
}

#[test]
fn a_lost_file_upload_response_aborts_the_upload() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = vec![0x55u8; 2000];

    // The second chunk is written, but its response is lost. The client sends
    // it again at the same offset, which fs_mgmt rejects as the file has
    // already grown past it.
    device.inject_faults([Fault::StaleResponse, Fault::DropResponse]);
    let err = client
        .fs_file_upload("/lfs1/data.bin", &data[..], data.len() as u64, None)
        .unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::FS, fs_mgmt_err::FILE_OFFSET_NOT_VALID)
    );
}
