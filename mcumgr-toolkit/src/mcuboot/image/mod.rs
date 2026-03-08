use std::io;

/// The firmware version
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ImageVersion {
    /// Major version
    pub major: u8,
    /// Minor version
    pub minor: u8,
    /// Revision
    pub revision: u16,
    /// Build number
    pub build_num: u32,
}
impl std::fmt::Display for ImageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.revision)?;
        if self.build_num != 0 {
            write!(f, ".{}", self.build_num)?;
        }
        Ok(())
    }
}

/// The hash id of a firmware image
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ImageHashId {
    Sha256([u8; SHA256_LEN]),
    Sha384([u8; SHA384_LEN]),
    Sha512([u8; SHA512_LEN]),
}

impl ImageHashId {
    /// The hash type, as a human readable string
    pub fn get_hash_type(&self) -> &'static str {
        match self {
            ImageHashId::Sha256(_) => "SHA256",
            ImageHashId::Sha384(_) => "SHA384",
            ImageHashId::Sha512(_) => "SHA512",
        }
    }
}

impl From<ImageHashId> for Vec<u8> {
    fn from(hash: ImageHashId) -> Self {
        match hash {
            ImageHashId::Sha256(val) => val.into(),
            ImageHashId::Sha384(val) => val.into(),
            ImageHashId::Sha512(val) => val.into(),
        }
    }
}

impl From<ImageHashId> for Box<[u8]> {
    fn from(hash: ImageHashId) -> Self {
        match hash {
            ImageHashId::Sha256(val) => val.into(),
            ImageHashId::Sha384(val) => val.into(),
            ImageHashId::Sha512(val) => val.into(),
        }
    }
}

impl AsRef<[u8]> for ImageHashId {
    fn as_ref(&self) -> &[u8] {
        match self {
            ImageHashId::Sha256(val) => val,
            ImageHashId::Sha384(val) => val,
            ImageHashId::Sha512(val) => val,
        }
    }
}

/// Information about an MCUboot firmware image
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ImageInfo {
    /// Firmware version
    pub version: ImageVersion,
    /// The identifying hash for the firmware
    ///
    /// Note that this will not be the same as the SHA256 of the whole file, it is the field in the
    /// MCUboot TLV section that contains a hash of the data which is used for signature
    /// verification purposes.
    pub hash: ImageHashId,
}

