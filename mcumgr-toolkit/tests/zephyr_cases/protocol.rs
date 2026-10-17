use crate::{
    assert_group_error, assert_smp_error,
    zephyr_sim::{
        Config, Fault, HeaderFault, SimulatedTransport, client, client_with,
        firmware::{crc32, firmware, image_hash, sha256},
        wire::{RemoteError, Value, bytes, lookup, map, number, text, uint},
    },
};
use mcumgr_toolkit::{
    MCUmgrClient,
    client::MCUmgrClientError,
    commands::McuMgrCommand,
    connection::ExecuteError,
    smp_errors::DeviceError,
    transport::{ReceiveError, SendError},
};
use std::sync::Arc;

struct Command {
    write: bool,
    group: u16,
    id: u8,
    payload: Value,
}
impl McuMgrCommand for Command {
    type Payload = Value;
    type Response = Value;
    fn is_write_operation(&self) -> bool {
        self.write
    }
    fn group_id(&self) -> u16 {
        self.group
    }
    fn command_id(&self) -> u8 {
        self.id
    }
    fn data(&self) -> &Value {
        &self.payload
    }
}

#[test]
fn raw_command_uses_user_owned_types_and_real_device_dispatch() {
    let (client, handle) = client();
    for write in [false, true] {
        let response = client
            .raw_command(&Command {
                write,
                group: 0,
                id: 0,
                payload: map([("d", text("raw echo"))]),
            })
            .unwrap();
        assert_eq!(lookup(&response, "r").unwrap().as_text(), Some("raw echo"));
        assert_eq!(
            handle.requests().last().unwrap().op(),
            if write { 2 } else { 0 }
        );
    }
    let error = client
        .raw_command(&Command {
            write: true,
            group: 0x1234,
            id: 0xab,
            payload: map([]),
        })
        .unwrap_err();
    assert_smp_error(error, 8);
    let req = handle.requests().pop().unwrap();
    assert_eq!(req.header[0], 0x0a);
    assert_eq!(
        &req.header[4..6],
        &[0x12, 0x34],
        "group ID is network byte order"
    );
    assert_eq!(req.header[7], 0xab);
}

#[test]
fn literal_zephyr_style_cbor_response_is_accepted() {
    let (client, handle) = client();
    // {"r": "ok"}, indefinite map as produced by non-canonical zcbor.
    handle.fault(Fault::RawBody(vec![
        0xbf, 0x61, b'r', 0x62, b'o', b'k', 0xff,
    ]));
    assert_eq!(client.os_echo("ok").unwrap(), "ok");
    // The same value as a definite-length map is also valid CBOR.
    handle.fault(Fault::RawBody(vec![0xa1, 0x61, b'r', 0x62, b'o', b'k']));
    assert_eq!(client.os_echo("ok").unwrap(), "ok");
}

#[test]
fn raw_typed_commands_cover_defaults_not_exposed_by_convenience_methods() {
    use mcumgr_toolkit::commands::{
        r#enum::GroupId,
        fs::FileDownload,
        os::{BootloaderInfoMcubootMode, Echo},
    };
    let (client, handle) = client();
    assert_eq!(client.raw_command(&Echo { d: "typed" }).unwrap().r, "typed");
    let group = client.raw_command(&GroupId { index: None }).unwrap();
    assert_eq!(group.group, 0);
    assert!(!group.end);
    let mode = client.raw_command(&BootloaderInfoMcubootMode {}).unwrap();
    assert_eq!(mode.mode, 1);
    assert!(!mode.no_downgrade);
    handle.edit(|d| {
        d.files.insert("/lfs/data".into(), b"123456789".to_vec());
    });
    let tail = client
        .raw_command(&FileDownload {
            name: "/lfs/data",
            off: 5,
        })
        .unwrap();
    assert_eq!((tail.off, tail.len, tail.data), (5, None, b"6789".to_vec()));
}

#[test]
fn retries_are_bounded_and_can_be_disabled() {
    for retries in [0, 1, 3] {
        let (client, handle) = client();
        client.set_retries(retries);
        for _ in 0..=retries {
            handle.fault(Fault::LoseRequest);
        }
        assert!(matches!(
            client.os_echo("timeout"),
            Err(MCUmgrClientError::ExecuteError(
                ExecuteError::ReceiveFailed(ReceiveError::Timeout)
            ))
        ));
        assert_eq!(handle.requests().len(), usize::from(retries) + 1);
        handle.assert_faults_consumed();
        assert_eq!(client.os_echo("recovered").unwrap(), "recovered");
    }
}

