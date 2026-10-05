//! The SMP server core, modelled after `subsys/mgmt/mcumgr/smp/src/smp.c`.

use ciborium::Value;

use super::Device;
use super::cbor::{BulkError, Decoded, Kind, encode, int, map, map_decode_bulk, text, uint};

pub const MGMT_HDR_SIZE: usize = 8;

/// `enum mcumgr_op_t`
pub mod op {
    pub const READ: u8 = 0;
    pub const READ_RSP: u8 = 1;
    pub const WRITE: u8 = 2;
    pub const WRITE_RSP: u8 = 3;
}

/// `enum mcumgr_err_t`
#[allow(dead_code)]
pub mod mgmt_err {
    pub const EOK: i32 = 0;
    pub const EUNKNOWN: i32 = 1;
    pub const ENOMEM: i32 = 2;
    pub const EINVAL: i32 = 3;
    pub const ETIMEOUT: i32 = 4;
    pub const ENOENT: i32 = 5;
    pub const EBADSTATE: i32 = 6;
    pub const EMSGSIZE: i32 = 7;
    pub const ENOTSUP: i32 = 8;
    pub const ECORRUPT: i32 = 9;
    pub const EBUSY: i32 = 10;
    pub const EACCESSDENIED: i32 = 11;
    pub const UNSUPPORTED_TOO_OLD: i32 = 12;
    pub const UNSUPPORTED_TOO_NEW: i32 = 13;
}

/// `enum mcumgr_group_t`
pub mod group_id {
    pub const OS: u16 = 0;
    pub const IMAGE: u16 = 1;
    pub const STAT: u16 = 2;
    pub const SETTINGS: u16 = 3;
    pub const FS: u16 = 8;
    pub const SHELL: u16 = 9;
    pub const ENUM: u16 = 10;
    pub const ZEPHYR_BASIC: u16 = 63;
}

/// `SMP_MCUMGR_VERSION_1` (original protocol) and `SMP_MCUMGR_VERSION_2`
pub const SMP_MCUMGR_VERSION_1: u8 = 0;
pub const SMP_MCUMGR_VERSION_2: u8 = 1;

/// `struct smp_hdr`, as laid out on a little endian target
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmpHdr {
    pub op: u8,
    pub version: u8,
    pub flags: u8,
    pub len: u16,
    pub group: u16,
    pub seq: u8,
    pub id: u8,
}

impl SmpHdr {
    pub fn parse(data: &[u8; MGMT_HDR_SIZE]) -> Self {
        Self {
            op: data[0] & 0b111,
            version: (data[0] >> 3) & 0b11,
            flags: data[1],
            len: u16::from_be_bytes([data[2], data[3]]),
            group: u16::from_be_bytes([data[4], data[5]]),
            seq: data[6],
            id: data[7],
        }
    }

    pub fn to_bytes(self) -> [u8; MGMT_HDR_SIZE] {
        let [len_hi, len_lo] = self.len.to_be_bytes();
        let [group_hi, group_lo] = self.group.to_be_bytes();
        [
            (self.op & 0b111) | ((self.version & 0b11) << 3),
            self.flags,
            len_hi,
            len_lo,
            group_hi,
            group_lo,
            self.seq,
            self.id,
        ]
    }

    /// `smp_make_rsp_hdr`
    fn make_rsp(&self, len: usize) -> SmpHdr {
        SmpHdr {
            op: if self.op == op::READ {
                op::READ_RSP
            } else {
                op::WRITE_RSP
            },
            version: self.version.min(SMP_MCUMGR_VERSION_2),
            flags: 0,
            len: len as u16,
            group: self.group,
            seq: self.seq,
            id: self.id,
        }
    }
}

/// The state a command handler works with: the decoded request and the
/// entries of the response's main map.
pub struct Ctx<'a> {
    pub hdr: SmpHdr,
    req: Option<&'a Value>,
    rsp: Vec<(Value, Value)>,
}

