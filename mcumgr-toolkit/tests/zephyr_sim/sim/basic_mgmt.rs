//! Zephyr basic management group, modelled after
//! `subsys/mgmt/mcumgr/grp/zephyr_basic/src/basic_mgmt.c`.

use super::Device;
use super::smp::{Ctx, Group, group_id, write};

const ZEPHYR_MGMT_GRP_BASIC_CMD_ERASE_STORAGE: u8 = 0;

/// `ZEPHYRBASIC_MGMT_ERR_FLASH_OPEN_FAILED`
pub const ZEPHYRBASIC_MGMT_ERR_FLASH_OPEN_FAILED: u16 = 2;

pub fn group() -> Group {
    Group::new(
        group_id::ZEPHYR_BASIC,
        "zephyr basic mgmt",
        &[(
            ZEPHYR_MGMT_GRP_BASIC_CMD_ERASE_STORAGE,
            write(storage_erase),
        )],
    )
}

/// `storage_erase_handler`
fn storage_erase(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    match &mut device.storage_partition {
        // flash_area_flatten(fa, 0, fa->fa_size)
        Some(partition) => partition.fill(0xff),
        None => ctx.add_cmd_err(
            group_id::ZEPHYR_BASIC,
            ZEPHYRBASIC_MGMT_ERR_FLASH_OPEN_FAILED,
        ),
    }
    Ok(())
}