/// Possible error values of [`get_image_info`].
#[derive(thiserror::Error, Debug, miette::Diagnostic)]
pub enum ImageParseError {
    /// The given image file is not an MCUboot image.
    #[error("Image is not an MCUboot image")]
    #[diagnostic(code(mcumgr_toolkit::mcuboot::image::unknown_type))]
    UnknownImageType,
    /// The given image file does not contain TLV entries.
    #[error("Image does not contain TLV entries")]
    #[diagnostic(code(mcumgr_toolkit::mcuboot::image::tlv_missing))]
    TlvMissing,
    /// The given image file does not contain an SHA hash id.
    #[error("Image does not contain an SHA hash id")]
    #[diagnostic(code(mcumgr_toolkit::mcuboot::image::hash_id_missing))]
    HashIdMissing,
    /// Failed to read from the image
    #[error("Image read failed")]
    #[diagnostic(code(mcumgr_toolkit::mcuboot::image::read))]
    ReadFailed(#[from] std::io::Error),
}

fn read_u32(data: &mut dyn std::io::Read) -> Result<u32, std::io::Error> {
    let mut bytes = [0u8; 4];
    data.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u16(data: &mut dyn std::io::Read) -> Result<u16, std::io::Error> {
    let mut bytes = [0u8; 2];
    data.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u8(data: &mut dyn std::io::Read) -> Result<u8, std::io::Error> {
    let mut byte = 0u8;
    data.read_exact(std::slice::from_mut(&mut byte))?;
    Ok(byte)
}

/// The identifying header of an MCUboot image
const IMAGE_MAGIC: u32 = 0x96f3b83d;
const IMAGE_TLV_INFO_MAGIC: u16 = 0x6907;
const IMAGE_TLV_SHA256: u16 = 0x10;
const IMAGE_TLV_SHA384: u16 = 0x11;
const IMAGE_TLV_SHA512: u16 = 0x12;
const SHA256_LEN: usize = 32;
const SHA384_LEN: usize = 48;
const SHA512_LEN: usize = 64;
const TLV_INFO_HEADER_SIZE: u32 = 4;
const TLV_ELEMENT_HEADER_SIZE: u32 = 4;

/// Extract information from an MCUboot image file
pub fn get_image_info(
    mut image_data: impl io::Read + io::Seek,
) -> Result<ImageInfo, ImageParseError> {
    let image_data = &mut image_data;

    let ih_magic = read_u32(image_data)?;
    log::debug!("ih_magic: 0x{ih_magic:08x}");
    if ih_magic != IMAGE_MAGIC {
        return Err(ImageParseError::UnknownImageType);
    }

    let ih_load_addr = read_u32(image_data)?;
    log::debug!("ih_load_addr: 0x{ih_load_addr:08x}");

    let ih_hdr_size = read_u16(image_data)?;
    log::debug!("ih_hdr_size: 0x{ih_hdr_size:04x}");

    let ih_protect_tlv_size = read_u16(image_data)?;
    log::debug!("ih_protect_tlv_size: 0x{ih_protect_tlv_size:04x}");

    let ih_img_size = read_u32(image_data)?;
    log::debug!("ih_img_size: 0x{ih_img_size:08x}");

    let ih_flags = read_u32(image_data)?;
    log::debug!("ih_flags: 0x{ih_flags:08x}");

    let ih_ver = ImageVersion {
        major: read_u8(image_data)?,
        minor: read_u8(image_data)?,
        revision: read_u16(image_data)?,
        build_num: read_u32(image_data)?,
    };
    log::debug!("ih_ver: {ih_ver:?}");

    image_data.seek(io::SeekFrom::Start(
        u64::from(ih_hdr_size) + u64::from(ih_protect_tlv_size) + u64::from(ih_img_size),
    ))?;

    let it_magic = match read_u16(image_data) {
        Ok(val) => val,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Err(ImageParseError::TlvMissing);
            }
            return Err(e.into());
        }
    };
    log::debug!("it_magic: 0x{it_magic:04x}");
    if it_magic != IMAGE_TLV_INFO_MAGIC {
        return Err(ImageParseError::TlvMissing);
    }

    let it_tlv_tot = read_u16(image_data)?;
    log::debug!("it_tlv_tot: 0x{it_tlv_tot:04x}");

    let mut id_hash = None;
    {
        let mut tlv_read: u32 = 0;
        // Loop while at least one tlv header can still be read
        while tlv_read + TLV_INFO_HEADER_SIZE + TLV_ELEMENT_HEADER_SIZE <= u32::from(it_tlv_tot) {
            let it_type = read_u16(image_data)?;
            let it_len = read_u16(image_data)?;

            if it_type == IMAGE_TLV_SHA256 && usize::from(it_len) == SHA256_LEN {
                let mut sha256_hash = [0u8; SHA256_LEN];
                image_data.read_exact(&mut sha256_hash)?;
                id_hash = Some(ImageHashId::Sha256(sha256_hash));
            } else if it_type == IMAGE_TLV_SHA384 && usize::from(it_len) == SHA384_LEN {
                let mut sha384_hash = [0u8; SHA384_LEN];
                image_data.read_exact(&mut sha384_hash)?;
                id_hash = Some(ImageHashId::Sha384(sha384_hash));
            } else if it_type == IMAGE_TLV_SHA512 && usize::from(it_len) == SHA512_LEN {
                let mut sha512_hash = [0u8; SHA512_LEN];
                image_data.read_exact(&mut sha512_hash)?;
                id_hash = Some(ImageHashId::Sha512(sha512_hash));
            } else {
                image_data.seek_relative(it_len.into())?;
            }

            log::debug!("- it_type: 0x{it_type:04x}, it_len: 0x{it_len:04x}");
            tlv_read += u32::from(it_len) + 4;
        }
    }

    if let Some(id_hash) = id_hash {
        Ok(ImageInfo {
            version: ih_ver,
            hash: id_hash,
        })
    } else {
        Err(ImageParseError::HashIdMissing)
    }
}

#[cfg(test)]
mod tests {
    use super::{ImageHashId, ImageParseError, ImageVersion, get_image_info};
    use std::io::{self, Cursor, Read, Seek, SeekFrom};

    const IMAGE_MAGIC: u32 = 0x96f3_b83d;
    const IMAGE_HEADER_SIZE: usize = 32;

    const IMAGE_TLV_INFO_MAGIC: u16 = 0x6907;
    const IMAGE_TLV_PROT_INFO_MAGIC: u16 = 0x6908;

