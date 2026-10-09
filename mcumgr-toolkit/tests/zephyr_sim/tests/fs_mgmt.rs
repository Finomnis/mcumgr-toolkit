//! File management group (group 8)

use std::collections::HashMap;

use mcumgr_toolkit::client::MCUmgrClientError;
use mcumgr_toolkit::commands::fs::{
    FileChecksumData, FileChecksumDataFormat, FileChecksumProperties,
};
use sha2::{Digest, Sha256};

use super::{device_error, group_error, smp_error};
use crate::sim::fs_mgmt::fs_mgmt_err;
use crate::sim::smp::{group_id, mgmt_err};
use crate::sim::{Config, SimDevice};

const FILE: u8 = 0;

fn test_data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 256) as u8).collect()
}

#[track_caller]
fn assert_fs_error(err: MCUmgrClientError, rc: u16) {
    assert_eq!(device_error(err), group_error(group_id::FS, rc));
}

#[test]
fn upload_and_download() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    for len in [1, 100, 339, 340, 341, 1000, 5000] {
        let data = test_data(len);
        let name = format!("/lfs1/file_{len}.bin");
        client
            .fs_file_upload(&name, &data[..], len as u64, None)
            .unwrap();
        assert_eq!(device.lock().fs.files[&name], data);
        assert!(!device.lock().fs.file_open());

        let mut downloaded = vec![];
        client
            .fs_file_download(&name, &mut downloaded, None)
            .unwrap();
        assert_eq!(downloaded, data);
        assert!(!device.lock().fs.file_open());
    }

    assert!(
        device
            .requests_for(group_id::FS, FILE)
            .iter()
            .all(|r| r.frame_len <= 384)
    );
    assert_eq!(device.lock().link.dropped_oversized, 0);
}

#[test]
fn upload_requests() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = test_data(1000);

    client
        .fs_file_upload("/lfs1/a.bin", &data[..], 1000, None)
        .unwrap();

    let requests = device.requests_for(group_id::FS, FILE);
    assert!(requests.len() > 1);
    assert_eq!(
        requests[0].field("len").unwrap().as_integer(),
        Some(1000.into())
    );
    let mut off = 0;
    for request in &requests {
        assert_eq!(
            request.field("name").unwrap().as_text(),
            Some("/lfs1/a.bin")
        );
        assert_eq!(request.field("off").unwrap().as_integer(), Some(off.into()));
        off += request.field("data").unwrap().as_bytes().unwrap().len();
    }
    assert!(requests[1..].iter().all(|r| r.field("len").is_none()));
    assert_eq!(off, 1000);
}

#[test]
fn download_uses_the_chunks_of_the_device() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = test_data(1000);
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/b.bin".into(), data.clone());

    let mut progress = vec![];
    let mut callback = |current: u64, total: u64| {
        progress.push((current, total));
        true
    };
    let mut downloaded = vec![];
    client
        .fs_file_download("/lfs1/b.bin", &mut downloaded, Some(&mut callback))
        .unwrap();
    assert_eq!(downloaded, data);

    // MCUMGR_GRP_FS_DL_CHUNK_SIZE for a 384 byte net_buf is 340
    assert_eq!(
        progress,
        [(0, 1000), (340, 1000), (680, 1000), (1000, 1000)]
    );
}

#[test]
fn upload_progress_and_cancellation() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = test_data(3000);

    let mut progress = vec![];
    let mut callback = |current: u64, total: u64| {
        progress.push((current, total));
        current < 1000
    };
    let err = client
        .fs_file_upload("/lfs1/c.bin", &data[..], 3000, Some(&mut callback))
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::ProgressCallbackError));
    assert!(progress.iter().all(|(_, total)| *total == 3000));
    assert!(progress.last().unwrap().0 >= 1000);
    assert!(device.lock().fs.files["/lfs1/c.bin"].len() < 3000);
}

#[test]
fn download_cancellation() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/d.bin".into(), test_data(2000));

    let mut stop = |current: u64, _: u64| current == 0;
    let mut downloaded = vec![];
    let err = client
        .fs_file_download("/lfs1/d.bin", &mut downloaded, Some(&mut stop))
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::ProgressCallbackError));

    // The device keeps the file open until told otherwise
    assert!(device.lock().fs.file_open());
    client.fs_file_close().unwrap();
    assert!(!device.lock().fs.file_open());
}

#[test]
fn upload_replaces_existing_files() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/e.txt".into(), test_data(5000));

    client
        .fs_file_upload("/lfs1/e.txt", &b"short"[..], 5, None)
        .unwrap();
    assert_eq!(device.lock().fs.files["/lfs1/e.txt"], b"short");
}

