//! A simulated Zephyr device running the MCUmgr SMP server.
//!
//! Everything in here is modelled after the upstream Zephyr sources in
//! `subsys/mgmt/mcumgr` (and MCUboot's `bootutil` for the slot trailers), not
//! after this crate. Each handler names the C function it mirrors so it can be
//! compared against upstream.
//!
//! Reference revisions: zephyrproject-rtos/zephyr@8f62a4ab82b5 (2026-10-04)
//! and mcu-tools/mcuboot@ba2099fa7da4 (2026-10-05).
//!
//! The device is configured through [`Config`], whose fields correspond to the
//! Zephyr Kconfig options of the same name and default to the upstream
//! defaults unless noted otherwise. All management groups are enabled, like in
//! Zephyr's `smp_svr` sample.
//!
//! Limitations: the bootloader is modelled as MCUboot in one of the swap modes
//! (scratch/move/offset). Direct-XIP, RAM-load and firmware-loader modes compile
//! different code paths in Zephyr and are not simulated. Requests that use the
//! original SMP protocol (version 0) are never sent by this crate, so the
//! translation of group errors into legacy `rc` values is not simulated either.

pub mod basic_mgmt;
pub mod cbor;
pub mod enum_mgmt;
pub mod fs_mgmt;
pub mod image;
pub mod img_mgmt;
pub mod os_mgmt;
pub mod serial;
pub mod settings_mgmt;
pub mod shell_mgmt;
pub mod smp;
pub mod stat_mgmt;
pub mod transport;
pub mod udp;

use std::sync::{Arc, Mutex, MutexGuard};

use mcumgr_toolkit::MCUmgrClient;

pub use transport::{Fault, SimTransport};

/// What `CONFIG_MCUMGR_GRP_OS_TASKSTAT_THREAD_NAME_CHOICE` selects
#[derive(Clone, Copy, Debug)]
pub enum TaskstatName {
    /// `CONFIG_MCUMGR_GRP_OS_TASKSTAT_USE_THREAD_NAME_FOR_NAME`
    Name,
    /// `CONFIG_MCUMGR_GRP_OS_TASKSTAT_USE_THREAD_PRIO_FOR_NAME`
    Priority,
    /// `CONFIG_MCUMGR_GRP_OS_TASKSTAT_USE_THREAD_IDX_FOR_NAME`
    Index,
}

/// The bootloader the application was built for
#[derive(Clone, Copy, Debug)]
pub enum Bootloader {
    /// `CONFIG_BOOTLOADER_MCUBOOT`; `mode` is the `MCUBOOT_MODE_*` value,
    /// `no_downgrade` is `CONFIG_MCUBOOT_BOOTLOADER_NO_DOWNGRADE`
    Mcuboot { mode: i32, no_downgrade: bool },
    /// Any other bootloader
    Other,
}

/// Values reported by `os_mgmt_info`
#[derive(Clone, Debug)]
pub struct OsInfo {
    /// `CONFIG_NET_HOSTNAME`/`bt_get_name()`; "unknown" if `None`
    pub node_name: Option<String>,
    /// `BUILD_VERSION`; "unknown" if `None`
    pub build_version: Option<String>,
    /// `KERNEL_VERSION_STRING`
    pub kernel_version: String,
    /// `CONFIG_MCUMGR_GRP_OS_INFO_BUILD_DATE_TIME`
    pub build_date_time: Option<String>,
    /// `CONFIG_ARCH`
    pub arch: String,
    /// `PROCESSOR_NAME`
    pub processor: String,
    /// `CONFIG_BOARD_TARGET`
    pub board_target: String,
}

/// Kconfig options of the simulated device
#[derive(Clone, Debug)]
pub struct Config {
    pub mcumgr_transport_netbuf_size: usize,
    pub mcumgr_transport_netbuf_count: u32,
    pub smp_support_original_protocol: bool,
    pub smp_legacy_rc_behaviour: bool,

    pub mcumgr_grp_os: bool,
    pub mcumgr_grp_img: bool,
    pub mcumgr_grp_stat: bool,
    pub mcumgr_grp_settings: bool,
    pub mcumgr_grp_fs: bool,
    pub mcumgr_grp_shell: bool,
    pub mcumgr_grp_enum: bool,
    pub mcumgr_grp_zephyr_basic: bool,

    pub bootloader: Bootloader,
    pub os_bootloader_info: bool,
    pub os_info: OsInfo,
    pub os_info_max_response_size: usize,
    pub os_taskstat_name: TaskstatName,
    pub os_taskstat_signed_priority: bool,
    pub os_taskstat_stack_info: bool,
    pub os_taskstat_only_supported_stats: bool,
    pub sched_thread_usage: bool,
    pub os_mpstat_only_supported_stats: bool,
    pub os_datetime_ms: bool,
    pub os_reset_hook: bool,
    pub os_reset_boot_mode: bool,

