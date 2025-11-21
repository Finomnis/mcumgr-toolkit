use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::commands::macros::{
    impl_deserialize_from_empty_map_and_into_unit, impl_serialize_as_empty_map,
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

/// [Date-Time Get](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#date-time-get) command
#[derive(Debug, Eq, PartialEq)]
pub struct DateTimeGet;
impl_serialize_as_empty_map!(DateTimeGet);

/// Response for [`DateTimeGet`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct DateTimeGetResponse {
    /// String in format: `yyyy-MM-dd'T'HH:mm:ss.SSSSSSZZZZZ`.
    pub datetime: String,
}

/// [Date-Time Set](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#date-time-set) command
#[derive(Serialize, Debug, Eq, PartialEq)]
pub struct DateTimeSet<'a> {
    /// String in format: `yyyy-MM-dd'T'HH:mm:ss.SSSSSSZZZZZ`.
    pub datetime: &'a str,
}

/// Response for [`DateTimeSet`] command
#[derive(Default, Debug, Eq, PartialEq)]
pub struct DateTimeSetResponse;
impl_deserialize_from_empty_map_and_into_unit!(DateTimeSetResponse);

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

#[cfg(test)]
mod tests {
    use super::super::macros::command_encode_decode_test;
    use super::*;
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
        datetime_get,
        (0, 0, 4),
        DateTimeGet,
        cbor!({}),
        cbor!({
            "datetime" => "2025-11-20T11:56:05.366345+01:00"
        }),
        DateTimeGetResponse{
            datetime: "2025-11-20T11:56:05.366345+01:00".to_string(),
        },
    }

    command_encode_decode_test! {
        datetime_set,
        (2, 0, 4),
        DateTimeSet{
            datetime: "2025-11-20T12:03:56.642Z"
        },
        cbor!({
            "datetime" => "2025-11-20T12:03:56.642Z"
        }),
        cbor!({}),
        DateTimeSetResponse,
    }

    command_encode_decode_test! {
        mcumgr_parameters,
        (0, 0, 6),
        MCUmgrParameters,
        cbor!({}),
        cbor!({"buf_size" => 42, "buf_count" => 69}),
        MCUmgrParametersResponse{buf_size: 42, buf_count: 69 },
    }
}
