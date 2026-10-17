//! Stateful protocol model derived from upstream Zephyr's C handlers.
//! No mcumgr-toolkit imports: group IDs, commands, fields, errors and transitions
//! come from the source references recorded in README.md, not the Rust client.
use std::collections::BTreeMap;

use super::firmware::{crc32, firmware, image_hash, sha256, version, version_string};
use super::wire::{
    Args, RemoteError, Request, Result, Value, bytes, insert, int, map, number, text, uint,
};

pub const GROUPS: [u16; 8] = [0, 1, 2, 3, 8, 9, 10, 63];

#[derive(Clone, Debug)]
pub struct Config {
    pub buffer_size: usize,
    pub transport_mtu: usize,
    pub download_chunk: usize,
    pub legacy_success_rc: bool,
    pub compact_images: bool,
    pub minimal_tasks: bool,
    pub enum_details: bool,
    pub no_downgrade: bool,
    pub image_check: bool,
    pub image_count: u32,
    pub bootloader: String,
    pub settings_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            buffer_size: 4096,
            transport_mtu: usize::MAX,
            download_chunk: 37,
            legacy_success_rc: false,
            compact_images: true,
            minimal_tasks: false,
            enum_details: true,
            no_downgrade: false,
            image_check: true,
            image_count: 1,
            bootloader: "MCUboot".into(),
            settings_limit: 32,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub image: u32,
    pub slot: u32,
    pub data: Vec<u8>,
    pub active: bool,
    pub confirmed: bool,
    pub pending: bool,
    pub permanent: bool,
}

#[derive(Clone, Debug)]
pub struct Upload {
    pub image: u32,
    pub size: usize,
    pub sha: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Device {
    pub config: Config,
    pub files: BTreeMap<String, Vec<u8>>,
    pub settings: BTreeMap<String, Vec<u8>>,
    pub persisted: BTreeMap<String, Vec<u8>>,
    pub commits: usize,
    pub datetime: Option<String>,
    pub resets: Vec<(bool, Option<u64>)>,
    pub storage: Vec<u8>,
    pub storage_erases: usize,
    pub shell_calls: Vec<Vec<String>>,
    pub slots: Vec<Slot>,
    pub upload: Option<Upload>,
    pub last_upload: Option<Upload>,
    pub open_file: Option<String>,
    pub file_closes: usize,
    pub corrupt_upload: bool,
    file_upload: Option<(String, usize, usize)>,
}

impl Device {
    pub fn new(config: Config) -> Self {
        let slots = (0..config.image_count)
            .map(|image| Slot {
                image,
                slot: 0,
                data: firmware(1, 128),
                active: true,
                confirmed: true,
                pending: false,
                permanent: false,
            })
            .collect();
        Self {
            config,
            files: BTreeMap::new(),
            settings: BTreeMap::new(),
            persisted: BTreeMap::new(),
            commits: 0,
            datetime: None,
            resets: Vec::new(),
            storage: vec![0x12; 256],
            storage_erases: 0,
            shell_calls: Vec::new(),
            slots,
            upload: None,
            last_upload: None,
            open_file: None,
            file_closes: 0,
            corrupt_upload: false,
            file_upload: None,
        }
    }

    pub fn dispatch(&mut self, req: &Request) -> Value {
        let args = Args(&req.body);
        let result = match req.group() {
            0 => self.os(req.op(), req.id(), &args),
            1 => self.image(req.op(), req.id(), &args),
            2 => self.stats(req.op(), req.id(), &args),
            3 => self.settings(req.op(), req.id(), &args),
            8 => self.fs(req.op(), req.id(), &args),
            9 if req.op() == 2 && req.id() == 0 => self.shell(&args),
            10 if req.op() == 0 => self.enumeration(req.id(), &args),
            63 if req.op() == 2 && req.id() == 0 => {
                self.storage.fill(0xff);
                self.storage_erases += 1;
                self.persisted.clear();
                Ok(map([]))
            }
            _ => Err(RemoteError::Smp(8)),
        };
        match result {
            Ok(mut body) => {
                if self.config.legacy_success_rc {
                    insert(&mut body, "rc", int(0));
                }
                body
            }
            Err(error) => error.body(),
        }
    }

