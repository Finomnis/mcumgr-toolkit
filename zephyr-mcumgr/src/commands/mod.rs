/// [File management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html) group commands
pub mod fs;
/// [Default/OS management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html) group commands
pub mod os;
/// [Shell management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_9.html) group commands
pub mod shell;

use serde::{Deserialize, Serialize};

/// SMP version 2 group based error message
#[derive(Debug, Deserialize)]
pub struct ErrResponseV2 {
    /// group of the group-based error code
    pub group: u16,
    /// contains the index of the group-based error code
    pub rc: i32,
}

/// [SMP error message](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html#minimal-response-smp-data)
#[derive(Debug, Deserialize)]
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

/// Implements the [`McuMgrCommand`] trait for a request/response pair.
///
/// # Parameters
/// - `$request`: The request type implementing the command
/// - `$response`: The response type for this command
/// - `$iswrite`: Boolean literal indicating if this is a write operation
/// - `$groupid`: The MCUmgr group
/// - `$commandid`: The MCUmgr command ID (u8)
macro_rules! impl_mcumgr_command {
    (@direction read) => {false};
    (@direction write) => {true};
    (($direction:tt, $groupid:ident, $commandid:literal): $request:ty => $response:ty) => {
        impl McuMgrCommand for $request {
            type Payload = Self;
            type Response = $response;
            fn is_write_operation(&self) -> bool {
                impl_mcumgr_command!(@direction $direction)
            }
            fn group_id(&self) -> u16 {
                $crate::MCUmgrGroup::$groupid as u16
            }
            fn command_id(&self) -> u8 {
                $commandid
            }
            fn data(&self) -> &Self {
                self
            }
        }
    };
}

impl_mcumgr_command!((write, MGMT_GROUP_ID_OS, 0): os::Echo<'_> => os::EchoResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 2): os::TaskStatistics => os::TaskStatisticsResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 6): os::MCUmgrParameters => os::MCUmgrParametersResponse);

impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 0): fs::FileUpload<'_, '_> => fs::FileUploadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 0): fs::FileDownload<'_> => fs::FileDownloadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 1): fs::FileStatus<'_> => fs::FileStatusResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 2): fs::FileChecksum<'_, '_> => fs::FileChecksumResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 3): fs::SupportedFileChecksumTypes => fs::SupportedFileChecksumTypesResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 4): fs::FileClose => ());

impl_mcumgr_command!((write, MGMT_GROUP_ID_SHELL, 0): shell::ShellCommandLineExecute<'_> => shell::ShellCommandLineExecuteResponse);
