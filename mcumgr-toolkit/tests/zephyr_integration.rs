//! Public-API integration tests against an independent Zephyr protocol model.
//! Run: cargo test -p mcumgr-toolkit --test zephyr_integration
mod zephyr_sim;

#[path = "zephyr_cases/files.rs"]
mod files;
#[path = "zephyr_cases/firmware_update.rs"]
mod firmware_update;
#[path = "zephyr_cases/images.rs"]
mod images;
#[path = "zephyr_cases/management.rs"]
mod management;
#[path = "zephyr_cases/os.rs"]
mod os;
#[path = "zephyr_cases/protocol.rs"]
mod protocol;

use mcumgr_toolkit::{
    client::MCUmgrClientError, connection::ExecuteError, smp_errors::DeviceError,
};

fn assert_group_error(error: MCUmgrClientError, expected_group: u16, expected_rc: i32) {
    match error {
        MCUmgrClientError::ExecuteError(ExecuteError::ErrorResponse(DeviceError::V2 {
            group,
            rc,
        })) => {
            assert_eq!((group, rc), (expected_group, expected_rc));
        }
        other => panic!("expected group {expected_group} error {expected_rc}; got {other:?}"),
    }
}

fn assert_smp_error(error: MCUmgrClientError, expected_rc: i32) {
    match error {
        MCUmgrClientError::ExecuteError(ExecuteError::ErrorResponse(DeviceError::V1 {
            rc,
            ..
        })) => assert_eq!(rc, expected_rc),
        other => panic!("expected SMP error {expected_rc}; got {other:?}"),
    }
}
