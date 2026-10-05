//! Image management group, modelled after
//! `subsys/mgmt/mcumgr/grp/img_mgmt/src/{img_mgmt.c,img_mgmt_state.c,zephyr_img_mgmt.c}`,
//! on top of a model of MCUboot's slot trailers (`boot/bootutil/src/bootutil_public.c`)
//! for a swap based bootloader mode.

use sha2::{Digest, Sha256};

use super::Device;
use ciborium::Value;

use super::cbor::{Kind, int, map, uint};
use super::smp::{Ctx, Group, group_id, mgmt_err, read, read_write, write};

const IMG_MGMT_ID_STATE: u8 = 0;
const IMG_MGMT_ID_UPLOAD: u8 = 1;
const IMG_MGMT_ID_ERASE: u8 = 5;
const IMG_MGMT_ID_SLOT_INFO: u8 = 6;

/// `enum img_mgmt_err_code_t`
#[allow(dead_code)]
pub mod img_mgmt_err {
    pub const UNKNOWN: u16 = 1;
    pub const FLASH_CONFIG_QUERY_FAIL: u16 = 2;
    pub const NO_IMAGE: u16 = 3;
    pub const NO_TLVS: u16 = 4;
    pub const INVALID_TLV: u16 = 5;
    pub const TLV_MULTIPLE_HASHES_FOUND: u16 = 6;
    pub const TLV_INVALID_SIZE: u16 = 7;
    pub const HASH_NOT_FOUND: u16 = 8;
    pub const NO_FREE_SLOT: u16 = 9;
    pub const FLASH_OPEN_FAILED: u16 = 10;
    pub const FLASH_READ_FAILED: u16 = 11;
    pub const FLASH_WRITE_FAILED: u16 = 12;
    pub const FLASH_ERASE_FAILED: u16 = 13;
    pub const INVALID_SLOT: u16 = 14;
    pub const NO_FREE_MEMORY: u16 = 15;
    pub const FLASH_CONTEXT_ALREADY_SET: u16 = 16;
    pub const FLASH_CONTEXT_NOT_SET: u16 = 17;
    pub const FLASH_AREA_DEVICE_NULL: u16 = 18;
    pub const INVALID_PAGE_OFFSET: u16 = 19;
    pub const INVALID_OFFSET: u16 = 20;
    pub const INVALID_LENGTH: u16 = 21;
    pub const INVALID_IMAGE_HEADER: u16 = 22;
    pub const INVALID_IMAGE_HEADER_MAGIC: u16 = 23;
    pub const INVALID_HASH: u16 = 24;
    pub const INVALID_FLASH_ADDRESS: u16 = 25;
    pub const VERSION_GET_FAILED: u16 = 26;
    pub const CURRENT_VERSION_IS_NEWER: u16 = 27;
    pub const IMAGE_ALREADY_PENDING: u16 = 28;
    pub const INVALID_IMAGE_VECTOR_TABLE: u16 = 29;
    pub const INVALID_IMAGE_TOO_LARGE: u16 = 30;
    pub const INVALID_IMAGE_DATA_OVERRUN: u16 = 31;
    pub const IMAGE_CONFIRMATION_DENIED: u16 = 32;
    pub const IMAGE_SETTING_TEST_TO_ACTIVE_DENIED: u16 = 33;
}
use img_mgmt_err as err;

/// MCUboot `IMAGE_MAGIC`
pub const IMAGE_MAGIC: u32 = 0x96f3_b83d;
/// MCUboot `IMAGE_TLV_INFO_MAGIC`
pub const IMAGE_TLV_INFO_MAGIC: u16 = 0x6907;
/// MCUboot `IMAGE_TLV_PROT_INFO_MAGIC`
pub const IMAGE_TLV_PROT_INFO_MAGIC: u16 = 0x6908;
/// MCUboot `IMAGE_TLV_SHA256`
pub const IMAGE_TLV_SHA256: u16 = 0x10;
/// MCUboot `IMAGE_F_NON_BOOTABLE`
pub const IMAGE_F_NON_BOOTABLE: u32 = 0x10;
const IMAGE_HEADER_SIZE: usize = 32;
const IMAGE_SHA_LEN: usize = 32;
const IMG_MGMT_DATA_SHA_LEN: usize = 32;

