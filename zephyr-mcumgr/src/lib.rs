#![deny(missing_docs)]
#![deny(unreachable_pub)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/Finomnis/zephyr-mcumgr-client/issues")]

/// A high level client for Zephyr's MCUmgr SMP functionality
pub mod client;

/// [MCUmgr command group](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html#specifications-of-management-groups-supported-by-zephyr) definitions
pub mod commands;

/// [SMP protocal layer](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html) implementation
pub mod connection;

/// [SMP transport layer](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_transport.html) implementation
pub mod transport;

pub use client::MCUmgrClient;
