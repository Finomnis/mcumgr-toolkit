//! Behavior of `MCUmgrClient` itself: framing, configuration and errors.

use std::time::Duration;

use mcumgr_toolkit::MCUmgrClient;
use mcumgr_toolkit::client::MCUmgrClientError;
use mcumgr_toolkit::commands::McuMgrCommand;
use serde::{Deserialize, Serialize};

use super::{assert_timeout, device_error, smp_error};
use crate::sim::image::ImageBuilder;
use crate::sim::smp::{group_id, mgmt_err, op};
use crate::sim::{Config, SimDevice};

#[test]
fn new_from_transport_connects_to_the_device() {
    let device = SimDevice::with_firmware();
    let client = MCUmgrClient::new_from_transport(device.transport());

    assert_eq!(client.os_echo("Hello Zephyr!").unwrap(), "Hello Zephyr!");
    client.check_connection().unwrap();
}

#[test]
fn new_from_transport_accepts_a_boxed_transport() {
    let device = SimDevice::with_firmware();
    let transport: Box<dyn mcumgr_toolkit::transport::Transport + Send> =
        Box::new(device.transport());
    let client = MCUmgrClient::new_from_transport(transport);

    assert_eq!(client.os_echo("boxed").unwrap(), "boxed");
}

#[test]
fn requests_use_smp_version_2_headers_with_consecutive_sequence_numbers() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client.os_echo("a").unwrap();
    client.os_mcumgr_parameters().unwrap();
    client.settings_commit().unwrap();

    let requests = device.requests();
    assert_eq!(requests.len(), 3);

    for request in &requests {
        // SMP_MCUMGR_VERSION_2, no flags
        assert_eq!(request.hdr.version, 1);
        assert_eq!(request.hdr.flags, 0);
        assert_eq!(request.hdr.len as usize + 8, request.frame_len);
    }

    assert_eq!(
        (requests[0].hdr.group, requests[0].hdr.id),
        (group_id::OS, 0)
    );
    assert_eq!(requests[0].hdr.op, op::READ);
    assert_eq!(
        (requests[1].hdr.group, requests[1].hdr.id),
        (group_id::OS, 6)
    );
    assert_eq!(requests[1].hdr.op, op::READ);
    assert_eq!(
        (requests[2].hdr.group, requests[2].hdr.id),
        (group_id::SETTINGS, 2)
    );
    assert_eq!(requests[2].hdr.op, op::WRITE);

    let seq = requests[0].hdr.seq;
    assert_eq!(requests[1].hdr.seq, seq.wrapping_add(1));
    assert_eq!(requests[2].hdr.seq, seq.wrapping_add(2));
}

#[test]
fn sequence_numbers_wrap_around() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    for i in 0..300 {
        assert_eq!(client.os_echo(i.to_string()).unwrap(), i.to_string());
    }

    let requests = device.requests();
    for pair in requests.windows(2) {
        assert_eq!(pair[1].hdr.seq, pair[0].hdr.seq.wrapping_add(1));
    }
}

#[test]
fn set_timeout_is_forwarded_to_the_transport() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client.set_timeout(Duration::from_millis(1234)).unwrap();
    client.set_timeout(Duration::from_secs(10)).unwrap();

    assert_eq!(
        device.lock().link.timeouts,
        [Duration::from_millis(1234), Duration::from_secs(10)]
    );
}

#[test]
fn responses_that_do_not_fit_the_device_buffer_are_reported_as_emsgsize() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(0);

    // The request map is definite-length, the response map indefinite-length
    // (one byte longer), so a request that just fits produces a response that
    // does not fit into CONFIG_MCUMGR_TRANSPORT_NETBUF_SIZE (384).
    let fits = "x".repeat(369);
    assert_eq!(client.os_echo(&fits).unwrap(), fits);

    let err = client.os_echo("x".repeat(370)).unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EMSGSIZE));

    // A request larger than the buffer is dropped by the device
    let err = client.os_echo("x".repeat(400)).unwrap_err();
    assert_timeout(err);
    assert_eq!(device.lock().link.dropped_oversized, 1);
}

#[test]
fn commands_of_disabled_groups_are_not_supported() {
    let device = SimDevice::new(Config {
        mcumgr_grp_shell: false,
        mcumgr_grp_zephyr_basic: false,
        ..Default::default()
    });
    let client = device.client();

    let err = client
        .shell_execute(&["kernel".into(), "version".into()], true)
        .unwrap_err();
    assert!(err.command_not_supported());
    assert_eq!(device_error(err), smp_error(mgmt_err::ENOTSUP));

    let err = client.zephyr_erase_storage().unwrap_err();
    assert!(err.command_not_supported());

    // Unsupported commands are not retried
    assert_eq!(device.requests().len(), 2);
}

#[test]
fn group_errors_do_not_count_as_unsupported_commands() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client.stats_get_group_data("no_such_group").unwrap_err();
    assert!(!err.command_not_supported());
    assert!(!MCUmgrClientError::ProgressCallbackError.command_not_supported());
}

