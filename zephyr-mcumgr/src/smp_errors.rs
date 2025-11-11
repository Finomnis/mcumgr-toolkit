use strum_macros::{Display, FromRepr};

use crate::MCUmgrGroup;

/// Errors the device can respond with when trying to execute an SMP command.
///
/// More information can be found [here](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html#minimal-response-smp-data).
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DeviceError {
    /// MCUmgr SMP v1 error codes
    V1 {
        /// Error code
        rc: i32,
    },
    /// MCUmgr SMP v2 error codes
    V2 {
        /// Group id
        group: u16,
        /// Group based error code
        rc: i32,
    },
}

fn v2_err_to_string(group: u16, rc: i32) -> Option<String> {
    match MCUmgrGroup::from_repr(group)? {
        MCUmgrGroup::MGMT_GROUP_ID_ENUM => EnumMgmtErrCode::from_repr(rc).map(|x| x.to_string()),
        MCUmgrGroup::MGMT_GROUP_ID_FS => FsMgmtErrCode::from_repr(rc).map(|x| x.to_string()),
        MCUmgrGroup::MGMT_GROUP_ID_IMAGE => ImgMgmtErrCode::from_repr(rc).map(|x| x.to_string()),
        MCUmgrGroup::MGMT_GROUP_ID_OS => OsMgmtErrCode::from_repr(rc).map(|x| x.to_string()),
        MCUmgrGroup::MGMT_GROUP_ID_SETTINGS => {
            SettingsMgmtRetCode::from_repr(rc).map(|x| x.to_string())
        }
        MCUmgrGroup::MGMT_GROUP_ID_SHELL => ShellMgmtErrCode::from_repr(rc).map(|x| x.to_string()),
        MCUmgrGroup::MGMT_GROUP_ID_STAT => StatMgmtErrCode::from_repr(rc).map(|x| x.to_string()),
        MCUmgrGroup::ZEPHYR_MGMT_GRP_BASIC => {
            ZephyrBasicGroupErrCode::from_repr(rc).map(|x| x.to_string())
        }
        _ => None,
    }
}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceError::V1 { rc } => {
                write!(f, "{}", MCUmgrErr::err_to_string(*rc))
            }
            DeviceError::V2 { group, rc } => match v2_err_to_string(*group, *rc) {
                Some(msg) => f.write_str(&msg),
                None => write!(f, "group={group},rc={rc}"),
            },
        }
    }
}

/// See [`enum mcumgr_err_t`](https://docs.zephyrproject.org/latest/doxygen/html/mgmt__defines_8h.html).
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum MCUmgrErr {
    /** No error (success). */
    MGMT_ERR_EOK = 0,

    /** Unknown error. */
    MGMT_ERR_EUNKNOWN,

    /** Insufficient memory (likely not enough space for CBOR object). */
    MGMT_ERR_ENOMEM,

    /** Error in input value. */
    MGMT_ERR_EINVAL,

    /** Operation timed out. */
    MGMT_ERR_ETIMEOUT,

    /** No such file/entry. */
    MGMT_ERR_ENOENT,

    /** Current state disallows command. */
    MGMT_ERR_EBADSTATE,

    /** Response too large. */
    MGMT_ERR_EMSGSIZE,

    /** Command not supported. */
    MGMT_ERR_ENOTSUP,

    /** Corrupt */
    MGMT_ERR_ECORRUPT,

    /** Command blocked by processing of other command */
    MGMT_ERR_EBUSY,

    /** Access to specific function, command or resource denied */
    MGMT_ERR_EACCESSDENIED,

    /** Requested SMP MCUmgr protocol version is not supported (too old) */
    MGMT_ERR_UNSUPPORTED_TOO_OLD,

    /** Requested SMP MCUmgr protocol version is not supported (too new) */
    MGMT_ERR_UNSUPPORTED_TOO_NEW,

    /** User errors defined from 256 onwards */
    MGMT_ERR_EPERUSER = 256,
}
impl MCUmgrErr {
    /// Converts a raw error code to a string
    pub fn err_to_string(err: i32) -> String {
        const PERUSER: MCUmgrErr = MCUmgrErr::MGMT_ERR_EPERUSER;
        if err < PERUSER as i32 {
            if let Some(err_enum) = Self::from_repr(err) {
                format!("{err_enum}")
            } else {
                format!("MGMT_ERR_UNKNOWN({err})")
            }
        } else {
            format!("{PERUSER}({err})")
        }
    }
}

/// See `enum settings_mgmt_ret_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
enum SettingsMgmtRetCode {
    /** No error, this is implied if there is no ret value in the response. */
    SETTINGS_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    SETTINGS_MGMT_ERR_UNKNOWN,

    /** The provided key name is too long to be used. */
    SETTINGS_MGMT_ERR_KEY_TOO_LONG,

    /** The provided key name does not exist. */
    SETTINGS_MGMT_ERR_KEY_NOT_FOUND,

    /** The provided key name does not support being read. */
    SETTINGS_MGMT_ERR_READ_NOT_SUPPORTED,

    /** The provided root key name does not exist. */
    SETTINGS_MGMT_ERR_ROOT_KEY_NOT_FOUND,

