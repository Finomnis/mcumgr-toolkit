//! A serial port connected to the simulated device through Zephyr's raw UART
//! transport, as implemented in `subsys/mgmt/mcumgr/transport/src/smp_raw_uart.c`,
//! `mcumgr_serial_process_frag_raw` in
//! `subsys/mgmt/mcumgr/transport/src/serial_util.c` and
//! `drivers/console/uart_mcumgr.c` with `CONFIG_UART_MCUMGR_RAW_PROTOCOL`.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::time::Duration;

use mcumgr_toolkit::transport::serial::ConfigurableTimeout;

use super::SimDevice;

/// `sizeof(struct smp_hdr)`
const SMP_HDR_SIZE: usize = 8;
/// `MGMT_OP_COUNT`
const MGMT_OP_COUNT: u8 = 4;

pub struct SimRawSerialPort {
    device: SimDevice,
    /// `mcumgr_raw_uart_rx_ctxt.nb`
    nb: Vec<u8>,
    output: VecDeque<u8>,
}

impl SimRawSerialPort {
    pub fn new(device: SimDevice) -> Self {
        Self {
            device,
            nb: vec![],
            output: VecDeque::new(),
        }
    }

    /// `mcumgr_serial_process_frag_raw`; returns a complete packet
    fn process_frag(&mut self, frag: &[u8]) -> Option<Vec<u8>> {
        let netbuf_size = self.device.lock().config.mcumgr_transport_netbuf_size;

        if frag.len() > netbuf_size - self.nb.len() {
            // Payload is too large to fit into buffer.
            self.nb.clear();
            return None;
        }
        self.nb.extend_from_slice(frag);

        if self.nb.len() < SMP_HDR_SIZE {
            // Missing header
            return None;
        }

        let total_size = u16::from_be_bytes([self.nb[2], self.nb[3]]) as usize + SMP_HDR_SIZE;
        if total_size > netbuf_size || self.nb[0] & 0b111 >= MGMT_OP_COUNT {
            // Longer than the MTU, or not a valid MCUmgr message
            self.nb.clear();
            None
        } else if self.nb.len() < total_size {
            // More fragments expected.
            None
        } else if self.nb.len() > total_size {
            // Payload longer than indicated in header.
            self.nb.clear();
            None
        } else {
            Some(std::mem::take(&mut self.nb))
        }
    }
}

impl Write for SimRawSerialPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // In raw mode, uart_mcumgr_rx_byte() hands every byte over as a
        // fragment of its own
        for byte in buf {
            if let Some(packet) = self.process_frag(&[*byte]) {
                let responses = {
                    let mut device = self.device.lock();
                    device.link.record(&packet);
                    device.receive_packet(&packet)
                };
                // smp_raw_uart_tx_pkt() sends the packet as it is
                for response in responses {
                    self.output.extend(response);
                }
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for SimRawSerialPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.output.is_empty() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.output.read(buf)
    }
}

impl ConfigurableTimeout for SimRawSerialPort {
    fn set_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.device.lock().link.timeouts.push(timeout);
        Ok(())
    }
}
