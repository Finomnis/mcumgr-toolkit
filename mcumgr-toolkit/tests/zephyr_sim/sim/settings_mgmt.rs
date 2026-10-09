//! Settings management group, modelled after
//! `subsys/mgmt/mcumgr/grp/settings_mgmt/src/settings_mgmt.c`, on top of a
//! model of Zephyr's settings subsystem (`subsys/settings/src`).

use std::collections::BTreeMap;

use super::Device;
use ciborium::Value;

use super::cbor::{Kind, uint};
use super::smp::{Ctx, Group, group_id, mgmt_err, read_write, write};

const SETTINGS_MGMT_ID_READ_WRITE: u8 = 0;
const SETTINGS_MGMT_ID_DELETE: u8 = 1;
const SETTINGS_MGMT_ID_COMMIT: u8 = 2;
const SETTINGS_MGMT_ID_LOAD_SAVE: u8 = 3;

/// `enum settings_mgmt_ret_code_t`
pub mod settings_mgmt_err {
    pub const UNKNOWN: u16 = 1;
    pub const KEY_TOO_LONG: u16 = 2;
    pub const KEY_NOT_FOUND: u16 = 3;
    pub const READ_NOT_SUPPORTED: u16 = 4;
    pub const ROOT_KEY_NOT_FOUND: u16 = 5;
    pub const WRITE_NOT_SUPPORTED: u16 = 6;
}
use settings_mgmt_err as err;

const EINVAL: i32 = 22;
const ENOENT: i32 = 2;
const ENOTSUP: i32 = 134;

/// A `struct settings_handler_static` that stores its values in RAM
#[derive(Clone, Debug)]
pub struct SettingsHandler {
    /// The root name of the subtree
    pub name: String,
    /// Keys (below the root) known to `h_set`/`h_get`, with their default
    pub defaults: BTreeMap<String, Vec<u8>>,
    /// Current runtime values
    pub values: BTreeMap<String, Vec<u8>>,
    /// Whether `h_get` is implemented
    pub readable: bool,
    /// Whether `h_set` is implemented
    pub writable: bool,
    /// How often `h_commit` was called
    pub commits: u32,
}

