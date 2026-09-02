//! A `Transport` implemented from OUTSIDE the crate.
//!
//! This file is the regression test for the two things that made the public
//! `Transport` trait unreachable from another crate. Integration tests are
//! compiled as separate crates that link the library as an ordinary dependency,
//! so they see exactly what a third party sees -- which is what makes this a
//! real test of the property rather than a restatement of it:
//!
//! * the impl below names `SMP_HEADER_SIZE` and `SMP_TRANSFER_BUFFER_SIZE` in
//!   its method signatures, so it fails to compile if either goes back to being
//!   private (E0603);
//! * and it is handed to `MCUmgrClient::new_from_transport`, so it fails to
//!   compile if that constructor is removed.
//!
//! An in-crate unit test could not check either: it can name private items.

use ciborium::Value;
use mcumgr_toolkit::MCUmgrClient;
use mcumgr_toolkit::transport::{
    ReceiveError, SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport,
};

/// A datagram bearer that echoes whole SMP frames.
///
/// Shaped like `UdpTransport` rather than the serial one: the bus delivers whole
/// messages, so there is no console framing, no base64 and no CRC. This is the
/// shape any datagram bearer takes -- UDP, ISO-TP on CAN, a socket to a
/// simulator.
#[derive(Default)]
struct EchoDatagram {
    /// The response to the frame most recently sent, if any.
    pending: Option<Vec<u8>>,
}

impl Transport for EchoDatagram {
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), SendError> {
        // The echo command answers under key "r" what it was asked under "d".
        let mut value: Value = ciborium::from_reader(data).unwrap();
        if let Some(map) = value.as_map_mut() {
            for (key, _) in map.iter_mut() {
                if let Some(key) = key.as_text_mut()
                    && key == "d"
                {
                    *key = "r".to_string();
                }
            }
        }
        let mut payload = Vec::new();
        ciborium::into_writer(&value, &mut payload).unwrap();

        let mut response = header;
        // op: READ(0) -> READ_RSP(1), WRITE(2) -> WRITE_RSP(3).
        response[0] = (response[0] & !0b111) | ((response[0] & 0b111) + 1);
        // The payload length may have shifted, so restate it rather than
        // trusting the request's.
        let [len_hi, len_lo] = (payload.len() as u16).to_be_bytes();
        response[2] = len_hi;
        response[3] = len_lo;

        let mut frame = response.to_vec();
        frame.extend_from_slice(&payload);
        self.pending = Some(frame);
        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], ReceiveError> {
        let frame = self.pending.take().ok_or(ReceiveError::Timeout)?;
        buffer[..frame.len()].copy_from_slice(&frame);
        Ok(&buffer[..frame.len()])
    }

    fn set_timeout(
        &mut self,
        _timeout: std::time::Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Nothing to do: this bearer answers from memory, immediately.
        Ok(())
    }
}

#[test]
fn a_transport_implemented_outside_the_crate_can_drive_a_client() {
    let client = MCUmgrClient::new_from_transport(EchoDatagram::default());

    let request = "Hello from a third-party transport";
    let response = client.os_echo(request).unwrap();
    assert_eq!(request, response);
}

#[test]
fn an_external_transport_round_trips_a_large_payload() {
    let client = MCUmgrClient::new_from_transport(EchoDatagram::default());

    // Long enough to matter, but within one frame: a datagram bearer hands over
    // whole messages, so there is nothing here to exercise segmentation.
    let request = "x".repeat(512);
    let response = client.os_echo(&request).unwrap();
    assert_eq!(request, response);
}
