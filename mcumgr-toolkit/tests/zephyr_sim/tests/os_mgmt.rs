//! OS management group (group 0)

use chrono::{NaiveDate, Timelike};
use mcumgr_toolkit::bootloader::{BootloaderInfo, BootloaderType, MCUbootMode};
use mcumgr_toolkit::commands::os::ThreadStateFlags;

use super::{device_error, group_error, smp_error};
use crate::sim::os_mgmt::{ResetRequest, os_mgmt_err};
use crate::sim::smp::{group_id, mgmt_err};
use crate::sim::{Bootloader, Config, SimDevice, TaskstatName};

#[test]
fn echo() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(client.os_echo("").unwrap(), "");
    assert_eq!(client.os_echo("Grüße, 世界 🦀").unwrap(), "Grüße, 世界 🦀");
    let long = "0123456789".repeat(30);
    assert_eq!(client.os_echo(&long).unwrap(), long);

    let request = device.requests().pop().unwrap();
    assert_eq!(request.field("d").unwrap().as_text(), Some(long.as_str()));
}

#[test]
fn task_statistics() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let tasks = client.os_task_statistics().unwrap();
    assert_eq!(tasks.len(), 3);

    let main = &tasks["main"];
    assert_eq!(main.prio, 0);
    assert_eq!(main.tid, 1);
    assert_eq!(main.state, 0x04);
    // Zephyr reports stack sizes in 4 byte words, the client in bytes
    assert_eq!(main.stksiz, Some(4096));
    assert_eq!(main.stkuse, Some(1536));
    // CONFIG_MCUMGR_GRP_OS_TASKSTAT_ONLY_SUPPORTED_STATS
    assert_eq!(main.runtime, None);
    assert_eq!(main.cswcnt, None);

    let sysworkq = &tasks["sysworkq"];
    assert_eq!(sysworkq.prio, -1);
    assert_eq!(sysworkq.tid, 2);
    assert_eq!(
        ThreadStateFlags::pretty_print(sysworkq.state as u8),
        "pending"
    );
    assert_eq!(tasks["idle"].prio, 15);
}

#[test]
fn task_statistics_with_all_fields() {
    let device = SimDevice::new(Config {
        os_taskstat_only_supported_stats: false,
        sched_thread_usage: true,
        os_taskstat_name: TaskstatName::Index,
        ..Default::default()
    });
    let client = device.client();

    let tasks = client.os_task_statistics().unwrap();
    let idle = &tasks["0"];
    assert_eq!(idle.runtime, Some(123_456_789));
    assert_eq!(idle.cswcnt, Some(0));
    assert_eq!(idle.stksiz, Some(320));
    assert_eq!(tasks["1"].stkuse, Some(1536));
}

#[test]
fn task_statistics_without_stack_info_and_unsigned_priorities() {
    let device = SimDevice::new(Config {
        os_taskstat_stack_info: false,
        os_taskstat_signed_priority: false,
        os_taskstat_name: TaskstatName::Priority,
        ..Default::default()
    });
    let client = device.client();

    let tasks = client.os_task_statistics().unwrap();
    let idle = &tasks["15"];
    assert_eq!(idle.stksiz, None);
    assert_eq!(idle.stkuse, None);
    // The priority of sysworkq (-1) is sent as unsigned 8 bit value
    assert_eq!(tasks["-1"].prio, 255);
}

#[test]
fn memory_pool_statistics() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let pools = client.os_memory_pool_statistics().unwrap();
    assert_eq!(pools.len(), 2);
    let pool = &pools["0"];
    // Not sent with CONFIG_MCUMGR_GRP_OS_MPSTAT_ONLY_SUPPORTED_STATS
    assert_eq!(pool.blksiz, 1);
    assert_eq!(pool.nblks, 4096);
    assert_eq!(pool.nfree, 3072);
    assert_eq!(pool.min, 2048);
    assert_eq!(pools["1"].nblks, 512);
    assert_eq!(pools["1"].min, 384);
}