    // grp/os_mgmt/src/os_mgmt.c: handler table and os_mgmt_* handlers.
    fn os(&mut self, op: u8, id: u8, a: &Args<'_>) -> Result<Value> {
        match (op, id) {
            (0 | 2, 0) => Ok(map([("r", text(a.str("d")?))])),
            (0, 2) => {
                let mut main = map([
                    ("prio", int(-2)),
                    ("tid", uint(1u32)),
                    ("state", uint(4u32)),
                ]);
                if !self.config.minimal_tasks {
                    // The C handler divides byte counts by four on the wire.
                    for (key, val) in [
                        ("stksiz", 256u64),
                        ("stkuse", 75),
                        ("runtime", 4_294_967_301),
                        ("cswcnt", 0),
                    ] {
                        insert(&mut main, key, uint(val));
                    }
                }
                Ok(map([("tasks", map([("main", main)]))]))
            }
            (0, 3) => Ok(map([(
                "mpools",
                map([(
                    "0",
                    map([
                        ("blksiz", uint(1u32)),
                        ("nblks", uint(8192u32)),
                        ("nfree", uint(6000u32)),
                        ("min", uint(4096u32)),
                    ]),
                )]),
            )])),
            (0, 4) => Ok(map([(
                "datetime",
                text(self.datetime.as_ref().ok_or(RemoteError::Group(0, 4))?),
            )])),
            (2, 4) => {
                let value = a.str("datetime")?;
                let date = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                    .map_err(|_| RemoteError::Smp(3))?;
                // Model CONFIG_MCUMGR_GRP_OS_DATETIME_MS=y.
                self.datetime = Some(date.format("%Y-%m-%dT%H:%M:%S%.3f").to_string());
                Ok(map([]))
            }
            (2, 5) => {
                let force = a.boolean("force", false)?;
                let boot = a.opt_uint("boot_mode")?;
                if boot.is_some_and(|x| x > 1) {
                    return Err(RemoteError::Smp(3));
                }
                self.resets.push((force, boot));
                if boot != Some(1) {
                    self.reboot();
                }
                Ok(map([]))
            }
            (0, 6) => Ok(map([
                ("buf_size", uint(self.config.buffer_size as u64)),
                ("buf_count", uint(4u32)),
            ])),
            (0, 7) => {
                let format = a.opt_str("format")?.unwrap_or("");
                if format.chars().any(|c| !"asnrvbmpio".contains(c)) {
                    return Err(RemoteError::Group(0, 2));
                }
                let fields = [
                    ('s', "Zephyr"),
                    ('n', "sim"),
                    ('r', "main"),
                    ('v', "4.4.99"),
                    ('b', "Sep 27 2026 00:00:00"),
                    ('m', "arm"),
                    ('p', "cortex-m4"),
                    ('i', "sim_board"),
                    ('o', "Zephyr"),
                ];
                // Zephyr emits fields in a fixed order, not the supplied order.
                let output = fields
                    .iter()
                    .filter(|(c, _)| {
                        format.contains('a')
                            || format.contains(*c)
                            || (*c == 's' && format.is_empty())
                    })
                    .map(|(_, v)| *v)
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(map([("output", text(output))]))
            }
            (0, 8) => {
                match a.opt_str("query")? {
                    None | Some("") => Ok(map([("bootloader", text(&self.config.bootloader))])),
                    Some("mode") if self.config.bootloader == "MCUboot" => {
                        let mut out = map([("mode", int(1))]); // swap using scratch
                        if self.config.no_downgrade {
                            insert(&mut out, "no-downgrade", Value::Bool(true));
                        }
                        Ok(out)
                    }
                    _ => Err(RemoteError::Group(0, 3)),
                }
            }
            _ => Err(RemoteError::Smp(8)),
        }
    }

    // grp/stat_mgmt/src/stat_mgmt.c; fixture values represent application stats.
    fn stats(&self, op: u8, id: u8, a: &Args<'_>) -> Result<Value> {
        match (op, id) {
            (0, 0) => {
                let name = a.str("name")?;
                if name != "net" {
                    return Err(RemoteError::Group(2, 2));
                }
                Ok(map([
                    ("name", text(name)),
                    (
                        "fields",
                        map([
                            ("rx", uint(123u32)),
                            ("tx", uint(u32::MAX)),
                            ("errors", uint(0u32)),
                        ]),
                    ),
                ]))
            }
            (0, 1) => Ok(map([("stat_list", Value::Array(vec![text("net")]))])),
            _ => Err(RemoteError::Smp(8)),
        }
    }

