use mcumgr_toolkit::MCUmgrClient;

use crate::errors::CliError;

#[derive(Default)]
pub struct Client(Option<MCUmgrClient>);

impl Client {
    pub fn new(client: MCUmgrClient) -> Self {
        Self(Some(client))
    }

    pub fn get(&self) -> Result<&MCUmgrClient, CliError> {
        self.0.as_ref().ok_or(CliError::NoBackendSelected)
    }
}