    /** The provided key name does not support being written. */
    SETTINGS_MGMT_ERR_WRITE_NOT_SUPPORTED,

    /** The provided key name does not support being deleted. */
    SETTINGS_MGMT_ERR_DELETE_NOT_SUPPORTED,
}

/// See `enum fs_mgmt_err_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum FsMgmtErrCode {
    /** No error (success). */
    FS_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    FS_MGMT_ERR_UNKNOWN,

    /** The specified file name is not valid. */
    FS_MGMT_ERR_FILE_INVALID_NAME,

    /** The specified file does not exist. */
    FS_MGMT_ERR_FILE_NOT_FOUND,

    /** The specified file is a directory, not a file. */
    FS_MGMT_ERR_FILE_IS_DIRECTORY,

    /** Error occurred whilst attempting to open a file. */
    FS_MGMT_ERR_FILE_OPEN_FAILED,

    /** Error occurred whilst attempting to seek to an offset in a file. */
    FS_MGMT_ERR_FILE_SEEK_FAILED,

    /** Error occurred whilst attempting to read data from a file. */
    FS_MGMT_ERR_FILE_READ_FAILED,

    /** Error occurred whilst trying to truncate file. */
    FS_MGMT_ERR_FILE_TRUNCATE_FAILED,

    /** Error occurred whilst trying to delete file. */
    FS_MGMT_ERR_FILE_DELETE_FAILED,

    /** Error occurred whilst attempting to write data to a file. */
    FS_MGMT_ERR_FILE_WRITE_FAILED,

    /**
     * The specified data offset is not valid, this could indicate that the file on the device
     * has changed since the previous command. The length of the current file on the device is
     * returned as "len", the user application needs to decide how to handle this (e.g. the
     * hash of the file could be requested and compared with the hash of the length of the
     * file being uploaded to see if they match or not).
     */
    FS_MGMT_ERR_FILE_OFFSET_NOT_VALID,

    /** The requested offset is larger than the size of the file on the device. */
    FS_MGMT_ERR_FILE_OFFSET_LARGER_THAN_FILE,

    /** The requested checksum or hash type was not found or is not supported by this build. */
    FS_MGMT_ERR_CHECKSUM_HASH_NOT_FOUND,

    /** The specified mount point was not found or is not mounted. */
    FS_MGMT_ERR_MOUNT_POINT_NOT_FOUND,

    /** The specified mount point is that of a read-only filesystem. */
    FS_MGMT_ERR_READ_ONLY_FILESYSTEM,

    /** The operation cannot be performed because the file is empty with no contents. */
    FS_MGMT_ERR_FILE_EMPTY,
}

/// See `enum img_mgmt_err_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum ImgMgmtErrCode {
    /** No error, this is implied if there is no ret value in the response */
    IMG_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    IMG_MGMT_ERR_UNKNOWN,

    /** Failed to query flash area configuration. */
    IMG_MGMT_ERR_FLASH_CONFIG_QUERY_FAIL,

    /** There is no image in the slot. */
    IMG_MGMT_ERR_NO_IMAGE,

    /** The image in the slot has no TLVs (tag, length, value). */
    IMG_MGMT_ERR_NO_TLVS,

    /** The image in the slot has an invalid TLV type and/or length. */
    IMG_MGMT_ERR_INVALID_TLV,

    /** The image in the slot has multiple hash TLVs, which is invalid. */
    IMG_MGMT_ERR_TLV_MULTIPLE_HASHES_FOUND,

    /** The image in the slot has an invalid TLV size. */
    IMG_MGMT_ERR_TLV_INVALID_SIZE,

    /** The image in the slot does not have a hash TLV, which is required.  */
    IMG_MGMT_ERR_HASH_NOT_FOUND,

    /** There is no free slot to place the image. */
    IMG_MGMT_ERR_NO_FREE_SLOT,

    /** Flash area opening failed. */
    IMG_MGMT_ERR_FLASH_OPEN_FAILED,

    /** Flash area reading failed. */
    IMG_MGMT_ERR_FLASH_READ_FAILED,

    /** Flash area writing failed. */
    IMG_MGMT_ERR_FLASH_WRITE_FAILED,

    /** Flash area erase failed. */
    IMG_MGMT_ERR_FLASH_ERASE_FAILED,

    /** The provided slot is not valid. */
    IMG_MGMT_ERR_INVALID_SLOT,

    /** Insufficient heap memory (malloc failed). */
    IMG_MGMT_ERR_NO_FREE_MEMORY,

    /** The flash context is already set. */
    IMG_MGMT_ERR_FLASH_CONTEXT_ALREADY_SET,

    /** The flash context is not set. */
    IMG_MGMT_ERR_FLASH_CONTEXT_NOT_SET,

    /** The device for the flash area is NULL. */
    IMG_MGMT_ERR_FLASH_AREA_DEVICE_NULL,

    /** The offset for a page number is invalid. */
    IMG_MGMT_ERR_INVALID_PAGE_OFFSET,

    /** The offset parameter was not provided and is required. */
    IMG_MGMT_ERR_INVALID_OFFSET,

    /** The length parameter was not provided and is required. */
    IMG_MGMT_ERR_INVALID_LENGTH,

    /** The image length is smaller than the size of an image header. */
    IMG_MGMT_ERR_INVALID_IMAGE_HEADER,

    /** The image header magic value does not match the expected value. */
    IMG_MGMT_ERR_INVALID_IMAGE_HEADER_MAGIC,

    /** The hash parameter provided is not valid. */
    IMG_MGMT_ERR_INVALID_HASH,

    /** The image load address does not match the address of the flash area. */
    IMG_MGMT_ERR_INVALID_FLASH_ADDRESS,

    /** Failed to get version of currently running application. */
    IMG_MGMT_ERR_VERSION_GET_FAILED,

    /** The currently running application is newer than the version being uploaded. */
    IMG_MGMT_ERR_CURRENT_VERSION_IS_NEWER,

    /** There is already an image operating pending. */
    IMG_MGMT_ERR_IMAGE_ALREADY_PENDING,

    /** The image vector table is invalid. */
    IMG_MGMT_ERR_INVALID_IMAGE_VECTOR_TABLE,

    /** The image it too large to fit. */
    IMG_MGMT_ERR_INVALID_IMAGE_TOO_LARGE,

    /** The amount of data sent is larger than the provided image size. */
    IMG_MGMT_ERR_INVALID_IMAGE_DATA_OVERRUN,

    /** Confirmation of image has been denied */
    IMG_MGMT_ERR_IMAGE_CONFIRMATION_DENIED,

    /** Setting test to active slot is not allowed */
    IMG_MGMT_ERR_IMAGE_SETTING_TEST_TO_ACTIVE_DENIED,

    /** Current active slot for image cannot be determined */
    IMG_MGMT_ERR_ACTIVE_SLOT_NOT_KNOWN,
}

