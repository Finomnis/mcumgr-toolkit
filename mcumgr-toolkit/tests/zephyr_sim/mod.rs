//! Test-only simulated Zephyr transport. See README.md for provenance and scope.
pub mod device;
pub mod firmware;
pub mod wire;

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use mcumgr_toolkit::{
    MCUmgrClient,
    transport::{ReceiveError, SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport},
};

pub use device::{Config, Device};
use wire::{RemoteError, Request, Value, encode};

#[derive(Debug, Clone)]
pub enum HeaderFault {
    Operation,
    Group,
    Command,
    Length,
    Truncated,
}

#[derive(Debug, Clone)]
pub enum Fault {
    SendTimeout,
    SendIo,
    ReceiveIo,
    ReceiveTooBig,
    /// A request lost before it reaches the server.
    LoseRequest,
    /// The server executes the request, but the reply never reaches the client.
    LoseReply,
    /// Send a delayed response from an earlier request before the correct one.
    StaleBeforeValid,
    Header(HeaderFault),
    /// Replace the response, without invoking a device handler.
    Reply(Value),
    /// Malformed / unusual CBOR, without invoking a device handler.
    RawBody(Vec<u8>),
}

#[derive(Debug, Clone)]
struct ScheduledFault {
    route: Option<(u16, u8)>,
    skip: usize,
    fault: Fault,
}

struct Shared {
    device: Device,
    requests: Vec<Request>,
    faults: VecDeque<ScheduledFault>,
    timeout: Duration,
    timeout_failure: bool,
    receives: usize,
}

#[derive(Clone)]
pub struct Handle(Arc<Mutex<Shared>>);

impl Handle {
    pub fn inspect<T>(&self, f: impl FnOnce(&Device) -> T) -> T {
        f(&self.0.lock().unwrap().device)
    }
    pub fn edit(&self, f: impl FnOnce(&mut Device)) {
        f(&mut self.0.lock().unwrap().device);
    }
    pub fn requests(&self) -> Vec<Request> {
        self.0.lock().unwrap().requests.clone()
    }
    pub fn timeout(&self) -> Duration {
        self.0.lock().unwrap().timeout
    }
    pub fn fail_timeout_configuration(&self) {
        self.0.lock().unwrap().timeout_failure = true;
    }
    pub fn receives(&self) -> usize {
        self.0.lock().unwrap().receives
    }
    pub fn fault(&self, fault: Fault) {
        self.schedule(None, 0, fault);
    }
    pub fn fault_on(&self, group: u16, id: u8, skip: usize, fault: Fault) {
        self.schedule(Some((group, id)), skip, fault);
    }
    fn schedule(&self, route: Option<(u16, u8)>, skip: usize, fault: Fault) {
        self.0
            .lock()
            .unwrap()
            .faults
            .push_back(ScheduledFault { route, skip, fault });
    }
    pub fn assert_faults_consumed(&self) {
        assert!(
            self.0.lock().unwrap().faults.is_empty(),
            "a planned fault was never exercised"
        );
    }
}

pub struct SimulatedTransport {
    shared: Handle,
    incoming: VecDeque<std::result::Result<Vec<u8>, ReceiveError>>,
}

impl SimulatedTransport {
    pub fn new(config: Config) -> (Self, Handle) {
        assert!(config.download_chunk > 0);
        let handle = Handle(Arc::new(Mutex::new(Shared {
            device: Device::new(config),
            requests: Vec::new(),
            faults: VecDeque::new(),
            timeout: Duration::from_millis(100),
            timeout_failure: false,
            receives: 0,
        })));
        (
            Self {
                shared: handle.clone(),
                incoming: VecDeque::new(),
            },
            handle,
        )
    }
}

