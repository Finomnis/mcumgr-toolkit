use std::sync::{Mutex, MutexGuard};

use pyo3::{exceptions::PyRuntimeError, prelude::*};

pub struct LockedClient<'a> {
    client: MutexGuard<'a, Option<::zephyr_mcumgr::MCUmgrClient>>,
}

impl<'a> LockedClient<'a> {
    pub fn lock(client: &'a Mutex<Option<::zephyr_mcumgr::MCUmgrClient>>) -> PyResult<Self> {
        let client = client
            .lock()
            .map_err(|e| PyRuntimeError::new_err(format!("{e}")))?;

        if client.is_none() {
            return Err(PyRuntimeError::new_err("Client already closed"));
        }

        Ok(Self { client })
    }

    /// Replaces the client object with None,
    /// dropping and closing it in the process.
    ///
    /// Must take ownership because after this function
    /// the deref invariant no longer holds.
    pub fn close(mut self) {
        self.client.take();
    }
}

impl<'a> std::ops::Deref for LockedClient<'a> {
    type Target = ::zephyr_mcumgr::MCUmgrClient;

    fn deref(&self) -> &Self::Target {
        // This *will* panic if invariant is broken, but only then.
        self.client
            .as_ref()
            .expect("LockedClient invariant: Option<MCUmgrClient> is always Some while guarded")
    }
}

impl<'a> std::ops::DerefMut for LockedClient<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.client
            .as_mut()
            .expect("LockedClient invariant: Option<MCUmgrClient> is always Some while guarded")
    }
}
