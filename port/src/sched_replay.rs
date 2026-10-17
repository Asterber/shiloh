//! Scheduler block replay (C26): drive [`Blocks`] with the media-state switches of a trace and compare the
//! counter changes it makes with the original's traced counter raise / lower calls, then check every traced
//! dispatch against the replayed matrix (a dispatch through a raised cell would refute C26).
//!
//! Inputs: `blk_reset`; `blk_state` (state) plus whether the media is "video kind" and the extra-cell test — those two
//! rules are unknown, so they are read off the trace (video = counter calls follow from the media-state switch,
//! FUN_0044f860; extra cell, `f090` = the (0,0x1b) cell is among them). Predicted: which cells, their order, +/−, the
//! same-state early return, balance. Command type / target kind of a dispatch: a static table
//! of the research notes (`vt_kinds.json`, not in this repository).

use crate::world::sched::{kind, Blocks, CellOp};
use serde_json::Value as J;
use std::collections::HashMap;
use std::ops::Not;

#[derive(Debug, Default)]
pub struct BlockReport {
    pub switches: usize,
    pub ops_trace: usize,
    pub ops_core: usize,
    pub ops_matched: usize,
    pub ops_bad: Vec<String>,
    pub dispatches: usize,
    pub no_target: usize,
    pub unresolved: usize,
    pub through_raised: Vec<String>,
    pub h51_ok: usize,
    pub h51_bad: Vec<String>,
    pub negative: bool,
    pub zero_at_end: bool,
}

fn hexs(v: &J) -> Option<u32> {
    u32::from_str_radix(v.as_str()?.trim_start_matches("0x"), 16).ok()
}

/// Is the record's return address inside the media-state switch (the range below)?
fn from_media_switch(r: &J) -> bool {
    hexs(&r["ret"]).is_some_and(|a| (0x0044_f860..0x0044_f92e).contains(&a))
}

/// `h51`: also predict the media inputs from the HDB (H51) and count agreement with the trace-derived ones.
pub fn replay_blocks(trace: &str, vt_kinds: &str, h51: Option<&crate::hdb::Hdb>) -> Result<BlockReport, String> {
    let tab: J = serde_json::from_str(vt_kinds).map_err(|e| e.to_string())?;
    let map = |k: &str| -> HashMap<String, Option<u32>> {
        tab[k].as_object().map(|o| o.iter().map(|(a, b)| (a.clone(), b.as_u64().map(|x| x as u32))).collect())
            .unwrap_or_default()
    };
    let (cmd_t, tgt_k) = (map("cmd_type"), map("target_kind"));
    let recs: Vec<J> = trace.lines().filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    let mut rep = BlockReport::default();
    let mut b = Blocks::default();
    for (i, r) in recs.iter().enumerate() {
        match r["fn"].as_str().unwrap_or("") {
            "blk_reset" => b.reset(),
            "blk_state" => {
                rep.switches += 1;
                let state = r["state"].as_u64().unwrap_or(0) as u32;
                // the original's counter calls made by this switch (they follow it directly)
                let traced: Vec<CellOp> = recs[i + 1..].iter()
                    .take_while(|x| matches!(x["fn"].as_str(), Some("blk_inc" | "blk_dec")) && from_media_switch(x))
                    .map(|x| CellOp {
                        typ: x["type"].as_u64().unwrap_or(0) as u32,
                        kind: x["kind"].as_u64().unwrap_or(0) as u32,
                        inc: x["fn"] == "blk_inc",
                    })
                    .collect();
                let video = !traced.is_empty();
                let f090 = traced.iter().any(|o| o.typ == 0 && o.kind == kind::MEDIA_PLAYER);
                if let Some(h) = h51 {
                    let media = hexs(&r["f28"]["id"]).unwrap_or(0);
                    let pred = crate::world::sched::media_inputs_h51(h, media);
                    // a same-state switch makes no counter calls whatever the media is: compare video only on changes
                    let changes = b.media_state_is(state).not();
                    match pred {
                        Some((v, f)) if (!changes || v == video) && (!video || f == f090) => rep.h51_ok += 1,
                        p => rep.h51_bad.push(format!("t={} media {media:#x}: H51 {p:?}, trace video={video} f090={f090}", r["t"])),
                    }
                }
                let core = b.media(state, video, f090);
                rep.ops_trace += traced.len();
                rep.ops_core += core.len();
                if core == traced {
                    rep.ops_matched += core.len();
                } else {
                    rep.ops_bad.push(format!("t={} state={state}: core {core:?} trace {traced:?}", r["t"]));
                }
                rep.negative |= b.negative();
            }
            "q_dispatch" => {
                rep.dispatches += 1;
                if r["tgt_vt"].is_null() {
                    rep.no_target += 1; // command with target 0 (e.g. the nav-click command, C24 S2)
                    continue;
                }
                let t = r["msg_vt"].as_str().and_then(|v| cmd_t.get(v).copied().flatten());
                let k = r["tgt_vt"].as_str().and_then(|v| tgt_k.get(v).copied().flatten());
                match (t, k) {
                    (Some(t), Some(k)) => {
                        if b.blocked(t, k) {
                            rep.through_raised.push(format!("t={} cmd {t} kind {k}", r["t"]));
                        }
                    }
                    _ => rep.unresolved += 1,
                }
            }
            _ => {}
        }
    }
    rep.zero_at_end = b.is_zero();
    Ok(rep)
}

impl BlockReport {
    pub fn summary(&self) -> String {
        format!(
            "scheduler blocks (C26): media switches {}; counter ops {}/{} (core {}), bad {}; dispatches {} \
             (no target {}, unresolved {}), through a raised cell {}; negative {}; zero at end {}\n",
            self.switches, self.ops_matched, self.ops_trace, self.ops_core, self.ops_bad.len(), self.dispatches,
            self.no_target, self.unresolved, self.through_raised.len(), self.negative, self.zero_at_end
        ) + &if self.h51_ok + self.h51_bad.len() > 0 {
            format!("  H51 media inputs from HDB: {}/{}\n", self.h51_ok, self.h51_ok + self.h51_bad.len())
        } else {
            String::new()
        }
    }
}
