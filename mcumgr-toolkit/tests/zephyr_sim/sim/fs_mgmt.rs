//! File system management group, modelled after
//! `subsys/mgmt/mcumgr/grp/fs_mgmt/src/fs_mgmt*.c`, on top of a small
//! model of Zephyr's VFS (`subsys/fs/fs.c`).

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use super::Device;
use ciborium::Value;

use super::cbor::{Kind, int, map, text, uint};
use super::smp::{Ctx, Group, group_id, mgmt_err, read, read_write, write};

const FS_MGMT_ID_FILE: u8 = 0;
const FS_MGMT_ID_STAT: u8 = 1;
const FS_MGMT_ID_HASH_CHECKSUM: u8 = 2;
const FS_MGMT_ID_SUPPORTED_HASH_CHECKSUM: u8 = 3;
const FS_MGMT_ID_OPENED_FILE: u8 = 4;

/// `enum fs_mgmt_err_code_t`
#[allow(dead_code)]
pub mod fs_mgmt_err {
    pub const UNKNOWN: u16 = 1;
    pub const FILE_INVALID_NAME: u16 = 2;
    pub const FILE_NOT_FOUND: u16 = 3;
    pub const FILE_IS_DIRECTORY: u16 = 4;
    pub const FILE_OPEN_FAILED: u16 = 5;
    pub const FILE_SEEK_FAILED: u16 = 6;
    pub const FILE_READ_FAILED: u16 = 7;
    pub const FILE_TRUNCATE_FAILED: u16 = 8;
    pub const FILE_DELETE_FAILED: u16 = 9;
    pub const FILE_WRITE_FAILED: u16 = 10;
    pub const FILE_OFFSET_NOT_VALID: u16 = 11;
    pub const FILE_OFFSET_LARGER_THAN_FILE: u16 = 12;
    pub const CHECKSUM_HASH_NOT_FOUND: u16 = 13;
    pub const MOUNT_POINT_NOT_FOUND: u16 = 14;
    pub const READ_ONLY_FILESYSTEM: u16 = 15;
    pub const FILE_EMPTY: u16 = 16;
}
use fs_mgmt_err as err;

const EINVAL: i32 = 22;
const ENOENT: i32 = 2;
const EROFS: i32 = 30;
const EISDIR: i32 = 21;

#[derive(Clone, Debug)]
pub struct MountPoint {
    pub path: String,
    pub read_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferState {
    Upload,
    Download,
}

/// `fs_mgmt_ctxt`
#[derive(Clone, Debug)]
struct Transfer {
    state: TransferState,
    off: u64,
    /// `len`, if `len_known`
    len: Option<u64>,
    path: String,
}

#[derive(Clone, Debug)]
pub struct FsState {
    pub mounts: Vec<MountPoint>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub dirs: BTreeSet<String>,
    transfer: Option<Transfer>,
}

impl Default for FsState {
    fn default() -> Self {
        Self {
            mounts: vec![
                MountPoint {
                    path: "/lfs1".into(),
                    read_only: false,
                },
                MountPoint {
                    path: "/rom".into(),
                    read_only: true,
                },
            ],
            files: BTreeMap::from([(
                "/rom/readme.txt".to_string(),
                b"Hello from the simulated Zephyr device!\n".to_vec(),
            )]),
            dirs: BTreeSet::from(["/lfs1/logs".to_string()]),
            transfer: None,
        }
    }
}

enum DirEntry {
    File(u64),
    Dir,
}

impl FsState {
    /// `fs_mgmt_cleanup`
    pub fn cleanup(&mut self) {
        self.transfer = None;
    }

    /// Whether a transfer is ongoing, i.e. a file is held open
    pub fn file_open(&self) -> bool {
        self.transfer.is_some()
    }

