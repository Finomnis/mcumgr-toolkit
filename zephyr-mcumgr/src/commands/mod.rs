pub mod os;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ErrResponseV2 {
    pub group: u32,
    pub rc: u32,
}

#[derive(Debug, Deserialize)]
pub struct ErrResponse {
    pub rc: Option<i32>,
    pub err: Option<ErrResponseV2>,
}

pub trait McuMgrRequest: Serialize {
    type Response: for<'a> Deserialize<'a>;
    const WRITE_OPERATION: bool;
    const GROUP_ID: u16;
    const COMMAND_ID: u8;
}
