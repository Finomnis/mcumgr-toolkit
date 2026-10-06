use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use mcumgr_toolkit::{
    MCUmgrClient,
    commands::image::ImageState,
    transport::{
        ReceiveError, SMP_HEADER_SIZE, SMP_TRANSFER_BUFFER_SIZE, SendError, Transport,
    },
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

const OP_READ: u8 = 0;
const OP_READ_RSP: u8 = 1;
const OP_WRITE: u8 = 2;
const OP_WRITE_RSP: u8 = 3;

const GROUP_OS: u16 = 0;
const GROUP_IMAGE: u16 = 1;

const CMD_IMAGE_STATE: u8 = 0;
const CMD_IMAGE_UPLOAD: u8 = 1;
const CMD_OS_RESET: u8 = 5;

pub const TARGET_HASH: [u8; 32] = [0xA5; 32];
pub const OLD_HASH: [u8; 32] = [0x11; 32];
pub const OTHER_HASH: [u8; 32] = [0x22; 32];

#[derive(Clone, Copy, Debug, Default)]
pub struct Flags {
    pub active: bool,
    pub confirmed: bool,
    pub pending: bool,
    pub permanent: bool,
}

pub const STABLE: Flags = Flags {
    active: true,
    confirmed: true,
    pending: false,
    permanent: false,
};
pub const ACTIVE: Flags = Flags {
    active: true,
    confirmed: false,
    pending: false,
    permanent: false,
};
pub const CONFIRMED: Flags = Flags {
    active: false,
    confirmed: true,
    pending: false,
    permanent: false,
};
pub const PENDING: Flags = Flags {
    active: false,
    confirmed: false,
    pending: true,
    permanent: false,
};
pub const PENDING_PERMANENT: Flags = Flags {
    active: false,
    confirmed: false,
    pending: true,
    permanent: true,
};

pub fn image_state(
    image: u32,
    slot: u32,
    hash: Option<&[u8]>,
    flags: Flags,
) -> ImageState {
    ImageState {
        image,
        slot,
        version: format!("1.0.{slot}"),
        hash: hash.map(ToOwned::to_owned),
        bootable: true,
        pending: flags.pending,
        confirmed: flags.confirmed,
        active: flags.active,
        permanent: flags.permanent,
    }
}

pub fn test_firmware() -> Vec<u8> {
    const IMAGE_MAGIC: u32 = 0x96F3_B83D;
    const HEADER_SIZE: u16 = 32;
    const TLV_INFO_MAGIC: u16 = 0x6907;
    const TLV_SHA256: u16 = 0x10;

    let payload = b"firmware";
    let mut image = Vec::new();

    image.extend_from_slice(&IMAGE_MAGIC.to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes()); // load address
    image.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    image.extend_from_slice(&0u16.to_le_bytes()); // protected TLV size
    image.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    image.extend_from_slice(&0u32.to_le_bytes()); // flags
    image.push(2); // major
    image.push(3); // minor
    image.extend_from_slice(&4u16.to_le_bytes()); // revision
    image.extend_from_slice(&5u32.to_le_bytes()); // build number
    image.extend_from_slice(&0u32.to_le_bytes()); // padding
    assert_eq!(image.len(), usize::from(HEADER_SIZE));

    image.extend_from_slice(payload);

    let tlv_total_len = 4 + 4 + TARGET_HASH.len();
    image.extend_from_slice(&TLV_INFO_MAGIC.to_le_bytes());
    image.extend_from_slice(&(tlv_total_len as u16).to_le_bytes());
    image.extend_from_slice(&TLV_SHA256.to_le_bytes());
    image.extend_from_slice(&(TARGET_HASH.len() as u16).to_le_bytes());
    image.extend_from_slice(&TARGET_HASH);

    image
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetStateCall {
    pub hash: Option<Vec<u8>>,
    pub confirm: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    GetState,
    Upload {
        image: Option<u32>,
        off: u64,
        len: usize,
    },
    SetState(SetStateCall),
    Reset,
}

#[derive(Debug)]
struct DeviceModel {
    before_upload: Vec<ImageState>,
    after_upload: Vec<ImageState>,
    upload_started: bool,
    events: Vec<Event>,
}

#[derive(Clone)]
pub struct DeviceHandle {
    inner: Arc<Mutex<DeviceModel>>,
}

impl DeviceHandle {
    pub fn events(&self) -> Vec<Event> {
        self.inner.lock().unwrap().events.clone()
    }

    pub fn upload_count(&self) -> usize {
        self.events()
            .iter()
            .filter(|event| matches!(event, Event::Upload { .. }))
            .count()
    }

    pub fn reset_count(&self) -> usize {
        self.events()
            .iter()
            .filter(|event| matches!(event, Event::Reset))
            .count()
    }

    pub fn set_state_calls(&self) -> Vec<SetStateCall> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                Event::SetState(call) => Some(call),
                _ => None,
            })
            .collect()
    }
}

