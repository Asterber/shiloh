//! Lock-step replay of the bot traces that end with an in-game save (fo-026, fo-027, fo-028, fo-029, fo-030; every
//! hypothesis on, loaded saves): regression guards for the save-screen rules (RE_NOTES H140..H143).
mod common;
use std::path::PathBuf;
use xf::hdb::Hdb;
use xf::replay::{replay, Report};
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn run(save: &str, name: &str, rules: Rules) -> Option<Report> {
    let data = root().join("data/InstalledGame");
    let save = root().join(format!("data/bot/{save}.x"));
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let trace = common::trace(&format!("{name}.jsonl"))?;
    Some(replay(&hdb, &gam, &trace, rules))
}

const SAVES: [(&str, &str); 5] = [
    ("fo-2c9-door", "fo-026"),
    ("fo-2c9-door", "fo-027"),
    ("fo-2c9-door", "fo-028"),
    ("fo-028-end", "fo-029"),
    ("fo-029-end", "fo-030"),
];

/// H140 (Esc menu "Save" → FUN_00515b50: engine type-5 show of 0x14269), H141 (game start drops the load screen silently, so
/// the save screen runs in save mode), H142 (SaveLoadInit / SaveSlotSelect / SaveKeyPress text assets of the slot items
/// 0x1427c..e), H143 (SaveLoadComit: the engine hides). Every save-screen registration, Activation / Deactivation, engine
/// dispatch and execution of the five traces is predicted, 0 extra; left unpredicted: the "overwrite?" box (null-owner REG,
/// fo-027 / fo-029 / fo-030). (The startup cursor double hit at ~9 s is dropped from the inputs by H144.)
#[test]
fn h140_h143_save_screen() {
    for (save, name) in SAVES {
        let Some(r) = run(save, name, Rules::all()) else { return };
        assert!(r.dispatch_bad.is_empty() && r.execs.missing.is_empty(), "{name}: {:?} {:?}", r.dispatch_bad, r.execs.missing);
        let extra = r.regs.extra.len()
            + r.lifecycle_iface.extra.len()
            + r.lifecycle_cursor.extra.len()
            + r.lifecycle_other.extra.len()
            + r.hide_order.extra.len()
            + r.execs.extra.len();
        assert_eq!(extra, 0, "{name}");
        assert_eq!(r.lifecycle_iface.matched, r.lifecycle_iface.trace, "{name}");
        assert_eq!(r.lifecycle_cursor.matched, r.lifecycle_cursor.trace, "{name}");
        assert_eq!(r.hide_order.matched, r.hide_order.trace, "{name}");
        let boxed = matches!(name, "fo-027" | "fo-029" | "fo-030");
        assert_eq!(r.regs.trace - r.regs.matched, boxed as usize, "{name}: {:?}", r.regs.missing);
        assert!(r.unimplemented.iter().all(|x| !x.1.contains("Save")), "{name}");
        // ablation: without the four rules the save screen is not predicted (fo-030: 4 dispatches, 31 registrations)
        let off = Rules {
            h140_save_screen_open: false,
            h141_game_start_layouts: false,
            h142_save_slot_text: false,
            h143_save_comit: false,
            ..Rules::all()
        };
        let o = run(save, name, off).unwrap();
        assert!(o.regs.matched < r.regs.matched && !o.dispatch_bad.is_empty(), "{name}");
    }
    let r = run("fo-029-end", "fo-030", Rules { h141_game_start_layouts: false, ..Rules::all() }).unwrap();
    // H141 off: the load screen 0x142ca stays "shown" → SaveLoadInit / SaveSlotSelect / SaveKeyPress run in load mode
    assert!(r.regs.trace - r.regs.matched > 10);
}

/// H144: every trace recorded with the current tracer (fo-014 .. fo-030 except fo-015 / fo-016 / fo-024) has exactly one
/// trig_register repeated with the same scope, owner, TL and return address and nothing traced in between — always in the
/// startup cursor loop (~7–9 s), a different cursor each run (0x11d / 0x11f / 0x120 / 0x122): a tracer
/// double hit. Dropped from the inputs, the trace's registrations are then all predicted (0 extra) on the Load traces.
#[test]
fn h144_tracer_double_hit() {
    for (save, name, owner) in [("fo-2b3-user", "fo-019", "0x11d"), ("fo-2c9-door", "fo-025", "0x122"), ("fo-029-end", "fo-030", "0x11d")] {
        let Some(r) = run(save, name, Rules::all()) else { return };
        assert_eq!(r.tracer_doubles.len(), 1, "{name}");
        assert_eq!(r.tracer_doubles[0].1, format!("REG {owner} repeated"), "{name}");
        assert!(r.regs.missing.iter().all(|x| x.0 > 20.0), "{name}: {:?}", r.regs.missing);
    }
}

