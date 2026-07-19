use crate::transport::ble::async_reactor::AsyncReactor;

pub struct BtleplugBackend {
    reactor: AsyncReactor,
    adapter: btleplug::platform::Adapter,
}
