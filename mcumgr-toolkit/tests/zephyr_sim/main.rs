//! Integration tests of the complete [`MCUmgrClient`](mcumgr_toolkit::MCUmgrClient)
//! API against a simulated Zephyr device.
//!
//! The simulator in [`sim`] re-implements the device side of MCUmgr after the
//! upstream Zephyr sources (`subsys/mgmt/mcumgr`) and MCUboot's slot handling,
//! and is connected to the client through the public
//! [`Transport`](mcumgr_toolkit::transport::Transport) trait via
//! [`MCUmgrClient::new_from_transport`](mcumgr_toolkit::MCUmgrClient::new_from_transport),
//! or through a simulated serial port speaking Zephyr's SMP-over-UART framing.

// Same as in the library: two ifs are often more readable
#![allow(clippy::collapsible_if)]

mod sim;
mod tests;