#[test]
fn memory_pool_statistics_with_block_size() {
    let device = SimDevice::new(Config {
        os_mpstat_only_supported_stats: false,
        ..Default::default()
    });
    let client = device.client();

    let pools = client.os_memory_pool_statistics().unwrap();
    assert_eq!(pools["0"].blksiz, 1);
    assert_eq!(pools["0"].nfree, 3072);
}

#[test]
fn datetime() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let err = client.os_get_datetime().unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::OS, os_mgmt_err::RTC_NOT_SET)
    );

    let time = NaiveDate::from_ymd_opt(2026, 10, 5)
        .unwrap()
        .and_hms_opt(13, 37, 42)
        .unwrap();
    client.os_set_datetime(time).unwrap();
    assert_eq!(device.lock().os.rtc, Some(time));
    assert_eq!(client.os_get_datetime().unwrap(), time);

    let request = device.requests_for(group_id::OS, 4).remove(1);
    assert_eq!(
        request.field("datetime").unwrap().as_text(),
        Some("2026-10-05T13:37:42")
    );
}

#[test]
fn datetime_milliseconds_are_dropped_by_devices_without_ms_support() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let time = NaiveDate::from_ymd_opt(2000, 1, 1)
        .unwrap()
        .and_hms_milli_opt(0, 0, 1, 250)
        .unwrap();
    client.os_set_datetime(time).unwrap();
    assert_eq!(
        client.os_get_datetime().unwrap(),
        time.with_nanosecond(0).unwrap()
    );
}

#[test]
fn datetime_with_milliseconds() {
    let device = SimDevice::new(Config {
        os_datetime_ms: true,
        ..Default::default()
    });
    let client = device.client();

    let time = NaiveDate::from_ymd_opt(2024, 2, 29)
        .unwrap()
        .and_hms_milli_opt(23, 59, 59, 999)
        .unwrap();
    client.os_set_datetime(time).unwrap();
    assert_eq!(client.os_get_datetime().unwrap(), time);
}

#[test]
fn datetime_errors() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    // Years before 1900 are rejected by the parser
    let time = NaiveDate::from_ymd_opt(1899, 12, 31)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let err = client.os_set_datetime(time).unwrap_err();
    assert_eq!(device_error(err), smp_error(mgmt_err::EINVAL));

    device.lock().os.rtc_broken = true;
    let time = NaiveDate::from_ymd_opt(2020, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let err = client.os_set_datetime(time).unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::OS, os_mgmt_err::RTC_COMMAND_FAILED)
    );
    let err = client.os_get_datetime().unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::OS, os_mgmt_err::RTC_COMMAND_FAILED)
    );
}

#[test]
fn system_reset() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    client.os_system_reset(false, None).unwrap();
    client.os_system_reset(true, Some(1)).unwrap();

    let device = device.lock();
    assert_eq!(device.boot_count, 3);
    // Without CONFIG_MCUMGR_GRP_OS_RESET_HOOK and
    // CONFIG_MCUMGR_GRP_OS_RESET_BOOT_MODE the payload is ignored
    assert_eq!(
        device.os.resets,
        [ResetRequest {
            force: false,
            boot_mode: None
        }; 2]
    );
    assert_eq!(device.os.boot_mode, None);
}

#[test]
fn system_reset_with_force_and_boot_mode() {
    let device = SimDevice::new(Config {
        os_reset_hook: true,
        os_reset_boot_mode: true,
        ..Default::default()
    });
    let client = device.client();

    client.os_system_reset(false, None).unwrap();
    client.os_system_reset(true, None).unwrap();
    client.os_system_reset(false, Some(1)).unwrap();
    client.os_system_reset(true, Some(255)).unwrap();

    let device = device.lock();
    assert_eq!(
        device.os.resets,
        [
            ResetRequest {
                force: false,
                boot_mode: None
            },
            ResetRequest {
                force: true,
                boot_mode: None
            },
            ResetRequest {
                force: false,
                boot_mode: Some(1)
            },
            ResetRequest {
                force: true,
                boot_mode: Some(255)
            },
        ]
    );
    assert_eq!(device.os.boot_mode, Some(255));
}

