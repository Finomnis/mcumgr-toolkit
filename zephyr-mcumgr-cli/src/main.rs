use std::time::{Duration, SystemTime};

use miette::IntoDiagnostic;
use zephyr_mcumgr::MCUmgrClient;

fn main() -> miette::Result<()> {
    let serial = serialport::new("COM14", 115200)
        .timeout(Duration::from_millis(500))
        .open()
        .into_diagnostic()?;

    let mut client = MCUmgrClient::from_serial(serial);

    println!("{:?}", client.os_echo("Hello world!")?);

    let t0 = SystemTime::now();
    let iters: usize = 1000;
    for _ in 0..iters {
        client.os_echo("Hello world!")?;
    }
    let t1 = SystemTime::now();

    let duration = t1.duration_since(t0).unwrap().as_secs_f32();
    println!("{:?}", iters as f32 / duration);

    Ok(())
}
