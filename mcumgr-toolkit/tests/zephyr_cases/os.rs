use crate::{
    assert_group_error, assert_smp_error,
    zephyr_sim::{
        Config, Fault, client, client_with,
        wire::{map, text},
    },
};
use mcumgr_toolkit::{bootloader::BootloaderInfo, client::MCUmgrClientError};

#[test]
fn echo_empty_unicode_and_binary_looking_text() {
    let (client, handle) = client();
    let inputs = ["", "hello", "Grüße 🦀", "line\n\0\r\t"];
    for input in inputs {
        assert_eq!(client.os_echo(input).unwrap(), input);
    }
    for (req, input) in handle.requests().iter().zip(inputs) {
        assert_eq!((req.group(), req.id()), (0, 0));
        assert_eq!(req.get("d").unwrap().as_text(), Some(input));
    }
}

#[test]
fn check_connection_uses_real_echo() {
    let (client, handle) = client();
    client.check_connection().unwrap();
    assert!(!handle.requests().is_empty());
    assert!(
        handle
            .requests()
            .iter()
            .all(|r| r.group() == 0 && r.id() == 0)
    );
}

#[test]
fn check_connection_rejects_incorrect_echo() {
    let (client, handle) = client();
    handle.fault(Fault::Reply(map([("r", text("incorrect echo"))])));
    assert!(client.check_connection().is_err());
    handle.assert_faults_consumed();
}

#[test]
fn task_statistics_convert_stack_words_to_bytes() {
    let (client, _) = client();
    let tasks = client.os_task_statistics().unwrap();
    assert_eq!(tasks.len(), 1);
    let main = &tasks["main"];
    assert_eq!((main.prio, main.tid, main.state), (-2, 1, 4));
    assert_eq!((main.stksiz, main.stkuse), (Some(1024), Some(300)));
    assert_eq!(main.runtime, Some(4_294_967_301));
    assert_eq!(main.cswcnt, Some(0));
}

#[test]
fn task_statistics_without_optional_kconfig_fields() {
    let (client, _) = client_with(Config {
        minimal_tasks: true,
        ..Config::default()
    });
    let tasks = client.os_task_statistics().unwrap();
    let main = &tasks["main"];
    assert_eq!(
        (main.stksiz, main.stkuse, main.runtime, main.cswcnt),
        (None, None, None, None)
    );
}

#[test]
fn upstream_memory_pool_statistics() {
    let (client, _) = client();
    let pools = client.os_memory_pool_statistics().unwrap();
    assert_eq!(pools.len(), 1);
    let pool = &pools["0"];
    assert_eq!(
        (pool.blksiz, pool.nblks, pool.nfree, pool.min),
        (1, 8192, 6000, 4096)
    );
}

#[test]
fn rtc_round_trip_and_unset_error() {
    let (client, handle) = client();
    assert_group_error(client.os_get_datetime().unwrap_err(), 0, 4);
    let datetime = chrono::NaiveDate::from_ymd_opt(2024, 2, 29)
        .unwrap()
        .and_hms_milli_opt(23, 59, 58, 123)
        .unwrap();
    client.os_set_datetime(datetime).unwrap();
    assert_eq!(
        handle.inspect(|d| d.datetime.clone()),
        Some("2024-02-29T23:59:58.123".into())
    );
    assert_eq!(client.os_get_datetime().unwrap(), datetime);
}

#[test]
fn rtc_accepts_seconds_without_fractional_part() {
    let (client, handle) = client();
    handle.edit(|d| d.datetime = Some("2026-09-27T01:02:03".into()));
    assert_eq!(
        client.os_get_datetime().unwrap().to_string(),
        "2026-09-27 01:02:03"
    );
}

#[test]
fn reset_options_reach_device() {
    let (client, handle) = client();
    client.os_system_reset(false, None).unwrap();
    client.os_system_reset(true, Some(1)).unwrap();
    client.os_system_reset(false, Some(0)).unwrap();
    assert_eq!(
        handle.inspect(|d| d.resets.clone()),
        [(false, None), (true, Some(1)), (false, Some(0))]
    );
    assert_smp_error(client.os_system_reset(false, Some(2)).unwrap_err(), 3);
}

#[test]
fn parameters_and_application_info() {
    let (client, _) = client();
    let parameters = client.os_mcumgr_parameters().unwrap();
    assert_eq!((parameters.buf_size, parameters.buf_count), (4096, 4));
    assert_eq!(client.os_application_info(None).unwrap(), "Zephyr");
    assert_eq!(client.os_application_info(Some("")).unwrap(), "Zephyr");
    assert_eq!(
        client.os_application_info(Some("mss")).unwrap(),
        "Zephyr arm"
    );
    assert_eq!(
        client.os_application_info(Some("a")).unwrap(),
        "Zephyr sim main 4.4.99 Sep 27 2026 00:00:00 arm cortex-m4 sim_board Zephyr"
    );
    assert_group_error(client.os_application_info(Some("?")).unwrap_err(), 0, 2);
}

#[test]
fn bootloader_mode_and_optional_downgrade_flag() {
    for no_downgrade in [false, true] {
        let (client, handle) = client_with(Config {
            no_downgrade,
            ..Config::default()
        });
        assert!(
            matches!(client.os_bootloader_info().unwrap(), BootloaderInfo::MCUboot { mode: 1, no_downgrade: value } if value == no_downgrade)
        );
        let requests = handle.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].get("query").unwrap().as_text(), Some("mode"));
    }
}

#[test]
fn unknown_bootloader_does_not_query_mcuboot_mode() {
    let (client, handle) = client_with(Config {
        bootloader: "custom boot".into(),
        ..Config::default()
    });
    assert!(
        matches!(client.os_bootloader_info().unwrap(), BootloaderInfo::Unknown { name } if name == "custom boot")
    );
    assert_eq!(handle.requests().len(), 1);
}

#[test]
fn timeout_configuration_and_failure() {
    let (client, handle) = client();
    let timeout = std::time::Duration::from_millis(17);
    client.set_timeout(timeout).unwrap();
    assert_eq!(handle.timeout(), timeout);
    handle.fail_timeout_configuration();
    assert!(matches!(
        client.set_timeout(timeout),
        Err(MCUmgrClientError::SetTimeoutFailed(_))
    ));
}
