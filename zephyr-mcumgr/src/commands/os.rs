use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct Echo<'a> {
    pub d: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct EchoResponse {
    pub r: String,
}

impl<'a> super::McuMgrRequest for Echo<'a> {
    type Response = EchoResponse;

    const WRITE_OPERATION: bool = true;
    const GROUP_ID: u16 = 0;
    const COMMAND_ID: u8 = 0;
}

#[derive(Debug, Serialize)]
pub struct TaskStatistics;

#[derive(Debug, Deserialize)]
pub struct TaskStatsEntry {
    pub prio: i32,
    pub tid: u32,
    pub state: u32,
    pub stkuse: Option<u64>,
    pub stksiz: Option<u64>,
    pub cswcnt: Option<u64>,
    pub runtime: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TaskStatisticsResponse {
    pub tasks: HashMap<String, TaskStatsEntry>,
}

impl super::McuMgrRequest for TaskStatistics {
    type Response = TaskStatisticsResponse;

    const WRITE_OPERATION: bool = false;
    const GROUP_ID: u16 = 0;
    const COMMAND_ID: u8 = 2;
}
