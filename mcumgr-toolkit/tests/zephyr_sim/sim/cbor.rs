//! CBOR handling with the semantics of Zephyr's `zcbor` usage in MCUmgr.
//!
//! Encoding follows `zcbor_encode.c`: integers use the shortest form, and
//! maps/lists are written with *indefinite* length unless
//! `CONFIG_ZCBOR_CANONICAL` is enabled (it is disabled by default, so real
//! devices answer with indefinite-length containers).
//!
//! Decoding follows `subsys/mgmt/mcumgr/util/src/zcbor_bulk.c`
//! (`zcbor_map_decode_bulk`): the payload must be a map with text keys,
//! unknown keys are skipped, a known key appearing twice is an error and a
//! value of the wrong type is an error.

use ciborium::Value;

/// A CBOR value as produced by the simulated device.
#[derive(Clone, Debug, PartialEq)]
pub enum Cbor {
    /// `zcbor_uint32_put` / `zcbor_uint64_put` / `zcbor_size_put`
    Uint(u64),
    /// `zcbor_int32_put` / `zcbor_int64_put`
    Int(i64),
    /// `zcbor_bstr_encode`
    Bytes(Vec<u8>),
    /// `zcbor_tstr_encode` / `zcbor_tstr_put_lit` / `zcbor_tstr_put_term`
    Text(String),
    /// `zcbor_bool_put`
    Bool(bool),
    /// `zcbor_map_start_encode` .. `zcbor_map_end_encode`
    Map(Vec<(Cbor, Cbor)>),
    /// `zcbor_list_start_encode` .. `zcbor_list_end_encode`
    List(Vec<Cbor>),
}

impl Cbor {
    pub fn text(s: impl Into<String>) -> Self {
        Cbor::Text(s.into())
    }

    pub fn map<K: Into<String>>(entries: impl IntoIterator<Item = (K, Cbor)>) -> Self {
        Cbor::Map(
            entries
                .into_iter()
                .map(|(k, v)| (Cbor::Text(k.into()), v))
                .collect(),
        )
    }

    pub fn encode(&self, out: &mut Vec<u8>, canonical: bool) {
        match self {
            Cbor::Uint(v) => write_head(out, 0, *v),
            Cbor::Int(v) => {
                if *v >= 0 {
                    write_head(out, 0, *v as u64)
                } else {
                    write_head(out, 1, (-1 - *v) as u64)
                }
            }
            Cbor::Bytes(b) => {
                write_head(out, 2, b.len() as u64);
                out.extend_from_slice(b);
            }
            Cbor::Text(s) => {
                write_head(out, 3, s.len() as u64);
                out.extend_from_slice(s.as_bytes());
            }
            Cbor::Bool(b) => out.push(if *b { 0xf5 } else { 0xf4 }),
            Cbor::List(items) => {
                if canonical {
                    write_head(out, 4, items.len() as u64);
                } else {
                    out.push(0x9f);
                }
                for item in items {
                    item.encode(out, canonical);
                }
                if !canonical {
                    out.push(0xff);
                }
            }
            Cbor::Map(entries) => {
                if canonical {
                    write_head(out, 5, entries.len() as u64);
                } else {
                    out.push(0xbf);
                }
                for (k, v) in entries {
                    k.encode(out, canonical);
                    v.encode(out, canonical);
                }
                if !canonical {
                    out.push(0xff);
                }
            }
        }
    }
}

fn write_head(out: &mut Vec<u8>, major: u8, value: u64) {
    let major = major << 5;
    if value < 24 {
        out.push(major | value as u8);
    } else if value <= u8::MAX as u64 {
        out.push(major | 24);
        out.push(value as u8);
    } else if value <= u16::MAX as u64 {
        out.push(major | 25);
        out.extend_from_slice(&(value as u16).to_be_bytes());
    } else if value <= u32::MAX as u64 {
        out.push(major | 26);
        out.extend_from_slice(&(value as u32).to_be_bytes());
    } else {
        out.push(major | 27);
        out.extend_from_slice(&value.to_be_bytes());
    }
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