    // grp/settings_mgmt/src/settings_mgmt.c. The application settings handler
    // is a byte-valued map; persistence and commit are distinct operations.
    fn settings(&mut self, op: u8, id: u8, a: &Args<'_>) -> Result<Value> {
        match (op, id) {
            (0 | 2, 0) | (2, 1) => {
                let name = a.str("name")?;
                if name.is_empty() {
                    return Err(RemoteError::Smp(3));
                }
                if name.len() >= 64 {
                    return Err(RemoteError::Group(3, 2));
                }
                if id == 1 {
                    self.settings.remove(name).ok_or(RemoteError::Group(3, 3))?;
                    return Ok(map([]));
                }
                if op == 2 {
                    self.settings.insert(name.into(), a.bytes("val")?.to_vec());
                    return Ok(map([]));
                }
                let value = self.settings.get(name).ok_or(RemoteError::Group(3, 3))?;
                let max =
                    a.opt_uint("max_size")?
                        .unwrap_or(self.config.settings_limit as u64) as usize;
                let mut out = map([(
                    "val",
                    bytes(&value[..value.len().min(max).min(self.config.settings_limit)]),
                )]);
                if max > self.config.settings_limit {
                    insert(
                        &mut out,
                        "max_size",
                        uint(self.config.settings_limit as u64),
                    );
                }
                Ok(out)
            }
            (2, 2) => {
                self.commits += 1;
                Ok(map([]))
            }
            (0, 3) => {
                self.settings.extend(self.persisted.clone());
                Ok(map([]))
            }
            (2, 3) => {
                let subtree = a.opt_str("name")?;
                for (key, value) in &self.settings {
                    if subtree.is_none_or(|prefix| {
                        key == prefix || key.starts_with(&format!("{prefix}/"))
                    }) {
                        self.persisted.insert(key.clone(), value.clone());
                    }
                }
                Ok(map([]))
            }
            _ => Err(RemoteError::Smp(8)),
        }
    }

    // grp/fs_mgmt/src/fs_mgmt.c and fs_mgmt_hash_checksum_{crc32,sha256}.c.
    fn fs(&mut self, op: u8, id: u8, a: &Args<'_>) -> Result<Value> {
        match (op, id) {
            (0, 0) => {
                let name = a.str("name")?;
                let off = a.uint("off")? as usize;
                let data = self.files.get(name).ok_or(RemoteError::Group(8, 3))?;
                self.open_file = Some(name.into());
                let chunk = &data[off.min(data.len())
                    ..off
                        .saturating_add(self.config.download_chunk)
                        .min(data.len())];
                let mut out = map([("off", uint(off as u64)), ("data", bytes(chunk))]);
                if off == 0 {
                    insert(&mut out, "len", uint(data.len() as u64));
                }
                if off + chunk.len() >= data.len() {
                    self.open_file = None;
                }
                Ok(out)
            }
            (2, 0) => {
                let name = a.str("name")?;
                let off = a.uint("off")? as usize;
                let chunk = a.bytes("data")?;
                if name.is_empty() {
                    return Err(RemoteError::Smp(3));
                }
                if off == 0 {
                    let len = a.uint("len")? as usize;
                    self.file_upload = Some((name.into(), 0, len));
                    // Zephyr truncates an existing file only with non-empty data.
                    if !chunk.is_empty() {
                        self.files.insert(name.into(), Vec::new());
                    }
                    self.files.entry(name.into()).or_default();
                }
                let (path, expected, len) =
                    self.file_upload.as_mut().ok_or(RemoteError::Group(8, 11))?;
                if path != name || *expected != off {
                    return Err(RemoteError::Group(8, 11));
                }
                self.files.get_mut(name).unwrap().extend_from_slice(chunk);
                *expected += chunk.len();
                self.open_file = if *expected >= *len {
                    None
                } else {
                    Some(name.into())
                };
                Ok(map([("off", uint(*expected as u64))]))
            }
            (0, 1) => {
                let data = self
                    .files
                    .get(a.str("name")?)
                    .ok_or(RemoteError::Group(8, 3))?;
                Ok(map([("len", uint(data.len() as u64))]))
            }
            (0, 2) => {
                let algorithm = a.opt_str("type")?.unwrap_or("crc32");
                if !matches!(algorithm, "crc32" | "sha256") {
                    return Err(RemoteError::Group(8, 13));
                }
                let file = self
                    .files
                    .get(a.str("name")?)
                    .ok_or(RemoteError::Group(8, 3))?;
                let off = a.opt_uint("off")?.unwrap_or(0) as usize;
                if file.is_empty() {
                    return Err(RemoteError::Group(8, 16));
                }
                if off >= file.len() {
                    return Err(RemoteError::Group(8, 12));
                }
                let len = a
                    .opt_uint("len")?
                    .unwrap_or(u64::MAX)
                    .min((file.len() - off) as u64) as usize;
                let data = &file[off..off + len];
                let output = if algorithm == "crc32" {
                    uint(crc32(data))
                } else {
                    bytes(sha256(data))
                };
                let mut out = map([
                    ("type", text(algorithm)),
                    ("len", uint(len as u64)),
                    ("output", output),
                ]);
                if off != 0 {
                    insert(&mut out, "off", uint(off as u64));
                }
                Ok(out)
            }
            (0, 3) => Ok(map([(
                "types",
                map([
                    ("crc32", map([("format", uint(0u32)), ("size", uint(4u32))])),
                    (
                        "sha256",
                        map([("format", uint(1u32)), ("size", uint(32u32))]),
                    ),
                ]),
            )])),
            (2, 4) => {
                self.open_file = None;
                self.file_upload = None;
                self.file_closes += 1;
                Ok(map([]))
            }
            _ => Err(RemoteError::Smp(8)),
        }
    }

