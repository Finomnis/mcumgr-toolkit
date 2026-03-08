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

/// The hash of a firmware image
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ImageHash {
    Sha256([u8; SHA256_LEN]),
    Sha384([u8; SHA384_LEN]),
    Sha512([u8; SHA512_LEN]),
}

impl ImageHash {
    /// The has type, as a human readable string
    pub fn get_hash_type(&self) -> &'static str {
        match self {
            ImageHash::Sha256(_) => "SHA256",
            ImageHash::Sha384(_) => "SHA384",
            ImageHash::Sha512(_) => "SHA512",
        }
    }
}

impl From<ImageHash> for Vec<u8> {
    fn from(hash: ImageHash) -> Self {
        match hash {
            ImageHash::Sha256(val) => val.into(),
            ImageHash::Sha384(val) => val.into(),
            ImageHash::Sha512(val) => val.into(),
        }
    }
}

impl From<ImageHash> for Box<[u8]> {
    fn from(hash: ImageHash) -> Self {
        match hash {
            ImageHash::Sha256(val) => val.into(),
            ImageHash::Sha384(val) => val.into(),
            ImageHash::Sha512(val) => val.into(),
        }
    }
}

impl AsRef<[u8]> for ImageHash {
    fn as_ref(&self) -> &[u8] {
        match self {
            ImageHash::Sha256(val) => val,
            ImageHash::Sha384(val) => val,
            ImageHash::Sha512(val) => val,
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
    pub hash: ImageHash,
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
    /// The given image file does not contain an SHA id hash.
    #[error("Image does not contain an SHA id hash")]
    #[diagnostic(code(mcumgr_toolkit::mcuboot::image::id_hash_missing))]
    IdHashMissing,
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
const IMAGE_TLV_SHA256: u8 = 0x10;
const IMAGE_TLV_SHA384: u8 = 0x11;
const IMAGE_TLV_SHA512: u8 = 0x12;
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

    let it_magic = read_u16(image_data)?;
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
            let it_type = read_u8(image_data)?;
            read_u8(image_data)?;
            let it_len = read_u16(image_data)?;

            if it_type == IMAGE_TLV_SHA256 && usize::from(it_len) == SHA256_LEN {
                let mut sha256_hash = [0u8; SHA256_LEN];
                image_data.read_exact(&mut sha256_hash)?;
                id_hash = Some(ImageHash::Sha256(sha256_hash));
            } else if it_type == IMAGE_TLV_SHA384 && usize::from(it_len) == SHA384_LEN {
                let mut sha384_hash = [0u8; SHA384_LEN];
                image_data.read_exact(&mut sha384_hash)?;
                id_hash = Some(ImageHash::Sha384(sha384_hash));
            } else if it_type == IMAGE_TLV_SHA512 && usize::from(it_len) == SHA512_LEN {
                let mut sha512_hash = [0u8; SHA512_LEN];
                image_data.read_exact(&mut sha512_hash)?;
                id_hash = Some(ImageHash::Sha512(sha512_hash));
            } else {
                image_data.seek_relative(it_len.into())?;
            }

            log::debug!("- it_type: 0x{it_type:02x}, it_len: 0x{it_len:02x}");
            tlv_read += u32::from(it_len) + 4;
        }
    }

    if let Some(id_hash) = id_hash {
        Ok(ImageInfo {
            version: ih_ver,
            hash: id_hash,
        })
    } else {
        Err(ImageParseError::IdHashMissing)
    }
}