#[test]
fn mcumgr_parameters() {
    let device = SimDevice::new(Config {
        mcumgr_transport_netbuf_size: 2475,
        mcumgr_transport_netbuf_count: 7,
        ..Default::default()
    });
    let client = device.client();

    let params = client.os_mcumgr_parameters().unwrap();
    assert_eq!(params.buf_size, 2475);
    assert_eq!(params.buf_count, 7);
}

#[test]
fn application_info() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    assert_eq!(client.os_application_info(None).unwrap(), "Zephyr");
    assert_eq!(client.os_application_info(Some("")).unwrap(), "Zephyr");
    assert_eq!(
        client.os_application_info(Some("a")).unwrap(),
        "Zephyr unknown v4.2.0-1234-gdeadbeef 4.2.99 arm cortex-m4 nrf52840dk/nrf52840 Zephyr"
    );
    assert_eq!(
        client.os_application_info(Some("vs")).unwrap(),
        "Zephyr 4.2.99"
    );
    assert_eq!(
        client.os_application_info(Some("i")).unwrap(),
        "nrf52840dk/nrf52840"
    );

    // 'b' is only valid with CONFIG_MCUMGR_GRP_OS_INFO_BUILD_DATE_TIME
    for format in ["x", "sb", "S"] {
        let err = client.os_application_info(Some(format)).unwrap_err();
        assert_eq!(
            device_error(err),
            group_error(group_id::OS, os_mgmt_err::INVALID_FORMAT)
        );
    }

    let requests = device.requests_for(group_id::OS, 7);
    assert!(requests[0].field("format").is_none());
    assert_eq!(requests[2].field("format").unwrap().as_text(), Some("a"));
}

#[test]
fn application_info_with_hostname_and_build_time() {
    let mut config = Config::default();
    config.os_info.node_name = Some("sim-device".into());
    config.os_info.build_date_time = Some("2026-10-05T08:00:00+0000".into());
    let device = SimDevice::new(config);
    let client = device.client();

    assert_eq!(
        client.os_application_info(Some("nb")).unwrap(),
        "sim-device 2026-10-05T08:00:00+0000"
    );
}

#[test]
fn bootloader_info() {
    let device = SimDevice::with_firmware();
    let client = device.client();

    let info = client.os_bootloader_info().unwrap();
    assert_eq!(
        info,
        BootloaderInfo::MCUboot {
            mode: MCUbootMode::MCUBOOT_MODE_SWAP_USING_MOVE as i32,
            no_downgrade: false
        }
    );
    assert_eq!(info.get_bootloader_type(), Ok(BootloaderType::MCUboot));

    // One request for the name, one for the mode
    let requests = device.requests_for(group_id::OS, 8);
    assert_eq!(requests.len(), 2);
    assert!(requests[0].field("query").is_none());
    assert_eq!(requests[1].field("query").unwrap().as_text(), Some("mode"));
}

#[test]
fn bootloader_info_with_downgrade_prevention() {
    let device = SimDevice::new(Config {
        bootloader: Bootloader::Mcuboot {
            mode: MCUbootMode::MCUBOOT_MODE_UPGRADE_ONLY as i32,
            no_downgrade: true,
        },
        ..Default::default()
    });
    let client = device.client();

    assert_eq!(
        client.os_bootloader_info().unwrap(),
        BootloaderInfo::MCUboot {
            mode: MCUbootMode::MCUBOOT_MODE_UPGRADE_ONLY as i32,
            no_downgrade: true
        }
    );
}

#[test]
fn bootloader_info_without_mcuboot() {
    let device = SimDevice::new(Config {
        bootloader: Bootloader::Other,
        ..Default::default()
    });
    let client = device.client();

    // Zephyr only knows how to describe MCUboot; anything else yields
    // OS_MGMT_ERR_QUERY_YIELDS_NO_ANSWER unless an application hook answers.
    let err = client.os_bootloader_info().unwrap_err();
    assert_eq!(
        device_error(err),
        group_error(group_id::OS, os_mgmt_err::QUERY_YIELDS_NO_ANSWER)
    );
}

#[test]
fn bootloader_info_not_enabled() {
    let device = SimDevice::new(Config {
        os_bootloader_info: false,
        ..Default::default()
    });
    let client = device.client();

    let err = client.os_bootloader_info().unwrap_err();
    assert!(err.command_not_supported());
}