/// MCUboot `BOOT_MAGIC_*`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Magic {
    #[default]
    Unset,
    Good,
    Bad,
}

/// The MCUboot image trailer of a slot (`struct boot_swap_state`)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trailer {
    pub magic: Magic,
    pub image_ok: bool,
    pub copy_done: bool,
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub flash: Vec<u8>,
    pub trailer: Trailer,
}

impl Slot {
    pub fn erased(size: usize) -> Self {
        Self {
            flash: vec![0xff; size],
            trailer: Trailer::default(),
        }
    }

    /// A slot that was programmed with `image`
    pub fn programmed(size: usize, image: &[u8], trailer: Trailer) -> Self {
        let mut slot = Self::erased(size);
        slot.flash[..image.len()].copy_from_slice(image);
        slot.trailer = trailer;
        slot
    }

    fn is_empty(&self) -> bool {
        self.trailer == Trailer::default() && self.flash.iter().all(|b| *b == 0xff)
    }
}

/// `g_img_mgmt_state`
#[derive(Clone, Debug, Default)]
struct UploadState {
    /// -1 in C
    area: Option<usize>,
    off: u64,
    size: u64,
    data_sha: [u8; IMG_MGMT_DATA_SHA_LEN],
    data_sha_len: usize,
}

#[derive(Clone, Debug)]
pub struct ImgState {
    /// Two slots per image; slot `n` belongs to image `n / 2`
    pub slots: Vec<Slot>,
    /// Simulates a defective flash that flips a bit in every written chunk
    pub corrupt_flash_writes: bool,
    upload: UploadState,
}

impl ImgState {
    pub fn new(images: usize, slot_size: usize) -> Self {
        Self {
            slots: vec![Slot::erased(slot_size); images * 2],
            corrupt_flash_writes: false,
            upload: UploadState::default(),
        }
    }

    /// `img_mgmt_reset_upload`
    pub fn reset_upload(&mut self) {
        self.upload = UploadState::default();
    }
}

/// `BOOT_SWAP_TYPE_*`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapType {
    None,
    Test,
    Perm,
    Revert,
}

/// `enum img_mgmt_next_boot_type`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NextBootType {
    Normal,
    Test,
    Revert,
}

/// The parts of `struct image_header` that img_mgmt looks at
struct ImageInfo {
    version: (u8, u8, u16, u32),
    hash: [u8; IMAGE_SHA_LEN],
    flags: u32,
}

fn le16(data: &[u8]) -> u16 {
    u16::from_le_bytes([data[0], data[1]])
}

