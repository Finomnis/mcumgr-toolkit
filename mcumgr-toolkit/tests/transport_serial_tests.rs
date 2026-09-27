mod common;
use common::LoopbackSerial;

use mcumgr_toolkit::transport::{
    SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport, serial::SerialTransport,
};
use proptest::prelude::*;
use rand::RngExt;

const CRC16_SIZE: usize = size_of::<u16>();
const SERIAL_MAX_SMP_FRAME_SIZE: usize = u16::MAX as usize - CRC16_SIZE;
const SERIAL_MAX_SMP_BODY_SIZE: usize = SERIAL_MAX_SMP_FRAME_SIZE - SMP_HEADER_SIZE;

fn create_loopback_transport() -> Box<dyn Transport> {
    Box::new(SerialTransport::new(LoopbackSerial::default())) as Box<dyn Transport>
}

proptest! {
    #[test]
    fn test_chunking_reassembly(
        header in prop::array::uniform::<_, 8>(any::<u8>()),
        data in prop::collection::vec(any::<u8>(), 10000),
    ) {
        // Verify chunking and reassembly works for any data size

        let mut transport = create_loopback_transport();

        transport.send_raw_frame(header, &data).unwrap();

        let mut recv_buffer = [0u8; SMP_TRANSFER_BUFFER_SIZE];
        let data_received = transport.recv_raw_frame(&mut recv_buffer).unwrap();

        assert_eq!(header, &data_received[..8], "Received header did not match!");
        assert_eq!(data, &data_received[8..], "Received data did not match! (len: {})", data.len());
    }

}

#[test]
fn test_chunking_upper_limit() {
    let mut transport = create_loopback_transport();
    let mut rng = rand::rng();

    let mut header = [0u8; SMP_HEADER_SIZE];
    rng.fill(&mut header);

    let mut data = vec![0u8; SERIAL_MAX_SMP_BODY_SIZE];
    rng.fill(data.as_mut_slice());

    transport.send_raw_frame(header, &data).unwrap();

    let mut recv_buffer = [0u8; SMP_TRANSFER_BUFFER_SIZE];
    let data_received = transport.recv_raw_frame(&mut recv_buffer).unwrap();

    assert_eq!(
        header,
        &data_received[..SMP_HEADER_SIZE],
        "Received header did not match!"
    );
    assert_eq!(
        data,
        &data_received[SMP_HEADER_SIZE..],
        "Received data did not match! (len: {})",
        data.len()
    );
}

#[test]
fn test_chunking_above_upper_limit_is_rejected() {
    let mut transport = create_loopback_transport();

    let data = vec![0; SERIAL_MAX_SMP_BODY_SIZE + 1];

    let result = transport.send_raw_frame([0; SMP_HEADER_SIZE], &data);

    assert!(matches!(result, Err(SendError::DataTooBig)));
}

#[test]
fn test_max_frame_size_is_reported_correctly() {
    let transport = create_loopback_transport();

    assert_eq!(transport.max_smp_frame_size(), SERIAL_MAX_SMP_FRAME_SIZE);
}
