use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// [Echo](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#echo-command) command
#[derive(Debug, Serialize)]
pub struct Echo<'a> {
    /// string to be replied by echo service
    pub d: &'a str,
}

/// Response for [`Echo`] command
#[derive(Debug, Deserialize)]
pub struct EchoResponse {
    /// replying echo string
    pub r: String,
}

impl<'a> super::McuMgrCommand for Echo<'a> {
    type Payload = Self;
    type Response = EchoResponse;
    fn is_write_operation(&self) -> bool {
        true
    }
    fn group_id(&self) -> u16 {
        0
    }
    fn command_id(&self) -> u8 {
        0
    }
    fn data(&self) -> &Self {
        self
    }
}

/// [Task statistics](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#task-statistics-command) command
#[derive(Debug, Serialize)]
pub struct TaskStatistics;

/// Statistics of an MCU task/thread
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
pub struct TaskStatisticsResponse {
    /// Dictionary of task names with their respective statistics
    pub tasks: HashMap<String, TaskStatisticsEntry>,
}

impl super::McuMgrCommand for TaskStatistics {
    type Payload = Self;
    type Response = TaskStatisticsResponse;
    fn is_write_operation(&self) -> bool {
        false
    }
    fn group_id(&self) -> u16 {
        0
    }
    fn command_id(&self) -> u8 {
        2
    }
    fn data(&self) -> &Self {
        self
    }
}

/// [MCUmgr Parameters](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_groups/smp_group_0.html#mcumgr-parameters) command
#[derive(Debug, Serialize)]
pub struct MCUmgrParameters;

/// Response for [`MCUmgrParameters`] command
#[derive(Debug, Deserialize)]
pub struct MCUmgrParametersResponse {
    /// Single SMP buffer size, this includes SMP header and CBOR payload
    pub buf_size: u32,
    /// Number of SMP buffers supported
    pub buf_count: u32,
}

impl super::McuMgrCommand for MCUmgrParameters {
    type Payload = Self;
    type Response = MCUmgrParametersResponse;
    fn is_write_operation(&self) -> bool {
        false
    }
    fn group_id(&self) -> u16 {
        0
    }
    fn command_id(&self) -> u8 {
        6
    }
    fn data(&self) -> &Self {
        self
    }
}
