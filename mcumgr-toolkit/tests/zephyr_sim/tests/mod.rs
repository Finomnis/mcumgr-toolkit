mod client;
mod config_variants;
mod enum_mgmt;
mod firmware_update;
mod fs_mgmt;
mod img_mgmt;
mod os_mgmt;
mod serial_transport;
mod settings_mgmt;
mod shell_mgmt;
mod simulator;
mod stat_mgmt;
mod transport;
mod udp_transport;
mod zephyr_basic;

use mcumgr_toolkit::client::MCUmgrClientError;
use mcumgr_toolkit::connection::ExecuteError;
use mcumgr_toolkit::smp_errors::DeviceError;
use mcumgr_toolkit::transport::ReceiveError;

/// The error the device answered with
#[track_caller]
pub fn device_error(err: MCUmgrClientError) -> DeviceError {
    match err {
        MCUmgrClientError::ExecuteError(ExecuteError::ErrorResponse(err)) => err,
        other => panic!("expected an error response from the device, got {other:?}"),
    }
}

/// A group error (`"err": {"group": .., "rc": ..}`) of SMP version 2
pub fn group_error(group: u16, rc: u16) -> DeviceError {
    DeviceError::V2 {
        group,
        rc: rc.into(),
    }
}

/// An SMP error (`"rc": ..`)
pub fn smp_error(rc: i32) -> DeviceError {
    DeviceError::V1 { rc, rsn: None }
}

#[track_caller]
pub fn assert_timeout(err: MCUmgrClientError) {
    assert!(
        matches!(
            err,
            MCUmgrClientError::ExecuteError(ExecuteError::ReceiveFailed(ReceiveError::Timeout))
        ),
        "expected a timeout, got {err:?}"
    );
}
