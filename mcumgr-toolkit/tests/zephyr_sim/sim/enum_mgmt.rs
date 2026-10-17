//! Enumeration management group, modelled after
//! `subsys/mgmt/mcumgr/grp/enum_mgmt/src/enum_mgmt.c`.

use super::Device;
use ciborium::Value;

use super::cbor::{Kind, map, text, uint};
use super::smp::{Ctx, Group, group_id, mgmt_err, read};

const ENUM_MGMT_ID_COUNT: u8 = 0;
const ENUM_MGMT_ID_LIST: u8 = 1;
const ENUM_MGMT_ID_SINGLE: u8 = 2;
const ENUM_MGMT_ID_DETAILS: u8 = 3;

/// `ENUM_MGMT_ERR_TOO_MANY_GROUP_ENTRIES`
pub const ENUM_MGMT_ERR_TOO_MANY_GROUP_ENTRIES: u16 = 2;

/// `ENUM_MGMT_ERR_INDEX_TOO_LARGE`
pub const ENUM_MGMT_ERR_INDEX_TOO_LARGE: u16 = 4;

pub fn group(config: &super::Config) -> Group {
    let mut handlers = vec![
        (ENUM_MGMT_ID_COUNT, read(count)),
        (ENUM_MGMT_ID_LIST, read(list)),
        (ENUM_MGMT_ID_SINGLE, read(single)),
    ];
    if config.enum_details {
        handlers.push((ENUM_MGMT_ID_DETAILS, read(details)));
    }
    Group::new(group_id::ENUM, "enum mgmt", &handlers)
}

/// `enum_mgmt_count`
fn count(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    ctx.put("count", uint(device.groups.len() as u64));
    Ok(())
}

/// `enum_mgmt_list`
fn list(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let groups = device.groups.iter().map(|g| uint(g.id)).collect();
    ctx.put("groups", Value::Array(groups));
    Ok(())
}

/// `enum_mgmt_single`
fn single(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    const MAX_MCUMGR_GROUPS: u64 = 65535;

    let decoded = ctx
        .decode(&[("index", Kind::U32)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let index = decoded.uint("index").unwrap_or(0);
    if index > MAX_MCUMGR_GROUPS {
        return Err(mgmt_err::EINVAL);
    }

    match device.groups.get(index as usize) {
        None => ctx.add_cmd_err(group_id::ENUM, ENUM_MGMT_ERR_INDEX_TOO_LARGE),
        Some(group) => {
            ctx.put("group", uint(group.id));
            if index as usize == device.groups.len() - 1 {
                ctx.put("end", Value::Bool(true));
            }
        }
    }
    Ok(())
}

/// `enum_mgmt_details`
fn details(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("groups", Kind::U32List)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let allowed = decoded.list("groups").unwrap_or_default();

    if device
        .config
        .enum_details_buffer_stack_entries
        .is_some_and(|max| allowed.len() > max)
    {
        ctx.add_cmd_err(group_id::ENUM, ENUM_MGMT_ERR_TOO_MANY_GROUP_ENTRIES);
        return Ok(());
    }

    let groups = device
        .groups
        .iter()
        .filter(|g| allowed.is_empty() || allowed.contains(&g.id.into()))
        .map(|g| {
            let mut entry = vec![("group", uint(g.id))];
            if device.config.enum_details_name {
                entry.push(("name", text(g.name)));
            }
            if device.config.enum_details_handlers {
                entry.push(("handlers", uint(g.handlers.len() as u64)));
            }
            map(entry)
        })
        .collect();
    ctx.put("groups", Value::Array(groups));
    Ok(())
}