fn le32(data: &[u8]) -> u32 {
    u32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

/// `img_mgmt_ver_str`
fn ver_str((major, minor, revision, build): (u8, u8, u16, u32)) -> String {
    if build != 0 {
        format!("{major}.{minor}.{revision}.{build}")
    } else {
        format!("{major}.{minor}.{revision}")
    }
}

pub fn group(config: &super::Config) -> Group {
    let mut handlers = vec![
        (IMG_MGMT_ID_STATE, read_write(state_read, state_write)),
        (IMG_MGMT_ID_UPLOAD, write(upload)),
        (IMG_MGMT_ID_ERASE, write(erase)),
    ];
    if config.img_slot_info {
        handlers.push((IMG_MGMT_ID_SLOT_INFO, read(slot_info)));
    }
    Group::new(group_id::IMAGE, "img mgmt", &handlers)
}

impl Device {
    fn img_slot_count(&self) -> usize {
        self.img.slots.len()
    }

    /// `img_mgmt_active_image`; the application always runs from image 0
    fn img_active_image(&self) -> usize {
        0
    }

    /// `img_mgmt_active_slot`
    fn img_active_slot(&self, image: usize) -> usize {
        image << 1
    }

    /// `img_mgmt_read`
    fn img_read(&self, slot: usize, off: usize, len: usize) -> Result<&[u8], u16> {
        let flash = &self.img.slots.get(slot).ok_or(err::INVALID_SLOT)?.flash;
        flash.get(off..off + len).ok_or(err::FLASH_READ_FAILED)
    }

    /// `img_mgmt_read_info`
    fn img_read_info(&self, slot: usize) -> Result<ImageInfo, u16> {
        let hdr = self.img_read(slot, 0, IMAGE_HEADER_SIZE)?;
        let magic = le32(&hdr[0..]);
        if magic == 0xffff_ffff {
            return Err(err::NO_IMAGE);
        } else if magic != IMAGE_MAGIC {
            return Err(err::INVALID_IMAGE_HEADER_MAGIC);
        }
        let hdr_size = le16(&hdr[8..]) as usize;
        let img_size = le32(&hdr[12..]) as usize;
        let flags = le32(&hdr[16..]);
        let version = (hdr[20], hdr[21], le16(&hdr[22..]), le32(&hdr[24..]));

        let find_tlvs = |start: &mut usize, end: &mut usize, magic: u16| -> Result<(), u16> {
            let info = self.img_read(slot, *start, 4)?;
            if le16(info) != magic {
                return Err(err::NO_TLVS);
            }
            let tot = le16(&info[2..]) as usize;
            *start += 4;
            *end = *start + tot;
            Ok(())
        };

        let mut data_off = hdr_size + img_size;
        let mut data_end = 0;
        if find_tlvs(&mut data_off, &mut data_end, IMAGE_TLV_PROT_INFO_MAGIC).is_ok() {
            data_off = data_end - 4;
        }
        find_tlvs(&mut data_off, &mut data_end, IMAGE_TLV_INFO_MAGIC).map_err(|_| err::NO_TLVS)?;

        let mut hash = None;
        while data_off + 4 <= data_end {
            let tlv = self.img_read(slot, data_off, 4)?;
            let (it_type, it_len) = (le16(tlv), le16(&tlv[2..]) as usize);
            if it_type == 0xff && it_len == 0xffff {
                return Err(err::INVALID_TLV);
            }
            if it_type != IMAGE_TLV_SHA256 || it_len != IMAGE_SHA_LEN {
                data_off += 4 + it_len;
                continue;
            }
            if hash.is_some() {
                return Err(err::TLV_MULTIPLE_HASHES_FOUND);
            }
            data_off += 4;
            if data_off + IMAGE_SHA_LEN > data_end {
                return Err(err::TLV_INVALID_SIZE);
            }
            hash = Some(
                self.img_read(slot, data_off, IMAGE_SHA_LEN)?
                    .try_into()
                    .unwrap(),
            );
            data_off += IMAGE_SHA_LEN;
        }

        Ok(ImageInfo {
            version,
            hash: hash.ok_or(err::HASH_NOT_FOUND)?,
            flags,
        })
    }

    /// `img_mgmt_find_by_hash`
    fn img_find_by_hash(&self, hash: &[u8]) -> Option<usize> {
        (0..self.img_slot_count())
            .find(|slot| matches!(self.img_read_info(*slot), Ok(info) if info.hash == hash))
    }

    /// `boot_swap_type_multi`, with the swap tables of the swap based modes
    pub fn img_swap_type(&self, image: usize) -> SwapType {
        let primary = self.img.slots[image * 2].trailer;
        let secondary = self.img.slots[image * 2 + 1].trailer;

        if secondary.magic == Magic::Good {
            if secondary.image_ok {
                SwapType::Perm
            } else {
                SwapType::Test
            }
        } else if primary.magic == Magic::Good && !primary.image_ok && primary.copy_done {
            SwapType::Revert
        } else {
            SwapType::None
        }
    }

    /// `img_mgmt_get_next_boot_slot`
    fn img_next_boot_slot(&self, image: usize) -> (usize, NextBootType) {
        let active_slot = self.img_active_slot(image);
        let opposite = active_slot ^ 1;
        match self.img_swap_type(image) {
            SwapType::None => (active_slot, NextBootType::Normal),
            SwapType::Perm => (opposite, NextBootType::Normal),
            SwapType::Revert => (opposite, NextBootType::Revert),
            SwapType::Test => (opposite, NextBootType::Test),
        }
    }

    /// `img_mgmt_slot_in_use`
    fn img_slot_in_use(&self, slot: usize) -> bool {
        let image = slot >> 1;
        if image >= self.config.img_updatable_image_number {
            // mcuboot_swap_type_multi() panics for unknown images, which makes
            // img_mgmt_get_next_boot_slot() report -1 / NEXT_BOOT_TYPE_NORMAL
            return true;
        }
        let active_slot = self.img_active_slot(image);
        let (nbs, ty) = self.img_next_boot_slot(image);

        if slot == nbs && ty == NextBootType::Revert {
            return true;
        }
        if ((slot == nbs && ty == NextBootType::Test)
            || (active_slot != nbs && ty == NextBootType::Normal))
            && !self.config.img_allow_erase_pending
        {
            return true;
        }
        active_slot == slot
    }

    /// `img_mgmt_get_unused_slot_area_id`
    fn img_unused_slot(&self, image: u64) -> Option<usize> {
        if self.config.img_updatable_image_number == 1 {
            (0..2).find(|slot| !self.img_slot_in_use(*slot))
        } else {
            let image = usize::try_from(image).ok()?;
            let slot = self.img_active_slot(image) ^ 1;
            (!self.img_slot_in_use(slot)).then_some(slot)
        }
    }

    /// `img_mgmt_vercmp`
    fn img_vercmp(&self, a: (u8, u8, u16, u32), b: (u8, u8, u16, u32)) -> std::cmp::Ordering {
        let ord = (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2));
        if self.config.img_version_cmp_use_build_number {
            ord.then(a.3.cmp(&b.3))
        } else {
            ord
        }
    }

    /// `img_mgmt_erase_slot`
    fn img_erase_slot(&mut self, slot: usize) -> Result<(), u16> {
        let size = self.config.img_slot_size;
        let slot = self.img.slots.get_mut(slot).ok_or(err::INVALID_SLOT)?;
        *slot = Slot::erased(size);
        Ok(())
    }

    /// `img_mgmt_erase_image_data`: erases the pages covering the image and
    /// the image trailer
    fn img_erase_image_data(&mut self, slot: usize, size: usize) {
        let page = self.config.img_flash_page_size;
        let erase_size = size.div_ceil(page) * page;
        let slot = &mut self.img.slots[slot];
        let erase_size = erase_size.min(slot.flash.len());
        slot.flash[..erase_size].fill(0xff);
        slot.trailer = Trailer::default();
    }

    /// `flash_img_check`: compares the SHA256 of the first `size` bytes of
    /// a slot against the expected hash
    fn img_flash_check(&self, slot: usize, size: u64, expected: &[u8; 32]) -> bool {
        let Some(data) = self.img.slots[slot].flash.get(..size as usize) else {
            return false;
        };
        Sha256::digest(data).as_slice() == expected
    }

    /// `boot_set_next` of MCUboot's `bootutil_public.c` (swap modes)
    fn boot_set_next(&mut self, slot: usize, active: bool, confirm: bool) -> Result<(), u16> {
        let confirm = confirm || active;
        let trailer = &mut self.img.slots[slot].trailer;
        match trailer.magic {
            Magic::Good => {
                if active && !trailer.image_ok {
                    trailer.image_ok = true;
                }
                Ok(())
            }
            Magic::Unset => {
                if !active {
                    trailer.magic = Magic::Good;
                    if confirm {
                        trailer.image_ok = true;
                    }
                }
                Ok(())
            }
            // BOOT_EBADVECT / BOOT_EBADIMAGE, translated by
            // img_mgmt_set_next_boot_slot_common()
            Magic::Bad if active => Err(err::INVALID_IMAGE_VECTOR_TABLE),
            Magic::Bad => Err(err::INVALID_IMAGE_HEADER_MAGIC),
        }
    }

    /// `img_mgmt_set_next_boot_slot`
    fn img_set_next_boot_slot(&mut self, slot: usize, confirm: bool) -> Result<(), u16> {
        let config = &self.config;
        let image = slot >> 1;
        let active_slot = self.img_active_slot(image);
        let (next_boot_slot, ty) = self.img_next_boot_slot(image);

        if !config.img_allow_confirm_non_active_image_any
            && confirm
            && image != self.img_active_image()
            && (!config.img_allow_confirm_non_active_image_secondary || slot == active_slot)
        {
            return Err(err::IMAGE_CONFIRMATION_DENIED);
        }

        if !config.img_allow_confirm_non_active_slot && confirm && slot != active_slot {
            return Err(err::IMAGE_CONFIRMATION_DENIED);
        }

        if !confirm && slot == active_slot {
            return Err(err::IMAGE_SETTING_TEST_TO_ACTIVE_DENIED);
        }

        match ty {
            NextBootType::Test => {
                if !confirm && slot == next_boot_slot {
                    return Ok(());
                }
                return Err(err::IMAGE_ALREADY_PENDING);
            }
            NextBootType::Normal => {
                if confirm && slot == next_boot_slot {
                    return Ok(());
                }
                if (slot == active_slot && active_slot != next_boot_slot)
                    || (!confirm && slot != active_slot && slot == next_boot_slot)
                {
                    return Err(err::IMAGE_ALREADY_PENDING);
                }
            }
            NextBootType::Revert => {
                if confirm && slot == next_boot_slot {
                    return Ok(());
                }
                if !confirm {
                    return Err(err::IMAGE_ALREADY_PENDING);
                }
            }
        }

        self.boot_set_next(slot, slot == active_slot, confirm)
    }

    /// `bootutil_img_validate`, reduced to the SHA256 check: the hash TLV
    /// covers the header, the image and the protected TLV area.
    fn mcuboot_validate(&self, slot: usize) -> bool {
        let Ok(info) = self.img_read_info(slot) else {
            return false;
        };
        let Ok(hdr) = self.img_read(slot, 0, IMAGE_HEADER_SIZE) else {
            return false;
        };
        let hashed_len =
            le16(&hdr[8..]) as usize + le32(&hdr[12..]) as usize + le16(&hdr[10..]) as usize;
        let Ok(hashed) = self.img_read(slot, 0, hashed_len) else {
            return false;
        };
        Sha256::digest(hashed).as_slice() == info.hash
    }

    /// What MCUboot does with an image pair when the device boots, for the
    /// swap based modes.
    pub fn mcuboot_boot(&mut self) {
        for image in 0..self.config.img_updatable_image_number {
            let (primary, secondary) = (image * 2, image * 2 + 1);
            match self.img_swap_type(image) {
                SwapType::None => {}
                SwapType::Test | SwapType::Perm if !self.mcuboot_validate(secondary) => {
                    // "Image in the secondary slot is not valid!"
                    self.img.slots[secondary] = Slot::erased(self.config.img_slot_size);
                }
                swap_type @ (SwapType::Test | SwapType::Perm) => {
                    self.img.slots.swap(primary, secondary);
                    self.img.slots[primary].trailer = Trailer {
                        magic: Magic::Good,
                        image_ok: swap_type == SwapType::Perm,
                        copy_done: true,
                    };
                    self.img.slots[secondary].trailer = Trailer::default();
                }
                SwapType::Revert => {
                    self.img.slots.swap(primary, secondary);
                    self.img.slots[primary].trailer = Trailer {
                        magic: Magic::Good,
                        image_ok: true,
                        copy_done: true,
                    };
                    self.img.slots[secondary].trailer = Trailer::default();
                }
            }
        }
    }

    /// `img_mgmt_state_encode_slot`
    fn img_encode_slot(&self, slot: usize, flags: u8) -> Option<Value> {
        const REPORT_SLOT_ACTIVE: u8 = 1 << 0;
        const REPORT_SLOT_PENDING: u8 = 1 << 1;
        const REPORT_SLOT_CONFIRMED: u8 = 1 << 2;
        const REPORT_SLOT_PERMANENT: u8 = 1 << 3;

        let info = self.img_read_info(slot).ok()?;

        let mut entries = vec![];
        if self.config.img_updatable_image_number > 1 {
            entries.push(("image", uint((slot >> 1) as u64)));
        }
        entries.push(("slot", uint((slot % 2) as u64)));
        entries.push(("version", Value::Text(ver_str(info.version))));
        entries.push(("hash", Value::Bytes(info.hash.to_vec())));

        let flag_entries = [
            ("bootable", info.flags & IMAGE_F_NON_BOOTABLE == 0),
            ("pending", flags & REPORT_SLOT_PENDING != 0),
            ("confirmed", flags & REPORT_SLOT_CONFIRMED != 0),
            ("active", flags & REPORT_SLOT_ACTIVE != 0),
            ("permanent", flags & REPORT_SLOT_PERMANENT != 0),
        ];
        for (label, value) in flag_entries {
            // In "frugal" lists flags are only added when they are true
            if value || !self.config.img_frugal_list {
                entries.push((label, Value::Bool(value)));
            }
        }

        Some(map(entries))
    }

    /// The body of `img_mgmt_state_read`
    fn img_encode_state(&self, ctx: &mut Ctx) {
        const REPORT_SLOT_ACTIVE: u8 = 1 << 0;
        const REPORT_SLOT_PENDING: u8 = 1 << 1;
        const REPORT_SLOT_CONFIRMED: u8 = 1 << 2;
        const REPORT_SLOT_PERMANENT: u8 = 1 << 3;

        let mut images = vec![];
        for image in 0..self.config.img_updatable_image_number {
            let (next_boot_slot, ty) = self.img_next_boot_slot(image);
            let slot_a = self.img_active_slot(image);
            let slot_o = slot_a ^ 1;
            let mut flags_a = REPORT_SLOT_ACTIVE;
            let mut flags_o = 0;

            if ty != NextBootType::Revert {
                flags_a |= REPORT_SLOT_CONFIRMED;
            }

            if next_boot_slot != slot_a {
                flags_o = match ty {
                    NextBootType::Normal => REPORT_SLOT_PENDING | REPORT_SLOT_PERMANENT,
                    NextBootType::Revert => REPORT_SLOT_CONFIRMED,
                    NextBootType::Test => REPORT_SLOT_PENDING,
                };
            }

            images.extend(self.img_encode_slot(slot_a, flags_a));
            images.extend(self.img_encode_slot(slot_o, flags_o));
        }

        ctx.put("images", Value::Array(images));
        if !self.config.img_frugal_list {
            ctx.put("splitStatus", int(0));
        }
    }
}