impl SettingsHandler {
    pub fn new(name: &str, defaults: &[(&str, &[u8])]) -> Self {
        let defaults: BTreeMap<String, Vec<u8>> = defaults
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_vec()))
            .collect();
        Self {
            name: name.into(),
            values: defaults.clone(),
            defaults,
            readable: true,
            writable: true,
            commits: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SettingsState {
    pub handlers: Vec<SettingsHandler>,
    /// The persistent storage backend (NVS/ZMS/file), full names
    pub storage: BTreeMap<String, Vec<u8>>,
    /// How often `settings_load()` ran
    pub loads: u32,
}

impl Default for SettingsState {
    fn default() -> Self {
        let mut readonly = SettingsHandler::new("factory", &[("serial", b"SN-0042")]);
        readonly.writable = false;
        let mut writeonly = SettingsHandler::new("secret", &[("key", b"")]);
        writeonly.readable = false;
        Self {
            handlers: vec![
                SettingsHandler::new(
                    "app",
                    &[
                        ("name", b"zephyr-sim"),
                        ("counter", &7u32.to_le_bytes()),
                        ("blob", &[0u8; 64]),
                    ],
                ),
                readonly,
                writeonly,
            ],
            storage: BTreeMap::new(),
            loads: 0,
        }
    }
}

impl SettingsState {
    /// `settings_parse_and_lookup`
    fn lookup(&mut self, name: &str) -> Option<(&mut SettingsHandler, String)> {
        let (root, key) = name.split_once('/').unwrap_or((name, ""));
        let handler = self.handlers.iter_mut().find(|h| h.name == root)?;
        Some((handler, key.to_string()))
    }

    /// `settings_runtime_get`
    fn runtime_get(&mut self, name: &str, max_size: usize) -> Result<Vec<u8>, i32> {
        let (handler, key) = self.lookup(name).ok_or(-EINVAL)?;
        if !handler.readable {
            return Err(-ENOTSUP);
        }
        let value = handler.values.get(&key).ok_or(-ENOENT)?;
        Ok(value[..value.len().min(max_size)].to_vec())
    }

    /// `settings_runtime_set`
    fn runtime_set(&mut self, name: &str, value: &[u8]) -> Result<(), i32> {
        let (handler, key) = self.lookup(name).ok_or(-EINVAL)?;
        if !handler.writable {
            return Err(-ENOTSUP);
        }
        if !handler.defaults.contains_key(&key) {
            return Err(-ENOENT);
        }
        handler.values.insert(key, value.to_vec());
        Ok(())
    }

    /// `settings_save_subtree`: every handler exports its values
    fn save(&mut self, subtree: Option<&str>) {
        for handler in &self.handlers {
            if subtree.is_some_and(|subtree| subtree != handler.name) {
                continue;
            }
            for (key, value) in &handler.values {
                self.storage
                    .insert(format!("{}/{key}", handler.name), value.clone());
            }
        }
    }

    /// `settings_load`: every stored value is handed to its handler
    pub fn load(&mut self) {
        self.loads += 1;
        let stored = self.storage.clone();
        for (name, value) in stored {
            if let Some((handler, key)) = self.lookup(&name) {
                if handler.defaults.contains_key(&key) {
                    handler.values.insert(key, value);
                }
            }
        }
        // settings_load_subtree() commits after loading
        self.commit();
    }

    /// `settings_commit`
    pub fn commit(&mut self) {
        for handler in &mut self.handlers {
            handler.commits += 1;
        }
    }

    /// What happens to the runtime values on a reboot: the application
    /// starts with its defaults and calls `settings_load()`.
    pub fn reboot(&mut self) {
        for handler in &mut self.handlers {
            handler.values = handler.defaults.clone();
        }
        self.load();
    }
}

pub fn group() -> Group {
    Group::new(
        group_id::SETTINGS,
        "settings mgmt",
        &[
            (
                SETTINGS_MGMT_ID_READ_WRITE,
                read_write(settings_read, settings_write),
            ),
            (SETTINGS_MGMT_ID_DELETE, write(settings_delete)),
            (SETTINGS_MGMT_ID_COMMIT, write(settings_commit)),
            (
                SETTINGS_MGMT_ID_LOAD_SAVE,
                read_write(settings_load, settings_save),
            ),
        ],
    )
}

/// `settings_mgmt_read`
fn settings_read(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let value_len = device.config.settings_value_len;
    let decoded = ctx
        .decode(&[("name", Kind::Tstr), ("max_size", Kind::U32)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    if name.is_empty() {
        return Err(mgmt_err::EINVAL);
    }
    if name.len() >= device.config.settings_name_len {
        ctx.add_cmd_err(group_id::SETTINGS, err::KEY_TOO_LONG);
        return Ok(());
    }

    let mut max_size = decoded.uint("max_size").unwrap_or(value_len as u64) as usize;
    let mut limited_size = false;
    if max_size > value_len {
        max_size = value_len;
        limited_size = true;
    }

    match device.settings.runtime_get(name, max_size) {
        Ok(value) => {
            ctx.put("val", Value::Bytes(value));
            if limited_size {
                ctx.put("max_size", uint(value_len as u64));
            }
        }
        Err(rc) => {
            let rc = match -rc {
                EINVAL => err::ROOT_KEY_NOT_FOUND,
                ENOENT => err::KEY_NOT_FOUND,
                ENOTSUP => err::READ_NOT_SUPPORTED,
                _ => err::UNKNOWN,
            };
            ctx.add_cmd_err(group_id::SETTINGS, rc);
        }
    }
    Ok(())
}

/// `settings_mgmt_write`
fn settings_write(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("name", Kind::Tstr), ("val", Kind::Bstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    if name.is_empty() {
        return Err(mgmt_err::EINVAL);
    }
    if name.len() >= device.config.settings_name_len {
        ctx.add_cmd_err(group_id::SETTINGS, err::KEY_TOO_LONG);
        return Ok(());
    }

    let value = decoded.bytes("val").unwrap_or_default();
    if let Err(rc) = device.settings.runtime_set(name, value) {
        let rc = match -rc {
            EINVAL => err::ROOT_KEY_NOT_FOUND,
            ENOENT => err::KEY_NOT_FOUND,
            ENOTSUP => err::WRITE_NOT_SUPPORTED,
            _ => err::UNKNOWN,
        };
        ctx.add_cmd_err(group_id::SETTINGS, rc);
    }
    Ok(())
}

/// `settings_mgmt_delete`
fn settings_delete(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("name", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    if name.is_empty() {
        return Err(mgmt_err::EINVAL);
    }
    if name.len() >= device.config.settings_name_len {
        ctx.add_cmd_err(group_id::SETTINGS, err::KEY_TOO_LONG);
        return Ok(());
    }

    // settings_delete() == settings_save_one(name, NULL, 0): it only
    // writes a deletion record to the storage backend.
    device.settings.storage.remove(name);
    Ok(())
}

/// `settings_mgmt_commit`
fn settings_commit(device: &mut Device, _: &mut Ctx) -> Result<(), i32> {
    device.settings.commit();
    Ok(())
}

/// `settings_mgmt_load`
fn settings_load(device: &mut Device, _: &mut Ctx) -> Result<(), i32> {
    device.settings.load();
    Ok(())
}

/// `settings_mgmt_save`
fn settings_save(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("name", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;

    let subtree = if decoded.found("name") {
        let name = decoded.str("name").unwrap();
        if name.is_empty() {
            return Err(mgmt_err::EINVAL);
        }
        if name.len() >= device.config.settings_name_len {
            ctx.add_cmd_err(group_id::SETTINGS, err::KEY_TOO_LONG);
            return Ok(());
        }
        Some(name.to_string())
    } else {
        None
    };

    device.settings.save(subtree.as_deref());
    Ok(())
}