    const IMAGE_TLV_KEYHASH: u16 = 0x01;
    const IMAGE_TLV_SHA256: u16 = 0x10;
    const IMAGE_TLV_SHA384: u16 = 0x11;
    const IMAGE_TLV_SHA512: u16 = 0x12;
    const IMAGE_TLV_ECDSA_SIG: u16 = 0x22;
    const IMAGE_TLV_SEC_CNT: u16 = 0x50;

    fn tlv(ty: u16, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + value.len());
        out.extend_from_slice(&ty.to_le_bytes());
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
        out.extend_from_slice(value);
        out
    }

    fn tlv_area(info_magic: u16, entries: &[Vec<u8>]) -> Vec<u8> {
        let total_len: usize = 4 + entries.iter().map(Vec::len).sum::<usize>();
        let mut out = Vec::with_capacity(total_len);
        out.extend_from_slice(&info_magic.to_le_bytes());
        out.extend_from_slice(&(total_len as u16).to_le_bytes());
        for entry in entries {
            out.extend_from_slice(entry);
        }
        out
    }

    fn image_header(
        image_magic: u32,
        hdr_size: u16,
        protect_tlv_size: u16,
        img_size: u32,
        version: ImageVersion,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(IMAGE_HEADER_SIZE);
        out.extend_from_slice(&image_magic.to_le_bytes()); // ih_magic
        out.extend_from_slice(&0x1122_3344u32.to_le_bytes()); // ih_load_addr
        out.extend_from_slice(&hdr_size.to_le_bytes()); // ih_hdr_size
        out.extend_from_slice(&protect_tlv_size.to_le_bytes()); // ih_protect_tlv_size
        out.extend_from_slice(&img_size.to_le_bytes()); // ih_img_size
        out.extend_from_slice(&0x5566_7788u32.to_le_bytes()); // ih_flags

        out.push(version.major);
        out.push(version.minor);
        out.extend_from_slice(&version.revision.to_le_bytes());
        out.extend_from_slice(&version.build_num.to_le_bytes());

        out.extend_from_slice(&0u32.to_le_bytes()); // _pad1
        assert_eq!(out.len(), IMAGE_HEADER_SIZE);
        out
    }

    fn build_image(
        image_magic: u32,
        hdr_size: u16,
        version: ImageVersion,
        payload: &[u8],
        protected_tlv_area: Option<Vec<u8>>,
        regular_tlv_area: Option<Vec<u8>>,
    ) -> Vec<u8> {
        let protected_tlv_size = protected_tlv_area
            .as_ref()
            .map(|v| v.len() as u16)
            .unwrap_or(0);

        let mut out = image_header(
            image_magic,
            hdr_size,
            protected_tlv_size,
            payload.len() as u32,
            version,
        );

        if hdr_size as usize > IMAGE_HEADER_SIZE {
            out.resize(hdr_size as usize, 0xAA);
        }

        out.extend_from_slice(payload);

        if let Some(protected) = protected_tlv_area {
            out.extend_from_slice(&protected);
        }
        if let Some(regular) = regular_tlv_area {
            out.extend_from_slice(&regular);
        }

        out
    }

    #[test]
    fn image_version_display_omits_zero_build_number() {
        let version = ImageVersion {
            major: 1,
            minor: 2,
            revision: 345,
            build_num: 0,
        };

        assert_eq!(version.to_string(), "1.2.345");
    }

    #[test]
    fn image_version_display_includes_nonzero_build_number() {
        let version = ImageVersion {
            major: 1,
            minor: 2,
            revision: 345,
            build_num: 6789,
        };

        assert_eq!(version.to_string(), "1.2.345.6789");
    }

    #[test]
    fn image_hash_id_reports_human_readable_hash_type() {
        assert_eq!(ImageHashId::Sha256([0x11; 32]).get_hash_type(), "SHA256");
        assert_eq!(ImageHashId::Sha384([0x22; 48]).get_hash_type(), "SHA384");
        assert_eq!(ImageHashId::Sha512([0x33; 64]).get_hash_type(), "SHA512");
    }

    #[test]
    fn image_hash_id_converts_to_vec_box_and_slice() {
        let sha256 = ImageHashId::Sha256([0xA5; 32]);
        let sha384 = ImageHashId::Sha384([0xB6; 48]);
        let sha512 = ImageHashId::Sha512([0xC7; 64]);

        let sha256_vec: Vec<u8> = sha256.into();
        let sha384_box: Box<[u8]> = sha384.into();

        assert_eq!(sha256_vec, vec![0xA5; 32]);
        assert_eq!(&*sha384_box, &[0xB6; 48]);
        assert_eq!(sha512.as_ref(), &[0xC7; 64]);
    }

