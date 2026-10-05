use crate::{
    assert_group_error,
    zephyr_sim::{
        Config, Fault, client, client_with,
        wire::{bytes, map, number, uint},
    },
};
use mcumgr_toolkit::{
    client::MCUmgrClientError,
    commands::fs::{FileChecksumData, FileChecksumDataFormat},
};
use std::io::{self, Read, Write};

#[test]
fn file_round_trips_at_chunk_and_cbor_integer_boundaries() {
    for size in [1, 23, 24, 37, 38, 127, 128, 255, 256, 1024, 4097] {
        let (client, handle) = client();
        client.set_frame_size(128);
        let input: Vec<u8> = (0..size).map(|i| (i * 71) as u8).collect();
        client
            .fs_file_upload("/lfs/round-trip", input.as_slice(), size as u64, None)
            .unwrap();
        assert_eq!(
            handle.inspect(|d| d.files.get("/lfs/round-trip").cloned()),
            Some(input.clone()),
            "size {size}"
        );
        assert_eq!(
            client.fs_file_status("/lfs/round-trip").unwrap().len,
            size as u64
        );
        let mut output = Vec::new();
        client
            .fs_file_download("/lfs/round-trip", &mut output, None)
            .unwrap();
        assert_eq!(output, input, "size {size}");
        assert!(handle.requests().iter().all(|r| r.frame_len() <= 128));
        let upload: Vec<_> = handle
            .requests()
            .into_iter()
            .filter(|r| r.group() == 8 && r.id() == 0 && r.op() == 2)
            .collect();
        let mut offset = 0;
        for request in upload {
            assert_eq!(request.get("off").and_then(number), Some(offset));
            assert_eq!(
                request.get("name").unwrap().as_text(),
                Some("/lfs/round-trip")
            );
            if offset == 0 {
                assert_eq!(request.get("len").and_then(number), Some(size as u64));
            }
            offset += request.get("data").unwrap().as_bytes().unwrap().len() as u64;
        }
        assert_eq!(offset, size as u64);
    }
}

#[test]
fn empty_file_upload_creates_file_on_device() {
    let (client, handle) = client();
    client
        .fs_file_upload("/lfs/empty", [].as_slice(), 0, None)
        .unwrap();
    assert_eq!(
        handle.inspect(|d| d.files.get("/lfs/empty").cloned()),
        Some(Vec::new())
    );
    assert_eq!(client.fs_file_status("/lfs/empty").unwrap().len, 0);
}

#[test]
fn empty_file_download() {
    let (client, handle) = client();
    handle.edit(|d| {
        d.files.insert("/lfs/empty".into(), Vec::new());
    });
    let mut output = Vec::new();
    client
        .fs_file_download("/lfs/empty", &mut output, None)
        .unwrap();
    assert!(output.is_empty());
}

#[test]
fn file_download_is_checked_against_preexisting_device_bytes() {
    let (client, handle) = client();
    let expected: Vec<_> = (0..201).map(|i| (i * 11) as u8).collect();
    handle.edit(|d| {
        d.files.insert("/lfs/input".into(), expected.clone());
    });
    let mut data = Vec::new();
    let mut progress = Vec::new();
    client
        .fs_file_download(
            "/lfs/input",
            &mut data,
            Some(&mut |done, total| {
                progress.push((done, total));
                true
            }),
        )
        .unwrap();
    assert_eq!(data, expected);
    assert_progress(&progress, expected.len() as u64);
    let requests = handle.requests();
    assert!(requests.len() > 1);
    for (n, r) in requests.iter().filter(|r| r.id() == 0).enumerate() {
        assert_eq!(r.get("off").and_then(number), Some((n * 37) as u64));
    }
}

fn assert_progress(progress: &[(u64, u64)], total: u64) {
    assert!(!progress.is_empty());
    assert!(
        progress
            .iter()
            .all(|(done, reported_total)| *done <= total && *reported_total == total)
    );
    assert!(progress.windows(2).all(|w| w[0].0 <= w[1].0));
    assert_eq!(progress.last(), Some(&(total, total)));
}

#[test]
fn file_upload_progress_and_overwrite() {
    let (client, handle) = client();
    handle.edit(|d| {
        d.files.insert("/lfs/out".into(), vec![0xff; 900]);
    });
    client.set_frame_size(128);
    let data: Vec<u8> = (0..501).map(|i| i as u8).collect();
    let mut progress = Vec::new();
    client
        .fs_file_upload(
            "/lfs/out",
            data.as_slice(),
            data.len() as u64,
            Some(&mut |n, size| {
                progress.push((n, size));
                true
            }),
        )
        .unwrap();
    assert_eq!(handle.inspect(|d| d.files["/lfs/out"].clone()), data);
    assert_progress(&progress, data.len() as u64);
}

