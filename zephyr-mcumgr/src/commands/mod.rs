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
    pub group: u32,
    /// contains the index of the group-based error code
    pub rc: u32,
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