/// `img_mgmt_state_read`
fn state_read(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    device.img_encode_state(ctx);
    Ok(())
}

/// `img_mgmt_state_write`
fn state_write(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("hash", Kind::Bstr), ("confirm", Kind::Bool)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let confirm = decoded.bool("confirm").unwrap_or(false);
    let hash = decoded.bytes("hash").unwrap_or_default();

    let slot = if hash.is_empty() {
        if confirm {
            device.img_active_slot(device.img_active_image())
        } else {
            // A 'test' without a hash is invalid.
            ctx.add_cmd_err(group_id::IMAGE, err::INVALID_HASH);
            return Ok(());
        }
    } else if hash.len() != IMAGE_SHA_LEN {
        ctx.add_cmd_err(group_id::IMAGE, err::INVALID_HASH);
        return Ok(());
    } else {
        match device.img_find_by_hash(hash) {
            Some(slot) => slot,
            None => {
                ctx.add_cmd_err(group_id::IMAGE, err::HASH_NOT_FOUND);
                return Ok(());
            }
        }
    };

    if let Err(rc) = device.img_set_next_boot_slot(slot, confirm) {
        ctx.add_cmd_err(group_id::IMAGE, rc);
        return Ok(());
    }

    // Send the current image state in the response.
    device.img_encode_state(ctx);
    Ok(())
}

