#![deny(missing_docs)]
#![deny(unreachable_pub)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/Finomnis/zephyr-mcumgr-client/issues")]

/// A high level client for Zephyr's MCUmgr SMP functionality
pub mod client;
pub use client::MCUmgrClient;

/// [MCUmgr command group](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html#specifications-of-management-groups-supported-by-zephyr) definitions
pub mod commands;

/// [SMP protocal layer](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_protocol.html) implementation
pub mod connection;

/// [SMP transport layer](https://docs.zephyrproject.org/latest/services/device_mgmt/smp_transport.html) implementation
pub mod transport;

/// Zephyr SMP error definitions
pub mod smp_errors;

/// See [`enum mcumgr_group_t`](https://docs.zephyrproject.org/latest/doxygen/html/mgmt__defines_8h.html).
#[derive(strum_macros::FromRepr, strum_macros::Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
#[allow(non_camel_case_types)]
#[allow(missing_docs)]
pub enum MCUmgrGroup {
    MGMT_GROUP_ID_OS = 0,
    MGMT_GROUP_ID_IMAGE,
    MGMT_GROUP_ID_STAT,
    MGMT_GROUP_ID_SETTINGS,
    MGMT_GROUP_ID_LOG,
    MGMT_GROUP_ID_CRASH,
    MGMT_GROUP_ID_SPLIT,
    MGMT_GROUP_ID_RUN,
    MGMT_GROUP_ID_FS,
    MGMT_GROUP_ID_SHELL,
    MGMT_GROUP_ID_ENUM,
    ZEPHYR_MGMT_GRP_BASIC = 63,
    MGMT_GROUP_ID_PERUSER = 64,
}

impl MCUmgrGroup {
    /// Converts a raw group id to a string
    pub fn group_id_to_string(group_id: u16) -> String {
        const PERUSER: MCUmgrGroup = MCUmgrGroup::MGMT_GROUP_ID_PERUSER;
        if group_id < PERUSER as u16 {
            if let Some(group_enum) = Self::from_repr(group_id) {
                format!("{group_enum}")
            } else {
                format!("MGMT_GROUP_ID_UNKNOWN({group_id})")
            }
        } else {
            format!("{PERUSER}({group_id})")
        }
    }
}
