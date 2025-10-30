use crate::transport::Transport;

struct Connection {
    transport: Box<dyn Transport>,
}

impl Connection {
    pub fn new<T: Transport>(transport: T) -> Self {
        Self {
            transport: Box::new(transport),
        }
    }
}
