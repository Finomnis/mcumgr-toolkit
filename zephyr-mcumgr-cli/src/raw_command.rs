use crate::args::RawCommandOp;

impl zephyr_mcumgr::commands::McuMgrCommand for crate::args::RawCommand {
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
}

impl serde::Serialize for crate::args::RawCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.data.serialize(serializer)
    }
}
