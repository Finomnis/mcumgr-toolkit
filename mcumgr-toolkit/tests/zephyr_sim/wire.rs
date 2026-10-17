//! Independent wire vocabulary. No mcumgr-toolkit protocol types belong here.
//!
//! Reference: Zephyr mgmt_defines.h, smp.h, smp.c at the revision in README.md.
//! ciborium is used only as a generic CBOR reader / scalar writer, never with
//! the client's request or response types. Containers are encoded indefinitely,
//! as in Zephyr's non-canonical zcbor build.

pub use ciborium::Value;

pub fn uint(n: impl Into<u64>) -> Value {
    Value::Integer(n.into().into())
}

pub fn int(n: i32) -> Value {
    Value::Integer(n.into())
}

pub fn text(s: impl Into<String>) -> Value {
    Value::Text(s.into())
}

pub fn bytes(b: impl AsRef<[u8]>) -> Value {
    Value::Bytes(b.as_ref().to_vec())
}

pub fn map(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Map(entries.into_iter().map(|(k, v)| (text(k), v)).collect())
}

pub fn lookup<'a>(map: &'a Value, key: &str) -> Option<&'a Value> {
    map.as_map()?
        .iter()
        .find(|(k, _)| k.as_text() == Some(key))
        .map(|(_, v)| v)
}

pub fn number(value: &Value) -> Option<u64> {
    value.as_integer()?.try_into().ok()
}

pub fn insert(value: &mut Value, key: &'static str, item: Value) {
    let entries = value.as_map_mut().expect("CBOR map");
    entries.retain(|(k, _)| k.as_text() != Some(key));
    entries.push((text(key), item));
}

pub fn encode(value: &Value) -> Vec<u8> {
    fn write(value: &Value, out: &mut Vec<u8>) {
        match value {
            Value::Map(entries) => {
                out.push(0xbf);
                for (k, v) in entries {
                    write(k, out);
                    write(v, out);
                }
                out.push(0xff);
            }
            Value::Array(entries) => {
                out.push(0x9f);
                for v in entries {
                    write(v, out);
                }
                out.push(0xff);
            }
            _ => ciborium::into_writer(value, out).unwrap(),
        }
    }
    let mut out = Vec::new();
    write(value, &mut out);
    out
}

#[derive(Debug, Clone)]
pub struct Request {
    pub header: [u8; 8],
    pub body: Value,
    pub wire_body: Vec<u8>,
}

impl Request {
    pub fn parse(header: [u8; 8], data: &[u8]) -> Self {
        assert_eq!(header[0] & 0xe0, 0, "reserved SMP bits");
        assert_eq!((header[0] >> 3) & 3, 1, "client must request SMP v2");
        assert!(matches!(header[0] & 7, 0 | 2), "invalid request opcode");
        assert_eq!(header[1], 0, "reserved flags");
        assert_eq!(
            usize::from(u16::from_be_bytes([header[2], header[3]])),
            data.len()
        );
        let mut cursor = std::io::Cursor::new(data);
        let body: Value = ciborium::from_reader(&mut cursor).expect("request must be CBOR");
        assert_eq!(
            cursor.position() as usize,
            data.len(),
            "trailing request bytes"
        );
        // Empty commands may omit a payload entirely at the API level, but the
        // SMP layer must still send a CBOR map to Zephyr.
        assert!(body.is_map(), "request must contain a CBOR map: {body:?}");
        Self {
            header,
            body,
            wire_body: data.to_vec(),
        }
    }

    pub fn op(&self) -> u8 {
        self.header[0] & 7
    }
    pub fn group(&self) -> u16 {
        u16::from_be_bytes([self.header[4], self.header[5]])
    }
    pub fn id(&self) -> u8 {
        self.header[7]
    }
    pub fn sequence(&self) -> u8 {
        self.header[6]
    }
    pub fn frame_len(&self) -> usize {
        8 + self.wire_body.len()
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        lookup(&self.body, key)
    }

    pub fn response(&self, body: &[u8]) -> Vec<u8> {
        let len = u16::try_from(body.len())
            .expect("simulated response fits SMP")
            .to_be_bytes();
        let group = self.group().to_be_bytes();
        let mut result = vec![
            8 | (self.op() + 1),
            0,
            len[0],
            len[1],
            group[0],
            group[1],
            self.sequence(),
            self.id(),
        ];
        result.extend_from_slice(body);
        result
    }
}

#[derive(Debug, Clone)]
pub enum RemoteError {
    Smp(i32),
    Group(u16, u16),
}

impl RemoteError {
    pub fn body(&self) -> Value {
        match *self {
            Self::Smp(rc) => map([("rc", int(rc))]),
            Self::Group(group, rc) => {
                map([("err", map([("group", uint(group)), ("rc", uint(rc))]))])
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, RemoteError>;

pub struct Args<'a>(pub &'a Value);

impl<'a> Args<'a> {
    pub fn get(&self, key: &str) -> Option<&'a Value> {
        lookup(self.0, key)
    }
    pub fn str(&self, key: &str) -> Result<&'a str> {
        self.get(key)
            .and_then(Value::as_text)
            .ok_or(RemoteError::Smp(3))
    }
    pub fn bytes(&self, key: &str) -> Result<&'a [u8]> {
        self.get(key)
            .and_then(Value::as_bytes)
            .map(Vec::as_slice)
            .ok_or(RemoteError::Smp(3))
    }
    pub fn uint(&self, key: &str) -> Result<u64> {
        self.get(key).and_then(number).ok_or(RemoteError::Smp(3))
    }
    pub fn opt_uint(&self, key: &str) -> Result<Option<u64>> {
        self.get(key).map(|_| self.uint(key)).transpose()
    }
    pub fn opt_str(&self, key: &str) -> Result<Option<&'a str>> {
        self.get(key).map(|_| self.str(key)).transpose()
    }
    pub fn boolean(&self, key: &str, default: bool) -> Result<bool> {
        self.get(key)
            .map(|v| v.as_bool().ok_or(RemoteError::Smp(3)))
            .unwrap_or(Ok(default))
    }
    pub fn array(&self, key: &str) -> Result<&'a [Value]> {
        self.get(key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or(RemoteError::Smp(3))
    }
}
