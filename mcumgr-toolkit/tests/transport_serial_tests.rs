mod common;
use common::LoopbackSerial;

use proptest::prelude::*;
use zephyr_mcumgr::transport::{Transport, serial::SerialTransport};

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

        let mut recv_buffer = [0u8; u16::MAX as usize];
        let data_received = transport.recv_raw_frame(&mut recv_buffer).unwrap();

        assert_eq!(header, &data_received[..8], "Received header did not match!");
        assert_eq!(data, &data_received[8..], "Received data did not match! (len: {})", data.len());
    }

}

#[test]
fn test_chunking_upper_limit() {
    let length = u16::MAX as usize - 8 /* SMP_HEADER_SIZE */ - size_of::<u16>() /* CRC16 */;

    let mut transport = create_loopback_transport();
    let mut rng = rand::rng();

    let mut header = [0u8; 8];
    rng.fill(&mut header);

    let mut data = vec![0u8; length];
    rng.fill(data.as_mut_slice());

    transport.send_raw_frame(header, &data).unwrap();

    let mut recv_buffer = [0u8; u16::MAX as usize];
    let data_received = transport.recv_raw_frame(&mut recv_buffer).unwrap();

    assert_eq!(
        header,
        &data_received[..8],
        "Received header did not match!"
    );
    assert_eq!(
        data,
        &data_received[8..],
        "Received data did not match! (len: {})",
        data.len()
    );
}
