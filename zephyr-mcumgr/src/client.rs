use std::io::{Read, Write};

use crate::{
    commands,
    connection::{Connection, ExecuteError},
    transport::{SERIAL_TRANSPORT_DEFAULT_MTU, SerialTransport},
};

pub struct MCUmgrClient {
    connection: Connection,
}

impl MCUmgrClient {
    pub fn from_serial<T: Read + Write + 'static>(serial: T) -> Self {
        Self {
            connection: Connection::new(SerialTransport::new(serial, SERIAL_TRANSPORT_DEFAULT_MTU)),
        }
    }

    pub fn os_echo(&mut self, msg: impl AsRef<str>) -> Result<String, ExecuteError> {
        self.connection
            .execute_cbor(&commands::os::Echo { d: msg.as_ref() })
            .map(|resp| resp.r)
    }
}