#[test]
fn transient_send_and_receive_failures_recover() {
    for fault in [
        Fault::SendTimeout,
        Fault::SendIo,
        Fault::ReceiveIo,
        Fault::LoseRequest,
        Fault::LoseReply,
    ] {
        let (client, handle) = client();
        client.set_retries(1);
        handle.fault(fault);
        assert_eq!(client.os_echo("retry").unwrap(), "retry");
        assert_eq!(handle.requests().len(), 2);
        handle.assert_faults_consumed();
    }
}

#[test]
fn transport_errors_preserve_their_categories() {
    for fault in [
        Fault::SendTimeout,
        Fault::SendIo,
        Fault::ReceiveIo,
        Fault::ReceiveTooBig,
    ] {
        let (client, handle) = client();
        handle.fault(fault.clone());
        let error = client.os_echo("failure").unwrap_err();
        let matches_category = match fault {
            Fault::SendTimeout => matches!(
                error,
                MCUmgrClientError::ExecuteError(ExecuteError::SendFailed(SendError::Timeout))
            ),
            Fault::SendIo => matches!(
                error,
                MCUmgrClientError::ExecuteError(ExecuteError::SendFailed(
                    SendError::TransportError(_)
                ))
            ),
            Fault::ReceiveIo => matches!(
                error,
                MCUmgrClientError::ExecuteError(ExecuteError::ReceiveFailed(
                    ReceiveError::TransportError(_)
                ))
            ),
            Fault::ReceiveTooBig => matches!(
                error,
                MCUmgrClientError::ExecuteError(ExecuteError::ReceiveFailed(
                    ReceiveError::FrameTooBig
                ))
            ),
            _ => unreachable!(),
        };
        assert!(matches_category, "{error:?}");
    }
}

#[test]
fn stale_sequences_are_discarded_without_resending() {
    let (client, handle) = client();
    handle.fault(Fault::StaleBeforeValid);
    assert_eq!(client.os_echo("fresh").unwrap(), "fresh");
    assert_eq!(handle.requests().len(), 1);
    assert_eq!(handle.receives(), 2);
}

#[test]
fn malformed_response_headers_are_rejected() {
    for kind in [
        HeaderFault::Operation,
        HeaderFault::Group,
        HeaderFault::Command,
        HeaderFault::Length,
        HeaderFault::Truncated,
    ] {
        let (client, handle) = client();
        handle.fault(Fault::Header(kind));
        assert!(matches!(
            client.os_echo("bad header"),
            Err(MCUmgrClientError::ExecuteError(
                ExecuteError::ReceiveFailed(ReceiveError::UnexpectedResponse)
            ))
        ));
    }
}

#[test]
fn malformed_cbor_and_missing_required_fields_are_rejected() {
    for body in [
        vec![],
        vec![0xbf, 0x61, b'r'],
        vec![0xa0],
        vec![0xa1, 0x61, b'r', 0x01],
        vec![0xff],
    ] {
        let (client, handle) = client();
        handle.fault(Fault::RawBody(body));
        assert!(matches!(
            client.os_echo("bad cbor"),
            Err(MCUmgrClientError::ExecuteError(ExecuteError::DecodeFailed(
                _
            )))
        ));
    }
}

#[test]
fn device_errors_are_not_retried_and_unknown_codes_are_preserved() {
    for (group, code) in [
        (0u16, 2u16),
        (1, 27),
        (3, 3),
        (8, 3),
        (9, 3),
        (10, 4),
        (63, 4),
        (0x4321, 5678),
    ] {
        let (client, handle) = client();
        client.set_retries(3);
        handle.fault(Fault::Reply(RemoteError::Group(group, code).body()));
        let error = client.os_echo("remote failure").unwrap_err();
        assert!(!error.command_not_supported());
        assert_group_error(error, group, i32::from(code));
        assert_eq!(handle.requests().len(), 1);
    }
}