#[test]
fn upload_into_a_directory() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .fs_file_upload("/lfs1/logs/today.log", &b"log"[..], 3, None)
        .unwrap();
    assert_eq!(
        client.fs_file_status("/lfs1/logs/today.log").unwrap().len,
        3
    );
}

#[test]
fn upload_errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client
        .fs_file_upload("/rom/new.txt", &b"x"[..], 1, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::READ_ONLY_FILESYSTEM);

    let err = client
        .fs_file_upload("/nand/new.txt", &b"x"[..], 1, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::MOUNT_POINT_NOT_FOUND);

    let err = client
        .fs_file_upload("/lfs1/no/such/dir.txt", &b"x"[..], 1, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::MOUNT_POINT_NOT_FOUND);

    let err = client
        .fs_file_upload("relative.txt", &b"x"[..], 1, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_INVALID_NAME);

    // Longer than CONFIG_MCUMGR_GRP_FS_PATH_LEN
    let long_name = format!("/lfs1/{}", "x".repeat(59));
    let err = client
        .fs_file_upload(&long_name, &b"x"[..], 1, None)
        .unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));

    // The reader fails
    let err = client
        .fs_file_upload("/lfs1/f.txt", &b"too short"[..], 100, None)
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::ReaderError(_)));
}

#[test]
fn download_errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client
        .fs_file_download("/lfs1/missing.txt", std::io::sink(), None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_NOT_FOUND);

    let err = client
        .fs_file_download("/lfs1/logs", std::io::sink(), None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_IS_DIRECTORY);

    let err = client
        .fs_file_download("/unknown/file", std::io::sink(), None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_NOT_FOUND);

    struct FailingWriter;
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let err = client
        .fs_file_download("/rom/readme.txt", FailingWriter, None)
        .unwrap_err();
    assert!(matches!(err, MCUmgrClientError::WriterError(_)));
}

#[test]
fn download_from_a_read_only_mount() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let mut data = vec![];
    client
        .fs_file_download("/rom/readme.txt", &mut data, None)
        .unwrap();
    assert_eq!(data, b"Hello from the simulated Zephyr device!\n");
}

#[test]
fn download_an_empty_file() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    device.lock().fs.files.insert("/lfs1/empty".into(), vec![]);

    let mut data = vec![0xff];
    data.clear();
    client
        .fs_file_download("/lfs1/empty", &mut data, None)
        .unwrap();
    assert!(data.is_empty());
}

#[test]
fn status() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(client.fs_file_status("/rom/readme.txt").unwrap().len, 40);

    let err = client.fs_file_status("/rom/missing").unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_NOT_FOUND);

    let err = client.fs_file_status("/lfs1").unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_IS_DIRECTORY);

    let err = client.fs_file_status("/").unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_INVALID_NAME);
}

#[test]
fn checksums() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = test_data(1000);
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/sum.bin".into(), data.clone());

    let crc32 = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

    // The default is crc32 if it is enabled
    let response = client
        .fs_file_checksum("/lfs1/sum.bin", None::<&str>, 0, None)
        .unwrap();
    assert_eq!(response.r#type, "crc32");
    assert_eq!((response.off, response.len), (0, 1000));
    assert_eq!(
        response.output,
        FileChecksumData::Checksum(crc32.checksum(&data))
    );
    assert_eq!(
        response.output.hex(),
        format!("{:08x}", crc32.checksum(&data))
    );

    let response = client
        .fs_file_checksum("/lfs1/sum.bin", Some("sha256"), 0, None)
        .unwrap();
    assert_eq!(response.r#type, "sha256");
    assert_eq!(
        response.output,
        FileChecksumData::Hash(Sha256::digest(&data).to_vec().into())
    );

    let response = client
        .fs_file_checksum("/lfs1/sum.bin", Some("sha256"), 100, Some(200))
        .unwrap();
    assert_eq!((response.off, response.len), (100, 200));
    assert_eq!(
        response.output,
        FileChecksumData::Hash(Sha256::digest(&data[100..300]).to_vec().into())
    );

    // A length beyond the end of the file covers the rest of the file
    let response = client
        .fs_file_checksum("/lfs1/sum.bin", Some("crc32"), 900, Some(5000))
        .unwrap();
    assert_eq!((response.off, response.len), (900, 100));
    assert_eq!(
        response.output,
        FileChecksumData::Checksum(crc32.checksum(&data[900..]))
    );

    let requests = device.requests_for(group_id::FS, 2);
    assert!(requests[0].field("type").is_none());
    assert!(requests[0].field("off").is_none());
    assert!(requests[0].field("len").is_none());
    assert_eq!(
        requests[2].field("off").unwrap().as_integer(),
        Some(100.into())
    );
}

#[test]
fn checksum_errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    device.lock().fs.files.insert("/lfs1/empty".into(), vec![]);

    let err = client
        .fs_file_checksum("/lfs1/empty", None::<&str>, 0, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_EMPTY);

    let err = client
        .fs_file_checksum("/rom/readme.txt", None::<&str>, 40, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_OFFSET_LARGER_THAN_FILE);

    let err = client
        .fs_file_checksum("/rom/readme.txt", Some("md5"), 0, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::CHECKSUM_HASH_NOT_FOUND);

    let err = client
        .fs_file_checksum("/rom/missing", Some("crc32"), 0, None)
        .unwrap_err();
    assert_fs_error(err, fs_mgmt_err::FILE_NOT_FOUND);

    // Type names are limited to 8 characters, lengths must not be 0
    let err = client
        .fs_file_checksum("/rom/readme.txt", Some("sha256sum"), 0, None)
        .unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));
    let err = client
        .fs_file_checksum("/rom/readme.txt", None::<&str>, 0, Some(0))
        .unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));
}

