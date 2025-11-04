use std::time::{Duration, SystemTime};

use miette::IntoDiagnostic;
use zephyr_mcumgr::MCUmgrClient;

fn main() -> miette::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let serial = serialport::new("COM14", 115200)
        .timeout(Duration::from_millis(500))
        .open()
        .into_diagnostic()?;

    let mut client = MCUmgrClient::from_serial(serial);
    client.use_auto_frame_size()?;

    println!("{:?}", client.os_echo("Hello world!")?);

    // let t0 = SystemTime::now();
    // let iters: usize = 1000;
    // for _ in 0..iters {
    //     client.os_echo("Hello world!")?;
    // }
    // let t1 = SystemTime::now();

    // let duration = t1.duration_since(t0).unwrap().as_secs_f32();
    // println!("{:?}", iters as f32 / duration);

    let mut data = vec![];
    let t0 = SystemTime::now();
    client.fs_file_download("/internal/go.tiff", &mut data)?;
    let t1 = SystemTime::now();
    let duration = t1.duration_since(t0).unwrap().as_secs_f32();
    println!("Download: {} bytes/s", data.len() as f32 / duration);

    // let data = b"12345678";

    let t0 = SystemTime::now();
    client.fs_file_upload("/internal/go2.tiff", data.as_slice(), data.len() as u64)?;
    let t1 = SystemTime::now();
    let duration = t1.duration_since(t0).unwrap().as_secs_f32();
    println!("Upload: {} bytes/s", data.len() as f32 / duration);

    Ok(())
}