/// See `enum os_mgmt_err_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum OsMgmtErrCode {
    /** No error, this is implied if there is no ret value in the response */
    OS_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    OS_MGMT_ERR_UNKNOWN,

    /** The provided format value is not valid. */
    OS_MGMT_ERR_INVALID_FORMAT,

    /** Query was not recognized. */
    OS_MGMT_ERR_QUERY_YIELDS_NO_ANSWER,

    /** RTC is not set */
    OS_MGMT_ERR_RTC_NOT_SET,

    /** RTC command failed */
    OS_MGMT_ERR_RTC_COMMAND_FAILED,

    /** Query was recognized but there is no valid value for the response. */
    OS_MGMT_ERR_QUERY_RESPONSE_VALUE_NOT_VALID,
}

/// See `enum shell_mgmt_err_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum ShellMgmtErrCode {
    /** No error, this is implied if there is no ret value in the response */
    SHELL_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    SHELL_MGMT_ERR_UNKNOWN,

    /** The provided command to execute is too long. */
    SHELL_MGMT_ERR_COMMAND_TOO_LONG,

    /** No command to execute was provided. */
    SHELL_MGMT_ERR_EMPTY_COMMAND,
}

/// See `enum stat_mgmt_err_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum StatMgmtErrCode {
    /** No error, this is implied if there is no ret value in the response */
    STAT_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    STAT_MGMT_ERR_UNKNOWN,

    /** The provided statistic group name was not found. */
    STAT_MGMT_ERR_INVALID_GROUP,

    /** The provided statistic name was not found. */
    STAT_MGMT_ERR_INVALID_STAT_NAME,

    /** The size of the statistic cannot be handled. */
    STAT_MGMT_ERR_INVALID_STAT_SIZE,

    /** Walk through of statistics was aborted. */
    STAT_MGMT_ERR_WALK_ABORTED,
}

/// See `enum enum_mgmt_err_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum EnumMgmtErrCode {
    /** No error, this is implied if there is no ret value in the response */
    ENUM_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    ENUM_MGMT_ERR_UNKNOWN,

    /** Too many entries were provided. */
    ENUM_MGMT_ERR_TOO_MANY_GROUP_ENTRIES,

    /** Insufficient heap memory to store entry data. */
    ENUM_MGMT_ERR_INSUFFICIENT_HEAP_FOR_ENTRIES,

    /** Provided index is larger than the number of supported grouped. */
    ENUM_MGMT_ERR_INDEX_TOO_LARGE,
}

/// See `enum zephyr_basic_group_err_code_t`.
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum ZephyrBasicGroupErrCode {
    /** No error, this is implied if there is no ret value in the response */
    ZEPHYRBASIC_MGMT_ERR_OK = 0,

    /** Unknown error occurred. */
    ZEPHYRBASIC_MGMT_ERR_UNKNOWN,

    /** Opening of the flash area has failed. */
    ZEPHYRBASIC_MGMT_ERR_FLASH_OPEN_FAILED,

    /** Querying the flash area parameters has failed. */
    ZEPHYRBASIC_MGMT_ERR_FLASH_CONFIG_QUERY_FAIL,

    /** Erasing the flash area has failed. */
    ZEPHYRBASIC_MGMT_ERR_FLASH_ERASE_FAILED,
}
