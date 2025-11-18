/// [File management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_8.html) group commands
pub mod fs;
/// [Default/OS management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html) group commands
pub mod os;
/// [Shell management](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_9.html) group commands
pub mod shell;

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

impl_mcumgr_command!((read, MGMT_GROUP_ID_OS, 0): os::Echo<'_> => os::EchoResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 2): os::TaskStatistics => os::TaskStatisticsResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_OS, 6): os::MCUmgrParameters => os::MCUmgrParametersResponse);

impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 0): fs::FileUpload<'_, '_> => fs::FileUploadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 0): fs::FileDownload<'_> => fs::FileDownloadResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 1): fs::FileStatus<'_> => fs::FileStatusResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 2): fs::FileChecksum<'_, '_> => fs::FileChecksumResponse);
impl_mcumgr_command!((read,  MGMT_GROUP_ID_FS, 3): fs::SupportedFileChecksumTypes => fs::SupportedFileChecksumTypesResponse);
impl_mcumgr_command!((write, MGMT_GROUP_ID_FS, 4): fs::FileClose => ());

impl_mcumgr_command!((write, MGMT_GROUP_ID_SHELL, 0): shell::ShellCommandLineExecute<'_> => shell::ShellCommandLineExecuteResponse);

#[cfg(test)]
macro_rules! command_encode_decode_test {
    (@is_write 0) => {false};
    (@is_write 2) => {true};
    ($name:ident, ($op:tt, $group_id:literal, $command_id:literal), $request:expr, $encoded_req:expr ,$encoded_res:expr, $response:expr $(,)?) => {
        #[test]
        fn $name() {
            use $crate::commands::McuMgrCommand;

            let expected_is_write = command_encode_decode_test!(@is_write $op);
            assert_eq!($request.is_write_operation(), expected_is_write);
            assert_eq!($request.group_id(), $group_id);
            assert_eq!($request.command_id(), $command_id);

            let mut encoded_request = vec![];
            ::ciborium::into_writer(&$request.data(), &mut encoded_request).unwrap();

            let mut expected_encoded_request = vec![];
            ::ciborium::into_writer(&$encoded_req.unwrap(), &mut expected_encoded_request).unwrap();

            assert_eq!(
                encoded_request.iter().map(|x|format!("{:02x}", x)).collect::<String>(),
                expected_encoded_request.iter().map(|x|format!("{:02x}", x)).collect::<String>(),
                "encoding mismatch"
            );

            let mut encoded_response = vec![];
            ::ciborium::into_writer(&$encoded_res.unwrap(), &mut encoded_response).unwrap();

            // Compile time type check
            fn types_match<T: $crate::commands::McuMgrCommand>(
                _req: T,
                _res: <T as $crate::commands::McuMgrCommand>::Response,
            ) {
            }
            types_match($request, $response);

            let response = ::ciborium::from_reader(encoded_response.as_slice()).unwrap();
            let expected_response = $response;

            assert_eq!(response, expected_response, "decoding mismatch");

            // As a type hint for ciborium
            types_match($request, response);
        }
    };
}

#[cfg(test)]
use command_encode_decode_test;