/// A command defined outside of this crate
#[derive(Serialize)]
struct CustomEcho<'a> {
    d: &'a str,
}

#[derive(Deserialize, Debug, PartialEq)]
struct CustomEchoResponse {
    r: String,
}

impl McuMgrCommand for CustomEcho<'_> {
    type Payload = Self;
    type Response = CustomEchoResponse;

    fn is_write_operation(&self) -> bool {
        true
    }

    fn group_id(&self) -> u16 {
        0
    }

    fn command_id(&self) -> u8 {
        0
    }

    fn data(&self) -> &Self::Payload {
        self
    }
}

/// A command of a user defined group the device does not know
#[derive(Serialize)]
struct UserGroupCommand {}

impl McuMgrCommand for UserGroupCommand {
    type Payload = Self;
    type Response = CustomEchoResponse;

    fn is_write_operation(&self) -> bool {
        false
    }

    fn group_id(&self) -> u16 {
        64
    }

    fn command_id(&self) -> u8 {
        3
    }

    fn data(&self) -> &Self::Payload {
        self
    }
}

#[test]
fn raw_command_executes_user_defined_commands() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let response = client.raw_command(&CustomEcho { d: "custom" }).unwrap();
    assert_eq!(response, CustomEchoResponse { r: "custom".into() });

    let request = device.requests().pop().unwrap();
    assert_eq!(request.hdr.op, op::WRITE);

    let err = client.raw_command(&UserGroupCommand {}).unwrap_err();
    assert!(err.command_not_supported());
}

#[test]
fn raw_command_executes_built_in_commands() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let response = client
        .raw_command(&mcumgr_toolkit::commands::os::Echo { d: "built-in" })
        .unwrap();
    assert_eq!(response.r, "built-in");
}

#[test]
fn use_auto_frame_size_adopts_the_device_buffer_size() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 2048,
        ..Default::default()
    });
    device
        .lock()
        .flash_image(0, &ImageBuilder::new((1, 0, 0, 0)).build());
    let client = device.client();
    let image = ImageBuilder::new((2, 0, 0, 0))
        .body(vec![0x42; 20_000])
        .build();

    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    let default_requests = device.requests_for(group_id::IMAGE, 1);
    assert!(default_requests.iter().all(|r| r.frame_len <= 384));

    client.image_erase(None).unwrap();
    device.clear_requests();

    client.use_auto_frame_size().unwrap();
    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    let auto_requests = device.requests_for(group_id::IMAGE, 1);
    assert!(auto_requests.iter().all(|r| r.frame_len <= 2048));
    assert!(auto_requests.iter().any(|r| r.frame_len > 1500));
    assert!(auto_requests.len() * 4 < default_requests.len());
    assert_eq!(device.lock().link.dropped_oversized, 0);
}

#[test]
fn use_auto_frame_size_is_limited_by_the_transport() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 4096,
        ..Default::default()
    });
    device.lock().link.max_smp_frame_size = Some(1000);
    let client = device.client();

    client.use_auto_frame_size().unwrap();
    client
        .fs_file_upload("/lfs1/big.bin", &[7u8; 10_000][..], 10_000, None)
        .unwrap();

    let requests = device.requests_for(group_id::FS, 0);
    assert!(requests.iter().all(|r| r.frame_len <= 1000));
    assert!(requests.iter().any(|r| r.frame_len > 900));
    assert_eq!(device.lock().fs.files["/lfs1/big.bin"], vec![7u8; 10_000]);
}

#[test]
fn set_frame_size_controls_the_chunk_size() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 1024,
        ..Default::default()
    });
    let client = device.client();

    client.set_frame_size(1024);
    client
        .fs_file_upload("/lfs1/a.bin", &[1u8; 5000][..], 5000, None)
        .unwrap();
    let requests = device.requests_for(group_id::FS, 0);
    assert!(requests.iter().all(|r| r.frame_len <= 1024));
    assert!(requests.iter().any(|r| r.frame_len > 1000));

    // Frames larger than the device's buffer get dropped by the device
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_retries(2);
    client.set_frame_size(1024);
    let err = client
        .fs_file_upload("/lfs1/a.bin", &[1u8; 5000][..], 5000, None)
        .unwrap_err();
    assert_timeout(err);
    assert_eq!(device.lock().link.dropped_oversized, 3);
}

#[test]
fn a_frame_size_too_small_for_any_data_is_rejected_by_the_client() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    client.set_frame_size(40);

    let image = ImageBuilder::new((2, 0, 0, 0)).build();
    let err = client
        .image_upload(&image, None, None, false, None)
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::FrameSizeTooSmall(_)));

    let err = client
        .fs_file_upload("/lfs1/a.bin", &[1u8; 10][..], 10, None)
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::FrameSizeTooSmall(_)));

    assert!(device.requests().is_empty());
}
