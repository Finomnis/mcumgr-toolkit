//! A serial port connected to the simulated device through Zephyr's SMP serial
//! transport, as implemented in `subsys/mgmt/mcumgr/transport/src/serial_util.c`
//! (`mcumgr_serial_process_frag`, `mcumgr_serial_tx_pkt`) and
//! `drivers/console/uart_mcumgr.c`.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::time::Duration;

use base64::prelude::*;
use mcumgr_toolkit::transport::serial::ConfigurableTimeout;

use super::SimDevice;

const MCUMGR_SERIAL_HDR_PKT: u16 = 0x0609;
const MCUMGR_SERIAL_HDR_FRAG: u16 = 0x0414;
const MCUMGR_SERIAL_MAX_FRAME: usize = 127;
/// `CONFIG_UART_MCUMGR_RX_BUF_SIZE`
const UART_MCUMGR_RX_BUF_SIZE: usize = 128;

/// `crc16_itu_t(0x0000, ...)`
fn crc16_itu_t(data: &[u8]) -> u16 {
    crc::Crc::<u16>::new(&crc::CRC_16_XMODEM).checksum(data)
}

/// `struct mcumgr_serial_rx_ctxt`
#[derive(Default)]
struct RxCtxt {
    /// The net_buf being assembled; `None` if not allocated
    nb: Option<Vec<u8>>,
    /// Bytes pulled from the front of the net_buf (the packet length)
    pulled: usize,
    pkt_len: usize,
}

pub struct SimSerialPort {
    device: SimDevice,
    line: Vec<u8>,
    rx: RxCtxt,
    output: VecDeque<u8>,
}

impl SimSerialPort {
    pub fn new(device: SimDevice) -> Self {
        Self {
            device,
            line: vec![],
            rx: RxCtxt::default(),
            output: VecDeque::new(),
        }
    }

    /// `mcumgr_serial_process_frag`; returns a complete packet
    fn process_frag(&mut self, frag: &[u8]) -> Option<Vec<u8>> {
        let netbuf_size = self.device.lock().config.mcumgr_transport_netbuf_size;
        let rx = &mut self.rx;

        let (op, data) = frag.split_first_chunk::<2>()?;
        match u16::from_be_bytes(*op) {
            MCUMGR_SERIAL_HDR_PKT => {
                rx.nb = Some(vec![]);
                rx.pulled = 0;
            }
            MCUMGR_SERIAL_HDR_FRAG => {
                if rx.nb.as_ref().is_none_or(|nb| nb.is_empty()) {
                    *rx = RxCtxt::default();
                    return None;
                }
            }
            _ => return None,
        }

        // base64_decode() into the tailroom of the net_buf
        let nb = rx.nb.as_mut().unwrap();
        let Ok(decoded) = BASE64_STANDARD.decode(data) else {
            *rx = RxCtxt::default();
            return None;
        };
        if rx.pulled + nb.len() + decoded.len() > netbuf_size {
            *rx = RxCtxt::default();
            return None;
        }
        nb.extend_from_slice(&decoded);

        if u16::from_be_bytes(*op) == MCUMGR_SERIAL_HDR_PKT {
            // mcumgr_serial_extract_len()
            if nb.len() < 2 {
                *rx = RxCtxt::default();
                return None;
            }
            rx.pkt_len = u16::from_be_bytes([nb[0], nb[1]]) as usize;
            nb.drain(..2);
            rx.pulled = 2;
            if rx.pkt_len <= 2 {
                *rx = RxCtxt::default();
                return None;
            }
        }

        let nb = rx.nb.as_mut().unwrap();
        if nb.len() < rx.pkt_len {
            // More fragments expected.
            return None;
        } else if nb.len() > rx.pkt_len {
            *rx = RxCtxt::default();
            return None;
        }

        let (packet, crc) = nb.split_last_chunk::<2>().unwrap();
        if crc16_itu_t(packet) != u16::from_be_bytes(*crc) {
            *rx = RxCtxt::default();
            return None;
        }

        let packet = packet.to_vec();
        *rx = RxCtxt::default();
        Some(packet)
    }

    /// `mcumgr_serial_tx_pkt`
    fn tx_pkt(&mut self, data: &[u8]) {
        /// `mcumgr_serial_tx_small`: base64 encodes up to three bytes
        fn tx_small(out: &mut Vec<u8>, raw: &[u8]) {
            out.extend(BASE64_STANDARD.encode(raw).bytes());
        }

        let mut out = vec![];
        let len = data.len();
        let crc = crc16_itu_t(data);
        let mut marker = MCUMGR_SERIAL_HDR_PKT;
        let mut first = true;
        let mut last = false;
        let mut src_off = 0;

        while src_off < len {
            let mut max_input = ((MCUMGR_SERIAL_MAX_FRAME - 3) >> 2) * 3;

            out.extend(marker.to_be_bytes());

            if first {
                let [len_hi, len_lo] = ((len + 2) as u16).to_be_bytes();
                tx_small(&mut out, &[len_hi, len_lo, data[0]]);
                src_off += 1;
                max_input -= 3;
            }

            let mut to_process = max_input.min(len - src_off) as isize;
            let reminder = max_input as isize - (len - src_off) as isize;
            if reminder == 0 || reminder == 1 {
                to_process -= 1;
                last = false;
            } else if reminder >= 2 {
                last = true;
            }

            while to_process >= 3 {
                tx_small(&mut out, &data[src_off..src_off + 3]);
                src_off += 3;
                to_process -= 3;
            }

            if last {
                let [crc_hi, crc_lo] = crc.to_be_bytes();
                match len - src_off {
                    0 => tx_small(&mut out, &[crc_hi, crc_lo]),
                    1 => {
                        tx_small(&mut out, &[data[src_off], crc_hi, crc_lo]);
                        src_off += 1;
                    }
                    2 => {
                        tx_small(&mut out, &[data[src_off], data[src_off + 1], crc_hi]);
                        tx_small(&mut out, &[crc_lo]);
                        src_off += 2;
                    }
                    _ => unreachable!(),
                }
            }

            out.push(b'\n');
            marker = MCUMGR_SERIAL_HDR_FRAG;
            first = false;
        }

        self.output.extend(out);
    }
}

impl Write for SimSerialPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for byte in buf {
            if *byte != b'\n' {
                self.line.push(*byte);
                continue;
            }

            let line = std::mem::take(&mut self.line);
            if line.len() > UART_MCUMGR_RX_BUF_SIZE {
                // uart_mcumgr drops lines that do not fit into its buffer
                continue;
            }

            if let Some(packet) = self.process_frag(&line) {
                let responses = {
                    let mut device = self.device.lock();
                    device.link.record(&packet);
                    device.receive_packet(&packet)
                };
                for response in responses {
                    self.tx_pkt(&response);
                }
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for SimSerialPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.output.is_empty() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.output.read(buf)
    }
}

impl ConfigurableTimeout for SimSerialPort {
    fn set_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.device.lock().link.timeouts.push(timeout);
        Ok(())
    }
}
