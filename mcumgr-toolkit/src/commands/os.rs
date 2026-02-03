use std::collections::HashMap;

use chrono::Timelike;
use serde::{Deserialize, Serialize};

use super::{
    is_default,
    macros::{impl_deserialize_from_empty_map_and_into_unit, impl_serialize_as_empty_map},
};

/// [Echo](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#echo-command) command
#[derive(Debug, Serialize, Eq, PartialEq)]
pub struct Echo<'a> {
    /// string to be replied by echo service
    pub d: &'a str,
}

/// Response for [`Echo`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct EchoResponse {
    /// replying echo string
    pub r: String,
}

/// [Task statistics](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#task-statistics-command) command
#[derive(Debug, Eq, PartialEq)]
pub struct TaskStatistics;
impl_serialize_as_empty_map!(TaskStatistics);

/// Statistics of an MCU task/thread
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TaskStatisticsEntry {
    /// task priority
    pub prio: i32,
    /// numeric task ID
    pub tid: u32,
    /// numeric task state
    pub state: u32,
    /// task’s/thread’s stack usage
    pub stkuse: Option<u64>,
    /// task’s/thread’s stack size
    pub stksiz: Option<u64>,
    /// task’s/thread’s context switches
    pub cswcnt: Option<u64>,
    /// task’s/thread’s runtime in “ticks”
    pub runtime: Option<u64>,
}

/// Flags inside of [`TaskStatisticsEntry::state`]
#[derive(strum::Display, strum::AsRefStr, strum::EnumIter, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
#[strum(serialize_all = "snake_case")]
pub enum ThreadStateFlags {
    /** Not a real thread */
    DUMMY = 1 << 0,

    /** Thread is waiting on an object */
    PENDING = 1 << 1,

    /** Thread is sleeping */
    SLEEPING = 1 << 2,

    /** Thread has terminated */
    DEAD = 1 << 3,

    /** Thread is suspended */
    SUSPENDED = 1 << 4,

    /** Thread is in the process of aborting */
    ABORTING = 1 << 5,

    /** Thread is in the process of suspending */
    SUSPENDING = 1 << 6,

    /** Thread is present in the ready queue */
    QUEUED = 1 << 7,
}

impl ThreadStateFlags {
    /// Converts the thread state to a human readable string
    pub fn pretty_print(thread_state: u8) -> String {
        use strum::IntoEnumIterator;

        let mut bit_names = vec![];
        for bit in Self::iter() {
            if (thread_state & bit as u8) != 0 {
                bit_names.push(format!("{bit}"));
            }
        }

        bit_names.join(" | ")
    }
}

/// Response for [`TaskStatistics`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct TaskStatisticsResponse {
    /// Dictionary of task names with their respective statistics
    pub tasks: HashMap<String, TaskStatisticsEntry>,
}

/// Parses a [`chrono::NaiveDateTime`] object with optional timezone specifiers
fn deserialize_datetime_and_ignore_timezone<'de, D>(
    de: D,
) -> Result<chrono::NaiveDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NaiveOrFixed {
        Naive(chrono::NaiveDateTime),
        Fixed(chrono::DateTime<chrono::FixedOffset>),
    }

    NaiveOrFixed::deserialize(de).map(|val| match val {
        NaiveOrFixed::Naive(naive_date_time) => naive_date_time,
        NaiveOrFixed::Fixed(date_time) => date_time.naive_local(),
    })
}

/// Serializes a [`chrono::NaiveDateTime`] object with zero or three fractional digits,
/// which is most compatible with Zephyr
fn serialize_datetime_for_zephyr<S>(
    value: &chrono::NaiveDateTime,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if value.time().nanosecond() != 0 {
        serializer.serialize_str(&format!("{}", value.format("%Y-%m-%dT%H:%M:%S%.3f")))
    } else {
        serializer.serialize_str(&format!("{}", value.format("%Y-%m-%dT%H:%M:%S")))
    }
}

/// [Date-Time Get](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#date-time-get) command
#[derive(Debug, Eq, PartialEq)]
pub struct DateTimeGet;
impl_serialize_as_empty_map!(DateTimeGet);

/// Response for [`DateTimeGet`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct DateTimeGetResponse {
    /// String in format: `yyyy-MM-dd'T'HH:mm:ss.SSS`.
    #[serde(deserialize_with = "deserialize_datetime_and_ignore_timezone")]
    pub datetime: chrono::NaiveDateTime,
}

/// [Date-Time Set](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#date-time-set) command
#[derive(Serialize, Debug, Eq, PartialEq)]
pub struct DateTimeSet {
    /// String in format: `yyyy-MM-dd'T'HH:mm:ss.SSS`.
    #[serde(serialize_with = "serialize_datetime_for_zephyr")]
    pub datetime: chrono::NaiveDateTime,
}

/// Response for [`DateTimeSet`] command
#[derive(Default, Debug, Eq, PartialEq)]
pub struct DateTimeSetResponse;
impl_deserialize_from_empty_map_and_into_unit!(DateTimeSetResponse);

