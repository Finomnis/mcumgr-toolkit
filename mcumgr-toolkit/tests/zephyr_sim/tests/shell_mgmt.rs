//! Shell management group (group 9)

use super::{assert_timeout, device_error, group_error};
use crate::sim::shell_mgmt::{ENOEXEC, shell_mgmt_err};
use crate::sim::smp::group_id;
use crate::sim::{Config, Fault, SimDevice};

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

#[test]
fn execute() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(
        client
            .shell_execute(&argv(&["kernel", "version"]), true)
            .unwrap(),
        (0, "Zephyr version 4.2.99\n".to_string())
    );
    assert_eq!(
        client
            .shell_execute(&argv(&["echo", "multiple", "arguments"]), false)
            .unwrap(),
        (0, "multiple arguments\n".to_string())
    );
    assert_eq!(
        client.shell_execute(&argv(&["fail", "-5"]), true).unwrap(),
        (-5, "failed\n".to_string())
    );
    assert_eq!(
        client.shell_execute(&argv(&["reboot_now"]), true).unwrap(),
        (-ENOEXEC, "reboot_now: command not found\n".to_string())
    );

    assert_eq!(
        device.lock().shell.history,
        [
            "kernel version",
            "echo multiple arguments",
            "fail -5",
            "reboot_now"
        ]
    );

    let request = device.requests_for(group_id::SHELL, 0).remove(0);
    let sent_argv: Vec<_> = request
        .field("argv")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_text().unwrap())
        .collect();
    assert_eq!(sent_argv, ["kernel", "version"]);
}

#[test]
fn errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client.shell_execute(&[], true).unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::SHELL, shell_mgmt_err::EMPTY_COMMAND)
    );

    // The command line has to fit into CONFIG_SHELL_CMD_BUFF_SIZE
    let err = client
        .shell_execute(&argv(&["echo", &"x".repeat(251)]), true)
        .unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::SHELL, shell_mgmt_err::COMMAND_TOO_LONG)
    );
    client
        .shell_execute(&argv(&["echo", &"x".repeat(250)]), true)
        .unwrap();
}

#[test]
fn retries_may_execute_a_command_twice() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    device.inject_faults([Fault::DropResponse]);
    let (ret, _) = client
        .shell_execute(&argv(&["echo", "once?"]), true)
        .unwrap();
    assert_eq!(ret, 0);
    assert_eq!(device.lock().shell.history.len(), 2);
}

#[test]
fn without_retries_a_command_runs_at_most_once() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    device.inject_faults([Fault::DropResponse]);
    let err = client
        .shell_execute(&argv(&["echo", "once"]), false)
        .unwrap_err();
    assert_timeout(err);
    assert_eq!(device.lock().shell.history, ["echo once"]);

    // Other commands are still retried
    device.inject_faults([Fault::DropResponse]);
    client.os_echo("retried").unwrap();
}

#[test]
fn larger_buffer() {
    let device = SimDevice::new(Config {
        shell_cmd_buff_size: 1024,
        mcumgr_transport_netbuf_size: 2048,
        ..Default::default()
    });
    let client = device.client();

    let long = "y".repeat(1000);
    let (_, output) = client.shell_execute(&argv(&["echo", &long]), true).unwrap();
    assert_eq!(output, format!("{long}\n"));
}
