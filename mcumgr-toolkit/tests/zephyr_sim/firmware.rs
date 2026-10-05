//! MCUboot fixtures assembled from bootutil/image.h from Zephyr's west.yml.
//! These are unsigned, hash-bearing test images, never production firmware.
use sha2::{Digest, Sha256};

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub fn crc32(data: &[u8]) -> u32 {
    // IEEE CRC32, reflected polynomial; independent of the client's CRC helper.
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

pub fn firmware(major: u8, payload_len: usize) -> Vec<u8> {
    let mut data = vec![0u8; 32];
    data[..4].copy_from_slice(&0x96f3_b83du32.to_le_bytes());
    data[8..10].copy_from_slice(&32u16.to_le_bytes());
    data[12..16].copy_from_slice(&(payload_len as u32).to_le_bytes());
    data[20] = major;
    data[21] = 2;
    data[22..24].copy_from_slice(&3u16.to_le_bytes());
    data[24..28].copy_from_slice(&4u32.to_le_bytes());
    data.extend((0..payload_len).map(|i| (i.wrapping_mul(37) + 11) as u8));
    let image_hash = sha256(&data);
    data.extend_from_slice(&0x6907u16.to_le_bytes());
    data.extend_from_slice(&40u16.to_le_bytes());
    data.extend_from_slice(&0x10u16.to_le_bytes());
    data.extend_from_slice(&32u16.to_le_bytes());
    data.extend_from_slice(&image_hash);
    data
}

pub fn version(data: &[u8]) -> (u8, u8, u16, u32) {
    (
        data[20],
        data[21],
        u16::from_le_bytes(data[22..24].try_into().unwrap()),
        u32::from_le_bytes(data[24..28].try_into().unwrap()),
    )
}

pub fn version_string(data: &[u8]) -> String {
    let (a, b, c, d) = version(data);
    format!("{a}.{b}.{c}.{d}")
}

pub fn image_hash(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 32 || data[..4] != 0x96f3_b83du32.to_le_bytes() {
        return None;
    }
    let header = usize::from(u16::from_le_bytes(data[8..10].try_into().ok()?));
    let size = u32::from_le_bytes(data[12..16].try_into().ok()?) as usize;
    let start = header.checked_add(size)?;
    let info = data.get(start..start + 4)?;
    if info[..2] != 0x6907u16.to_le_bytes() {
        return None;
    }
    let end = start.checked_add(usize::from(u16::from_le_bytes(info[2..4].try_into().ok()?)))?;
    let mut offset = start + 4;
    while offset + 4 <= end {
        let tlv = data.get(offset..offset + 4)?;
        let kind = u16::from_le_bytes(tlv[..2].try_into().ok()?);
        let len = usize::from(u16::from_le_bytes(tlv[2..].try_into().ok()?));
        offset += 4;
        if kind == 0x10 && len == 32 {
            return data.get(offset..offset + len).map(<[u8]>::to_vec);
        }
        offset += len;
    }
    None
}