/// [System Reset](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#system-reset) command
#[derive(Serialize, Debug, Eq, PartialEq)]
pub struct SystemReset {
    /// Forces reset
    #[serde(skip_serializing_if = "is_default")]
    pub force: bool,
    /// Boot mode
    ///
    /// - 0: Normal boot
    /// - 1: Bootloader recovery mode
    ///
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_mode: Option<u8>,
}

/// Response for [`SystemReset`] command
#[derive(Default, Debug, Eq, PartialEq)]
pub struct SystemResetResponse;
impl_deserialize_from_empty_map_and_into_unit!(SystemResetResponse);

/// [MCUmgr Parameters](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#mcumgr-parameters) command
#[derive(Debug, Eq, PartialEq)]
pub struct MCUmgrParameters;
impl_serialize_as_empty_map!(MCUmgrParameters);

/// Response for [`MCUmgrParameters`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct MCUmgrParametersResponse {
    /// Single SMP buffer size, this includes SMP header and CBOR payload
    pub buf_size: u32,
    /// Number of SMP buffers supported
    pub buf_count: u32,
}

/// [OS/Application Info](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#os-application-info) command
#[derive(Serialize, Debug, Eq, PartialEq)]
pub struct ApplicationInfo<'a> {
    /// Format specifier of returned response
    ///
    /// For more info, see [the SMP documentation](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#os-application-info-request).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<&'a str>,
}

/// Response for [`ApplicationInfo`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ApplicationInfoResponse {
    /// Text response including requested parameters
    pub output: String,
}

/// [Bootloader Information](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#bootloader-information) command
#[derive(Debug, Eq, PartialEq)]
pub struct BootloaderInfo;
impl_serialize_as_empty_map!(BootloaderInfo);

/// Response for [`BootloaderInfo`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct BootloaderInfoResponse {
    /// String representing bootloader name
    pub bootloader: String,
}

/// [Bootloader Information MCUboot Mode](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#bootloader-information-mcuboot) subcommand
#[derive(Serialize, Debug, Eq, PartialEq)]
#[serde(tag = "query", rename = "mode")]
pub struct BootloaderInfoMcubootMode {}