#[test]
fn supported_checksum_types() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(
        client.fs_supported_checksum_types().unwrap(),
        HashMap::from([
            (
                "crc32".to_string(),
                FileChecksumProperties {
                    format: FileChecksumDataFormat::Numerical,
                    size: 4
                }
            ),
            (
                "sha256".to_string(),
                FileChecksumProperties {
                    format: FileChecksumDataFormat::ByteArray,
                    size: 32
                }
            ),
        ])
    );
}

#[test]
fn sha256_is_the_default_without_crc32() {
    let device = SimDevice::new(Config {
        fs_checksum_ieee_crc32: false,
        ..Default::default()
    });
    let client = device.client();

    let response = client
        .fs_file_checksum("/rom/readme.txt", None::<&str>, 0, None)
        .unwrap();
    assert_eq!(response.r#type, "sha256");
    assert_eq!(client.fs_supported_checksum_types().unwrap().len(), 1);
}

#[test]
fn checksums_not_enabled() {
    let device = SimDevice::new(Config {
        fs_checksum_ieee_crc32: false,
        fs_hash_sha256: false,
        ..Default::default()
    });
    let client = device.client();

    let err = client
        .fs_file_checksum("/rom/readme.txt", None::<&str>, 0, None)
        .unwrap_err();
    assert!(err.command_not_supported());
    let err = client.fs_supported_checksum_types().unwrap_err();
    assert!(err.command_not_supported());
}

#[test]
fn close() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // Closing without an open file is fine
    client.fs_file_close().unwrap();

    let mut cancel = |current: u64, _: u64| current < 100;
    let _ = client.fs_file_upload("/lfs1/g.bin", &test_data(1000)[..], 1000, Some(&mut cancel));
    assert!(device.lock().fs.file_open());

    client.fs_file_close().unwrap();
    assert!(!device.lock().fs.file_open());
}

#[test]
fn upload_an_empty_file() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client
        .fs_file_upload("/lfs1/empty.txt", &b""[..], 0, None)
        .unwrap();

    // The file has to exist afterwards, just like after uploading any other
    // file. fs_mgmt creates it for an upload request with `len` 0.
    assert_eq!(client.fs_file_status("/lfs1/empty.txt").unwrap().len, 0);
}

#[test]
fn upload_an_empty_file_over_an_existing_one() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/e.txt".into(), test_data(500));

    client
        .fs_file_upload("/lfs1/e.txt", &b""[..], 0, None)
        .unwrap();
    assert_eq!(client.fs_file_status("/lfs1/e.txt").unwrap().len, 0);
}

#[test]
fn restart_an_unfinished_upload() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = test_data(3000);

    let mut stop = |current: u64, _: u64| current < 1000;
    client
        .fs_file_upload("/lfs1/r.bin", &data[..], 3000, Some(&mut stop))
        .unwrap_err();
    // The device keeps the file open, waiting for the rest of the upload
    assert!(device.lock().fs.file_open());

    // Uploading the file again starts over at offset 0
    client
        .fs_file_upload("/lfs1/r.bin", &data[..], 3000, None)
        .unwrap();
    assert_eq!(device.lock().fs.files["/lfs1/r.bin"], data);
    assert!(!device.lock().fs.file_open());
}