/// H145: media ends as trace input (the media end's obj_remove + blk_state f28 records): the AssetRefs the core
/// played get their Deactivation — fo-026 12 (0x127a4, 0x57f4 / 0x57f6, 0x4d24, 0x5071, 0x5077, 0x8c96 ×3, 0x8c9a ×3),
/// fo-030 2 (0x127a4, 0xfc91); fo-025's stopped looping 0xcc41 (H100 mode 3) is not deactivated twice. Ablation: the
/// misses come back.
#[test]
fn h145_media_end_input() {
    for (save, name, n) in [("fo-2c9-door", "fo-025", 3), ("fo-2c9-door", "fo-026", 12), ("fo-029-end", "fo-030", 2)] {
        let Some(r) = run(save, name, Rules::all()) else { return };
        assert_eq!(r.media_ends_used, if name == "fo-025" { 2 } else { n }, "{name}");
        let o = &r.lifecycle_other;
        assert_eq!((o.matched, o.extra.len()), (o.trace, 0), "{name}: {:?}", o.missing);
        let off = run(save, name, Rules { h145_media_end_input: false, ..Rules::all() }).unwrap();
        let k = if name == "fo-025" { 2 } else { n };
        assert_eq!(off.lifecycle_other.trace - off.lifecycle_other.matched, k, "{name}");
    }
}

/// H146 (StopAllTimers: no running timer in the core at fo-027 446.0 / fo-028 444.9 / fo-030 356.8 → no engine stop
/// message, as traced), H147 (RandGen[0x5c7, 2]: var 0x5c7 unknown → fo-027 / fo-028's 0xcc8c condition not evaluable
/// instead of a wrong / lucky value), H148 (SetIsNode3Done / SetIsNode6Done write 0 to 0x800 / 0x805: the save fo-030-end.x
/// holds 0), H149 (DispEvIcons no effect): no unimplemented case, no condition mismatch, no dispatch mismatch left.
#[test]
fn h146_h149_native_procs() {
    for (save, name, unknown) in [("fo-2c9-door", "fo-027", 1), ("fo-2c9-door", "fo-028", 1), ("fo-028-end", "fo-029", 0), ("fo-029-end", "fo-030", 0)] {
        let Some(r) = run(save, name, Rules::all()) else { return };
        assert!(r.unimplemented.is_empty(), "{name}: {:?}", r.unimplemented);
        assert!(r.cond_bad.is_empty() && r.dispatch_bad.is_empty(), "{name}");
        assert_eq!(r.cond_unknown, unknown, "{name}");
        let off = Rules { h146_stop_all_timers: false, h147_rand_gen: false, h148_node_done: false, h149_disp_ev_icons: false, ..Rules::all() };
        let o = run(save, name, off).unwrap();
        assert!(!o.unimplemented.is_empty(), "{name}");
    }
}

/// H144 (b): fo-012 has no `load` group; the C30 gate hits are inferred from the Activation commands (q_dispatch vt
/// 0x654830 on a not-yet-active object, fire_post 8 before the next dispatch = passed). With them fo-012 replays like
/// fo-013 (the same run with the gate traced): every registration, post, dispatch, hotspot and lifecycle event, 0 extra.
#[test]
fn h144_gate_inference_fo012() {
    let Some(r) = run("fo-2b3-user", "fo-012", Rules::all()) else { return };
    assert_eq!((r.regs.matched, r.regs.trace, r.regs.extra.len()), (82, 82, 0));
    assert_eq!((r.posts.matched, r.posts.trace, r.dispatch_ok, r.dispatch_total), (56, 56, 58, 58));
    assert_eq!((r.hotspots.matched, r.hotspots.trace, r.lifecycle.matched, r.lifecycle.extra.len()), (4, 4, 6, 0));
    assert_eq!((r.lifecycle_other.extra.len(), r.lifecycle_iface.extra.len()), (0, 0));
    assert_eq!((r.gate_ok, r.gate_total), (81, 81));
}

/// H160: the dialogue box's state commands (q_dispatch vt 0x655e58 on vt 0x655d98, value untraced) — fo-033: Load (state
/// 1), line click (0), end of the line (1, 184.00); fo-027 / fo-028 DisplayDBox (1); fo-029 Load (1) + CharView removal (0);
/// fo-034 Load (1) + close box (0). H145 covers the line's players (Deact 0x4e75 / 0x4e77). Ablation: fo-033's third
/// command is missing.
#[test]
fn h160_dbox_reshow() {
    for (save, name, n) in
        [("fo-028-end", "fo-033", 3), ("fo-2c9-door", "fo-027", 1), ("fo-2c9-door", "fo-028", 1), ("fo-028-end", "fo-029", 2), ("fo-028-end", "fo-034", 2)]
    {
        let Some(r) = run(save, name, Rules::all()) else { return };
        let d = &r.dbox_states;
        assert_eq!((d.matched, d.trace, d.core), (n, n, n), "{name}");
        if name == "fo-033" {
            assert_eq!(r.media_ends_used, 2);
            let o = &r.lifecycle_other;
            assert_eq!((o.matched, o.extra.len()), (o.trace, 0), "{:?}", o.missing);
            let off = run(save, name, Rules { h160_dbox_reshow: false, ..Rules::all() }).unwrap();
            assert_eq!((off.dbox_states.matched, off.dbox_states.trace), (2, 3));
        }
    }
}