struct ShortReader<'a>(&'a [u8]);
impl Read for ShortReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let len = out.len().min(self.0.len()).min(3);
        out[..len].copy_from_slice(&self.0[..len]);
        self.0 = &self.0[len..];
        Ok(len)
    }
}

struct ShortWriter(Vec<u8>);
impl Write for ShortWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let len = input.len().min(2);
        self.0.extend_from_slice(&input[..len]);
        Ok(len)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn partial_reader_and_writer_operations_are_handled() {
    let (client, handle) = client();
    let expected: Vec<u8> = (0..197).map(|i| i as u8).collect();
    client
        .fs_file_upload(
            "/lfs/short",
            ShortReader(&expected),
            expected.len() as u64,
            None,
        )
        .unwrap();
    assert_eq!(handle.inspect(|d| d.files["/lfs/short"].clone()), expected);
    let mut writer = ShortWriter(Vec::new());
    client
        .fs_file_download("/lfs/short", &mut writer, None)
        .unwrap();
    assert_eq!(writer.0, expected);
}

struct BrokenIo;
impl Read for BrokenIo {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("reader failed"))
    }
}
impl Write for BrokenIo {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("writer failed"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn file_reader_and_writer_errors_preserve_error_category() {
    let (client, handle) = client();
    assert!(matches!(
        client.fs_file_upload("/lfs/broken", BrokenIo, 100, None),
        Err(MCUmgrClientError::ReaderError(_))
    ));
    handle.edit(|d| {
        d.files.insert("/lfs/existing".into(), b"hello".to_vec());
    });
    assert!(matches!(
        client.fs_file_download("/lfs/existing", BrokenIo, None),
        Err(MCUmgrClientError::WriterError(_))
    ));
}

#[test]
fn file_upload_premature_eof_is_an_error() {
    let (client, _) = client();
    assert!(
        client
            .fs_file_upload("/lfs/truncated", b"short".as_slice(), 500, None)
            .is_err()
    );
}

#[test]
fn file_transfer_callbacks_can_cancel_and_connection_remains_usable() {
    for upload in [false, true] {
        let (client, handle) = client();
        client.set_frame_size(128);
        let data = vec![0x5a; 800];
        handle.edit(|d| {
            d.files.insert("/lfs/file".into(), data.clone());
        });
        let mut calls = 0;
        let mut cancel = |_: u64, _: u64| {
            calls += 1;
            false
        };
        let result = if upload {
            client.fs_file_upload("/lfs/new", data.as_slice(), 800, Some(&mut cancel))
        } else {
            client.fs_file_download("/lfs/file", Vec::new(), Some(&mut cancel))
        };
        assert!(matches!(
            result,
            Err(MCUmgrClientError::ProgressCallbackError)
        ));
        assert_eq!(calls, 1);
        client.fs_file_close().unwrap();
        assert!(handle.inspect(|d| d.open_file.is_none()));
        assert_eq!(client.os_echo("still alive").unwrap(), "still alive");
    }
}

#[test]
fn file_download_retries_lost_response_without_duplicate_output() {
    let (client, handle) = client();
    client.set_retries(1);
    let expected = vec![0x3a; 300];
    handle.edit(|d| {
        d.files.insert("/lfs/file".into(), expected.clone());
    });
    handle.fault_on(8, 0, 1, Fault::LoseReply);
    let mut output = Vec::new();
    client
        .fs_file_download("/lfs/file", &mut output, None)
        .unwrap();
    assert_eq!(output, expected);
    handle.assert_faults_consumed();
}

#[test]
fn file_close_is_idempotent() {
    let (client, handle) = client();
    client.fs_file_close().unwrap();
    client.fs_file_close().unwrap();
    assert_eq!(handle.inspect(|d| d.file_closes), 2);
}

#[test]
fn file_missing_and_checksum_errors_use_zephyr_group_codes() {
    let (client, handle) = client();
    assert_group_error(client.fs_file_status("/lfs/missing").unwrap_err(), 8, 3);
    assert_group_error(
        client
            .fs_file_download("/lfs/missing", Vec::new(), None)
            .unwrap_err(),
        8,
        3,
    );
    handle.edit(|d| {
        d.files.insert("/lfs/empty".into(), vec![]);
        d.files.insert("/lfs/data".into(), b"abc".to_vec());
    });
    assert_group_error(
        client
            .fs_file_checksum("/lfs/empty", None::<&str>, 0, None)
            .unwrap_err(),
        8,
        16,
    );
    assert_group_error(
        client
            .fs_file_checksum("/lfs/data", Some("md5"), 0, None)
            .unwrap_err(),
        8,
        13,
    );
    assert_group_error(
        client
            .fs_file_checksum("/lfs/data", None::<&str>, 3, None)
            .unwrap_err(),
        8,
        12,
    );
}

#[test]
fn checksum_types_and_independent_known_vectors() {
    let (client, handle) = client();
    handle.edit(|d| {
        d.files.insert("/lfs/crc".into(), b"123456789".to_vec());
        d.files
            .insert("/lfs/hash".into(), b"prefixabcSUFFIX".to_vec());
    });
    let types = client.fs_supported_checksum_types().unwrap();
    assert_eq!(types.len(), 2);
    assert_eq!(types["crc32"].size, 4);
    assert!(matches!(
        types["crc32"].format,
        FileChecksumDataFormat::Numerical
    ));
    assert_eq!(types["sha256"].size, 32);
    assert!(matches!(
        types["sha256"].format,
        FileChecksumDataFormat::ByteArray
    ));
    let crc = client
        .fs_file_checksum("/lfs/crc", None::<&str>, 0, None)
        .unwrap();
    assert_eq!((crc.r#type.as_str(), crc.off, crc.len), ("crc32", 0, 9));
    assert!(matches!(
        crc.output,
        FileChecksumData::Checksum(0xcbf4_3926)
    ));
    let hash = client
        .fs_file_checksum("/lfs/hash", Some("sha256"), 6, Some(3))
        .unwrap();
    assert_eq!((hash.r#type.as_str(), hash.off, hash.len), ("sha256", 6, 3));
    match hash.output {
        FileChecksumData::Hash(value) => assert_eq!(
            hex::encode(value),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ),
        other => panic!("expected SHA256 bytes, got {other:?}"),
    }
    let tail = client
        .fs_file_checksum("/lfs/hash", Some("crc32"), 9, Some(1000))
        .unwrap();
    assert_eq!(tail.len, 6, "length is bounded by EOF");
}

#[test]
fn malformed_download_offsets_sizes_and_missing_length_are_rejected() {
    let cases = [
        (
            map([
                ("off", uint(1u32)),
                ("len", uint(1u32)),
                ("data", bytes(b"x")),
            ]),
            "offset",
        ),
        (map([("off", uint(0u32)), ("data", bytes(b"x"))]), "length"),
        (
            map([
                ("off", uint(0u32)),
                ("len", uint(1u32)),
                ("data", bytes(b"too long")),
            ]),
            "size",
        ),
    ];
    for (body, category) in cases {
        let (client, handle) = client();
        handle.fault(Fault::Reply(body));
        let error = client
            .fs_file_download("/lfs/file", Vec::new(), None)
            .unwrap_err();
        assert!(
            match category {
                "offset" => matches!(error, MCUmgrClientError::UnexpectedOffset),
                "length" => matches!(error, MCUmgrClientError::MissingSize),
                _ => matches!(error, MCUmgrClientError::SizeMismatch),
            },
            "{category}: {error:?}"
        );
    }
}

#[test]
fn upload_rejects_impossible_acknowledgement() {
    let (client, handle) = client();
    handle.fault(Fault::Reply(map([("off", uint(999_999u32))])));
    let result = client.fs_file_upload("/lfs/file", b"data".as_slice(), 4, None);
    assert!(
        matches!(result, Err(MCUmgrClientError::UnexpectedOffset)),
        "{result:?}"
    );
}

#[test]
fn explicit_and_automatic_frame_limits_include_header_and_cbor_overhead() {
    for (device_size, transport_mtu, manual) in [
        (256, 192, false),
        (192, 256, false),
        (512, usize::MAX, true),
    ] {
        let (client, handle) = client_with(Config {
            buffer_size: device_size,
            transport_mtu,
            ..Config::default()
        });
        let limit = if manual {
            client.set_frame_size(128);
            128
        } else {
            client.use_auto_frame_size().unwrap();
            device_size.min(transport_mtu)
        };
        let data = vec![0xa5; 1500];
        client
            .fs_file_upload("/lfs/name-µ", data.as_slice(), data.len() as u64, None)
            .unwrap();
        assert_eq!(handle.inspect(|d| d.files["/lfs/name-µ"].clone()), data);
        let requests = handle.requests();
        assert!(requests.iter().all(|r| r.frame_len() <= limit));
        assert!(requests.iter().filter(|r| r.group() == 8).count() > 1);
    }
}

#[test]
fn too_small_transfer_frame_fails_before_io() {
    let (client, handle) = client();
    client.set_frame_size(8);
    assert!(matches!(
        client.fs_file_upload("/lfs/file", b"x".as_slice(), 1, None),
        Err(MCUmgrClientError::FrameSizeTooSmall(_))
    ));
    assert!(handle.requests().is_empty());
}