pub fn scripted_client(
    before_upload: Vec<ImageState>,
    after_upload: Vec<ImageState>,
) -> (MCUmgrClient, DeviceHandle) {
    let model = Arc::new(Mutex::new(DeviceModel {
        before_upload,
        after_upload,
        upload_started: false,
        events: Vec::new(),
    }));

    let handle = DeviceHandle {
        inner: Arc::clone(&model),
    };
    let transport = ScriptedTransport {
        model,
        pending_response: None,
    };

    (MCUmgrClient::new_from_transport(transport), handle)
}

struct ScriptedTransport {
    model: Arc<Mutex<DeviceModel>>,
    pending_response: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct UploadRequest {
    #[serde(default)]
    image: Option<u32>,
    off: u64,
    data: ByteBuf,
}

#[derive(Deserialize)]
struct SetStateRequest {
    #[serde(default)]
    hash: Option<ByteBuf>,
    confirm: bool,
}

#[derive(Serialize)]
struct UploadResponse {
    off: u64,
    #[serde(rename = "match")]
    matches: bool,
}

#[derive(Serialize)]
struct ImageStateResponse {
    images: Vec<WireImageState>,
}

#[derive(Serialize)]
struct WireImageState {
    image: u32,
    slot: u32,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<ByteBuf>,
    bootable: bool,
    pending: bool,
    confirmed: bool,
    active: bool,
    permanent: bool,
}

impl From<&ImageState> for WireImageState {
    fn from(value: &ImageState) -> Self {
        Self {
            image: value.image,
            slot: value.slot,
            version: value.version.clone(),
            hash: value.hash.clone().map(ByteBuf::from),
            bootable: value.bootable,
            pending: value.pending,
            confirmed: value.confirmed,
            active: value.active,
            permanent: value.permanent,
        }
    }
}

impl ScriptedTransport {
    fn encode<T: Serialize>(value: &T) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::into_writer(value, &mut out).unwrap();
        out
    }

    fn state_response(model: &DeviceModel) -> Vec<u8> {
        let images = if model.upload_started {
            &model.after_upload
        } else {
            &model.before_upload
        };
        Self::encode(&ImageStateResponse {
            images: images.iter().map(WireImageState::from).collect(),
        })
    }

    fn response_frame(request_header: [u8; SMP_HEADER_SIZE], body: Vec<u8>) -> Vec<u8> {
        let request_op = request_header[0] & 0b111;
        let version = (request_header[0] >> 3) & 0b11;
        let response_op = match request_op {
            OP_READ => OP_READ_RSP,
            OP_WRITE => OP_WRITE_RSP,
            other => panic!("unexpected SMP request op {other}"),
        };

        let body_len: u16 = body.len().try_into().unwrap();
        let [len_hi, len_lo] = body_len.to_be_bytes();
        let mut response_header = request_header;
        response_header[0] = (version << 3) | response_op;
        response_header[1] = 0;
        response_header[2] = len_hi;
        response_header[3] = len_lo;

        let mut frame = Vec::with_capacity(SMP_HEADER_SIZE + body.len());
        frame.extend_from_slice(&response_header);
        frame.extend_from_slice(&body);
        frame
    }
}

impl Transport for ScriptedTransport {
    fn send_raw_frame(
        &mut self,
        header: [u8; SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), SendError> {
        let group_id = u16::from_be_bytes([header[4], header[5]]);
        let command_id = header[7];
        let write = (header[0] & 0b111) == OP_WRITE;

        let response_body = {
            let mut model = self.model.lock().unwrap();
            match (group_id, command_id, write) {
                (GROUP_IMAGE, CMD_IMAGE_STATE, false) => {
                    model.events.push(Event::GetState);
                    Self::state_response(&model)
                }
                (GROUP_IMAGE, CMD_IMAGE_UPLOAD, true) => {
                    let request: UploadRequest = ciborium::from_reader(data).unwrap();
                    model.events.push(Event::Upload {
                        image: request.image,
                        off: request.off,
                        len: request.data.len(),
                    });
                    model.upload_started = true;
                    Self::encode(&UploadResponse {
                        off: request.off + request.data.len() as u64,
                        matches: true,
                    })
                }
                (GROUP_IMAGE, CMD_IMAGE_STATE, true) => {
                    let request: SetStateRequest = ciborium::from_reader(data).unwrap();
                    model.events.push(Event::SetState(SetStateCall {
                        hash: request.hash.map(ByteBuf::into_vec),
                        confirm: request.confirm,
                    }));
                    Self::state_response(&model)
                }
                (GROUP_OS, CMD_OS_RESET, true) => {
                    model.events.push(Event::Reset);
                    Self::encode(&std::collections::BTreeMap::<String, u8>::new())
                }
                other => panic!("unexpected SMP command: {other:?}"),
            }
        };

        self.pending_response = Some(Self::response_frame(header, response_body));
        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], ReceiveError> {
        let frame = self
            .pending_response
            .take()
            .ok_or(ReceiveError::UnexpectedResponse)?;
        if frame.len() > buffer.len() {
            return Err(ReceiveError::FrameTooBig);
        }
        buffer[..frame.len()].copy_from_slice(&frame);
        Ok(&buffer[..frame.len()])
    }

    fn set_timeout(
        &mut self,
        _timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}
