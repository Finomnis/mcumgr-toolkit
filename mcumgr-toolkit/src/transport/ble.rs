use std::{pin::Pin, time::Duration};

use btleplug::{
    api::{Central, CentralEvent, Characteristic, Manager, Peripheral as _, ScanFilter},
    platform::{Adapter, Peripheral},
};
use uuid::{Uuid, uuid};

use crate::transport::{SendError, Transport};

/// The error type of [`BleRuntime`].
pub type BleRuntimeError = btleplug::Error;

/// A runtime manager that encapsulates all the
/// async BLE boilerplate code.
pub struct BleRuntime {
    runtime: Box<tokio::runtime::Runtime>,
    adapter: btleplug::platform::Adapter,
}

/// The BLE service UUID that signals SMP capability
pub const SMP_UUID: Uuid = uuid!("8D53DC1D-1DB7-4CD3-868B-8A527460AA84");
/// The BLE characteristic UUID used to communicate SMP messages
pub const CHARACTERISTIC_UUID: Uuid = uuid!("DA2E7828-FBCE-4E01-AE9E-261174997C48");

impl BleRuntime {
    /// Create a new [`BleRuntime`].
    ///
    /// # Arguments
    ///
    /// * `serial` - A serial port object, like [`serialport::SerialPort`].
    ///
    pub fn new() -> Result<Self, BleRuntimeError> {
        let runtime = Box::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
        );

        let adapter = runtime.block_on(async {
            let manager = btleplug::platform::Manager::new().await?;

            let adapter = manager
                .adapters()
                .await?
                .into_iter()
                .next()
                .ok_or(BleRuntimeError::NoAdapterAvailable)?;

            Result::<_, BleRuntimeError>::Ok(adapter)
        })?;

        Ok(Self { runtime, adapter })
    }

    /// Execute the given function while scanning for devices
    pub fn scan<F, R>(&mut self, f: F) -> Result<R, BleRuntimeError>
    where
        F: AsyncFnOnce(Pin<Box<dyn futures::Stream<Item = CentralEvent> + Send>>, &Adapter) -> R,
    {
        let future = async {
            self.adapter.clear_peripherals().await?;

            let events = self.adapter.events().await?;

            self.adapter
                .start_scan(ScanFilter {
                    services: vec![SMP_UUID],
                })
                .await?;

            let result = f(events, &self.adapter).await;

            self.adapter.stop_scan().await?;

            Ok(result)
        };

        self.block_on(future)
    }

    /// Run a future to completion
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.runtime.block_on(future)
    }

    /// Creates a BLE transport to a connected device
    pub fn into_transport(
        self,
        device: Peripheral,
        timeout: Duration,
    ) -> Result<BleTransport, BleRuntimeError> {
        let characteristic = device
            .characteristics()
            .iter()
            .find(|ch| ch.service_uuid == SMP_UUID && ch.uuid == CHARACTERISTIC_UUID)
            .cloned()
            .ok_or(BleRuntimeError::NoSuchCharacteristic)?;

        Ok(BleTransport {
            runtime: self,
            device,
            characteristic,
            timeout,
            send_buffer: Vec::new(),
        })
    }
}

/// An active connection to a BLE device
pub struct BleTransport {
    runtime: BleRuntime,
    device: Peripheral,
    characteristic: Characteristic,
    timeout: Duration,
    send_buffer: Vec<u8>,
}

impl Transport for BleTransport {
    fn send_raw_frame(
        &mut self,
        header: [u8; super::SMP_HEADER_SIZE],
        data: &[u8],
    ) -> Result<(), super::SendError> {
        self.send_buffer.clear();
        self.send_buffer.extend_from_slice(&header);
        self.send_buffer.extend_from_slice(data);

        self.runtime.block_on(self.device.write(
            &self.characteristic,
            &self.send_buffer,
            btleplug::api::WriteType::WithoutResponse,
        ))?;

        Ok(())
    }

    fn recv_raw_frame<'a>(
        &mut self,
        buffer: &'a mut [u8; super::SMP_TRANSFER_BUFFER_SIZE],
    ) -> Result<&'a [u8], super::ReceiveError> {
        todo!()
    }

    fn set_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.timeout = timeout;
        Ok(())
    }
}
