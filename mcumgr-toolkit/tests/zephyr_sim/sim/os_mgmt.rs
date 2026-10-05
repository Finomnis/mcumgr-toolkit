//! OS management group, modelled after
//! `subsys/mgmt/mcumgr/grp/os_mgmt/src/os_mgmt.c`.

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};

use super::cbor::{Cbor, Kind};
use super::smp::{Ctx, Group, group_id, mgmt_err, read, read_write, write};
use super::{Bootloader, Device, TaskstatName};

const OS_MGMT_ID_ECHO: u8 = 0;
const OS_MGMT_ID_TASKSTAT: u8 = 2;
const OS_MGMT_ID_MPSTAT: u8 = 3;
const OS_MGMT_ID_DATETIME_STR: u8 = 4;
const OS_MGMT_ID_RESET: u8 = 5;
const OS_MGMT_ID_MCUMGR_PARAMS: u8 = 6;
const OS_MGMT_ID_INFO: u8 = 7;
const OS_MGMT_ID_BOOTLOADER_INFO: u8 = 8;

/// `enum os_mgmt_err_code_t`
pub mod os_mgmt_err {
    pub const INVALID_FORMAT: u16 = 2;
    pub const QUERY_YIELDS_NO_ANSWER: u16 = 3;
    pub const RTC_NOT_SET: u16 = 4;
    pub const RTC_COMMAND_FAILED: u16 = 5;
}

/// A thread as reported by `k_thread_foreach`
#[derive(Clone, Debug)]
pub struct Thread {
    pub name: String,
    pub prio: i8,
    pub state: u8,
    /// `thread->stack_info.size`, in bytes
    pub stack_size: u32,
    /// Bytes of stack in use
    pub stack_used: u32,
    /// Execution cycles (`CONFIG_SCHED_THREAD_USAGE`)
    pub execution_cycles: u64,
}

/// A heap as reported by `sys_heap_runtime_stats_get`
#[derive(Clone, Debug)]
pub struct Heap {
    pub allocated_bytes: u32,
    pub free_bytes: u32,
    pub max_allocated_bytes: u32,
}