    fn mount_point(&self, path: &str) -> Option<&MountPoint> {
        self.mounts.iter().find(|mp| {
            path.strip_prefix(&mp.path)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    }

    /// `fs_stat`
    fn stat(&self, path: &str) -> Result<DirEntry, i32> {
        if path.len() <= 1 || !path.starts_with('/') {
            return Err(-EINVAL);
        }
        let mp = self.mount_point(path).ok_or(-ENOENT)?;
        if path == mp.path || self.dirs.contains(path) {
            return Ok(DirEntry::Dir);
        }
        let file = self.files.get(path).ok_or(-ENOENT)?;
        Ok(DirEntry::File(file.len() as u64))
    }

    /// `fs_open(path, FS_O_CREATE | FS_O_WRITE)`
    fn open_for_writing(&mut self, path: &str) -> Result<(), i32> {
        if path.len() <= 1 || !path.starts_with('/') {
            return Err(-EINVAL);
        }
        let mp = self.mount_point(path).ok_or(-ENOENT)?;
        let read_only = mp.read_only;
        let mp_path = mp.path.clone();
        if path == mp_path || self.dirs.contains(path) {
            return Err(-EISDIR);
        }
        // The parent directory has to exist
        let parent = &path[..path.rfind('/').unwrap()];
        if parent != mp_path && !self.dirs.contains(parent) {
            return Err(-ENOENT);
        }
        if read_only {
            return Err(-EROFS);
        }
        self.files.entry(path.to_string()).or_default();
        Ok(())
    }

    /// `fs_mgmt_filelen`
    fn filelen(&self, path: &str) -> Result<u64, u16> {
        match self.stat(path) {
            Ok(DirEntry::File(len)) => Ok(len),
            Ok(DirEntry::Dir) => Err(err::FILE_IS_DIRECTORY),
            Err(rc) if rc == -EINVAL => Err(err::FILE_INVALID_NAME),
            Err(rc) if rc == -ENOENT => Err(err::FILE_NOT_FOUND),
            Err(_) => Err(err::UNKNOWN),
        }
    }

    /// `fs_mgmt_upload_download_finish_check`
    fn finish_check(&mut self) {
        if let Some(t) = &self.transfer {
            if t.len.is_some_and(|len| t.off >= len) {
                self.cleanup();
            }
        }
    }
}

/// A `struct fs_mgmt_hash_checksum_group`
struct HashChecksum {
    name: &'static str,
    byte_string: bool,
    output_size: u32,
}

const CRC32: HashChecksum = HashChecksum {
    name: "crc32",
    byte_string: false,
    output_size: 4,
};
const SHA256: HashChecksum = HashChecksum {
    name: "sha256",
    byte_string: true,
    output_size: 32,
};

impl Device {
    /// Registered hash/checksum handlers, in registration order
    fn fs_hash_checksums(&self) -> Vec<HashChecksum> {
        let mut result = vec![];
        if self.config.fs_checksum_ieee_crc32 {
            result.push(CRC32);
        }
        if self.config.fs_hash_sha256 {
            result.push(SHA256);
        }
        result
    }

    /// `MCUMGR_GRP_FS_DL_CHUNK_SIZE`
    fn fs_dl_chunk_size(&self) -> usize {
        let max_offset_len = self.config.fs_max_offset_len;
        let cbor_and_other_hdr = 8
            + (9 + 1)
            + (1 + 3 + max_offset_len)
            + (1 + 4 + max_offset_len)
            + (1 + 2 + 1)
            + (1 + 3 + max_offset_len);
        self.config.mcumgr_transport_netbuf_size - cbor_and_other_hdr
    }
}

pub fn group(config: &super::Config) -> Group {
    let mut handlers = vec![
        (FS_MGMT_ID_FILE, read_write(file_download, file_upload)),
        (FS_MGMT_ID_STAT, read(file_status)),
        (FS_MGMT_ID_OPENED_FILE, write(close_opened_file)),
    ];
    if config.fs_checksum_ieee_crc32 || config.fs_hash_sha256 {
        handlers.push((FS_MGMT_ID_HASH_CHECKSUM, read(file_hash_checksum)));
        handlers.push((
            FS_MGMT_ID_SUPPORTED_HASH_CHECKSUM,
            read(supported_hash_checksum),
        ));
    }
    Group::new(group_id::FS, "fs mgmt", &handlers)
}

/// `fs_mgmt_file_download`
fn file_download(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("off", Kind::U64), ("name", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    let Some(off) = decoded.uint("off") else {
        return Err(mgmt_err::EINVAL);
    };
    if name.is_empty() || name.len() > device.config.fs_path_len {
        return Err(mgmt_err::EINVAL);
    }

    let fs = &mut device.fs;
    if !matches!(&fs.transfer, Some(t) if t.state == TransferState::Download && t.path == name) {
        fs.cleanup();
    }

    if fs.transfer.is_none() {
        let len = match fs.filelen(name) {
            Ok(len) => len,
            Err(rc) => {
                ctx.add_cmd_err(group_id::FS, rc);
                return Ok(());
            }
        };
        fs.transfer = Some(Transfer {
            state: TransferState::Download,
            off: 0,
            len: Some(len),
            path: name.to_string(),
        });
    }

    let chunk_size = device.fs_dl_chunk_size();
    let fs = &mut device.fs;
    let file = fs.files.get(name).cloned().unwrap_or_default();
    let transfer = fs.transfer.as_mut().unwrap();

    // Seek, then read up to MCUMGR_GRP_FS_DL_CHUNK_SIZE bytes
    transfer.off = off;
    let start = (off as usize).min(file.len());
    let data = file[start..(start + chunk_size).min(file.len())].to_vec();
    transfer.off += data.len() as u64;
    let len = transfer.len.unwrap();

    if device.config.smp_legacy_rc_behaviour {
        ctx.put("rc", int(0));
    }
    ctx.put("off", uint(off));
    ctx.put("data", Value::Bytes(data));
    if off == 0 {
        ctx.put("len", uint(len));
    }

    fs.finish_check();
    Ok(())
}

/// `fs_mgmt_file_upload`
fn file_upload(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[
            ("off", Kind::U64),
            ("name", Kind::Tstr),
            ("data", Kind::Bstr),
            ("len", Kind::U64),
        ])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    let data = decoded.bytes("data").unwrap_or_default();
    let len = decoded.uint("len");
    let Some(off) = decoded.uint("off") else {
        return Err(mgmt_err::EINVAL);
    };
    if name.is_empty() || name.len() > device.config.fs_path_len || (off == 0 && len.is_none()) {
        return Err(mgmt_err::EINVAL);
    }

    let fs = &mut device.fs;
    if !matches!(&fs.transfer, Some(t) if t.state == TransferState::Upload && t.path == name) {
        fs.cleanup();
    }

    if fs.transfer.is_none() {
        if let Err(rc) = fs.open_for_writing(name) {
            let rc = match -rc {
                EINVAL => err::FILE_INVALID_NAME,
                ENOENT => err::MOUNT_POINT_NOT_FOUND,
                EROFS => err::READ_ONLY_FILESYSTEM,
                _ => err::UNKNOWN,
            };
            ctx.add_cmd_err(group_id::FS, rc);
            return Ok(());
        }
        fs.transfer = Some(Transfer {
            state: TransferState::Upload,
            off: 0,
            len: None,
            path: name.to_string(),
        });
    }

    let mut existing_file_size = 0;
    if off == 0 {
        let transfer = fs.transfer.as_mut().unwrap();
        transfer.len = len;
        match fs.filelen(name) {
            Ok(size) => existing_file_size = size,
            Err(rc) => {
                ctx.add_cmd_err(group_id::FS, rc);
                fs.cleanup();
                return Ok(());
            }
        }
    } else if fs.transfer.as_ref().unwrap().off == 0 {
        match fs.filelen(name) {
            Ok(size) => fs.transfer.as_mut().unwrap().off = size,
            Err(rc) => {
                ctx.add_cmd_err(group_id::FS, rc);
                fs.cleanup();
                return Ok(());
            }
        }
    }

    let expected_off = fs.transfer.as_ref().unwrap().off;
    if off > 0 && off != expected_off {
        // Offset mismatch, send file length, client needs to handle this
        ctx.add_cmd_err(group_id::FS, err::FILE_OFFSET_NOT_VALID);
        ctx.put("len", uint(expected_off));
        fs.cleanup();
        return Ok(());
    }

    if !data.is_empty() || off == 0 {
        let file = fs.files.get_mut(name).unwrap();
        if off == 0 && existing_file_size != 0 {
            file.clear();
        }
        file.extend_from_slice(data);
        fs.transfer.as_mut().unwrap().off += data.len() as u64;
    }

    let ctxt_off = fs.transfer.as_ref().unwrap().off;
    fs.finish_check();

    if device.config.smp_legacy_rc_behaviour {
        ctx.put("rc", int(0));
    }
    ctx.put("off", uint(ctxt_off));
    Ok(())
}

/// `fs_mgmt_file_status`
fn file_status(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("name", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    if name.is_empty() || name.len() > device.config.fs_path_len {
        return Err(mgmt_err::EINVAL);
    }

    match device.fs.filelen(name) {
        Ok(len) => {
            if device.config.smp_legacy_rc_behaviour {
                ctx.put("rc", int(0));
            }
            ctx.put("len", uint(len));
        }
        Err(rc) => ctx.add_cmd_err(group_id::FS, rc),
    }
    Ok(())
}

/// `fs_mgmt_file_hash_checksum`
fn file_hash_checksum(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    const HASH_CHECKSUM_TYPE_SIZE: usize = 8;

    let decoded = ctx
        .decode(&[
            ("type", Kind::Tstr),
            ("name", Kind::Tstr),
            ("off", Kind::U64),
            ("len", Kind::U64),
        ])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    let ty = decoded.str("type").unwrap_or("");
    let off = decoded.uint("off").unwrap_or(0);
    let len = decoded.uint("len").unwrap_or(u64::MAX);
    if name.is_empty()
        || name.len() > device.config.fs_path_len
        || ty.len() > HASH_CHECKSUM_TYPE_SIZE
        || len == 0
    {
        return Err(mgmt_err::EINVAL);
    }

    let handlers = device.fs_hash_checksums();
    // MCUMGR_GRP_FS_CHECKSUM_HASH_DEFAULT
    let ty = if ty.is_empty() { handlers[0].name } else { ty };
    let Some(group) = handlers.iter().find(|h| h.name == ty) else {
        ctx.add_cmd_err(group_id::FS, err::CHECKSUM_HASH_NOT_FOUND);
        return Ok(());
    };

    let file_len = match device.fs.filelen(name) {
        Ok(len) => len,
        Err(rc) => {
            ctx.add_cmd_err(group_id::FS, rc);
            return Ok(());
        }
    };
    if file_len <= off {
        ctx.add_cmd_err(
            group_id::FS,
            if file_len == 0 {
                err::FILE_EMPTY
            } else {
                err::FILE_OFFSET_LARGER_THAN_FILE
            },
        );
        return Ok(());
    }

    let file = &device.fs.files[name];
    let data = &file[off as usize..];
    let data = &data[..data.len().min(usize::try_from(len).unwrap_or(usize::MAX))];

    ctx.put("type", text(group.name));
    if off != 0 {
        ctx.put("off", uint(off));
    }
    ctx.put("len", uint(data.len() as u64));
    if group.byte_string {
        ctx.put("output", Value::Bytes(Sha256::digest(data).to_vec()));
    } else {
        // crc32_ieee_update(0, ...)
        let crc = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC).checksum(data);
        ctx.put("output", uint(crc));
    }
    Ok(())
}

/// `fs_mgmt_supported_hash_checksum`
fn supported_hash_checksum(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let types = device
        .fs_hash_checksums()
        .into_iter()
        .map(|h| {
            (
                text(h.name),
                map([
                    ("format", uint(h.byte_string)),
                    ("size", uint(h.output_size)),
                ]),
            )
        })
        .collect();
    ctx.put("types", Value::Map(types));
    Ok(())
}

/// `fs_mgmt_close_opened_file`
fn close_opened_file(device: &mut Device, _: &mut Ctx) -> Result<(), i32> {
    device.fs.cleanup();
    Ok(())
}
