#![allow(unused)]

use std::{
    collections::VecDeque,
    io::{Read, Write},
};

use zephyr_mcumgr::transport::serial::ConfigurableTimeout;

#[derive(Default)]
pub(crate) struct LoopbackSerial {
    data: VecDeque<u8>,
}

impl Read for LoopbackSerial {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.data.read(buf)
    }
}

impl Write for LoopbackSerial {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.data.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.data.flush()
    }
}

impl ConfigurableTimeout for LoopbackSerial {
    fn set_timeout(&mut self, _: std::time::Duration) -> Result<(), miette::Report> {
        Ok(())
    }
}

impl Drop for LoopbackSerial {
    fn drop(&mut self) {
        if !self.data.is_empty() {
            panic!("LoopbackSerial contains leftover data");
        }
    }
}

#[derive(Default)]
pub(crate) struct EchoSerial {
    input_buffer: VecDeque<u8>,
    output_buffer: VecDeque<u8>,
}

const FRAME_START_1: u8 = 6;
const FRAME_START_2: u8 = 9;
const FRAME_START_CONT_1: u8 = 4;
const FRAME_START_CONT_2: u8 = 20;
const FRAME_END: u8 = 0x0a;

impl EchoSerial {
    fn process_input_data(&mut self) {
        let mut data = vec![];

        assert_eq!(Some(FRAME_START_1), self.input_buffer.pop_front());
        assert_eq!(Some(FRAME_START_2), self.input_buffer.pop_front());

        loop {
            loop {
                let next = self.input_buffer.pop_front().unwrap();
                if next == FRAME_END {
                    break;
                }
                data.push(next);
            }

            if self.input_buffer.is_empty() {
                break;
            }

            assert_eq!(Some(FRAME_START_CONT_1), self.input_buffer.pop_front());
            assert_eq!(Some(FRAME_START_CONT_2), self.input_buffer.pop_front());
        }

        let data = self.process_message(&data);

        self.output_buffer.push_back(FRAME_START_1);
        self.output_buffer.push_back(FRAME_START_2);
        for chunk in data.chunks(4) {
            for elem in chunk {
                self.output_buffer.push_back(*elem);
            }
            self.output_buffer.push_back(FRAME_END);
            self.output_buffer.push_back(FRAME_START_CONT_1);
            self.output_buffer.push_back(FRAME_START_CONT_2);
        }
        self.output_buffer.push_back(FRAME_END);
    }

    fn process_message(&self, data: &[u8]) -> Vec<u8> {
        use base64::prelude::*;

        let data = BASE64_STANDARD.decode(data).unwrap();
        let (len, data) = data.split_first_chunk().unwrap();
        let len = u16::from_be_bytes(*len);
        assert_eq!(len as usize, data.len());

        let (data, crc) = data.split_last_chunk().unwrap();
        let crc = u16::from_be_bytes(*crc);
        let crc_algo = crc::Crc::<u16>::new(&crc::CRC_16_XMODEM);
        let actual_crc = crc_algo.checksum(data);
        assert_eq!(crc, actual_crc);

        let (header, data): (&[u8; 8], _) = data.split_first_chunk().unwrap();

        let mut data: ciborium::Value = ciborium::from_reader(data).unwrap();
        if let Some(data_map) = data.as_map_mut() {
            for (key, value) in data_map {
                if let Some(key) = key.as_text_mut() {
                    if key == "d" {
                        *key = "r".to_string();
                    }
                }
            }
        }

        let mut response_smp = vec![];
        response_smp.extend_from_slice(header);
        response_smp[0] |= 1;
        ciborium::into_writer(&data, &mut response_smp).unwrap();
        let new_crc = crc_algo.checksum(&response_smp);
        response_smp.extend_from_slice(&new_crc.to_be_bytes());

        BASE64_STANDARD
            .encode(
                (response_smp.len() as u16)
                    .to_be_bytes()
                    .into_iter()
                    .chain(response_smp)
                    .collect::<Vec<_>>(),
            )
            .into()
    }
}

impl Read for EchoSerial {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.input_buffer.is_empty() {
            self.process_input_data();
        }

        self.output_buffer.read(buf)
    }
}

impl Write for EchoSerial {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.input_buffer.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.input_buffer.flush()
    }
}

impl ConfigurableTimeout for EchoSerial {
    fn set_timeout(&mut self, _: std::time::Duration) -> Result<(), miette::Report> {
        Ok(())
    }
}

impl Drop for EchoSerial {
    fn drop(&mut self) {
        if !self.input_buffer.is_empty() {
            panic!("EchoSerial contains leftover input data");
        }
        if !self.output_buffer.is_empty() {
            panic!("EchoSerial contains leftover output data");
        }
    }
}
