//! Shell management group, modelled after
//! `subsys/mgmt/mcumgr/grp/shell_mgmt/src/shell_mgmt.c`, executing commands
//! on a simulated dummy shell backend.

use super::Device;
use ciborium::Value;

use super::cbor::int;
use super::smp::{Ctx, Group, group_id, mgmt_err, write};

const SHELL_MGMT_ID_EXEC: u8 = 0;

/// `enum shell_mgmt_err_code_t`
pub mod shell_mgmt_err {
    pub const COMMAND_TOO_LONG: u16 = 2;
    pub const EMPTY_COMMAND: u16 = 3;
}

/// `-ENOEXEC`, returned by `shell_execute_cmd` for unknown commands
pub const ENOEXEC: i32 = 8;

#[derive(Clone, Debug, Default)]
pub struct ShellState {
    /// Every command line that was executed, in order
    pub history: Vec<String>,
}

pub fn group() -> Group {
    Group::new(
        group_id::SHELL,
        "shell mgmt",
        &[(SHELL_MGMT_ID_EXEC, write(shell_exec))],
    )
}

/// `shell_execute_cmd` on the dummy backend; returns (return code, output)
fn execute(line: &str) -> (i32, String) {
    let argv: Vec<&str> = line.split_whitespace().collect();
    match argv.as_slice() {
        ["kernel", "version"] => (0, "Zephyr version 4.2.99\n".into()),
        ["echo", args @ ..] => (0, format!("{}\n", args.join(" "))),
        ["fail", code] => (code.parse().unwrap_or(-1), "failed\n".into()),
        [] => (0, String::new()),
        [cmd, ..] => (-ENOEXEC, format!("{cmd}: command not found\n")),
    }
}

/// `shell_mgmt_exec`
fn shell_exec(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let line_size = device.config.shell_cmd_buff_size + 1;

    let map = ctx
        .request()
        .and_then(|req| req.as_map())
        .ok_or(mgmt_err::EINVAL)?;

    // Expecting single array named "argv"
    let mut argv = None;
    for (key, value) in map {
        let Some(key) = key.as_text() else {
            // zcbor_tstr_decode() failed
            break;
        };
        if key == "argv" {
            argv = Some(value);
            break;
        }
    }
    let argv = argv
        .and_then(|argv| argv.as_array())
        .ok_or(mgmt_err::EINVAL)?;

    // Compose command line; composing stops at the first non-text entry
    let mut line = String::new();
    for arg in argv {
        let Some(arg) = arg.as_text() else {
            break;
        };
        if line.len() + arg.len() >= line_size - 1 {
            ctx.add_cmd_err(group_id::SHELL, shell_mgmt_err::COMMAND_TOO_LONG);
            return Ok(());
        }
        line.push_str(arg);
        line.push(' ');
    }
    if line.is_empty() {
        ctx.add_cmd_err(group_id::SHELL, shell_mgmt_err::EMPTY_COMMAND);
        return Ok(());
    }
    line.pop();

    let (ret, output) = execute(&line);
    device.shell.history.push(line);

    ctx.put("o", Value::Text(output));
    ctx.put("ret", int(ret));
    Ok(())
}
