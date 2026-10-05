//! Checks of the simulator itself, on raw SMP frames, so that the other tests
//! can rely on it behaving like Zephyr.

use ciborium::Value;

use super::assert_timeout;
use crate::sim::smp::{MGMT_HDR_SIZE, SmpHdr, group_id, mgmt_err, op};
use crate::sim::{Config, SimDevice};

fn request(op: u8, version: u8, group: u16, id: u8, seq: u8, payload: &Value) -> Vec<u8> {
    let mut body = vec![];
    ciborium::into_writer(payload, &mut body).unwrap();
    let hdr = SmpHdr {
        op,
        version,
        flags: 0,
        len: body.len() as u16,
        group,
        seq,
        id,
    };
    let mut frame = hdr.to_bytes().to_vec();
    frame.extend_from_slice(&body);
    frame
}

fn echo_request(version: u8, seq: u8, text: &str) -> Vec<u8> {
    let payload = Value::Map(vec![(Value::Text("d".into()), Value::Text(text.into()))]);
    request(op::READ, version, group_id::OS, 0, seq, &payload)
}

fn parse(frame: &[u8]) -> (SmpHdr, Value) {
    let (hdr, body) = frame.split_first_chunk::<MGMT_HDR_SIZE>().unwrap();
    let hdr = SmpHdr::parse(hdr);
    assert_eq!(hdr.len as usize, body.len());
    (hdr, ciborium::from_reader(body).unwrap())
}

fn rc_response(rc: i32) -> Value {
    Value::Map(vec![(Value::Text("rc".into()), Value::Integer(rc.into()))])
}

#[test]
fn responses_use_indefinite_length_maps_by_default() {
    let device = SimDevice::with_firmware();
    let responses = device.lock().receive_packet(&echo_request(1, 42, "raw"));

    let frame = &responses[0];
    assert_eq!(frame[MGMT_HDR_SIZE], 0xbf);
    assert_eq!(*frame.last().unwrap(), 0xff);

    let (hdr, body) = parse(frame);
    assert_eq!(
        hdr,
        SmpHdr {
            op: op::READ_RSP,
            version: 1,
            flags: 0,
            len: hdr.len,
            group: group_id::OS,
            seq: 42,
            id: 0,
        }
    );
    assert_eq!(
        body,
        Value::Map(vec![(Value::Text("r".into()), Value::Text("raw".into()))])
    );
}

#[test]
fn responses_use_definite_length_maps_with_canonical_cbor() {
    let device = SimDevice::new(Config {
        zcbor_canonical: true,
        ..Default::default()
    });
    let responses = device.lock().receive_packet(&echo_request(1, 0, "raw"));
    assert_eq!(responses[0][MGMT_HDR_SIZE], 0xa1);
}

#[test]
fn protocol_errors() {
    let device = SimDevice::with_firmware();
    let mut device = device.lock();

    // Unknown group, unknown command, command without write handler
    for (op, group, id) in [(op::READ, 42, 0), (op::READ, 0, 99), (op::WRITE, 0, 2)] {
        let frame = request(op, 1, group, id, 7, &Value::Map(vec![]));
        let (hdr, body) = parse(&device.receive_packet(&frame)[0]);
        assert_eq!((hdr.group, hdr.id, hdr.seq), (group, id, 7));
        assert_eq!(body, rc_response(mgmt_err::ENOTSUP));
    }

    // Responses are not accepted as requests
    let frame = request(op::READ_RSP, 1, 0, 0, 0, &Value::Map(vec![]));
    let (_, body) = parse(&device.receive_packet(&frame)[0]);
    assert_eq!(body, rc_response(mgmt_err::ENOTSUP));

    // Future protocol versions
    let (hdr, body) = parse(&device.receive_packet(&echo_request(2, 0, "v3"))[0]);
    assert_eq!(body, rc_response(mgmt_err::UNSUPPORTED_TOO_NEW));
    assert_eq!(hdr.version, 1);

    // The payload is shorter than announced in the header
    let mut frame = echo_request(1, 0, "cut");
    frame.pop();
    let (_, body) = parse(&device.receive_packet(&frame)[0]);
    assert_eq!(body, rc_response(mgmt_err::ECORRUPT));

    // Without a complete header, nothing is sent
    assert!(device.receive_packet(&[0, 0, 0]).is_empty());

    // Payloads that are no CBOR map
    let frame = request(op::READ, 1, 0, 0, 0, &Value::Text("d".into()));
    let (_, body) = parse(&device.receive_packet(&frame)[0]);
    assert_eq!(body, rc_response(mgmt_err::EINVAL));
}

#[test]
fn original_protocol_version() {
    let device = SimDevice::with_firmware();
    let (hdr, body) = parse(&device.lock().receive_packet(&echo_request(0, 0, "v1"))[0]);
    assert_eq!(hdr.version, 0);
    assert_eq!(
        body,
        Value::Map(vec![(Value::Text("r".into()), Value::Text("v1".into()))])
    );

    let device = SimDevice::new(Config {
        smp_support_original_protocol: false,
        ..Default::default()
    });
    let (_, body) = parse(&device.lock().receive_packet(&echo_request(0, 0, "v1"))[0]);
    assert_eq!(body, rc_response(mgmt_err::UNSUPPORTED_TOO_OLD));
}

#[test]
fn several_requests_in_one_packet() {
    let device = SimDevice::with_firmware();
    let mut packet = echo_request(1, 1, "first");
    packet.extend(echo_request(1, 2, "second"));

    let responses = device.lock().receive_packet(&packet);
    assert_eq!(responses.len(), 2);
    assert_eq!(parse(&responses[0]).0.seq, 1);
    assert_eq!(parse(&responses[1]).0.seq, 2);
}

#[test]
fn duplicate_keys_and_wrong_types_are_rejected() {
    let device = SimDevice::with_firmware();
    let mut device = device.lock();

    let duplicate = Value::Map(vec![
        (Value::Text("d".into()), Value::Text("a".into())),
        (Value::Text("d".into()), Value::Text("b".into())),
    ]);
    let wrong_type = Value::Map(vec![(Value::Text("d".into()), Value::Integer(1.into()))]);
    let unknown_key_only = Value::Map(vec![(Value::Text("x".into()), Value::Integer(1.into()))]);

    for payload in [duplicate, wrong_type, unknown_key_only] {
        let frame = request(op::READ, 1, group_id::OS, 0, 0, &payload);
        let (_, body) = parse(&device.receive_packet(&frame)[0]);
        assert_eq!(body, rc_response(mgmt_err::EINVAL));
    }
}

#[test]
fn serial_framing_overhead_counts_against_the_net_buf() {
    // Zephyr decodes the frame length and CRC16 into the same net_buf as the
    // SMP frame, so over serial an SMP frame may be at most
    // CONFIG_MCUMGR_TRANSPORT_NETBUF_SIZE - 4 bytes long.
    let device = SimDevice::with_firmware();
    let serial = device.serial_client();
    serial.set_retries(0);
    let datagram = device.client();

    // 366 characters make a 380 byte request
    let text = "s".repeat(366);
    assert_eq!(serial.os_echo(&text).unwrap(), text);

    let text = "s".repeat(367);
    assert_timeout(serial.os_echo(&text).unwrap_err());
    assert_eq!(datagram.os_echo(&text).unwrap(), text);
}