    // grp/shell_mgmt/src/shell_mgmt.c; fixture commands stand in for the Zephyr
    // application's shell. Arguments are joined by spaces by the actual handler.
    fn shell(&mut self, a: &Args<'_>) -> Result<Value> {
        let argv = a
            .array("argv")?
            .iter()
            .map(|v| v.as_text().map(str::to_string).ok_or(RemoteError::Smp(3)))
            .collect::<Result<Vec<_>>>()?;
        if argv.is_empty() || argv.join(" ").is_empty() {
            return Err(RemoteError::Group(9, 3));
        }
        self.shell_calls.push(argv.clone());
        let (ret, output) = match argv.first().map(String::as_str) {
            Some("echo") => (0, format!("{}\n", argv[1..].join(" "))),
            Some("fail") => (-22, "invalid argument\n".into()),
            _ => (-8, "unknown command\n".into()),
        };
        Ok(map([("o", text(output)), ("ret", int(ret))]))
    }

    // grp/enum_mgmt/src/enum_mgmt.c; registration order is a fixture choice.
    fn enumeration(&self, id: u8, a: &Args<'_>) -> Result<Value> {
        match id {
            0 => Ok(map([("count", uint(GROUPS.len() as u64))])),
            1 => Ok(map([(
                "groups",
                Value::Array(GROUPS.into_iter().map(uint).collect()),
            )])),
            2 => {
                let index = a.opt_uint("index")?.unwrap_or(0) as usize;
                let group = *GROUPS.get(index).ok_or(RemoteError::Group(10, 4))?;
                let mut out = map([("group", uint(group))]);
                if index + 1 == GROUPS.len() {
                    insert(&mut out, "end", Value::Bool(true));
                }
                Ok(out)
            }
            3 => {
                let requested = if a.get("groups").is_some() {
                    Some(a.array("groups")?)
                } else {
                    None
                };
                let names = [
                    "os mgmt",
                    "img mgmt",
                    "stat mgmt",
                    "settings mgmt",
                    "fs mgmt",
                    "shell mgmt",
                    "enum mgmt",
                    "zephyr basic mgmt",
                ];
                let counts = [9u32, 7, 2, 4, 5, 1, 4, 1];
                let entries = GROUPS
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| {
                        requested.is_none_or(|r| {
                            r.is_empty() || r.iter().any(|v| number(v) == Some(u64::from(**g)))
                        })
                    })
                    .map(|(index, &g)| {
                        let mut entry = map([("group", uint(g))]);
                        if self.config.enum_details {
                            insert(&mut entry, "name", text(names[index]));
                            insert(&mut entry, "handlers", uint(counts[index]));
                        }
                        entry
                    })
                    .collect();
                Ok(map([("groups", Value::Array(entries))]))
            }
            _ => Err(RemoteError::Smp(8)),
        }
    }

