use serde::{Deserialize, Serialize};

/// [Shell command line execute](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_9.html#shell-command-line-execute) command
#[derive(Debug, Serialize)]
pub struct ShellCommandLineExecute<'a> {
    /// array consisting of strings representing command and its arguments
    pub argv: &'a [String],
}

/// Response for [`ShellCommandLineExecute`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ShellCommandLineExecuteResponse {
    /// command output
    pub o: String,
    /// return code from shell command execution
    pub ret: i32,
}

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use ciborium::cbor;

    command_encode_decode_test! {
        shell,
        (2, 9, 0),
        ShellCommandLineExecute{
            argv: &[
                "kernel".to_string(),
                "version".to_string(),
            ],
        },
        cbor!({
            "argv" => ["kernel", "version"]
        }),
        cbor!({
            "o" => "some_zephyr_version",
            "ret" => -4
        }),
        ShellCommandLineExecuteResponse{
            o: "some_zephyr_version".to_string(),
            ret: -4,
        },
    }
}