#[test]
fn generic_smp_errors_preserve_reason_and_support_detection() {
    let (client, handle) = client();
    let mut body = RemoteError::Smp(8).body();
    crate::zephyr_sim::wire::insert(&mut body, "rsn", text("handler disabled"));
    handle.fault(Fault::Reply(body));
    let error = client.os_echo("unsupported").unwrap_err();
    assert!(error.command_not_supported());
    assert!(
        matches!(error, MCUmgrClientError::ExecuteError(ExecuteError::ErrorResponse(DeviceError::V1 { rc: 8, rsn: Some(reason) })) if reason == "handler disabled")
    );
    handle.fault(Fault::Reply(RemoteError::Smp(11).body()));
    let error = client.os_echo("denied").unwrap_err();
    assert!(!error.command_not_supported());
    assert_smp_error(error, 11);
}

#[test]
fn more_than_256_commands_do_not_alias_adjacent_sequences() {
    let (client, handle) = client();
    for n in 0..300 {
        assert_eq!(client.os_echo(n.to_string()).unwrap(), n.to_string());
    }
    let requests = handle.requests();
    assert_eq!(requests.len(), 300);
    for window in requests.windows(2) {
        assert_ne!(window[1].sequence(), window[0].sequence());
    }
}

#[test]
fn one_client_serializes_requests_from_multiple_threads() {
    let (transport, handle) = SimulatedTransport::new(Config::default());
    let client = Arc::new(MCUmgrClient::new_from_transport(transport));
    client.set_retries(0);
    std::thread::scope(|scope| {
        for thread in 0..4 {
            let client = Arc::clone(&client);
            scope.spawn(move || {
                for n in 0..25 {
                    let msg = format!("{thread}:{n}");
                    assert_eq!(client.os_echo(&msg).unwrap(), msg);
                }
            });
        }
    });
    assert_eq!(handle.requests().len(), 100);
}

#[test]
fn transport_mtu_is_enforced_and_large_payloads_do_not_panic() {
    let (limited_client, _) = client_with(Config {
        transport_mtu: 64,
        ..Config::default()
    });
    assert!(matches!(
        limited_client.os_echo("x".repeat(100)),
        Err(MCUmgrClientError::ExecuteError(ExecuteError::SendFailed(
            SendError::DataTooBig
        )))
    ));
    let (client, handle) = client();
    assert!(client.os_echo("x".repeat(65_536)).is_err());
    assert!(handle.requests().is_empty());
}

#[test]
fn simulator_hashes_and_firmware_fixtures_have_external_known_vectors() {
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    assert_eq!(
        hex::encode(sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let data = firmware(2, 100);
    assert_eq!(&data[..4], &[0x3d, 0xb8, 0xf3, 0x96]);
    assert_eq!(data.len(), 32 + 100 + 40);
    assert_eq!(&data[132..140], &[0x07, 0x69, 40, 0, 0x10, 0, 32, 0]);
    assert_eq!(image_hash(&data).unwrap(), sha256(&data[..132]));
    assert_ne!(image_hash(&data).unwrap(), sha256(&data));
}

#[test]
fn raw_image_upload_demonstrates_zephyr_offset_resynchronization() {
    let (client, handle) = client();
    let data = firmware(2, 300);
    let first = Command {
        write: true,
        group: 1,
        id: 1,
        payload: map([
            ("off", uint(0u32)),
            ("len", uint(data.len() as u64)),
            ("sha", bytes(sha256(&data))),
            ("data", bytes(&data[..64])),
        ]),
    };
    assert_eq!(
        lookup(&client.raw_command(&first).unwrap(), "off").and_then(number),
        Some(64)
    );
    let wrong = Command {
        write: true,
        group: 1,
        id: 1,
        payload: map([("off", uint(100u32)), ("data", bytes(&data[100..150]))]),
    };
    assert_eq!(
        lookup(&client.raw_command(&wrong).unwrap(), "off").and_then(number),
        Some(64)
    );
    assert_eq!(
        handle.inspect(|d| d.upload.as_ref().unwrap().data.len()),
        64
    );
    client.image_upload(&data, None, None, false, None).unwrap();
    assert_eq!(
        handle.inspect(|d| d.last_upload.as_ref().unwrap().data.clone()),
        data
    );
}
