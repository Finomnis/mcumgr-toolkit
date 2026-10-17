//! CBOR helpers for the simulated device.
//!
//! Responses are built as [`ciborium::Value`]s and encoded by ciborium.
//!
//! Decoding of requests follows `subsys/mgmt/mcumgr/util/src/zcbor_bulk.c`
//! (`zcbor_map_decode_bulk`): the payload must be a map with text keys,
//! unknown keys are skipped, a known key appearing twice is an error and a
//! value of the wrong type is an error.

use ciborium::Value;

/// `zcbor_uint32_put` / `zcbor_uint64_put` / `zcbor_size_put`
pub fn uint(value: impl Into<u64>) -> Value {
    Value::Integer(value.into().into())
}

/// `zcbor_int32_put` / `zcbor_int64_put`
pub fn int(value: impl Into<i64>) -> Value {
    Value::Integer(value.into().into())
}

/// `zcbor_tstr_put_lit` / `zcbor_tstr_encode`
pub fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}

/// A map with text keys
pub fn map<K: Into<String>>(entries: impl IntoIterator<Item = (K, Value)>) -> Value {
    Value::Map(entries.into_iter().map(|(k, v)| (text(k), v)).collect())
}

pub fn encode(value: &Value) -> Vec<u8> {
    let mut out = vec![];
    ciborium::into_writer(value, &mut out).unwrap();
    out
}

/// The zcbor decoder function used for a key in `zcbor_map_decode_bulk`.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// `zcbor_tstr_decode`
    Tstr,
    /// `zcbor_bstr_decode`
    Bstr,
    /// `zcbor_uint32_decode`
    U32,
    /// `zcbor_uint64_decode`
    U64,
    /// `zcbor_size_decode`; `size_t` is 32 bit on the simulated MCU
    Size,
    /// `zcbor_bool_decode`
    Bool,
    /// A list of `zcbor_uint32_decode` values (`enum_mgmt_cb_list_entries`)
    U32List,
}

#[derive(Clone, Debug)]
pub enum Field {
    Str(String),
    Bytes(Vec<u8>),
    Uint(u64),
    Bool(bool),
    List(Vec<u64>),
}

/// Result of a successful `zcbor_map_decode_bulk` call.
#[derive(Debug, Default)]
pub struct Decoded {
    fields: Vec<(&'static str, Field)>,
}

impl Decoded {
    /// The `matched` output parameter of `zcbor_map_decode_bulk`
    pub fn matched(&self) -> usize {
        self.fields.len()
    }

    /// `zcbor_map_decode_bulk_key_found`
    pub fn found(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn get(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Field::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn bytes(&self, key: &str) -> Option<&[u8]> {
        match self.get(key)? {
            Field::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn uint(&self, key: &str) -> Option<u64> {
        match self.get(key)? {
            Field::Uint(v) => Some(*v),
            _ => None,
        }
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Field::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn list(&self, key: &str) -> Option<&[u64]> {
        match self.get(key)? {
            Field::List(v) => Some(v),
            _ => None,
        }
    }
}

/// Why `zcbor_map_decode_bulk` failed; all of them lead to `MGMT_ERR_EINVAL`
/// in the command handlers.
#[derive(Debug)]
pub enum BulkError {
    /// -EBADMSG
    NotAMap,
    /// -EADDRINUSE
    DuplicateKey,
    /// -ENOMSG
    WrongType,
}

fn uint_of(value: &Value, max: u64) -> Option<u64> {
    let value = value.as_integer()?;
    let value = u64::try_from(value).ok()?;
    (value <= max).then_some(value)
}

fn decode_field(value: &Value, kind: Kind) -> Option<Field> {
    Some(match kind {
        Kind::Tstr => Field::Str(value.as_text()?.to_string()),
        Kind::Bstr => Field::Bytes(value.as_bytes()?.clone()),
        Kind::U32 | Kind::Size => Field::Uint(uint_of(value, u32::MAX as u64)?),
        Kind::U64 => Field::Uint(uint_of(value, u64::MAX)?),
        Kind::Bool => Field::Bool(value.as_bool()?),
        Kind::U32List => Field::List(
            value
                .as_array()?
                .iter()
                .map(|v| uint_of(v, u32::MAX as u64))
                .collect::<Option<_>>()?,
        ),
    })
}

/// `zcbor_map_decode_bulk`
pub fn map_decode_bulk(
    payload: Option<&Value>,
    spec: &[(&'static str, Kind)],
) -> Result<Decoded, BulkError> {
    let entries = payload.and_then(Value::as_map).ok_or(BulkError::NotAMap)?;

    let mut decoded = Decoded::default();
    for (key, value) in entries {
        // `zcbor_tstr_decode` on a non-text key stops the loop before the end
        // of the map, which makes `zcbor_map_end_decode` fail.
        let key = key.as_text().ok_or(BulkError::NotAMap)?;
        let Some((name, kind)) = spec.iter().find(|(name, _)| *name == key) else {
            continue;
        };
        if decoded.found(name) {
            return Err(BulkError::DuplicateKey);
        }
        let field = decode_field(value, *kind).ok_or(BulkError::WrongType)?;
        decoded.fields.push((name, field));
    }

    Ok(decoded)
}
