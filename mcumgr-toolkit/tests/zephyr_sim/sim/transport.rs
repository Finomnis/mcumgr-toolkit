//! A datagram transport to the simulated device, implementing this crate's
//! public [`Transport`] trait, with fault injection on the link.
//!
//! The link behaves like Zephyr's UDP/BT/dummy transports: every frame is one
//! SMP packet, packets larger than `CONFIG_MCUMGR_TRANSPORT_NETBUF_SIZE` are
//! dropped by the device, and every response is sent back as its own packet.

use std::collections::VecDeque;
use std::time::Duration;

use ciborium::Value;
use mcumgr_toolkit::transport::{
    ReceiveError, SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport,
};

use super::SimDevice;
use super::smp::{MGMT_HDR_SIZE, SmpHdr};

/// A link failure, applied to the next request sent through a [`SimTransport`]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The request never reaches the device
    DropRequest,
    /// The device executes the request, but the response is lost
    DropResponse,
    /// The response is delayed until after the response to the next request
    LateResponse,
    /// A response with a different sequence number arrives first
    StaleResponse,
    /// The response carries a different group id
    WrongGroup,
    /// Only the first bytes of the response arrive
    TruncatedResponse,
    /// The transport reports an error while sending
    SendError,
}

/// A request as received by the device
#[derive(Clone, Debug)]
pub struct Request {
    pub hdr: SmpHdr,
    /// The CBOR payload, if it could be decoded
    pub payload: Option<Value>,
    /// Size of header + payload
    pub frame_len: usize,
}

impl Request {
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.payload
            .as_ref()?
            .as_map()?
            .iter()
            .find(|(k, _)| k.as_text() == Some(key))
            .map(|(_, v)| v)
    }
}

#[derive(Debug, Default)]
pub struct LinkState {
    pub faults: VecDeque<Fault>,
    /// Requests that reached the device
    pub requests: Vec<Request>,
    /// Timeouts configured through the transports
    pub timeouts: Vec<Duration>,
    /// Packets dropped because they exceeded the net_buf size
    pub dropped_oversized: usize,
    /// What [`Transport::max_smp_frame_size`] reports; unlimited if `None`
    pub max_smp_frame_size: Option<usize>,
    /// Responses held back by [`Fault::LateResponse`]
    late: Vec<Vec<u8>>,
}

impl LinkState {
    pub fn record(&mut self, packet: &[u8]) {
        if let Some((hdr, payload)) = packet.split_first_chunk::<MGMT_HDR_SIZE>() {
            self.requests.push(Request {
                hdr: SmpHdr::parse(hdr),
                payload: ciborium::from_reader(payload).ok(),
                frame_len: packet.len(),
            });
        }
    }
}

pub struct SimTransport {
    device: SimDevice,
    rx: VecDeque<Vec<u8>>,
}

impl SimTransport {
    pub fn new(device: SimDevice) -> Self {
        Self {
            device,
            rx: VecDeque::new(),
        }
    }
}

impl Transport for SimTransport {
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), SendError> {
        let mut device = self.device.lock();
        let fault = device.link.faults.pop_front();

        match fault {
            Some(Fault::SendError) => {
                return Err(SendError::TransportError("simulated send failure".into()));
            }
            Some(Fault::DropRequest) => return Ok(()),
            _ => {}
        }

        let mut packet = header.to_vec();
        packet.extend_from_slice(data);
        device.link.record(&packet);
        let responses = device.receive_packet(&packet);

        let late = std::mem::take(&mut device.link.late);
        self.rx.extend(late);

        for response in responses {
            match fault {
                Some(Fault::DropResponse) => {}
                Some(Fault::LateResponse) => device.link.late.push(response),
                Some(Fault::StaleResponse) => {
                    let mut stale = response.clone();
                    stale[6] = stale[6].wrapping_sub(17);
                    self.rx.push_back(stale);
                    self.rx.push_back(response);
                }
                Some(Fault::WrongGroup) => {
                    let mut wrong = response;
                    wrong[5] ^= 0x01;
                    self.rx.push_back(wrong);
                }
                Some(Fault::TruncatedResponse) => {
                    self.rx.push_back(response[..MGMT_HDR_SIZE / 2].to_vec());
                }
                _ => self.rx.push_back(response),
            }
        }

        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], ReceiveError> {
        let frame = self.rx.pop_front().ok_or(ReceiveError::Timeout)?;
        buffer[..frame.len()].copy_from_slice(&frame);
        Ok(&buffer[..frame.len()])
    }

    fn set_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.device.lock().link.timeouts.push(timeout);
        Ok(())
    }

    fn max_smp_frame_size(&self) -> usize {
        self.device
            .lock()
            .link
            .max_smp_frame_size
            .unwrap_or(usize::MAX)
    }
}
