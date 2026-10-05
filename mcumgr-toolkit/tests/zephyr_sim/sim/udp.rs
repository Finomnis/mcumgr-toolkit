//! The simulated device listening on a UDP socket, like Zephyr's
//! `subsys/mgmt/mcumgr/transport/src/smp_udp.c`.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use super::SimDevice;

/// `CONFIG_MCUMGR_TRANSPORT_UDP_MTU`
const MCUMGR_TRANSPORT_UDP_MTU: usize = 1500;

/// A running UDP server; stops when dropped
pub struct UdpServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl UdpServer {
    pub fn start(device: SimDevice) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        let addr = socket.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));

        let thread = std::thread::spawn({
            let stop = stop.clone();
            move || {
                let mut buffer = [0u8; MCUMGR_TRANSPORT_UDP_MTU];
                while !stop.load(Ordering::Relaxed) {
                    let Ok((len, source)) = socket.recv_from(&mut buffer) else {
                        continue;
                    };
                    let packet = &buffer[..len];
                    let responses = {
                        let mut device = device.lock();
                        device.link.record(packet);
                        device.receive_packet(packet)
                    };
                    for response in responses {
                        socket.send_to(&response, source).unwrap();
                    }
                }
            }
        });

        Self {
            addr,
            stop,
            thread: Some(thread),
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for UdpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