/// `img_mgmt_erase`
fn erase(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("slot", Kind::U32)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let default_slot = device.img_active_slot(device.img_active_image()) ^ 1;
    let slot = decoded
        .uint("slot")
        .map_or(default_slot, |slot| slot as usize);

    if device.img_read_info(slot).is_ok() && device.img_slot_in_use(slot) {
        ctx.add_cmd_err(group_id::IMAGE, err::NO_FREE_SLOT);
        return Ok(());
    }

    let rc = device.img_erase_slot(slot);
    device.img.reset_upload();

    if let Err(rc) = rc {
        ctx.add_cmd_err(group_id::IMAGE, rc);
        return Ok(());
    }

    if device.config.smp_legacy_rc_behaviour {
        ctx.put("rc", int(0));
    }
    Ok(())
}

/// `img_mgmt_slot_info`
fn slot_info(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let mut images = vec![];
    for image in 0..device.config.img_updatable_image_number {
        let mut slots = vec![];
        for i in [image * 2, image * 2 + 1] {
            let mut entry = vec![
                ("slot", uint((i % 2) as u64)),
                ("size", uint(device.img.slots[i].flash.len() as u64)),
            ];
            if device.img_active_slot(image) != i {
                entry.push(("upload_image_id", uint(image as u64)));
            }
            slots.push(map(entry));
        }

        let mut entry = vec![
            ("image", uint(image as u64)),
            ("slots", Value::Array(slots)),
        ];
        if let Some(footer) = device.config.img_too_large_sysbuild_footer {
            // img_mgmt_slot_max_size(), CONFIG_MCUMGR_GRP_IMG_TOO_LARGE_SYSBUILD
            let sizes = [
                device.img.slots[image * 2].flash.len() as u64,
                device.img.slots[image * 2 + 1].flash.len() as u64,
            ];
            if sizes[0] > 0 && sizes[1] > 0 && footer >= sizes[0].abs_diff(sizes[1]) {
                entry.push(("max_image_size", uint(sizes[0] - footer)));
            }
        }
        images.push(map(entry));
    }
    ctx.put("images", Value::Array(images));
    Ok(())
}