    pub img_updatable_image_number: usize,
    /// Size of every image slot partition
    pub img_slot_size: usize,
    /// Erase block size of the flash
    pub img_flash_page_size: usize,
    /// `CONFIG_IMG_ENABLE_IMAGE_CHECK` (enabled in Zephyr's smp_svr sample)
    pub img_enable_image_check: bool,
    pub img_frugal_list: bool,
    /// `CONFIG_MCUMGR_GRP_IMG_SLOT_INFO` (enabled by default only with
    /// multiple images upstream; enabled here so it can be tested)
    pub img_slot_info: bool,
    /// `CONFIG_MCUBOOT_UPDATE_FOOTER_SIZE` if
    /// `CONFIG_MCUMGR_GRP_IMG_TOO_LARGE_SYSBUILD` is enabled
    pub img_too_large_sysbuild_footer: Option<u64>,
    pub img_allow_erase_pending: bool,
    pub img_allow_confirm_non_active_slot: bool,
    pub img_allow_confirm_non_active_image_secondary: bool,
    pub img_allow_confirm_non_active_image_any: bool,
    pub img_version_cmp_use_build_number: bool,

    pub stat_max_name_len: usize,

    pub settings_name_len: usize,
    pub settings_value_len: usize,

    pub fs_path_len: usize,
    pub fs_max_offset_len: usize,
    pub fs_checksum_ieee_crc32: bool,
    pub fs_hash_sha256: bool,

    pub shell_cmd_buff_size: usize,

    pub enum_details: bool,
    pub enum_details_name: bool,
    pub enum_details_handlers: bool,

    /// Size of `storage_partition`, `None` if there is none
    pub storage_partition_size: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mcumgr_transport_netbuf_size: 384,
            mcumgr_transport_netbuf_count: 4,
            smp_support_original_protocol: true,
            smp_legacy_rc_behaviour: false,

            mcumgr_grp_os: true,
            mcumgr_grp_img: true,
            mcumgr_grp_stat: true,
            mcumgr_grp_settings: true,
            mcumgr_grp_fs: true,
            mcumgr_grp_shell: true,
            mcumgr_grp_enum: true,
            mcumgr_grp_zephyr_basic: true,

            bootloader: Bootloader::Mcuboot {
                // MCUBOOT_MODE_SWAP_USING_MOVE
                mode: 3,
                no_downgrade: false,
            },
            os_bootloader_info: true,
            os_info: OsInfo {
                node_name: None,
                build_version: Some("v4.2.0-1234-gdeadbeef".into()),
                kernel_version: "4.2.99".into(),
                build_date_time: None,
                arch: "arm".into(),
                processor: "cortex-m4".into(),
                board_target: "nrf52840dk/nrf52840".into(),
            },
            os_info_max_response_size: 256,
            os_taskstat_name: TaskstatName::Name,
            os_taskstat_signed_priority: true,
            os_taskstat_stack_info: true,
            os_taskstat_only_supported_stats: true,
            sched_thread_usage: false,
            os_mpstat_only_supported_stats: true,
            os_datetime_ms: false,
            os_reset_hook: false,
            os_reset_boot_mode: false,

            img_updatable_image_number: 1,
            img_slot_size: 64 * 1024,
            img_flash_page_size: 4096,
            img_enable_image_check: true,
            img_frugal_list: false,
            img_slot_info: true,
            img_too_large_sysbuild_footer: None,
            img_allow_erase_pending: true,
            img_allow_confirm_non_active_slot: true,
            img_allow_confirm_non_active_image_secondary: true,
            img_allow_confirm_non_active_image_any: false,
            img_version_cmp_use_build_number: false,

            stat_max_name_len: 32,

            settings_name_len: 32,
            settings_value_len: 32,

            fs_path_len: 64,
            fs_max_offset_len: 3,
            fs_checksum_ieee_crc32: true,
            fs_hash_sha256: true,

            shell_cmd_buff_size: 256,

            enum_details: true,
            enum_details_name: true,
            enum_details_handlers: true,

            storage_partition_size: Some(8 * 1024),
        }
    }
}

/// The complete state of the simulated device
pub struct Device {
    pub config: Config,
    /// Registered management groups, in registration order
    pub groups: Vec<smp::Group>,
    pub os: os_mgmt::OsState,
    pub img: img_mgmt::ImgState,
    pub stat: stat_mgmt::StatState,
    pub settings: settings_mgmt::SettingsState,
    pub fs: fs_mgmt::FsState,
    pub shell: shell_mgmt::ShellState,
    pub storage_partition: Option<Vec<u8>>,
    /// Number of times the device booted
    pub boot_count: u32,
    /// A reset was requested and happens once the response is out
    pub reboot_pending: bool,
    /// Link layer state, shared by all transports to this device
    pub link: transport::LinkState,
}

