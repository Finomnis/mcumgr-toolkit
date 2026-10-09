//! Builds MCUboot images in the format produced by `imgtool sign`, as
//! described by MCUboot's `boot/bootutil/include/bootutil/image.h`.

use sha2::{Digest, Sha256};

use super::img_mgmt::{
    IMAGE_F_NON_BOOTABLE, IMAGE_MAGIC, IMAGE_TLV_INFO_MAGIC, IMAGE_TLV_PROT_INFO_MAGIC,
    IMAGE_TLV_SHA256,
};

/// MCUboot `IMAGE_TLV_KEYHASH`
pub const IMAGE_TLV_KEYHASH: u16 = 0x01;
/// MCUboot `IMAGE_TLV_SEC_CNT`, a protected TLV
pub const IMAGE_TLV_SEC_CNT: u16 = 0x50;

#[derive(Clone, Debug)]
pub struct ImageBuilder {
    version: (u8, u8, u16, u32),
    header_size: u16,
    body: Vec<u8>,
    flags: u32,
    protected_tlvs: Vec<(u16, Vec<u8>)>,
    tlvs: Vec<(u16, Vec<u8>)>,
}

impl ImageBuilder {
    /// An image with the given `major.minor.revision+build` version and a
    /// deterministic pseudo random body
    pub fn new(version: (u8, u8, u16, u32)) -> Self {
        let seed = u32::from(version.0) << 24 ^ u32::from(version.1) << 16 ^ u32::from(version.2);
        let mut state = seed ^ version.3 ^ 0x1234_5678;
        let body = (0..3000)
            .map(|_| {
                // xorshift32
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        Self {
            version,
            // Zephyr's CONFIG_ROM_START_OFFSET
            header_size: 0x200,
            body,
            flags: 0,
            protected_tlvs: vec![],
            tlvs: vec![(IMAGE_TLV_KEYHASH, vec![0xab; 32])],
        }
    }

    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    pub fn non_bootable(mut self) -> Self {
        self.flags |= IMAGE_F_NON_BOOTABLE;
        self
    }

    pub fn protected_tlv(mut self, ty: u16, data: Vec<u8>) -> Self {
        self.protected_tlvs.push((ty, data));
        self
    }

    fn tlv_area(magic: u16, tlvs: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut entries = vec![];
        for (ty, data) in tlvs {
            entries.extend_from_slice(&ty.to_le_bytes());
            entries.extend_from_slice(&(data.len() as u16).to_le_bytes());
            entries.extend_from_slice(data);
        }
        // `it_tlv_tot` includes the `image_tlv_info` header
        let mut area = magic.to_le_bytes().to_vec();
        area.extend_from_slice(&((entries.len() + 4) as u16).to_le_bytes());
        area.extend_from_slice(&entries);
        area
    }

    /// The image data up to (excluding) the unprotected TLV area, which is
    /// what the SHA256 TLV covers
    fn hashed_part(&self) -> Vec<u8> {
        let protected = if self.protected_tlvs.is_empty() {
            vec![]
        } else {
            Self::tlv_area(IMAGE_TLV_PROT_INFO_MAGIC, &self.protected_tlvs)
        };

        let mut image = vec![];
        image.extend_from_slice(&IMAGE_MAGIC.to_le_bytes());
        image.extend_from_slice(&0u32.to_le_bytes()); // ih_load_addr
        image.extend_from_slice(&self.header_size.to_le_bytes());
        image.extend_from_slice(&(protected.len() as u16).to_le_bytes());
        image.extend_from_slice(&(self.body.len() as u32).to_le_bytes());
        image.extend_from_slice(&self.flags.to_le_bytes());
        image.push(self.version.0);
        image.push(self.version.1);
        image.extend_from_slice(&self.version.2.to_le_bytes());
        image.extend_from_slice(&self.version.3.to_le_bytes());
        image.extend_from_slice(&0u32.to_le_bytes()); // _pad1
        image.resize(self.header_size.into(), 0);
        image.extend_from_slice(&self.body);
        image.extend_from_slice(&protected);
        image
    }

    /// The SHA256 the image is identified by (the `IMAGE_TLV_SHA256` value)
    pub fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.hashed_part()).into()
    }

    pub fn build(&self) -> Vec<u8> {
        let mut tlvs = vec![(IMAGE_TLV_SHA256, self.hash().to_vec())];
        tlvs.extend(self.tlvs.iter().cloned());

        let mut image = self.hashed_part();
        image.extend_from_slice(&Self::tlv_area(IMAGE_TLV_INFO_MAGIC, &tlvs));
        image
    }
}
