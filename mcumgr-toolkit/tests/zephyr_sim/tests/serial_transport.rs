//! The client's serial transport against Zephyr's SMP serial framing

use crate::sim::image::ImageBuilder;
use crate::sim::smp::group_id;
use crate::sim::{Config, SimDevice};

#[test]
fn echo() {
    let device = SimDevice::with_firmware();
    let client = device.serial_client();

    assert_eq!(client.os_echo("serial").unwrap(), "serial");

    // Requests and responses that span multiple serial frames
    let long = "abcdefghij".repeat(30);
    assert_eq!(client.os_echo(&long).unwrap(), long);
}

#[test]
fn every_payload_length_survives_the_framing() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 1024,
        ..Default::default()
    });
    let client = device.serial_client();

    for len in 0..600 {
        let message = "z".repeat(len);
        assert_eq!(client.os_echo(&message).unwrap(), message);
    }
    assert_eq!(device.requests().len(), 600);
}

#[test]
fn set_timeout_reaches_the_serial_port() {
    let device = SimDevice::with_firmware();
    let client = device.serial_client();

    client
        .set_timeout(std::time::Duration::from_millis(500))
        .unwrap();
    assert_eq!(
        device.lock().link.timeouts,
        [std::time::Duration::from_millis(500)]
    );
}

#[test]
fn image_upload() {
    let device = SimDevice::with_firmware();
    let client = device.serial_client();
    let image = ImageBuilder::new((2, 0, 0, 0))
        .body(vec![0x33; 8000])
        .build();

    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);
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
    let client = device.serial_client();
    let image = ImageBuilder::new((2, 0, 0, 0))
        .body(vec![0x33; 30_000])
        .build();

    client.use_auto_frame_size().unwrap();
    client
        .image_upload(&image, None, None, false, None)
        .unwrap();
    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);

    // Zephyr's receive buffer also holds the length prefix and the CRC16, so
    // frames may use all but 4 bytes of it
    let requests = device.requests_for(group_id::IMAGE, 1);
    assert!(requests.iter().all(|r| r.frame_len <= 2048 - 4));
    assert!(requests.iter().any(|r| r.frame_len > 2000));
}

#[test]
fn file_transfer() {
    let device = SimDevice::with_firmware();
    let client = device.serial_client();
    let data: Vec<u8> = (0..3000u32).map(|i| (i % 256) as u8).collect();

    client
        .fs_file_upload("/lfs1/serial.bin", &data[..], data.len() as u64, None)
        .unwrap();
    let mut downloaded = vec![];
    client
        .fs_file_download("/lfs1/serial.bin", &mut downloaded, None)
        .unwrap();
    assert_eq!(downloaded, data);
}

#[test]
fn file_transfer_with_auto_frame_size() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 4096,
        ..Default::default()
    });
    let client = device.serial_client();
    let data: Vec<u8> = (0..30_000u32).map(|i| (i % 253) as u8).collect();

    client.use_auto_frame_size().unwrap();
    client
        .fs_file_upload("/lfs1/serial.bin", &data[..], data.len() as u64, None)
        .unwrap();
    let requests = device.requests_for(group_id::FS, 0);
    assert!(requests.iter().all(|r| r.frame_len <= 4096 - 4));
    assert!(requests.iter().any(|r| r.frame_len > 4000));

    let mut downloaded = vec![];
    client
        .fs_file_download("/lfs1/serial.bin", &mut downloaded, None)
        .unwrap();
    assert_eq!(downloaded, data);
}