impl Device {
    pub fn new(config: Config) -> Self {
        // Handlers are registered in the order of their MCUMGR_HANDLER_DEFINE
        // names, as iterable sections are sorted by name.
        let mut groups = vec![];
        if config.mcumgr_grp_enum {
            groups.push(enum_mgmt::group(&config));
        }
        if config.mcumgr_grp_fs {
            groups.push(fs_mgmt::group(&config));
        }
        if config.mcumgr_grp_img {
            groups.push(img_mgmt::group(&config));
        }
        if config.mcumgr_grp_os {
            groups.push(os_mgmt::group(&config));
        }
        if config.mcumgr_grp_settings {
            groups.push(settings_mgmt::group());
        }
        if config.mcumgr_grp_shell {
            groups.push(shell_mgmt::group());
        }
        if config.mcumgr_grp_stat {
            groups.push(stat_mgmt::group());
        }
        if config.mcumgr_grp_zephyr_basic {
            groups.push(basic_mgmt::group());
        }

        Self {
            groups,
            os: Default::default(),
            img: img_mgmt::ImgState::new(config.img_updatable_image_number, config.img_slot_size),
            stat: Default::default(),
            settings: Default::default(),
            fs: Default::default(),
            shell: Default::default(),
            storage_partition: config.storage_partition_size.map(|size| vec![0x5a; size]),
            boot_count: 1,
            reboot_pending: false,
            link: Default::default(),
            config,
        }
    }

    /// Reboots the device: MCUboot runs, RAM state is lost and the
    /// application starts again.
    pub fn reboot(&mut self) {
        self.reboot_pending = false;
        self.boot_count += 1;
        self.mcuboot_boot();
        self.img.reset_upload();
        self.fs.cleanup();
        self.settings.reboot();
    }

    /// Processes a request packet the way the transport receive path does
    /// (`smp_rx_req` -> `smp_process_request_packet`), including a reboot
    /// requested by it.
    pub fn receive_packet(&mut self, packet: &[u8]) -> Vec<Vec<u8>> {
        // Packets that do not fit into a net_buf are dropped by the transport
        if packet.len() > self.config.mcumgr_transport_netbuf_size {
            self.link.dropped_oversized += 1;
            return vec![];
        }
        let responses = self.process_request_packet(packet);
        if self.reboot_pending {
            self.reboot();
        }
        responses
    }

    /// Installs an MCUboot image in the primary slot of `image` as if it was
    /// flashed by a programmer and then confirmed.
    pub fn flash_image(&mut self, image: usize, data: &[u8]) {
        self.img.slots[image * 2] = img_mgmt::Slot::programmed(
            self.config.img_slot_size,
            data,
            img_mgmt::Trailer {
                magic: img_mgmt::Magic::Good,
                image_ok: true,
                copy_done: false,
            },
        );
    }
}

/// A handle to a simulated device, shared with the transports connected to it
#[derive(Clone)]
pub struct SimDevice(Arc<Mutex<Device>>);

impl SimDevice {
    pub fn new(config: Config) -> Self {
        Self(Arc::new(Mutex::new(Device::new(config))))
    }

    /// A device with the default configuration, running firmware `1.2.3`
    pub fn with_firmware() -> Self {
        let device = Self::new(Config::default());
        device
            .lock()
            .flash_image(0, &image::ImageBuilder::new((1, 2, 3, 0)).build());
        device
    }

    pub fn lock(&self) -> MutexGuard<'_, Device> {
        self.0.lock().unwrap()
    }

    /// A datagram transport to this device
    pub fn transport(&self) -> SimTransport {
        SimTransport::new(self.clone())
    }

    /// A client connected through [`MCUmgrClient::new_from_transport`]
    pub fn client(&self) -> MCUmgrClient {
        MCUmgrClient::new_from_transport(self.transport())
    }

    /// A client connected through Zephyr's serial (UART/console) transport
    pub fn serial_client(&self) -> MCUmgrClient {
        MCUmgrClient::new_from_serial(serial::SimSerialPort::new(self.clone()))
    }

    /// Queues faults for the next requests, see [`Fault`]
    pub fn inject_faults(&self, faults: impl IntoIterator<Item = Fault>) {
        self.lock().link.faults.extend(faults);
    }

    /// All requests the device received
    pub fn requests(&self) -> Vec<transport::Request> {
        self.lock().link.requests.clone()
    }

    /// Requests received for one command
    pub fn requests_for(&self, group: u16, id: u8) -> Vec<transport::Request> {
        self.requests()
            .into_iter()
            .filter(|r| r.hdr.group == group && r.hdr.id == id)
            .collect()
    }

    pub fn clear_requests(&self) {
        self.lock().link.requests.clear();
    }
}