impl Ctx<'_> {
    /// `zcbor_map_decode_bulk` on the request payload
    pub fn decode(&self, spec: &[(&'static str, Kind)]) -> Result<Decoded, BulkError> {
        map_decode_bulk(self.req, spec)
    }

    /// The raw request payload, for handlers that decode by hand
    pub fn request(&self) -> Option<&Value> {
        self.req
    }

    pub fn put(&mut self, key: &str, value: Value) {
        self.rsp.push((text(key), value));
    }

    /// `smp_add_cmd_err`
    pub fn add_cmd_err(&mut self, group: u16, ret: u16) {
        if ret != 0 {
            self.put("err", map([("group", uint(group)), ("rc", uint(ret))]));
        }
    }
}

/// A command handler; `Err` carries a `MGMT_ERR_*` code that turns the whole
/// response into an `rc` error response.
pub type HandlerFn = fn(&mut Device, &mut Ctx) -> Result<(), i32>;

/// `struct mgmt_handler`
#[derive(Clone, Copy, Default)]
pub struct Handler {
    pub read: Option<HandlerFn>,
    pub write: Option<HandlerFn>,
}

/// `struct mgmt_group`
pub struct Group {
    pub id: u16,
    pub name: &'static str,
    /// Indexed by command id. Unset entries are "holes" in the C array.
    pub handlers: Vec<Handler>,
}

impl Group {
    pub fn new(id: u16, name: &'static str, entries: &[(u8, Handler)]) -> Self {
        let count = entries
            .iter()
            .map(|(id, _)| *id as usize + 1)
            .max()
            .unwrap_or(0);
        let mut handlers = vec![Handler::default(); count];
        for (id, handler) in entries {
            handlers[*id as usize] = *handler;
        }
        Self { id, name, handlers }
    }
}

pub fn read(f: HandlerFn) -> Handler {
    Handler {
        read: Some(f),
        write: None,
    }
}

pub fn write(f: HandlerFn) -> Handler {
    Handler {
        read: None,
        write: Some(f),
    }
}

pub fn read_write(r: HandlerFn, w: HandlerFn) -> Handler {
    Handler {
        read: Some(r),
        write: Some(w),
    }
}

impl Device {
    /// `smp_process_request_packet`: processes every request in a packet and
    /// returns the response packets that the transport sends back.
    pub fn process_request_packet(&mut self, mut req: &[u8]) -> Vec<Vec<u8>> {
        let mut responses = vec![];

        while !req.is_empty() {
            let Some((hdr, rest)) = req.split_first_chunk::<MGMT_HDR_SIZE>() else {
                // MGMT_ERR_ECORRUPT, but without a valid header nothing is sent.
                break;
            };
            let hdr = SmpHdr::parse(hdr);
            req = rest;

            if req.len() < hdr.len as usize {
                responses.push(self.build_err_rsp(&hdr, mgmt_err::ECORRUPT));
                break;
            }
            let (payload, rest) = req.split_at(hdr.len as usize);
            req = rest;

            if hdr.op != op::READ && hdr.op != op::WRITE {
                // Neither CONFIG_MCUMGR_GRP_TRANSPORT nor CONFIG_SMP_CLIENT
                responses.push(self.build_err_rsp(&hdr, mgmt_err::ENOTSUP));
                break;
            }

            match self.handle_single_req(&hdr, payload) {
                Ok(rsp) => responses.push(rsp),
                Err(rc) => {
                    responses.push(self.build_err_rsp(&hdr, rc));
                    break;
                }
            }
        }

        responses
    }

    /// `smp_handle_single_req`
    fn handle_single_req(&mut self, hdr: &SmpHdr, payload: &[u8]) -> Result<Vec<u8>, i32> {
        if hdr.version == SMP_MCUMGR_VERSION_1 && !self.config.smp_support_original_protocol {
            return Err(mgmt_err::UNSUPPORTED_TOO_OLD);
        }
        if hdr.version > SMP_MCUMGR_VERSION_2 {
            return Err(mgmt_err::UNSUPPORTED_TOO_NEW);
        }

        // The zcbor reader fails on anything that is not well-formed CBOR;
        // handlers then fail to decode their map and report EINVAL.
        let request: Option<Value> = ciborium::from_reader(payload).ok();
        let mut ctx = Ctx {
            hdr: *hdr,
            req: request.as_ref(),
            rsp: vec![],
        };

        self.handle_single_payload(&mut ctx)?;

        let body = encode(&Value::Map(ctx.rsp));

        // The response is encoded into a net_buf of
        // CONFIG_MCUMGR_TRANSPORT_NETBUF_SIZE bytes; zcbor fails when it
        // runs out of space and the handler reports MGMT_ERR_EMSGSIZE.
        if MGMT_HDR_SIZE + body.len() > self.config.mcumgr_transport_netbuf_size {
            return Err(mgmt_err::EMSGSIZE);
        }

        let mut frame = hdr.make_rsp(body.len()).to_bytes().to_vec();
        frame.extend_from_slice(&body);
        Ok(frame)
    }

    /// `smp_handle_single_payload`
    fn handle_single_payload(&mut self, ctx: &mut Ctx) -> Result<(), i32> {
        let handler = self
            .groups
            .iter()
            .find(|group| group.id == ctx.hdr.group)
            .ok_or(mgmt_err::ENOTSUP)?
            .handlers
            .get(ctx.hdr.id as usize)
            .copied()
            .ok_or(mgmt_err::ENOTSUP)?;

        let handler_fn = match ctx.hdr.op {
            op::READ => handler.read,
            op::WRITE => handler.write,
            _ => return Err(mgmt_err::EINVAL),
        }
        .ok_or(mgmt_err::ENOTSUP)?;

        handler_fn(self, ctx)
    }

    /// `smp_build_err_rsp`
    fn build_err_rsp(&self, req_hdr: &SmpHdr, status: i32) -> Vec<u8> {
        let body = encode(&map([("rc", int(status))]));

        let mut frame = req_hdr.make_rsp(body.len()).to_bytes().to_vec();
        frame.extend_from_slice(&body);
        frame
    }
}
