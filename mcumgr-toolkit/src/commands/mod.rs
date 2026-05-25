/// [Enumeration management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_10.html) group commands
pub mod r#enum;
/// [File management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html) group commands
pub mod fs;
/// [Application/software image management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_1.html) group commands
pub mod image;
/// [Default/OS management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html) group commands
pub mod os;
/// [Settings (config) management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_3.html) group commands
pub mod settings;
/// [Shell management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_9.html) group commands
pub mod shell;
/// [Statistics management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_2.html) group commands
pub mod stats;
/// [Zephyr management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_63.html) group commands
pub mod zephyr;

mod macros;
use macros::impl_mcumgr_command;

use serde::{Deserialize, Serialize};

/// SMP version 2 group based error message
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ErrResponseV2 {
    /// group of the group-based error code
    pub group: u16,
    /// contains the index of the group-based error code
    pub rc: i32,
}

/// [SMP error message](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html#minimal-response-smp-data)
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ErrResponse {
    /// SMP version 1 error code
    pub rc: Option<i32>,
    /// SMP version 1 error string
    pub rsn: Option<String>,
    /// SMP version 2 error message
    pub err: Option<ErrResponseV2>,
}

/// An MCUmgr command that can be executed through [`Connection::execute_command`](crate::connection::Connection::execute_command).
pub trait McuMgrCommand {
    /// the data payload type
    type Payload: Serialize;
    /// the response type of the command
    type Response: for<'a> Deserialize<'a>;
    /// whether this command is a read or write operation
    fn is_write_operation(&self) -> bool;
    /// the group ID of the command
    fn group_id(&self) -> u16;
    /// the command ID
    fn command_id(&self) -> u8;
    /// the data
    fn data(&self) -> &Self::Payload;
}

/// Checks if a value is the default value
fn is_default<T: Default + PartialEq>(val: &T) -> bool {
    val == &T::default()
}

fn data_too_large_error() -> std::io::Error {
    std::io::Error::other(Box::<dyn std::error::Error + Send + Sync>::from(
        "Serialized data too large".to_string(),
    ))
}

// A writer that simply counts the number of bytes written to it
struct CountingWriter {
    bytes_written: usize,
}
impl CountingWriter {
    fn new() -> Self {
        Self { bytes_written: 0 }
    }
}
impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = buf.len();
        self.bytes_written = self
            .bytes_written
            .checked_add(len)
            .ok_or_else(data_too_large_error)?;
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 0): os::Echo<'_> => os::EchoResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 2): os::TaskStatistics => os::TaskStatisticsResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 3): os::MemoryPoolStatistics => os::MemoryPoolStatisticsResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 4): os::DateTimeGet => os::DateTimeGetResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_OS, 4): os::DateTimeSet => os::DateTimeSetResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_OS, 5): os::SystemReset => os::SystemResetResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 6): os::MCUmgrParameters => os::MCUmgrParametersResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 7): os::ApplicationInfo<'_> => os::ApplicationInfoResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 8): os::BootloaderInfo => os::BootloaderInfoResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 8): os::BootloaderInfoMcubootMode => os::BootloaderInfoMcubootModeResponse);

impl_mcumgr_command!((read,  MGMT_GROUP_ID_IMAGE, 0): image::GetImageState => image::ImageStateResponse);
impl_mcumgr_command!((write,  MGMT_GROUP_ID_IMAGE, 0): image::SetImageState<'_> => image::ImageStateResponse);
impl_mcumgr_command!((write,  MGMT_GROUP_ID_IMAGE, 1): image::ImageUpload<'_, '_> => image::ImageUploadResponse);
impl_mcumgr_command!((write,  MGMT_GROUP_ID_IMAGE, 5): image::ImageErase => image::ImageEraseResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_IMAGE, 6): image::SlotInfo => image::SlotInfoResponse);

