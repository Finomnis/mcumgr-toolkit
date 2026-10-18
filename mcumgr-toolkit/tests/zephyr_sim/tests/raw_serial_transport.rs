//! The client's raw serial transport against Zephyr's raw UART transport

use super::assert_timeout;
use crate::sim::image::ImageBuilder;
use crate::sim::smp::group_id;
use crate::sim::{Config, SimDevice};

#[test]
fn echo() {
    let device = SimDevice::with_firmware();
    let client = device.raw_serial_client();

    assert_eq!(client.os_echo("raw").unwrap(), "raw");

    let long = "abcdefghij".repeat(30);
    assert_eq!(client.os_echo(&long).unwrap(), long);
}

#[test]
fn every_payload_length_gets_through() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 1024,
        ..Default::default()
    });
    let client = device.raw_serial_client();

    for len in 0..600 {
        let message = "z".repeat(len);
        assert_eq!(client.os_echo(&message).unwrap(), message);
    }
    assert_eq!(device.requests().len(), 600);
}

#[test]
fn set_timeout_reaches_the_serial_port() {
    let device = SimDevice::with_firmware();
    let client = device.raw_serial_client();

    client
        .set_timeout(std::time::Duration::from_millis(500))
        .unwrap();
    assert_eq!(
        device.lock().link.timeouts,
        [std::time::Duration::from_millis(500)]
    );
}

#[test]
fn frames_may_fill_the_whole_net_buf() {
    // Unlike the SMP serial transport, the raw transport stores nothing but
    // the SMP frame in its CONFIG_MCUMGR_TRANSPORT_NETBUF_SIZE receive buffer
    let device = SimDevice::with_firmware();
    let client = device.raw_serial_client();
    client.set_retries(0);

    // 370 characters make a 384 byte request
    let text = "r".repeat(370);
    assert_eq!(client.os_echo(&text).unwrap(), text);

    let text = "r".repeat(371);
    assert_timeout(client.os_echo(&text).unwrap_err());
}

#[test]
fn image_upload_with_auto_frame_size() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 2048,
        ..Default::default()
    });
    device
        .lock()
        .flash_image(0, &ImageBuilder::new((1, 0, 0, 0)).build());
    let client = device.raw_serial_client();
    let image = ImageBuilder::new((2, 0, 0, 0))
        .body(vec![0x44; 30_000])
        .build();

    client.use_auto_frame_size().unwrap();
    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);

    let requests = device.requests_for(group_id::IMAGE, 1);
    assert!(requests.iter().all(|r| r.frame_len <= 2048));
    assert!(requests.iter().any(|r| r.frame_len > 2040));
}

#[test]
fn file_transfer_with_auto_frame_size() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 4096,
        ..Default::default()
    });
    let client = device.raw_serial_client();
    let data: Vec<u8> = (0..30_000u32).map(|i| (i % 251) as u8).collect();

    client.use_auto_frame_size().unwrap();
    client
        .fs_file_upload("/lfs1/raw.bin", &data[..], data.len() as u64, None)
        .unwrap();
    let requests = device.requests_for(group_id::FS, 0);
    assert!(requests.iter().all(|r| r.frame_len <= 4096));
    assert!(requests.iter().any(|r| r.frame_len > 4080));

    let mut downloaded = vec![];
    client
        .fs_file_download("/lfs1/raw.bin", &mut downloaded, None)
        .unwrap();
    assert_eq!(downloaded, data);
}
