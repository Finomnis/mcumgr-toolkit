//! The client's UDP transport against the device served over UDP

use std::net::{Ipv4Addr, UdpSocket};
use std::time::Duration;

use mcumgr_toolkit::MCUmgrClient;

use super::assert_timeout;
use crate::sim::image::ImageBuilder;
use crate::sim::smp::group_id;
use crate::sim::udp::UdpServer;
use crate::sim::{Config, SimDevice};

#[test]
fn new_from_udp() {
    let device = SimDevice::with_firmware();
    let server = UdpServer::start(device.clone());
    let client = MCUmgrClient::new_from_udp(server.addr(), Duration::from_secs(5)).unwrap();

    assert_eq!(client.os_echo("over UDP").unwrap(), "over UDP");
    client.check_connection().unwrap();
    assert_eq!(client.os_mcumgr_parameters().unwrap().buf_size, 384);
    assert_eq!(device.requests().len(), 3);
}

#[test]
fn auto_frame_size_is_capped_by_the_udp_transport() {
    // CONFIG_MCUMGR_TRANSPORT_NETBUF_SIZE defaults to 2048 with UDP
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 2048,
        mcumgr_transport_netbuf_count: 2,
        ..Default::default()
    });
    device
        .lock()
        .flash_image(0, &ImageBuilder::new((1, 0, 0, 0)).build());
    let server = UdpServer::start(device.clone());
    let client = MCUmgrClient::new_from_udp(server.addr(), Duration::from_secs(5)).unwrap();

    client.use_auto_frame_size().unwrap();
    let image = ImageBuilder::new((2, 0, 0, 0))
        .body(vec![0x10; 30_000])
        .build();
    client
        .image_upload(&image, None, None, false, None)
        .unwrap();

    let requests = device.requests_for(group_id::IMAGE, 1);
    assert!(requests.iter().all(|r| r.frame_len <= 1024));
    assert!(requests.iter().any(|r| r.frame_len > 900));
    assert_eq!(device.lock().img.slots[1].flash[..image.len()], image[..]);
}

#[test]
fn an_unresponsive_device_times_out() {
    let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let client =
        MCUmgrClient::new_from_udp(silent.local_addr().unwrap(), Duration::from_millis(20))
            .unwrap();
    client.set_retries(1);

    assert_timeout(client.os_echo("anyone there?").unwrap_err());
}
