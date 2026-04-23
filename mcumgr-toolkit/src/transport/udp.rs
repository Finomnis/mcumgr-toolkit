use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::Duration,
};

use super::{ReceiveError, SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport};

/// A transport layer implementation for UDP sockets.
pub struct UdpTransport {
    socket: UdpSocket,
    send_buffer: Vec<u8>,
}

impl UdpTransport {
    /// Create a new [`UdpTransport`] connected to the given remote address.
    ///
    /// `addr` accepts anything that implements [`ToSocketAddrs`], including
    /// `"host:port"` strings — DNS resolution happens here.
    ///
    pub fn new(addr: impl ToSocketAddrs, timeout: Duration) -> std::io::Result<Self> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                "no addresses resolved",
            )
        })?;
        let bind_addr: SocketAddr = if addr.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let socket = UdpSocket::bind(bind_addr)?;
        socket.connect(addr)?;
        socket.set_read_timeout(Some(timeout))?;
        Ok(Self {
            socket,
            send_buffer: Vec::new(),
        })
    }
}

impl Transport for UdpTransport {
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), SendError> {
        self.send_buffer.clear();
        self.send_buffer.extend_from_slice(&header);
        self.send_buffer.extend_from_slice(data);
        self.socket.send(&self.send_buffer)?;
        log::debug!("Sent UDP SMP Frame ({} bytes)", data.len());
        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], ReceiveError> {
        // On Unix, SO_RCVTIMEO returns EAGAIN (WouldBlock) when the deadline
        // passes rather than ETIMEDOUT.  Normalise to TimedOut so the rest of
        // the stack (connection retry logic, the erase-progressively hint in
        // client.rs) behaves identically across platforms.
        let len = self.socket.recv(buffer).map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                std::io::Error::new(std::io::ErrorKind::TimedOut, e)
            } else {
                e
            }
        })?;
        if len < SMP_HEADER_SIZE {
            return Err(ReceiveError::UnexpectedResponse);
        }
        log::debug!("Received UDP SMP Frame ({} bytes)", len - SMP_HEADER_SIZE);
        Ok(&buffer[..len])
    }

    fn set_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.socket
            .set_read_timeout(Some(timeout))
            .map_err(Into::into)
    }

    /// UDP datagrams larger than the network MTU are silently dropped by most
    /// embedded IP stacks.  Cap at 1024 bytes — a conservative limit that fits
    /// inside a single Ethernet frame even after VPN/tunnel overhead.
    /// Users who know their network supports larger unfragmented datagrams can
    /// raise this with [`MCUmgrClient::set_frame_size`](crate::MCUmgrClient::set_frame_size).
    fn max_smp_frame_size(&self) -> usize {
        1024
    }
}