impl_mcumgr_command!((read, MGMT_GROUP_ID_STAT, 0): stats::GroupData<'_> => stats::GroupDataResponse);
impl_mcumgr_command!((read, MGMT_GROUP_ID_STAT, 1): stats::ListGroups => stats::ListGroupsResponse);

impl_mcumgr_command!((read, MGMT_GROUP_ID_SETTINGS, 0): settings::ReadSetting<'_> => settings::ReadSettingResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_SETTINGS, 0): settings::WriteSetting<'_, '_> => settings::WriteSettingResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_SETTINGS, 1): settings::DeleteSetting<'_> => settings::DeleteSettingResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_SETTINGS, 2): settings::CommitSettings => settings::CommitSettingsResponse);
impl_mcumgr_command!((read, MGMT_GROUP_ID_SETTINGS, 3): settings::LoadSettings => settings::LoadSettingsResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_SETTINGS, 3): settings::SaveSettings<'_> => settings::SaveSettingsResponse);

impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 0): fs::FileUpload<'_, '_> => fs::FileUploadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 0): fs::FileDownload<'_> => fs::FileDownloadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 1): fs::FileStatus<'_> => fs::FileStatusResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 2): fs::FileChecksum<'_, '_> => fs::FileChecksumResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 3): fs::SupportedFileChecksumTypes => fs::SupportedFileChecksumTypesResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 4): fs::FileClose => fs::FileCloseResponse);

impl_mcumgr_command!((write, MGMT_GROUP_ID_SHELL, 0): shell::ShellCommandLineExecute<'_> => shell::ShellCommandLineExecuteResponse);

impl_mcumgr_command!((read, MGMT_GROUP_ID_ENUM, 0): r#enum::GroupCount => r#enum::GroupCountResponse);
impl_mcumgr_command!((read, MGMT_GROUP_ID_ENUM, 1): r#enum::ListGroups => r#enum::ListGroupsResponse);
impl_mcumgr_command!((read, MGMT_GROUP_ID_ENUM, 2): r#enum::GroupId => r#enum::GroupIdResponse);
impl_mcumgr_command!((read, MGMT_GROUP_ID_ENUM, 3): r#enum::GroupDetails<'_> => r#enum::GroupDetailsResponse);

impl_mcumgr_command!((write, ZEPHYR_MGMT_GRP_BASIC, 0): zephyr::EraseStorage => zephyr::EraseStorageResponse);

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::cbor;

    #[test]
    fn decode_error_none() {
        let mut cbor_data = vec![];
        ciborium::into_writer(
            &cbor!({
                "foo" => 42,
            })
            .unwrap(),
            &mut cbor_data,
        )
        .unwrap();
        let err: ErrResponse = ciborium::from_reader(cbor_data.as_slice()).unwrap();
        assert_eq!(
            err,
            ErrResponse {
                rc: None,
                rsn: None,
                err: None,
            }
        );
    }

    #[test]
    fn decode_error_v1() {
        let mut cbor_data = vec![];
        ciborium::into_writer(
            &cbor!({
                "rc" => 10,
            })
            .unwrap(),
            &mut cbor_data,
        )
        .unwrap();
        let err: ErrResponse = ciborium::from_reader(cbor_data.as_slice()).unwrap();
        assert_eq!(
            err,
            ErrResponse {
                rc: Some(10),
                rsn: None,
                err: None,
            }
        );
    }

    #[test]
    fn decode_error_v1_with_msg() {
        let mut cbor_data = vec![];
        ciborium::into_writer(
            &cbor!({
                "rc" => 1,
                "rsn" => "Test Reason!",
            })
            .unwrap(),
            &mut cbor_data,
        )
        .unwrap();
        let err: ErrResponse = ciborium::from_reader(cbor_data.as_slice()).unwrap();
        assert_eq!(
            err,
            ErrResponse {
                rc: Some(1),
                rsn: Some("Test Reason!".to_string()),
                err: None,
            }
        );
    }

    #[test]
    fn decode_error_v2() {
        let mut cbor_data = vec![];
        ciborium::into_writer(
            &cbor!({
                "err" => {
                    "group" => 4,
                    "rc" => 20,
                }
            })
            .unwrap(),
            &mut cbor_data,
        )
        .unwrap();
        let err: ErrResponse = ciborium::from_reader(cbor_data.as_slice()).unwrap();
        assert_eq!(
            err,
            ErrResponse {
                rc: None,
                rsn: None,
                err: Some(ErrResponseV2 { group: 4, rc: 20 })
            }
        );
    }

    #[test]
    fn is_default() {
        assert!(super::is_default(&0));
        assert!(!super::is_default(&5));
    }
}
