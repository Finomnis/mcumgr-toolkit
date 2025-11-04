#![deny(missing_docs)]
#![deny(unreachable_pub)]
#![forbid(unsafe_code)]
#![doc = include_str!("../../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/Finomnis/zephyr-mcumgr-client/issues")]

mod client;

pub mod commands;
pub mod connection;
pub mod transport;

pub use client::MCUmgrClient;