    #[test]
    fn parses_sha256_image_and_scans_past_other_unprotected_tlvs() {
        let version = ImageVersion {
            major: 7,
            minor: 9,
            revision: 0x1234,
            build_num: 0x89AB_CDEF,
        };
        let payload = b"payload-bytes";
        let hash = [0x10; 32];

        let regular_tlv_area = tlv_area(
            IMAGE_TLV_INFO_MAGIC,
            &[
                tlv(IMAGE_TLV_KEYHASH, &[0x01, 0x02, 0x03, 0x04]),
                tlv(IMAGE_TLV_SHA256, &hash),
                tlv(IMAGE_TLV_ECDSA_SIG, &[0x55; 8]),
            ],
        );

        let image = build_image(
            IMAGE_MAGIC,
            IMAGE_HEADER_SIZE as u16,
            version,
            payload,
            None,
            Some(regular_tlv_area),
        );

        let info = get_image_info(Cursor::new(image)).expect("valid SHA256 image should parse");
        assert_eq!(info.version, version);
        assert_eq!(info.hash, ImageHashId::Sha256(hash));
    }

    #[test]
    fn parses_sha384_image_and_respects_hdr_size_for_payload_offset() {
        let version = ImageVersion {
            major: 3,
            minor: 4,
            revision: 0xBEEF,
            build_num: 0x0102_0304,
        };
        let payload = [0xDE, 0xAD, 0xBE, 0xEF, 0x42];
        let hash = [0x44; 48];

        let regular_tlv_area = tlv_area(IMAGE_TLV_INFO_MAGIC, &[tlv(IMAGE_TLV_SHA384, &hash)]);

        // Use a non-default header size to verify the parser honors ih_hdr_size
        // instead of assuming the fixed 32-byte struct size.
        let image = build_image(
            IMAGE_MAGIC,
            64,
            version,
            &payload,
            None,
            Some(regular_tlv_area),
        );

        let info = get_image_info(Cursor::new(image)).expect("valid SHA384 image should parse");
        assert_eq!(info.version, version);
        assert_eq!(info.hash, ImageHashId::Sha384(hash));
    }

    #[test]
    fn parses_sha512_image_after_protected_tlv_block() {
        let version = ImageVersion {
            major: 5,
            minor: 6,
            revision: 0x2468,
            build_num: 0x1357_9BDF,
        };
        let payload = b"firmware";
        let hash = [0x77; 64];

        let protected_tlv_area = tlv_area(
            IMAGE_TLV_PROT_INFO_MAGIC,
            &[tlv(IMAGE_TLV_SEC_CNT, &[0x05, 0x00, 0x00, 0x00])],
        );

        let regular_tlv_area = tlv_area(
            IMAGE_TLV_INFO_MAGIC,
            &[
                tlv(IMAGE_TLV_KEYHASH, &[0xAA; 16]),
                tlv(IMAGE_TLV_SHA512, &hash),
            ],
        );

        let image = build_image(
            IMAGE_MAGIC,
            IMAGE_HEADER_SIZE as u16,
            version,
            payload,
            Some(protected_tlv_area),
            Some(regular_tlv_area),
        );

        let info = get_image_info(Cursor::new(image))
            .expect("valid image with protected TLVs should parse");
        assert_eq!(info.version, version);
        assert_eq!(info.hash, ImageHashId::Sha512(hash));
    }

    #[test]
    fn rejects_non_mcuboot_magic() {
        let version = ImageVersion {
            major: 1,
            minor: 0,
            revision: 1,
            build_num: 1,
        };
        let payload = b"x";
        let regular_tlv_area =
            tlv_area(IMAGE_TLV_INFO_MAGIC, &[tlv(IMAGE_TLV_SHA256, &[0x11; 32])]);

        let image = build_image(
            0x0000_0000,
            IMAGE_HEADER_SIZE as u16,
            version,
            payload,
            None,
            Some(regular_tlv_area),
        );

        let err = get_image_info(Cursor::new(image)).unwrap_err();
        assert!(matches!(err, ImageParseError::UnknownImageType));
    }

