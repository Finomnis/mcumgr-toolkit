/// [File management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html) group commands
pub mod fs;
/// [Default/OS management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html) group commands
pub mod os;
/// [Shell management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_9.html) group commands
pub mod shell;
/// [Zephyr management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_63.html) group commands
pub mod zephyr;

mod macros;
use macros::impl_mcumgr_command;

use serde::{Deserialize, Serialize};

/// SMP version 2 group based error message
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ErrResponseV2 {
    /// group of the group-based error code
    pub group: u16,
    /// contains the index of the group-based error code
    pub rc: i32,
}

/// [SMP error message](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html#minimal-response-smp-data)
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ErrResponse {
    /// SMP version 1 error code
    pub rc: Option<i32>,
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

impl_mcumgr_command!((read, MGMT_GROUP_ID_OS, 0): os::Echo<'_> => os::EchoResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 2): os::TaskStatistics => os::TaskStatisticsResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 6): os::MCUmgrParameters => os::MCUmgrParametersResponse);

impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 0): fs::FileUpload<'_, '_> => fs::FileUploadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 0): fs::FileDownload<'_> => fs::FileDownloadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 1): fs::FileStatus<'_> => fs::FileStatusResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 2): fs::FileChecksum<'_, '_> => fs::FileChecksumResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 3): fs::SupportedFileChecksumTypes => fs::SupportedFileChecksumTypesResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 4): fs::FileClose => fs::FileCloseResponse);

impl_mcumgr_command!((write, MGMT_GROUP_ID_SHELL, 0): shell::ShellCommandLineExecute<'_> => shell::ShellCommandLineExecuteResponse);

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
                err: None
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
                err: None
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
