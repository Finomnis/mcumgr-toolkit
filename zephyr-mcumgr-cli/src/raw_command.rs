use crate::args::RawCommandOp;

impl zephyr_mcumgr::commands::McuMgrCommand for crate::args::RawCommand {
    type Payload = serde_json::Value;
    type Response = serde_json::Value;

    fn is_write_operation(&self) -> bool {
        match self.op {
            RawCommandOp::Read => false,
            RawCommandOp::Write => true,
        }
    }

    fn group_id(&self) -> u16 {
        self.group_id
    }

    fn command_id(&self) -> u8 {
        self.command_id
    }

    fn data(&self) -> &serde_json::Value {
        &self.data
    }
}