/// `img_mgmt_upload`, including `img_mgmt_upload_inspect`
fn upload(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[
            ("image", Kind::U32),
            ("data", Kind::Bstr),
            ("len", Kind::Size),
            ("off", Kind::Size),
            ("sha", Kind::Bstr),
            ("upgrade", Kind::Bool),
        ])
        .map_err(|_| mgmt_err::EINVAL)?;
    let req_image = decoded.uint("image").unwrap_or(0);
    let req_data = decoded.bytes("data").unwrap_or_default().to_vec();
    let req_size = decoded.uint("len");
    let req_off = decoded.uint("off");
    let req_sha = decoded.bytes("sha").unwrap_or_default().to_vec();
    let req_upgrade = decoded.bool("upgrade").unwrap_or(false);

    struct Action {
        area: usize,
        size: u64,
        erase: bool,
    }

    // img_mgmt_upload_inspect(); Ok(None) means "do not proceed"
    let inspect = |device: &Device| -> Result<Option<Action>, u16> {
        let off = req_off.ok_or(err::INVALID_OFFSET)?;
        let state = &device.img.upload;

        if off == 0 {
            if req_data.len() < IMAGE_HEADER_SIZE {
                return Err(err::INVALID_IMAGE_HEADER);
            }
            let size = req_size.ok_or(err::INVALID_LENGTH)?;
            if le32(&req_data) != IMAGE_MAGIC {
                return Err(err::INVALID_IMAGE_HEADER_MAGIC);
            }
            if req_sha.len() > IMG_MGMT_DATA_SHA_LEN {
                return Err(err::INVALID_HASH);
            }

            // Resume an interrupted upload of the same data
            if !req_sha.is_empty()
                && state.area.is_some()
                && state.data_sha_len == req_sha.len()
                && state.data_sha[..req_sha.len()] == req_sha[..]
            {
                return Ok(None);
            }

            let area = device.img_unused_slot(req_image).ok_or(err::NO_FREE_SLOT)?;
            if size > device.img.slots[area].flash.len() as u64 {
                return Err(err::INVALID_IMAGE_TOO_LARGE);
            }

            if req_upgrade {
                let active_slot = device.img_active_slot(device.img_active_image());
                let current = device
                    .img_read_info(active_slot)
                    .map_err(|_| err::VERSION_GET_FAILED)?;
                let new = (
                    req_data[20],
                    req_data[21],
                    le16(&req_data[22..]),
                    le32(&req_data[24..]),
                );
                if device.img_vercmp(current.version, new).is_ge() {
                    return Err(err::CURRENT_VERSION_IS_NEWER);
                }
            }

            Ok(Some(Action {
                area,
                size,
                erase: !device.img.slots[area].is_empty(),
            }))
        } else {
            if off != state.off {
                // Respond with the offset we are expecting data for
                return Ok(None);
            }
            if off + req_data.len() as u64 > state.size {
                return Err(err::INVALID_IMAGE_DATA_OVERRUN);
            }
            Ok(Some(Action {
                area: state.area.ok_or(err::FLASH_CONTEXT_NOT_SET)?,
                size: state.size,
                erase: false,
            }))
        }
    };

    let good_rsp = |device: &Device, ctx: &mut Ctx| {
        if device.config.smp_legacy_rc_behaviour {
            ctx.put("rc", int(0));
        }
        ctx.put("off", uint(device.img.upload.off));
    };

    let action = match inspect(device) {
        Err(rc) => {
            ctx.add_cmd_err(group_id::IMAGE, rc);
            device.img.reset_upload();
            return Ok(());
        }
        Ok(None) => {
            good_rsp(device, ctx);
            return Ok(());
        }
        Ok(Some(action)) => action,
    };

    device.img.upload.area = Some(action.area);
    device.img.upload.size = action.size;

    let mut last = false;
    let mut reset = false;
    let mut data_match = false;

    let off = req_off.unwrap_or_default();
    let mut done = false;
    if off == 0 {
        let state = &mut device.img.upload;
        state.off = 0;
        state.data_sha_len = req_sha.len();
        state.data_sha = [0; IMG_MGMT_DATA_SHA_LEN];
        state.data_sha[..req_sha.len()].copy_from_slice(&req_sha);

        // CONFIG_IMG_ENABLE_IMAGE_CHECK: the slot might hold this data already
        if device.config.img_enable_image_check
            && device.img.upload.data_sha_len == IMG_MGMT_DATA_SHA_LEN
            && device.img_flash_check(action.area, action.size, &device.img.upload.data_sha)
        {
            device.img.upload.off = action.size;
            reset = true;
            last = true;
            data_match = true;
            done = true;
        } else if action.erase {
            device.img_erase_image_data(action.area, action.size as usize);
        }
    }

    if !done && !req_data.is_empty() {
        let state = &device.img.upload;
        if state.off + req_data.len() as u64 == state.size {
            last = true;
        }

        // img_mgmt_write_image_data()
        let flash = &mut device.img.slots[action.area].flash;
        let start = off as usize;
        flash[start..start + req_data.len()].copy_from_slice(&req_data);
        if device.img.corrupt_flash_writes {
            flash[start] ^= 0x01;
        }
        device.img.upload.off += req_data.len() as u64;

        if device.img.upload.off == device.img.upload.size {
            reset = true;
            if device.config.img_enable_image_check {
                data_match = device.img_flash_check(
                    action.area,
                    device.img.upload.size,
                    &device.img.upload.data_sha,
                );
            }
        }
    }

    good_rsp(device, ctx);
    if device.config.img_enable_image_check && last {
        ctx.put("match", Value::Bool(data_match));
    }
    if reset {
        device.img.reset_upload();
    }
    Ok(())
}
