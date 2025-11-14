mod common;
use common::EchoSerial;
use zephyr_mcumgr::MCUmgrClient;

#[test]
fn echo() {
    let mut client = MCUmgrClient::new_from_serial(EchoSerial::default());

    let request = "Hello world!";
    let response = client.os_echo(request).unwrap();

    assert_eq!(request, response);
}