#[test]
fn upload_sizes_around_the_chunk_size() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // The client uses the same chunk size for every request of an upload
    let name = "/lfs1/chunks.bin";
    client
        .fs_file_upload(name, &test_data(5000)[..], 5000, None)
        .unwrap();
    let chunk = device.requests_for(group_id::FS, FILE)[0]
        .field("data")
        .unwrap()
        .as_bytes()
        .unwrap()
        .len();
    assert!(chunk > 300);

    for len in [
        1,
        chunk - 1,
        chunk,
        chunk + 1,
        2 * chunk - 1,
        2 * chunk,
        2 * chunk + 1,
    ] {
        device.clear_requests();
        let data = test_data(len);
        client
            .fs_file_upload(name, &data[..], len as u64, None)
            .unwrap();

        assert_eq!(device.lock().fs.files[name], data, "length {len}");
        assert_eq!(
            device.requests_for(group_id::FS, FILE).len(),
            len.div_ceil(chunk),
            "length {len}"
        );
        // The device closes the file once `len` bytes arrived
        assert!(!device.lock().fs.file_open(), "length {len}");
    }
}

#[test]
fn download_sizes_around_the_chunk_size() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    // MCUMGR_GRP_FS_DL_CHUNK_SIZE
    let chunk = 340;

    for len in [
        1,
        chunk - 1,
        chunk,
        chunk + 1,
        2 * chunk - 1,
        2 * chunk,
        2 * chunk + 1,
        3 * chunk,
    ] {
        let data = test_data(len);
        device
            .lock()
            .fs
            .files
            .insert("/lfs1/dl.bin".into(), data.clone());
        device.clear_requests();

        let mut downloaded = vec![];
        client
            .fs_file_download("/lfs1/dl.bin", &mut downloaded, None)
            .unwrap();

        assert_eq!(downloaded, data, "length {len}");
        assert_eq!(
            device.requests_for(group_id::FS, FILE).len(),
            len.div_ceil(chunk),
            "length {len}"
        );
        assert!(!device.lock().fs.file_open(), "length {len}");
    }
}

#[test]
fn download_of_a_file_that_shrinks_during_the_transfer() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/rotating.log".into(), test_data(1000));

    // After the first chunk, the application truncates the file. The device
    // answers the following requests with empty data at the requested offset.
    let mut calls = 0;
    let mut truncate_after_first_chunk = |current: u64, _: u64| {
        calls += 1;
        if current > 0 {
            if let Some(file) = device.lock().fs.files.get_mut("/lfs1/rotating.log") {
                file.truncate(100);
            }
        }
        // Guards this test against hanging
        calls < 50
    };
    let mut downloaded = vec![];
    let err = client
        .fs_file_download(
            "/lfs1/rotating.log",
            &mut downloaded,
            Some(&mut truncate_after_first_chunk),
        )
        .unwrap_err();

    // The client has to give up instead of requesting the same offset forever
    assert!(
        matches!(err, MCUmgrClientError::SizeMismatch),
        "expected SizeMismatch, got {err:?}"
    );
    assert!(device.requests_for(group_id::FS, FILE).len() < 5);
}

#[test]
fn upload_reads_only_the_given_size_from_the_reader() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = test_data(1000);

    client
        .fs_file_upload("/lfs1/part.bin", &data[..], 700, None)
        .unwrap();
    assert_eq!(device.lock().fs.files["/lfs1/part.bin"], data[..700]);
}

#[test]
fn file_names_at_the_length_limit_and_with_unicode() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // CONFIG_MCUMGR_GRP_FS_PATH_LEN bytes exactly
    let longest = format!("/lfs1/{}", "n".repeat(58));
    assert_eq!(longest.len(), 64);
    // Multi byte characters count as bytes
    let unicode = "/lfs1/größe-日本.txt";

    for name in [longest.as_str(), unicode] {
        let data = test_data(500);
        client.fs_file_upload(name, &data[..], 500, None).unwrap();
        assert_eq!(client.fs_file_status(name).unwrap().len, 500);
        let mut downloaded = vec![];
        client
            .fs_file_download(name, &mut downloaded, None)
            .unwrap();
        assert_eq!(downloaded, data);
        client
            .fs_file_checksum(name, Some("sha256"), 0, None)
            .unwrap();
    }

    let unicode_too_long = format!("/lfs1/{}", "ü".repeat(30));
    assert_eq!(unicode_too_long.len(), 66);
    let err = client.fs_file_status(&unicode_too_long).unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));
}

#[test]
fn checksum_of_the_last_byte() {
    let device = SimDevice::with_firmware();
    let client = device.client();
    let data = test_data(1000);
    device
        .lock()
        .fs
        .files
        .insert("/lfs1/sum.bin".into(), data.clone());

    let response = client
        .fs_file_checksum("/lfs1/sum.bin", Some("sha256"), 999, None)
        .unwrap();
    assert_eq!((response.off, response.len), (999, 1));
    assert_eq!(
        response.output,
        FileChecksumData::Hash(Sha256::digest(&data[999..]).to_vec().into())
    );
}
