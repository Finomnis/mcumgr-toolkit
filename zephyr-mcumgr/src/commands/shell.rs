use serde::{Deserialize, Serialize};

/// [Shell command line execute](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_9.html#shell-command-line-execute) command
#[derive(Debug, Serialize)]
pub struct ShellCommandLineExecute<'a> {
    /// array consisting of strings representing command and its arguments
    pub argv: &'a [String],
}

/// Response for [`ShellCommandLineExecute`] command
#[derive(Debug, Deserialize)]
pub struct ShellCommandLineExecuteResponse {
    /// command output
    pub o: String,
    /// return code from shell command execution
    pub ret: i32,
}

impl<'a> super::McuMgrCommand for ShellCommandLineExecute<'a> {
    type Payload = Self;
    type Response = ShellCommandLineExecuteResponse;
    fn is_write_operation(&self) -> bool {
        true
    }
    fn group_id(&self) -> u16 {
        9
    }
    fn command_id(&self) -> u8 {
        0
    }
    fn data(&self) -> &Self {
        self
    }
}