/// What a reset request decoded to, i.e. what the reset hook would see
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetRequest {
    pub force: bool,
    pub boot_mode: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct OsState {
    pub threads: Vec<Thread>,
    pub heaps: Vec<Heap>,
    /// `None` makes `rtc_get_time` fail with `-ENODATA`
    pub rtc: Option<NaiveDateTime>,
    /// Makes `rtc_set_time`/`rtc_get_time` fail with an I/O error
    pub rtc_broken: bool,
    /// Every reset request that was accepted, in order
    pub resets: Vec<ResetRequest>,
    /// The boot mode set through the retention boot mode API
    pub boot_mode: Option<u8>,
}

impl Default for OsState {
    fn default() -> Self {
        Self {
            threads: vec![
                Thread {
                    name: "idle".into(),
                    prio: 15,
                    state: 0,
                    stack_size: 320,
                    stack_used: 64,
                    execution_cycles: 123_456_789,
                },
                Thread {
                    name: "main".into(),
                    prio: 0,
                    state: 0x04,
                    stack_size: 4096,
                    stack_used: 1536,
                    execution_cycles: 42_000,
                },
                Thread {
                    name: "sysworkq".into(),
                    prio: -1,
                    state: 0x02,
                    stack_size: 2048,
                    stack_used: 512,
                    execution_cycles: 1_000,
                },
            ],
            heaps: vec![
                Heap {
                    allocated_bytes: 1024,
                    free_bytes: 3072,
                    max_allocated_bytes: 2048,
                },
                Heap {
                    allocated_bytes: 0,
                    free_bytes: 512,
                    max_allocated_bytes: 128,
                },
            ],
            rtc: None,
            rtc_broken: false,
            resets: vec![],
            boot_mode: None,
        }
    }
}

pub fn group(config: &super::Config) -> Group {
    let mut handlers = vec![
        (OS_MGMT_ID_ECHO, read_write(echo, echo)),
        (OS_MGMT_ID_TASKSTAT, read(taskstat_read)),
        (OS_MGMT_ID_MPSTAT, read(mpstat_read)),
        (
            OS_MGMT_ID_DATETIME_STR,
            read_write(datetime_read, datetime_write),
        ),
        (OS_MGMT_ID_RESET, write(reset)),
        (OS_MGMT_ID_MCUMGR_PARAMS, read(mcumgr_params)),
        (OS_MGMT_ID_INFO, read(info)),
    ];
    if config.os_bootloader_info {
        handlers.push((OS_MGMT_ID_BOOTLOADER_INFO, read(bootloader_info)));
    }
    Group::new(group_id::OS, "os mgmt", &handlers)
}

/// `os_mgmt_echo`
fn echo(_: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("d", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    if decoded.matched() == 0 {
        return Err(mgmt_err::EINVAL);
    }
    let data = decoded.str("d").unwrap().to_string();
    ctx.put("r", Cbor::Text(data));
    Ok(())
}

/// `os_mgmt_taskstat_read`
fn taskstat_read(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let config = &device.config;
    let mut tasks = vec![];

    for (idx, thread) in device.os.threads.iter().enumerate() {
        let name = match config.os_taskstat_name {
            TaskstatName::Name => thread.name.clone(),
            TaskstatName::Priority => thread.prio.to_string(),
            TaskstatName::Index => idx.to_string(),
        };

        let mut entry = vec![];
        entry.push((
            "prio",
            if config.os_taskstat_signed_priority {
                Cbor::Int(thread.prio.into())
            } else {
                Cbor::Uint((thread.prio as u8).into())
            },
        ));
        entry.push(("tid", Cbor::Uint(idx as u64)));
        entry.push(("state", Cbor::Uint(thread.state.into())));
        if config.os_taskstat_stack_info {
            // Zephyr reports the stack in units of 4 byte words
            entry.push(("stksiz", Cbor::Uint((thread.stack_size / 4).into())));
            entry.push(("stkuse", Cbor::Uint((thread.stack_used / 4).into())));
        }
        if config.sched_thread_usage {
            entry.push(("runtime", Cbor::Uint(thread.execution_cycles)));
        } else if !config.os_taskstat_only_supported_stats {
            entry.push(("runtime", Cbor::Uint(0)));
        }
        if !config.os_taskstat_only_supported_stats {
            entry.push(("cswcnt", Cbor::Uint(0)));
            entry.push(("last_checkin", Cbor::Uint(0)));
            entry.push(("next_checkin", Cbor::Uint(0)));
        }

        tasks.push((Cbor::Text(name), Cbor::map(entry)));
    }

    ctx.put("tasks", Cbor::Map(tasks));
    Ok(())
}

/// `os_mgmt_mpstat_read`
fn mpstat_read(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let mut pools = vec![];
    for (i, heap) in device.os.heaps.iter().enumerate() {
        let total = heap.allocated_bytes + heap.free_bytes;
        let mut entry = vec![];
        if !device.config.os_mpstat_only_supported_stats {
            entry.push(("blksiz", Cbor::Uint(1)));
        }
        entry.push(("nblks", Cbor::Uint(total.into())));
        entry.push(("nfree", Cbor::Uint(heap.free_bytes.into())));
        entry.push(("min", Cbor::Uint((total - heap.max_allocated_bytes).into())));
        pools.push((Cbor::Text(i.to_string()), Cbor::map(entry)));
    }
    ctx.put("mpools", Cbor::Map(pools));
    Ok(())
}

/// `os_mgmt_reset`
fn reset(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let mut request = ResetRequest {
        force: false,
        boot_mode: None,
    };

    // "Since this is a core command, if we fail to decode the data, ignore
    // the error and continue with the default parameters."
    let mut spec = vec![];
    if device.config.os_reset_hook {
        spec.push(("force", Kind::Bool));
    }
    if device.config.os_reset_boot_mode {
        spec.push(("boot_mode", Kind::U32));
    }
    if !spec.is_empty() {
        if let Ok(decoded) = ctx.decode(&spec) {
            request.force = decoded.bool("force").unwrap_or(false);
            if let Some(boot_mode) = decoded.uint("boot_mode") {
                request.boot_mode = Some(u8::try_from(boot_mode).map_err(|_| mgmt_err::EINVAL)?);
            }
        }
    }

    if request.boot_mode.is_some() {
        // bootmode_set()
        device.os.boot_mode = request.boot_mode;
    }
    device.os.resets.push(request);
    // The actual reboot happens CONFIG_MCUMGR_GRP_OS_RESET_MS after the
    // response has been sent.
    device.reboot_pending = true;
    Ok(())
}

/// `os_mgmt_mcumgr_params`
fn mcumgr_params(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    ctx.put(
        "buf_size",
        Cbor::Uint(device.config.mcumgr_transport_netbuf_size as u64),
    );
    ctx.put(
        "buf_count",
        Cbor::Uint(device.config.mcumgr_transport_netbuf_count.into()),
    );
    Ok(())
}

/// `os_mgmt_bootloader_info`
fn bootloader_info(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("query", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;

    let mut has_output = false;
    if let Bootloader::Mcuboot { mode, no_downgrade } = device.config.bootloader {
        if decoded.matched() == 0 {
            ctx.put("bootloader", Cbor::text("MCUboot"));
            has_output = true;
        } else if decoded.str("query") == Some("mode") {
            ctx.put("mode", Cbor::Int(mode.into()));
            if no_downgrade {
                ctx.put("no-downgrade", Cbor::Bool(true));
            }
            has_output = true;
        }
    }

    if !has_output {
        ctx.add_cmd_err(group_id::OS, os_mgmt_err::QUERY_YIELDS_NO_ANSWER);
    }
    Ok(())
}

/// `os_mgmt_info`
fn info(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    const KERNEL_NAME: u32 = 1 << 0;
    const NODE_NAME: u32 = 1 << 1;
    const KERNEL_RELEASE: u32 = 1 << 2;
    const KERNEL_VERSION: u32 = 1 << 3;
    const BUILD_DATE_TIME: u32 = 1 << 4;
    const MACHINE: u32 = 1 << 5;
    const PROCESSOR: u32 = 1 << 6;
    const HARDWARE_PLATFORM: u32 = 1 << 7;
    const OPERATING_SYSTEM: u32 = 1 << 8;

    let info = &device.config.os_info;
    let decoded = ctx
        .decode(&[("format", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let format = decoded.str("format").unwrap_or("");

    let mut format_bitmask = 0;
    let mut valid_formats = 0;
    for c in format.bytes() {
        let bit = match c {
            b'a' => {
                format_bitmask = KERNEL_NAME
                    | NODE_NAME
                    | KERNEL_RELEASE
                    | KERNEL_VERSION
                    | if info.build_date_time.is_some() {
                        BUILD_DATE_TIME
                    } else {
                        0
                    }
                    | MACHINE
                    | PROCESSOR
                    | HARDWARE_PLATFORM
                    | OPERATING_SYSTEM;
                0
            }
            b's' => KERNEL_NAME,
            b'n' => NODE_NAME,
            b'r' => KERNEL_RELEASE,
            b'v' => KERNEL_VERSION,
            b'b' if info.build_date_time.is_some() => BUILD_DATE_TIME,
            b'm' => MACHINE,
            b'p' => PROCESSOR,
            b'i' => HARDWARE_PLATFORM,
            b'o' => OPERATING_SYSTEM,
            _ => continue,
        };
        format_bitmask |= bit;
        valid_formats += 1;
    }

    if valid_formats != format.len() {
        ctx.add_cmd_err(group_id::OS, os_mgmt_err::INVALID_FORMAT);
        return Ok(());
    } else if format_bitmask == 0 {
        format_bitmask = KERNEL_NAME;
    }

    let parts = [
        (KERNEL_NAME, Some("Zephyr")),
        (
            NODE_NAME,
            Some(info.node_name.as_deref().unwrap_or("unknown")),
        ),
        (
            KERNEL_RELEASE,
            Some(info.build_version.as_deref().unwrap_or("unknown")),
        ),
        (KERNEL_VERSION, Some(info.kernel_version.as_str())),
        (BUILD_DATE_TIME, info.build_date_time.as_deref()),
        (MACHINE, Some(info.arch.as_str())),
        (PROCESSOR, Some(info.processor.as_str())),
        (HARDWARE_PLATFORM, Some(info.board_target.as_str())),
        (OPERATING_SYSTEM, Some("Zephyr")),
    ];
    let output = parts
        .iter()
        .filter(|(bit, _)| format_bitmask & bit != 0)
        .filter_map(|(_, text)| *text)
        .collect::<Vec<_>>()
        .join(" ");

    if output.len() >= device.config.os_info_max_response_size {
        return Err(mgmt_err::EMSGSIZE);
    }

    ctx.put("output", Cbor::Text(output));
    Ok(())
}

/// `os_mgmt_datetime_read`
fn datetime_read(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let time = match device.os.rtc {
        _ if device.os.rtc_broken => {
            ctx.add_cmd_err(group_id::OS, os_mgmt_err::RTC_COMMAND_FAILED);
            return Ok(());
        }
        None => {
            ctx.add_cmd_err(group_id::OS, os_mgmt_err::RTC_NOT_SET);
            return Ok(());
        }
        Some(time) => time,
    };

    let mut text = format!(
        "{:4}-{:02}-{:02}T{:02}:{:02}:{:02}",
        time.year(),
        time.month(),
        time.day(),
        time.hour(),
        time.minute(),
        time.second()
    );
    if device.config.os_datetime_ms {
        text += &format!(".{:03}", time.nanosecond() / 1_000_000);
    }

    ctx.put("datetime", Cbor::Text(text));
    Ok(())
}

/// `strtol(pos, &new_pos, 10)` on a NUL terminated string
fn strtol(s: &[u8], pos: usize) -> (i64, usize) {
    let mut i = pos;
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut negative = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        negative = s[i] == b'-';
        i += 1;
    }
    let digits_start = i;
    let mut value: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add((s[i] - b'0').into());
        i += 1;
    }
    if i == digits_start {
        // No conversion performed
        return (0, pos);
    }
    (if negative { -value } else { value }, i)
}

/// `os_mgmt_datetime_write`
fn datetime_write(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    const MIN_LEN: usize = 19;
    const MAX_LEN: usize = 31;
    // (min, max) per field: year, month, day, hour, minute, second
    const LIMITS: [(i64, i64); 6] = [(1900, 11899), (1, 12), (1, 31), (0, 23), (0, 59), (0, 59)];

    let decoded = ctx
        .decode(&[("datetime", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let datetime = decoded.str("datetime").unwrap_or("").as_bytes();
    if datetime.len() < MIN_LEN || datetime.len() > MAX_LEN {
        return Err(mgmt_err::EINVAL);
    }

    // NUL terminated copy, as in the C code
    let mut s = datetime.to_vec();
    s.push(0);
    let end = datetime.len();

    let mut values = [0i64; 6];
    let mut pos = 0;
    for (value, (min, max)) in values.iter_mut().zip(LIMITS) {
        if pos == end {
            return Err(mgmt_err::EINVAL);
        }
        let (parsed, new_pos) = strtol(&s[..end], pos);
        if new_pos == pos {
            return Err(mgmt_err::EINVAL);
        }
        if parsed < min || parsed > max {
            return Err(mgmt_err::EINVAL);
        }
        *value = parsed;
        pos = new_pos + 1;
    }

    let mut msec = 0;
    if device.config.os_datetime_ms && s[pos - 1] == b'.' {
        let mut mul = 100;
        while pos < end && s[pos].is_ascii_digit() && mul >= 1 {
            msec += u32::from(s[pos] - b'0') * mul;
            mul /= 10;
            pos += 1;
        }
    }

    // rtc_set_time() rejects dates that do not exist
    let new_time = NaiveDate::from_ymd_opt(values[0] as i32, values[1] as u32, values[2] as u32)
        .and_then(|date| {
            date.and_hms_milli_opt(values[3] as u32, values[4] as u32, values[5] as u32, msec)
        });

    match new_time {
        Some(time) if !device.os.rtc_broken => device.os.rtc = Some(time),
        _ => ctx.add_cmd_err(group_id::OS, os_mgmt_err::RTC_COMMAND_FAILED),
    }
    Ok(())
}