    fn image_states(&self) -> Value {
        let mut ordered: Vec<_> = self.slots.iter().collect();
        ordered.sort_by_key(|s| (s.image, s.slot));
        let slots = ordered
            .into_iter()
            .filter_map(|slot| {
                let hash = image_hash(&slot.data)?;
                let mut out = map([
                    ("slot", uint(slot.slot)),
                    ("version", text(version_string(&slot.data))),
                    ("hash", bytes(hash)),
                ]);
                if self.config.image_count > 1 {
                    insert(&mut out, "image", uint(slot.image));
                }
                for (key, value) in [
                    ("bootable", true),
                    ("active", slot.active),
                    ("confirmed", slot.confirmed),
                    ("pending", slot.pending),
                    ("permanent", slot.permanent),
                ] {
                    if value || !self.config.compact_images {
                        insert(&mut out, key, Value::Bool(value));
                    }
                }
                Some(out)
            })
            .collect();
        let mut result = map([("images", Value::Array(slots))]);
        if !self.config.compact_images {
            insert(&mut result, "splitStatus", int(0));
        }
        result
    }

    // img_mgmt.c, img_mgmt_state.c, zephyr_img_mgmt.c. Model the two-slot
    // swap-using-scratch configuration, optionally for multiple image pairs.
    fn image(&mut self, op: u8, id: u8, a: &Args<'_>) -> Result<Value> {
        match (op, id) {
            (0, 0) => Ok(self.image_states()),
            (2, 0) => {
                let confirm = a.boolean("confirm", false)?;
                let hash = a
                    .get("hash")
                    .map(|_| a.bytes("hash"))
                    .transpose()?
                    .unwrap_or(&[]);
                let index = if hash.is_empty() && confirm {
                    self.slots.iter().position(|s| s.image == 0 && s.active)
                } else {
                    if hash.len() != 32 {
                        return Err(RemoteError::Group(1, 24));
                    }
                    self.slots
                        .iter()
                        .position(|s| image_hash(&s.data).as_deref() == Some(hash))
                }
                .ok_or(RemoteError::Group(1, 8))?;
                let slot = &mut self.slots[index];
                if slot.active {
                    if !confirm {
                        return Err(RemoteError::Group(1, 33));
                    }
                    slot.confirmed = true;
                    let image = slot.image;
                    for other in &mut self.slots {
                        if other.image == image && !other.active {
                            other.confirmed = false;
                        }
                    }
                } else {
                    slot.pending = true;
                    slot.permanent = confirm;
                }
                Ok(self.image_states())
            }
            (2, 1) => self.image_upload(a),
            (2, 5) => {
                let slot = a.opt_uint("slot")?.unwrap_or(1) as u32;
                if slot >= self.config.image_count * 2 {
                    return Err(RemoteError::Group(1, 14));
                }
                if self
                    .slots
                    .iter()
                    .any(|s| s.image * 2 + s.slot == slot && (s.active || s.pending))
                {
                    return Err(RemoteError::Group(1, 9));
                }
                self.slots.retain(|s| s.image * 2 + s.slot != slot);
                self.upload = None;
                Ok(map([]))
            }
            (0, 6) => Ok(map([(
                "images",
                Value::Array(
                    (0..self.config.image_count)
                        .map(|image| {
                            map([
                                ("image", uint(image)),
                                ("max_image_size", uint(65_024u32)),
                                (
                                    "slots",
                                    Value::Array(vec![
                                        map([("slot", uint(0u32)), ("size", uint(65_536u32))]),
                                        map([
                                            ("slot", uint(1u32)),
                                            ("size", uint(65_536u32)),
                                            ("upload_image_id", uint(image)),
                                        ]),
                                    ]),
                                ),
                            ])
                        })
                        .collect(),
                ),
            )])),
            _ => Err(RemoteError::Smp(8)),
        }
    }

