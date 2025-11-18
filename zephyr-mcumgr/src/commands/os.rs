use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::commands::macros::impl_serialize_as_empty_map;

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
#[derive(Debug, Deserialize, Eq, PartialEq)]
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

/// Response for [`TaskStatistics`] command
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct TaskStatisticsResponse {
    /// Dictionary of task names with their respective statistics
    pub tasks: HashMap<String, TaskStatisticsEntry>,
}

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
        mcumgr_parameters,
        (0, 0, 6),
        MCUmgrParameters,
        cbor!({}),
        cbor!({"buf_size" => 42, "buf_count" => 69}),
        MCUmgrParametersResponse{buf_size: 42, buf_count: 69 },
    }
}