    #[test]
    fn rejects_image_without_any_tlv_info_header() {
        let version = ImageVersion {
            major: 9,
            minor: 9,
            revision: 9,
            build_num: 9,
        };
        let payload = b"no tlvs here";

        let image = build_image(
            IMAGE_MAGIC,
            IMAGE_HEADER_SIZE as u16,
            version,
            payload,
            None,
            None,
        );

        let err = get_image_info(Cursor::new(image)).unwrap_err();
        assert!(matches!(err, ImageParseError::TlvMissing));
    }

    #[test]
    fn rejects_protected_tlv_block_without_following_regular_tlv_info_header() {
        let version = ImageVersion {
            major: 2,
            minor: 1,
            revision: 0x0102,
            build_num: 3,
        };
        let payload = b"abc";

        let protected_tlv_area = tlv_area(
            IMAGE_TLV_PROT_INFO_MAGIC,
            &[tlv(IMAGE_TLV_SEC_CNT, &[1, 0, 0, 0])],
        );

        // Per the spec, if IMAGE_TLV_PROT_INFO_MAGIC is present, a normal
        // IMAGE_TLV_INFO_MAGIC block must follow after ih_protect_tlv_size bytes.
        let image = build_image(
            IMAGE_MAGIC,
            IMAGE_HEADER_SIZE as u16,
            version,
            payload,
            Some(protected_tlv_area),
            None,
        );

        let err = get_image_info(Cursor::new(image)).unwrap_err();
        assert!(matches!(err, ImageParseError::TlvMissing));
    }

    #[test]
    fn rejects_image_with_wrong_tlv_info_magic() {
        let version = ImageVersion {
            major: 1,
            minor: 2,
            revision: 3,
            build_num: 4,
        };
        let image = build_image(
            IMAGE_MAGIC,
            IMAGE_HEADER_SIZE as u16,
            version,
            b"x",
            None,
            Some(tlv_area(0xFFFF, &[tlv(IMAGE_TLV_SHA256, &[0x11; 32])])),
        );

        let err = get_image_info(Cursor::new(image)).unwrap_err();
        assert!(matches!(err, ImageParseError::TlvMissing));
    }

    #[test]
    fn rejects_image_with_tlv_area_but_without_any_supported_hash_tlv() {
        let version = ImageVersion {
            major: 8,
            minor: 1,
            revision: 2,
            build_num: 3,
        };
        let payload = b"firmware";
        let regular_tlv_area = tlv_area(
            IMAGE_TLV_INFO_MAGIC,
            &[
                tlv(IMAGE_TLV_KEYHASH, &[0x11; 16]),
                tlv(IMAGE_TLV_ECDSA_SIG, &[0x22; 8]),
            ],
        );

        let image = build_image(
            IMAGE_MAGIC,
            IMAGE_HEADER_SIZE as u16,
            version,
            payload,
            None,
            Some(regular_tlv_area),
        );

        let err = get_image_info(Cursor::new(image)).unwrap_err();
        assert!(matches!(err, ImageParseError::HashIdMissing));
    }

    #[test]
    fn rejects_sha256_tlv_with_wrong_payload_length() {
        // Type 0x10 with 16 bytes instead of the required 32 must be skipped,
        // not parsed as a valid hash → HashIdMissing.
        let version = ImageVersion {
            major: 1,
            minor: 0,
            revision: 0,
            build_num: 0,
        };
        let regular_tlv_area = tlv_area(
            IMAGE_TLV_INFO_MAGIC,
            &[tlv(IMAGE_TLV_SHA256, &[0x42u8; 16])], // wrong length
        );
        let image = build_image(
            IMAGE_MAGIC,
            IMAGE_HEADER_SIZE as u16,
            version,
            b"payload",
            None,
            Some(regular_tlv_area),
        );
        let err = get_image_info(Cursor::new(image)).unwrap_err();
        assert!(matches!(err, ImageParseError::HashIdMissing));
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected read failure"))
        }
    }

    impl Seek for FailingReader {
        fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
            Ok(0)
        }
    }

    #[test]
    fn propagates_io_failures_as_read_failed() {
        let err = get_image_info(FailingReader).unwrap_err();

        match err {
            ImageParseError::ReadFailed(inner) => {
                assert_eq!(inner.kind(), io::ErrorKind::Other);
                assert_eq!(inner.to_string(), "injected read failure");
            }
            other => panic!("expected ReadFailed, got {other:?}"),
        }
    }
}
