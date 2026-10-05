//! Statistics management group, modelled after
//! `subsys/mgmt/mcumgr/grp/stat_mgmt/src/stat_mgmt.c`.

use super::Device;
use ciborium::Value;

use super::cbor::{Kind, int, text, uint};
use super::smp::{Ctx, Group, group_id, mgmt_err, read};

const STAT_MGMT_ID_SHOW: u8 = 0;
const STAT_MGMT_ID_LIST: u8 = 1;

/// `STAT_MGMT_ERR_INVALID_STAT_NAME`
pub const STAT_MGMT_ERR_INVALID_STAT_NAME: u16 = 3;

/// A group registered with `STATS_INIT_AND_REG`
#[derive(Clone, Debug)]
pub struct StatGroup {
    pub name: String,
    /// Field names (`CONFIG_STATS_NAMES`) and values
    pub fields: Vec<(String, u64)>,
}

#[derive(Clone, Debug)]
pub struct StatState {
    pub groups: Vec<StatGroup>,
}

impl Default for StatState {
    fn default() -> Self {
        Self {
            groups: vec![
                StatGroup {
                    name: "smp_svr_stats".into(),
                    fields: vec![("ticks".into(), 1234)],
                },
                StatGroup {
                    name: "ble_ll".into(),
                    fields: vec![
                        ("rx_pdu".into(), 17),
                        ("tx_pdu".into(), 19),
                        ("crc_err".into(), 0),
                    ],
                },
            ],
        }
    }
}

pub fn group() -> Group {
    Group::new(
        group_id::STAT,
        "stat mgmt",
        &[
            (STAT_MGMT_ID_SHOW, read(show)),
            (STAT_MGMT_ID_LIST, read(list)),
        ],
    )
}

/// `stat_mgmt_show`
fn show(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    let decoded = ctx
        .decode(&[("name", Kind::Tstr)])
        .map_err(|_| mgmt_err::EINVAL)?;
    let name = decoded.str("name").unwrap_or("");
    if name.is_empty() || name.len() >= device.config.stat_max_name_len {
        return Err(mgmt_err::EINVAL);
    }

    let Some(group) = device.stat.groups.iter().find(|g| g.name == name) else {
        ctx.add_cmd_err(group_id::STAT, STAT_MGMT_ERR_INVALID_STAT_NAME);
        return Ok(());
    };

    if device.config.smp_legacy_rc_behaviour {
        ctx.put("rc", int(0));
    }
    ctx.put("name", text(name));
    ctx.put(
        "fields",
        Value::Map(
            group
                .fields
                .iter()
                // stat_mgmt_cb_encode() uses zcbor_uint32_put()
                .map(|(name, value)| (text(name), uint(*value as u32 as u64)))
                .collect(),
        ),
    );
    Ok(())
}

/// `stat_mgmt_list`
fn list(device: &mut Device, ctx: &mut Ctx) -> Result<(), i32> {
    if device.config.smp_legacy_rc_behaviour {
        ctx.put("rc", int(0));
    }
    ctx.put(
        "stat_list",
        Value::Array(device.stat.groups.iter().map(|g| text(&g.name)).collect()),
    );
    Ok(())
}