/// Response for [`BootloaderInfoMcubootMode`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct BootloaderInfoMcubootModeResponse {
    /// The bootloader mode
    pub mode: i32,
    /// MCUboot has downgrade prevention enabled
    #[serde(default, rename = "no-downgrade")]
    pub no_downgrade: bool,
}

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    use ciborium::cbor;

    #[test]
    fn thread_state_flags_to_string() {
        assert_eq!(
            ThreadStateFlags::pretty_print(0xff),
            "dummy | pending | sleeping | dead | suspended | aborting | suspending | queued"
        );

        assert_eq!(ThreadStateFlags::pretty_print(0b00000001), "dummy");
        assert_eq!(ThreadStateFlags::pretty_print(0b00000010), "pending");
        assert_eq!(ThreadStateFlags::pretty_print(0b00000100), "sleeping");
        assert_eq!(ThreadStateFlags::pretty_print(0b00001000), "dead");
        assert_eq!(ThreadStateFlags::pretty_print(0b00010000), "suspended");
        assert_eq!(ThreadStateFlags::pretty_print(0b00100000), "aborting");
        assert_eq!(ThreadStateFlags::pretty_print(0b01000000), "suspending");
        assert_eq!(ThreadStateFlags::pretty_print(0b10000000), "queued");

        assert_eq!(ThreadStateFlags::pretty_print(0), "");
    }

    command_encode_decode_test! {
        echo,
        (0, 0, 0),
        Echo{d: "Hello World!"},
        cbor!({"d" => "Hello World!"}),
        cbor!({"r" => "Hello World!"}),
        EchoResponse{r: "Hello World!".to_string()},
    }

    command_encode_decode_test! {
        task_statistics_empty,
        (0, 0, 2),
        TaskStatistics,
        cbor!({}),
        cbor!({"tasks" => {}}),
        TaskStatisticsResponse{ tasks: HashMap::new() },
    }

    command_encode_decode_test! {
        task_statistics,
        (0, 0, 2),
        TaskStatistics,
        cbor!({}),
        cbor!({"tasks" => {
            "task_a" => {
                "prio" => 20,
                "tid" => 5,
                "state" => 10,
            },
            "task_b" => {
                "prio"         => 30,
                "tid"          => 31,
                "state"        => 32,
                "stkuse"       => 33,
                "stksiz"       => 34,
                "cswcnt"       => 35,
                "runtime"      => 36,
                "last_checkin" => 0,
                "next_checkin" => 0,
            },
        }}),
        TaskStatisticsResponse{ tasks: HashMap::from([
            (
                "task_a".to_string(),
                TaskStatisticsEntry{
                    prio: 20,
                    tid: 5,
                    state: 10,
                    stkuse: None,
                    stksiz: None,
                    cswcnt: None,
                    runtime: None,
                },
            ), (
                "task_b".to_string(),
                TaskStatisticsEntry{
                    prio: 30,
                    tid: 31,
                    state: 32,
                    stkuse: Some(33),
                    stksiz: Some(34),
                    cswcnt: Some(35),
                    runtime: Some(36),
                },
            ),
        ]) },
    }

    command_encode_decode_test! {
        datetime_get_with_timezone,
        (0, 0, 4),
        DateTimeGet,
        cbor!({}),
        cbor!({
            "datetime" => "2025-11-20T11:56:05.366345+01:00"
        }),
        DateTimeGetResponse{
            datetime: NaiveDateTime::new(NaiveDate::from_ymd_opt(2025, 11, 20).unwrap(), NaiveTime::from_hms_micro_opt(11,56,5,366345).unwrap()),
        },
    }

    command_encode_decode_test! {
        datetime_get_with_millis,
        (0, 0, 4),
        DateTimeGet,
        cbor!({}),
        cbor!({
            "datetime" => "2025-11-20T11:56:05.366"
        }),
        DateTimeGetResponse{
            datetime: NaiveDateTime::new(NaiveDate::from_ymd_opt(2025, 11, 20).unwrap(), NaiveTime::from_hms_milli_opt(11,56,5,366).unwrap()),
        },
    }

    command_encode_decode_test! {
        datetime_get_without_millis,
        (0, 0, 4),
        DateTimeGet,
        cbor!({}),
        cbor!({
            "datetime" => "2025-11-20T11:56:05"
        }),
        DateTimeGetResponse{
            datetime: NaiveDateTime::new(NaiveDate::from_ymd_opt(2025, 11, 20).unwrap(), NaiveTime::from_hms_opt(11,56,5).unwrap()),
        },
    }

    command_encode_decode_test! {
        datetime_set_with_millis,
        (2, 0, 4),
        DateTimeSet{
            datetime: NaiveDateTime::new(NaiveDate::from_ymd_opt(2025, 11, 20).unwrap(), NaiveTime::from_hms_micro_opt(12,3,56,642133).unwrap())
        },
        cbor!({
            "datetime" => "2025-11-20T12:03:56.642"
        }),
        cbor!({}),
        DateTimeSetResponse,
    }

    command_encode_decode_test! {
        datetime_set_without_millis,
        (2, 0, 4),
        DateTimeSet{
            datetime: NaiveDateTime::new(NaiveDate::from_ymd_opt(2025, 11, 20).unwrap(), NaiveTime::from_hms_opt(12,3,56).unwrap())
        },
        cbor!({
            "datetime" => "2025-11-20T12:03:56"
        }),
        cbor!({}),
        DateTimeSetResponse,
    }

    command_encode_decode_test! {
        system_reset_minimal,
        (2, 0, 5),
        SystemReset{
            force: false,
            boot_mode: None,
        },
        cbor!({}),
        cbor!({}),
        SystemResetResponse,
    }

    command_encode_decode_test! {
        system_reset_full,
        (2, 0, 5),
        SystemReset{
            force: true,
            boot_mode: Some(42),
        },
        cbor!({
            "force" => true,
            "boot_mode" => 42,
        }),
        cbor!({}),
        SystemResetResponse,
    }

    command_encode_decode_test! {
        mcumgr_parameters,
        (0, 0, 6),
        MCUmgrParameters,
        cbor!({}),
        cbor!({"buf_size" => 42, "buf_count" => 69}),
        MCUmgrParametersResponse{buf_size: 42, buf_count: 69 },
    }

    command_encode_decode_test! {
        application_info_without_format,
        (0, 0, 7),
        ApplicationInfo{
            format: None,
        },
        cbor!({}),
        cbor!({
            "output" => "foo",
        }),
        ApplicationInfoResponse{
            output: "foo".to_string(),
        }
    }

    command_encode_decode_test! {
        application_info_with_format,
        (0, 0, 7),
        ApplicationInfo{
            format: Some("abc"),
        },
        cbor!({
            "format" => "abc",
        }),
        cbor!({
            "output" => "bar",
        }),
        ApplicationInfoResponse{
            output: "bar".to_string(),
        }
    }

    command_encode_decode_test! {
        bootloader_info,
        (0, 0, 8),
        BootloaderInfo,
        cbor!({}),
        cbor!({
            "bootloader" => "MCUboot",
        }),
        BootloaderInfoResponse{
            bootloader: "MCUboot".to_string(),
        }
    }

    command_encode_decode_test! {
        bootloader_info_mcuboot_mode,
        (0, 0, 8),
        BootloaderInfoMcubootMode{},
        cbor!({
            "query" => "mode",
        }),
        cbor!({
            "mode" => 5,
            "no-downgrade" => true,
        }),
        BootloaderInfoMcubootModeResponse{
            mode: 5,
            no_downgrade: true,
        }
    }

    command_encode_decode_test! {
        bootloader_info_mcuboot_mode_default_values,
        (0, 0, 8),
        BootloaderInfoMcubootMode{},
        cbor!({
            "query" => "mode",
        }),
        cbor!({
            "mode" => -1,
        }),
        BootloaderInfoMcubootModeResponse{
            mode: -1,
            no_downgrade: false,
        }
    }
}
