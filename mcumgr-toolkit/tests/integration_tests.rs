mod common;
use common::EchoSerial;
use mcumgr_toolkit::MCUmgrClient;
use rand::prelude::*;

#[test]
fn echo() {
    let client = MCUmgrClient::new_from_serial(EchoSerial::default());

    let request = "Hello world!";
    let response = client.os_echo(request).unwrap();
    assert_eq!(request, response);

    let request: String = rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(10000)
        .map(char::from)
        .collect();
    let response = client.os_echo(&request).unwrap();
    assert_eq!(request, response);
}