    fn image_upload(&mut self, a: &Args<'_>) -> Result<Value> {
        let off = a.opt_uint("off")?.ok_or(RemoteError::Group(1, 20))? as usize;
        let chunk = a.bytes("data")?;
        if off == 0 {
            if chunk.len() < 32 {
                return Err(RemoteError::Group(1, 22));
            }
            let size = a.opt_uint("len")?.ok_or(RemoteError::Group(1, 21))? as usize;
            if chunk[..4] != 0x96f3_b83du32.to_le_bytes() {
                return Err(RemoteError::Group(1, 23));
            }
            let sha = a
                .get("sha")
                .map(|_| a.bytes("sha"))
                .transpose()?
                .unwrap_or(&[]);
            if sha.len() > 32 {
                return Err(RemoteError::Group(1, 24));
            }
            if let Some(upload) = &self.upload {
                if !sha.is_empty() && sha == upload.sha {
                    return Ok(map([("off", uint(upload.data.len() as u64))]));
                }
            }
            let image = a.opt_uint("image")?.unwrap_or(0) as u32;
            if image >= self.config.image_count
                || self.slots.iter().any(|s| s.image == image && s.pending)
            {
                return Err(RemoteError::Group(1, 9));
            }
            if size > 65_024 {
                return Err(RemoteError::Group(1, 30));
            }
            if a.boolean("upgrade", false)? {
                let current = self
                    .slots
                    .iter()
                    .find(|s| s.image == image && s.active)
                    .unwrap();
                if version(chunk) <= version(&current.data) {
                    return Err(RemoteError::Group(1, 27));
                }
            }
            // img_mgmt_upload recognizes a fully uploaded image already in flash.
            if self.config.image_check
                && sha.len() == 32
                && self.slots.iter().any(|s| {
                    s.image == image
                        && !s.active
                        && s.data.len() >= size
                        && sha256(&s.data[..size]).as_slice() == sha
                })
            {
                self.upload = None;
                return Ok(map([
                    ("off", uint(size as u64)),
                    ("match", Value::Bool(true)),
                ]));
            }
            self.slots.retain(|s| s.image != image || s.active);
            self.upload = Some(Upload {
                image,
                size,
                sha: sha.to_vec(),
                data: Vec::new(),
            });
        }
        let Some(upload) = &mut self.upload else {
            return Ok(map([("off", uint(0u32))]));
        };
        if off != upload.data.len() {
            return Ok(map([("off", uint(upload.data.len() as u64))]));
        }
        if off + chunk.len() > upload.size {
            return Err(RemoteError::Group(1, 31));
        }
        upload.data.extend_from_slice(chunk);
        let mut out = map([("off", uint(upload.data.len() as u64))]);
        if upload.data.len() == upload.size {
            let mut upload = self.upload.take().unwrap();
            if self.corrupt_upload {
                upload.data[32] ^= 1;
            }
            if self.config.image_check {
                insert(
                    &mut out,
                    "match",
                    Value::Bool(sha256(&upload.data).as_slice() == upload.sha),
                );
            }
            self.slots.push(Slot {
                image: upload.image,
                slot: 1,
                data: upload.data.clone(),
                active: false,
                pending: false,
                confirmed: false,
                permanent: false,
            });
            self.last_upload = Some(upload);
        }
        Ok(out)
    }

    pub fn reboot(&mut self) {
        for image in 0..self.config.image_count {
            let pending = self
                .slots
                .iter()
                .position(|s| s.image == image && s.pending);
            let active = self.slots.iter().position(|s| s.image == image && s.active);
            if let (Some(p), Some(a)) = (pending, active) {
                self.slots[a].active = false;
                self.slots[a].slot = 1;
                self.slots[a].confirmed = !self.slots[p].permanent;
                self.slots[p].active = true;
                self.slots[p].slot = 0;
                self.slots[p].confirmed = self.slots[p].permanent;
                self.slots[p].permanent = false;
                self.slots[p].pending = false;
            } else if let Some(a) = active {
                // A test image that was never confirmed reverts on the next boot.
                if !self.slots[a].confirmed {
                    if let Some(old) = self
                        .slots
                        .iter()
                        .position(|s| s.image == image && !s.active)
                    {
                        self.slots[a].active = false;
                        self.slots[a].slot = 1;
                        self.slots[old].active = true;
                        self.slots[old].slot = 0;
                        self.slots[old].confirmed = true;
                    }
                }
            }
        }
        self.upload = None;
        self.open_file = None;
    }
}