impl Transport for SimulatedTransport {
    // Deliberately implement only the raw hooks. The real client's default
    // Transport::send_frame / receive_frame remain in the exercised path.
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> std::result::Result<(), SendError> {
        let request = Request::parse(header, data);
        let mut state = self.shared.0.lock().unwrap();
        assert!(
            state.requests.len() < 10_000,
            "request budget exceeded; probable client transfer loop"
        );
        let oversize = request.frame_len()
            > state
                .device
                .config
                .buffer_size
                .min(state.device.config.transport_mtu);
        state.requests.push(request.clone());
        if oversize {
            return Err(SendError::DataTooBig);
        }
        let fault = if let Some(next) = state.faults.front_mut() {
            if next
                .route
                .is_none_or(|route| route == (request.group(), request.id()))
            {
                if next.skip == 0 {
                    state.faults.pop_front().map(|f| f.fault)
                } else {
                    next.skip -= 1;
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        match fault {
            Some(Fault::SendTimeout) => return Err(SendError::Timeout),
            Some(Fault::SendIo) => {
                return Err(
                    io::Error::new(io::ErrorKind::BrokenPipe, "simulated send failure").into(),
                );
            }
            Some(Fault::LoseRequest) => {
                self.incoming.push_back(Err(ReceiveError::Timeout));
                return Ok(());
            }
            _ => {}
        }
        let mut body = match &fault {
            Some(Fault::Reply(body)) => encode(body),
            Some(Fault::RawBody(body)) => body.clone(),
            _ => encode(&state.device.dispatch(&request)),
        };
        // Zephyr's zcbor writer cannot grow beyond the configured net_buf.
        if body.len() + 8 > state.device.config.buffer_size {
            body = encode(&RemoteError::Smp(7).body());
        }
        let mut reply = request.response(&body);
        match fault {
            Some(Fault::LoseReply) => self.incoming.push_back(Err(ReceiveError::Timeout)),
            Some(Fault::ReceiveIo) => self.incoming.push_back(Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "simulated receive failure",
            )
            .into())),
            Some(Fault::ReceiveTooBig) => self.incoming.push_back(Err(ReceiveError::FrameTooBig)),
            Some(Fault::StaleBeforeValid) => {
                let mut stale = reply.clone();
                stale[6] = stale[6].wrapping_sub(1);
                self.incoming.push_back(Ok(stale));
                self.incoming.push_back(Ok(reply));
            }
            Some(Fault::Header(kind)) => {
                match kind {
                    HeaderFault::Operation => reply[0] ^= 2,
                    HeaderFault::Group => reply[5] ^= 1,
                    HeaderFault::Command => reply[7] ^= 1,
                    HeaderFault::Length => reply[3] ^= 1,
                    HeaderFault::Truncated => reply.truncate(7),
                }
                self.incoming.push_back(Ok(reply));
            }
            _ => self.incoming.push_back(Ok(reply)),
        }
        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> std::result::Result<&'a [u8], ReceiveError> {
        self.shared.0.lock().unwrap().receives += 1;
        let frame = self
            .incoming
            .pop_front()
            .unwrap_or(Err(ReceiveError::Timeout))?;
        if frame.len() > buffer.len() || frame.len() > self.max_smp_frame_size() {
            return Err(ReceiveError::FrameTooBig);
        }
        buffer[..frame.len()].copy_from_slice(&frame);
        Ok(&buffer[..frame.len()])
    }

    fn set_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.shared.0.lock().unwrap();
        if state.timeout_failure {
            return Err(io::Error::other("simulated timeout configuration failure").into());
        }
        state.timeout = timeout;
        Ok(())
    }

    fn max_smp_frame_size(&self) -> usize {
        self.shared.inspect(|d| d.config.transport_mtu)
    }
}

pub fn client_with(config: Config) -> (MCUmgrClient, Handle) {
    let (transport, handle) = SimulatedTransport::new(config);
    let client = MCUmgrClient::new_from_transport(transport);
    client.set_retries(0);
    (client, handle)
}

pub fn client() -> (MCUmgrClient, Handle) {
    client_with(Config::default())
}
