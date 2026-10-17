//! Regression guard: lock-step replay of fo-005 / fo-006 (needs data/InstalledGame, not in git; skipped
//! when absent). Numbers are the ones recorded in RE_NOTES (port replay section); every remaining
//! discrepancy there is explained (unknown native procedure PDAEscape, VCTitle TL rule, …).
mod common;
use std::path::PathBuf;
use xf::hdb::Hdb;
use xf::replay::{parse_trace, replay, Report};
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn run(name: &str, rules: Rules) -> Option<Report> {
    let data = root().join("data/InstalledGame");
    if !data.join("XFiles.hdb").exists() {
        eprintln!("skipping: {} not present", data.display());
        return None;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(data.join("XFiles.gam")).unwrap();
    let text = common::trace_text(name)?;
    Some(replay(&hdb, &gam, &parse_trace(&text).unwrap(), rules))
}

fn check(r: &Report) {
    assert_eq!((r.reg_tl_ok, r.reg_total, r.reg_tl_bad.len()), (94, 94, 0)); // all owners have a TL rule (C20, H56 VCTitle, H57 Node/EvidenceIcon/Cursor/IFaceLayout)
    assert_eq!((r.posts.matched, r.posts.trace, r.posts.core), (214, 214, 214));
    assert_eq!((r.dispatch_ok, r.dispatch_total), (214, 214));
    assert_eq!((r.cond_ok, r.cond_bad.len(), r.cond_unknown), (199, 0, 15));
    assert_eq!((r.execs.matched, r.execs.trace, r.execs.core), (157, 157, 157));
    assert_eq!((r.transitions.matched, r.transitions.trace, r.transitions.core), (6, 6, 6));
    assert_eq!((r.hotspots.matched, r.hotspots.trace, r.hotspots.core), (16, 16, 16));
    assert_eq!((r.lifecycle.matched, r.lifecycle.trace, r.lifecycle.core), (50, 50, 50));
    assert_eq!(r.regs.extra.len(), 0);
    assert_eq!(r.lifecycle_other.extra.len(), 0); // Act/Deact of Node/Location/AssetRef: never an extra one
}

#[test]
fn fo005_confirmed_rules() {
    if let Some(r) = run("fo-005.jsonl", Rules::default()) {
        check(&r);
        assert_eq!(r.regs.matched, 28);
    }
}

#[test]
fn fo006_confirmed_rules() {
    if let Some(r) = run("fo-006.jsonl", Rules::default()) {
        check(&r);
        assert_eq!(r.regs.matched, 28);
    }
}

#[test]
fn fo006_with_h48() {
    if let Some(r) = run("fo-006.jsonl", Rules { h48_play_first_enabled: true, ..Rules::default() }) {
        check(&r);
        assert_eq!(r.regs.matched, 34);
    }
}

#[test]
fn fo006_with_h48_h53() {
    if let Some(r) = run("fo-006.jsonl", Rules { h48_play_first_enabled: true, h53_background: true, ..Rules::default() }) {
        check(&r);
        assert_eq!(r.regs.matched, 42);
        assert_eq!((r.lifecycle_other.matched, r.lifecycle_other.trace), (23, 60));
    }
}

#[test]
fn fo008_scheduler_blocks() {
    let data = root().join("data/InstalledGame");
    if !data.join("XFiles.hdb").exists() {
        return;
    }
    let Some(text) = common::trace_text("fo-008.jsonl") else { return };
    let Some(tab) = common::generated_text("reverse/vt_kinds.json") else { return };
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let b = xf::sched_replay::replay_blocks(&text, &tab, Some(&hdb)).unwrap();
    assert_eq!((b.h51_ok, b.h51_bad.len()), (9, 0));
    assert_eq!((b.switches, b.ops_matched, b.ops_trace, b.ops_core, b.ops_bad.len()), (9, 32, 32, 32, 0));
    assert_eq!((b.dispatches, b.unresolved, b.through_raised.len()), (1749, 0, 0));
    assert!(!b.negative && b.zero_at_end);
}

#[test]
fn fo009_new_route() {
    if let Some(r) = run("fo-009.jsonl", Rules { h48_play_first_enabled: true, h53_background: true, ..Rules::default() }) {
        assert_eq!((r.reg_tl_ok, r.reg_total, r.reg_tl_bad.len()), (152, 152, 0)); // H56 + H57
        assert_eq!((r.regs.matched, r.regs.extra.len()), (57, 0));
        assert_eq!((r.posts.matched, r.posts.trace, r.posts.core), (229, 229, 229));
        assert_eq!((r.dispatch_ok, r.dispatch_total), (229, 229));
        assert_eq!((r.cond_ok, r.cond_bad.len()), (214, 0));
        assert_eq!((r.execs.matched, r.execs.trace, r.execs.core), (183, 183, 183));
        assert_eq!((r.transitions.matched, r.transitions.trace, r.transitions.core), (9, 9, 9));
        assert_eq!((r.hotspots.matched, r.hotspots.trace, r.hotspots.core), (25, 25, 25));
        assert_eq!((r.lifecycle.matched, r.lifecycle.trace, r.lifecycle.core), (69, 69, 69));
        assert_eq!((r.lifecycle_other.matched, r.lifecycle_other.trace, r.lifecycle_other.core), (30, 101, 30));
    }
}

/// H58 (type 5 / IFaceLayout), H60 (PreloadEmIcon), post-hoc on fo-009: the core's own registrations of the drawer layouts
/// 0xe4b9 (twice) / 0x14992, their InterfaceItems and the items' AssetRefs, and of the 3 EmotionIcons + 6 AssetRefs —
/// 0 extra; every predicted Act/Deact of those owners is in the trace (only the icons' Deactivation at 213.7 s is not
/// predicted causally since H68).
#[test]
fn fo009_h58_h60() {
    if let Some(r) = run("fo-009.jsonl", Rules::all()) {
        assert_eq!((r.reg_tl_ok, r.reg_total, r.reg_tl_bad.len()), (152, 152, 0));
        assert_eq!((r.regs.matched, r.regs.core, r.regs.extra.len()), (152, 152, 0)); // +13 Explorables (H55), +8 H66, +1 H67 type 7, +25 H69 startup, +4 H71 game start
        assert_eq!((r.regs_expl.matched, r.regs_expl.trace, r.regs_expl.core), (13, 13, 13));
        assert_eq!((r.lifecycle_expl.matched, r.lifecycle_expl.trace, r.lifecycle_expl.core), (26, 26, 26));
        assert_eq!((r.posts.matched, r.posts.trace, r.posts.core), (229, 229, 229));
        assert_eq!((r.execs.matched, r.execs.trace, r.execs.core), (183, 183, 183));
        // H68: the click on icon 0x46fc queues the icons' state-0 commands; they run at 213.7 (3 icon + 6 asset Deacts,
        // causal — before H68 they matched only by order against H61's later Disable)
        assert_eq!((r.lifecycle_iface.matched, r.lifecycle_iface.trace, r.lifecycle_iface.core), (43, 43, 43)); // + H66 Act 0x442b, 0x442f
        assert_eq!(r.lifecycle_iface.early.len(), 0);
        assert_eq!((r.lifecycle_other.matched, r.lifecycle_other.trace, r.lifecycle_other.core), (101, 101, 101)); // + H145 7 media ends (94 without), + H66 6 asset Acts, + H67 Act 0x76bf, + H69 12 cursor assets, + H71 0xdd7c
        assert_eq!((r.lifecycle_misc.matched, r.lifecycle_misc.trace, r.lifecycle_misc.core), (2, 2, 2)); // H71 Act 0xdb79, 0x12b
        assert_eq!((r.lifecycle_cursor.matched, r.lifecycle_cursor.trace, r.lifecycle_cursor.core), (13, 13, 13));
        assert_eq!(r.lifecycle_other.early.len(), 0);
        assert_eq!(r.lifecycle_expl.early.len(), 0);
        // H58 hide (0xe4e9 ×2, 0x149ad): item, asset, item, asset … in item order
        assert_eq!((r.hide_order.matched, r.hide_order.trace, r.hide_order.core), (32, 32, 32));
        assert_eq!((r.timers_trace, r.timers_core, r.timer_bad.len()), (0, 0, 0)); // 0x9d59 cancelled by 0x7ac6
    }
}

/// H59 (type-2 timer), post-hoc: fo-007/fo-008 — the one Timer Expiration (View 0x2b3) is explained by timer 0 of
/// 0x9d59 (30000 ms) and no other expiry is predicted; fo-005/fo-006 — the timer is cancelled (0x7ac6), nothing fires.
#[test]
fn h59_timer_and_h60_icons_fo005_fo008() {
    // fo-005: (5, 8, 5) since H207 is on (H980): the icons' Deact at 111.6 has its H68 cause outside the replay
    // input (no q_dispatch group), and the later Disable 0x745a.. now correctly does nothing (the original's flag is 0
    // after the click: fo-056-end.x), so those 3 Deacts are no longer paired by order with H61's Deacts
    for (name, regs, timers, early, iface) in [
        ("fo-005.jsonl", 90, 0, 0, 5),
        ("fo-006.jsonl", 90, 0, 0, 8),
        ("fo-007.jsonl", 93, 1, 0, 8),
        ("fo-008.jsonl", 93, 1, 0, 8),
    ] // fo-005/006: no blk group → no H71 input; fo-006: H68 state commands without tgt_vt (H144 c) → causal
    {
        if let Some(r) = run(name, Rules::all()) {
            assert_eq!((r.regs.matched, r.regs.extra.len()), (regs, 0), "{name}");
            assert_eq!((r.timers_ok, r.timers_trace, r.timers_core, r.timer_bad.len()), (timers, timers, timers, 0), "{name}");
            // H60 Act ×3 + H61 Deact ×3 + H66 Act of VCInventory 0x442b / 0x442f; fo-007/fo-008: Timer → 0x7a41..0x7a43 Disable → Deact at once (causal);
            // fo-005/fo-006: Deact at 111.6 / 213.9 by an undecoded cause, Disable 0x745a.. later → matched by order only
            assert_eq!((r.lifecycle_iface.matched, r.lifecycle_iface.trace, r.lifecycle_iface.core), (iface, 8, iface), "{name}");
            assert_eq!(r.lifecycle_iface.early.len(), early, "{name}");
            // H55: 0x15620, 0x15621 (Enable), 0x1129c, 0xac5d (Enable), 0x16f1e — registrations and Act/Deact, 0 extra
            assert_eq!((r.regs_expl.matched, r.regs_expl.trace, r.regs_expl.core), (5, 5, 5), "{name}");
            assert_eq!((r.lifecycle_expl.matched, r.lifecycle_expl.trace, r.lifecycle_expl.core), (9, 9, 9), "{name}");
        }
    }
}

/// H66 (SetStartView → FUN_00477940), post-hoc on the Load traces (bot saves; data/bot not in git — skipped when
/// absent): the save's VCInventory list (VCGameState la0 = 0x442b, 0x442f) and the save's enabled EmotionIcons (fo-2b3:
/// 0x46fc, 0x4700, 0x6fde; fo-2c9: none) are registered by the core with their assets — 0 extra; every predicted
/// Act/Deact of icons / inventories / assets is in the trace.
#[test]
fn h66_start_icons_load() {
    let data = root().join("data/InstalledGame");
    for (name, save, regs, iface) in
        [("fo-013.jsonl", "fo-2b3-user.x", 82, 23), ("fo-014.jsonl", "fo-2b3-user.x", 82, 23), ("fo-015.jsonl", "fo-2c9-door.x", 73, 17)] // H72: the whole load screen
    {
        let save = root().join("data/bot").join(save);
        if !data.join("XFiles.hdb").exists() || !save.exists() {
            return;
        }
        let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
        let gam = Hdb::open(&save).unwrap();
        let Some(text) = common::trace_text(name) else { return };
        let r = replay(&hdb, &gam, &parse_trace(&text).unwrap(), Rules::all());
        assert_eq!((r.regs.matched, r.regs.core, r.regs.extra.len()), (regs, regs, 0), "{name}");
        assert_eq!((r.lifecycle_iface.matched, r.lifecycle_iface.extra.len()), (iface, 0), "{name}");
        assert_eq!(r.lifecycle_other.extra.len(), 0, "{name}");
        assert!(r.gate_bad.is_empty(), "{name}: {:?}", r.gate_bad);
    }
}

/// H67 (Load restores the save's VCGameState l64 players and l154 timers; type 7 = play), post-hoc on the Load traces:
/// the restored player's AssetRef 0x76bf is registered when its command runs (0 extra), the restored type-2 message (action
/// 0, fo-2b3 saves: timer 0, 0x733b ms) is run and fo-014's Timer on View 0x2b3 (no click) is explained by it.
#[test]
fn h67_saved_media_timers_load() {
    let data = root().join("data/InstalledGame");
    for (name, save, execs, timers) in
        [("fo-013.jsonl", "fo-2b3-user.x", 51, 0), ("fo-014.jsonl", "fo-2b3-user.x", 53, 1), ("fo-015.jsonl", "fo-2c9-door.x", 27, 0)] // + H70 menu type 5
    {
        let save = root().join("data/bot").join(save);
        if !data.join("XFiles.hdb").exists() || !save.exists() {
            return;
        }
        let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
        let gam = Hdb::open(&save).unwrap();
        let Some(text) = common::trace_text(name) else { return };
        let r = replay(&hdb, &gam, &parse_trace(&text).unwrap(), Rules::all());
        assert!(r.regs.missing.iter().all(|m| !m.1.contains("0x76bf")), "{name}: {:?}", r.regs.missing);
        assert_eq!((r.execs.matched, r.execs.extra.len()), (execs, 0), "{name}");
        assert_eq!((r.timers_ok, r.timers_trace, r.timer_bad.len()), (timers, timers, 0), "{name}");
    }
}

/// H76 (post-hoc fo-009, fo-021): View entry / exit lifecycle fires held behind the actions queued before them. fo-021: the
/// three entries of View 0x3ef (Enable-built 0xd242 / 0xd244 before 0x3f4 / 0x3f9) and their exits (Disable → Deact at
/// once) in trace order — the one extra is H75's re-entry of 0x2bb; fo-020 / fo-009 unchanged. Ablation: off → 127/139.
#[test]
fn h76_entry_hotspot_order() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    // both off = A2 (H89 is on in Rules::all() since H1241; with it H76's hold is bypassed)
    let off = Rules { h76_entry_hotspot_order: false, h89_queue_model: false, ..Rules::all() };
    // H89: the depth-first queue model gives the same without H76's hold; the H76 runner alone too
    let h89 = Rules { h76_entry_hotspot_order: false, h89_queue_model: true, ..Rules::all() };
    let h76 = Rules { h89_queue_model: false, ..Rules::all() };
    for (name, rules, want) in [
        ("fo-021.jsonl", Rules::all(), (139, 139, 0)), // the Deact 0x2bb extra was H75 (merged)
        ("fo-021.jsonl", off, (127, 139, 12)), // 17 before H75 was merged
        ("fo-021.jsonl", h89, (139, 139, 0)),
        ("fo-021.jsonl", h76, (139, 139, 0)),
        ("fo-020.jsonl", Rules::all(), (144, 144, 0)),
        ("fo-020.jsonl", h89, (144, 144, 0)),
    ] {
        let Some(text) = common::trace_text(name) else { return };
        let r = replay(&hdb, &gam, &parse_trace(&text).unwrap(), rules);
        assert_eq!((r.lifecycle.matched, r.lifecycle.trace, r.lifecycle.extra.len()), want, "{name}");
    }
    for rules in [Rules::all(), h89, h76] {
        if let Some(r) = run("fo-009.jsonl", rules) {
            assert_eq!((r.lifecycle.matched, r.lifecycle.trace, r.lifecycle.extra.len()), (69, 69, 0));
            assert_eq!((r.lifecycle_expl.matched, r.lifecycle_expl.extra.len(), r.lifecycle_expl.early.len()), (26, 0, 0));
        }
    }
}

/// H96 / H97 (post-hoc fo-023): PDAInitAddr shows address card 0xb09e in item 0x89f8 (REG 0xb09e, early Deact of 0x89f6),
/// PDAFlipAddr[3] posts two engine type-3 messages (action 0). Ablation: off → execs 175, REG 371, dispatch 248.
#[test]
fn h96_h97_pda_addr() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(text) = common::trace_text("fo-023.jsonl") else { return };
    let off = Rules { h96_pda_init_addr: false, h97_pda_flip_addr: false, ..Rules::all() };
    for (rules, regs, execs, dispatch) in [(Rules::all(), 372, 177, 250), (off, 371, 175, 248)] {
        let r = replay(&hdb, &gam, &parse_trace(&text).unwrap(), rules);
        assert_eq!((r.regs.matched, r.regs.extra.len()), (regs, 0)); // 0xcc41 ×2 removed by H100
        assert_eq!((r.execs.matched, r.execs.extra.len()), (execs, 0));
        assert_eq!(r.dispatch_ok, dispatch);
        assert_eq!(r.lifecycle_other.early.len(), if regs == 372 { 0 } else { 1 });
    }
}

/// H100 (static + post-hoc fo-023): the periodic Timer on View 0x4d5 re-runs type-0x87 action 0x1238a (mode 2, looping
/// player of 0xcc41) at 1033.6 / 1177.8; the looping player still exists, so no REG / Act (FUN_00491380). Ablation: off →
/// 2 extra REG and 2 extra Act of 0xcc41.
#[test]
fn h100_looping_player_exists() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(text) = common::trace_text("fo-023.jsonl") else { return };
    // C61 (one player per AssetRef, any mode) also stops the second play — the ablation turns both off
    for (rules, extra) in [(Rules::all(), 0), (Rules { h100_player_mode: false, c61_one_player: false, ..Rules::all() }, 2)] {
        let r = replay(&hdb, &gam, &parse_trace(&text).unwrap(), rules);
        assert_eq!((r.regs.matched, r.regs.extra.len()), (372, extra));
        assert!(r.regs.extra.iter().all(|x| x.1 == "0xcc41"));
    }
}

/// H58 order (static: the layout show / hide, FUN_004390e0; post-hoc fo-022 / fo-023): a layout's items are activated and, on hide,
/// deactivated in the order of its sorted container (key f4c & 0xff). fo-009's equal keys (0) keep list order; fo-023's
/// one miss on the branch (the PDA's early Deact of 0x89f6) is gone with H96. Ablation: off → 8/23 and 23/58.
#[test]
fn h58_item_order() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let off = Rules { h58_item_order: false, ..Rules::all() };
    for (name, rules, want) in [
        ("fo-022.jsonl", Rules::all(), (23, 23, 0)),
        ("fo-023.jsonl", Rules::all(), (58, 58, 0)), // with H96 the PDA's early Deact of 0x89f6 is predicted too
        ("fo-022.jsonl", off, (8, 23, 15)),
        ("fo-023.jsonl", off, (23, 58, 35)), // H96 on: 21/57 → 23/58 (RE_NOTES H96)
    ] {
        let Some(text) = common::trace_text(name) else { return };
        let r = replay(&hdb, &gam, &parse_trace(&text).unwrap(), rules);
        assert_eq!((r.hide_order.matched, r.hide_order.trace, r.hide_order.extra.len()), want, "{name}");
    }
    if let Some(r) = run("fo-009.jsonl", Rules::all()) {
        assert_eq!((r.hide_order.matched, r.hide_order.trace, r.hide_order.extra.len()), (32, 32, 0));
    }
}

/// fo-025 (save fo-2c9-door.x, every hypothesis on; post-hoc after the run): H100 mode 3 (0xd281 stops 0xcc41's looping
/// player → its Deact 885.72), H101 (type-3 Enable 0x7c6b builds VCInventory 0x7afb + assets 0x7af7 / 0x7af9 / 0xd53d),
/// H102 (LoginInit['C'] text assets of 0xc67b / 0xc68c, removed in the hide of 0x3b99), H103 (the kind-2 entry's
/// transition effect re-arms the expired timer 5 → third Timer fire, conditions evaluable). Each ablation restores the
/// pre-change number.
#[test]
fn fo025_new_rules() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(trace) = common::trace("fo-025.jsonl") else { return };
    let r = replay(&hdb, &gam, &trace, Rules::all());
    assert_eq!((r.reg_tl_ok, r.reg_total), (198, 198)); // 199 before H144 (9.19 0x122: tracer double hit, dropped)
    assert_eq!((r.regs.matched, r.regs.trace, r.regs.extra.len()), (198, 198, 0));
    assert_eq!(r.tracer_doubles.len(), 1);
    assert_eq!((r.posts.matched, r.posts.trace, r.posts.extra.len()), (196, 196, 0));
    assert_eq!((r.dispatch_ok, r.dispatch_total), (197, 197));
    assert_eq!((r.cond_ok, r.cond_bad.len(), r.cond_unknown), (197, 0, 0));
    assert_eq!((r.execs.matched, r.execs.trace, r.execs.extra.len()), (143, 143, 0));
    assert_eq!((r.transitions.matched, r.transitions.extra.len()), (7, 0));
    assert_eq!((r.lifecycle.matched, r.lifecycle.trace, r.lifecycle.extra.len()), (68, 68, 0));
    assert_eq!((r.lifecycle_other.matched, r.lifecycle_other.trace, r.lifecycle_other.extra.len()), (63, 63, 0)); // 61 without H145 (media ends 0x57fc / 0x57f2)
    assert_eq!((r.lifecycle_iface.matched, r.lifecycle_iface.trace, r.lifecycle_iface.extra.len()), (108, 108, 0));
    assert_eq!((r.hide_order.matched, r.hide_order.trace, r.hide_order.extra.len()), (50, 50, 0));
    assert_eq!((r.lifecycle_cursor.matched, r.lifecycle_cursor.trace, r.lifecycle_cursor.extra.len()), (20, 20, 0));
    assert_eq!((r.timers_ok, r.timers_trace, r.timer_bad.len()), (3, 3, 0));
    assert!(r.unimplemented.is_empty(), "{:?}", r.unimplemented);
    // ablations
    let r = replay(&hdb, &gam, &trace, Rules { h101_inventory_enable: false, ..Rules::all() });
    assert_eq!((r.regs.matched, r.lifecycle_other.matched, r.lifecycle_iface.matched), (194, 60, 107)); // 58 without H145
    let r = replay(&hdb, &gam, &trace, Rules { h102_login_init: false, ..Rules::all() });
    assert_eq!((r.regs.matched, r.hide_order.matched, r.lifecycle_cursor.matched), (196, 48, 16));
    let r = replay(&hdb, &gam, &trace, Rules { h103_timer_pause_resume: false, ..Rules::all() });
    assert_eq!((r.cond_ok, r.cond_unknown, r.timers_ok), (194, 3, 2));
    let mode3 = Rules {
        h101_inventory_enable: false,
        h102_login_init: false,
        h103_timer_pause_resume: false,
        h145_media_end_input: false,
        ..Rules::all()
    };
    let r = replay(&hdb, &gam, &trace, mode3);
    assert_eq!((r.lifecycle_other.matched, r.lifecycle_other.extra.len()), (58, 0)); // H100 mode 3: Deact 0xcc41
    assert!(r.lifecycle_other.missing.iter().all(|x| x.1 != "Deact 0xcc41"));
    // fo-023: H103 makes the third Timer fire (1025.93) explained and its conditions evaluable
    let Some(text) = common::trace_text("fo-023.jsonl") else { return };
    let t23 = parse_trace(&text).unwrap();
    for (rules, want) in [(Rules::all(), (250, 0, 3)), (Rules { h103_timer_pause_resume: false, ..Rules::all() }, (247, 3, 2))] {
        let r = replay(&hdb, &gam, &t23, rules);
        assert_eq!((r.cond_ok, r.cond_unknown, r.timers_ok), want);
    }
}

/// H895 / C88 (`traces/fo-112.jsonl` 83.338–83.382): EndGame 0xc0f2's game-end object passes the game start's blk_reset
/// (FUN_0048cd70, the replay's game-start marker) but registers nothing — no VCGameState 0x14cb / VCTitle 0x12b.
#[test]
fn h895_game_end_registers_nothing_fo112() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-064-end.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(trace) = common::trace("fo-112.jsonl") else { return };
    let r = replay(&hdb, &gam, &trace, Rules::all());
    assert_eq!(r.regs.extra.len(), 0, "{:?}", r.regs.extra);
    assert_eq!(r.lifecycle_other.extra.len(), 0);
    let off = replay(&hdb, &gam, &trace, Rules { h895_game_end_no_start: false, ..Rules::all() });
    let extra: Vec<&str> = off.regs.extra.iter().map(|e| e.1.as_str()).collect();
    assert_eq!(extra, ["0x14cb", "0x12b"]);
}

/// H896 (`traces/fo-115.jsonl` 168.24–176.83): with the traced key events (key_in) the replay's ING KeyPress gets
/// the NSA plate and the Enter runs the Search button's list (0x14072 / 0xe905, trigger 0x9dcb) as on the original; without
/// them the typed text stays empty and the Enter takes the not-found path 0x15fb5 → 0x15fb7.
#[test]
fn h896_replay_keys_fo115() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-078-end.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(trace) = common::trace("fo-115.jsonl") else { return };
    let r = replay(&hdb, &gam, &trace, Rules::all());
    assert_eq!((r.posts.matched, r.posts.trace, r.posts.extra.len()), (364, 364, 0));
    let off = replay(&hdb, &gam, &trace, Rules { h896_replay_keys: false, ..Rules::all() });
    assert!(off.posts.extra.iter().any(|e| e.1.starts_with("0x15fb7")), "{:?}", off.posts.extra);
}

/// H922 + C61 (post-hoc `traces/fo-127.jsonl`): the Load's box of Wong builds its IdeaIcons in the box's queued
/// state-1 command, after the Conversations' Activations (40.262–40.268); 0x8b1e at 119.102 plays 0x5bbe while its player
/// still exists → no second REG / Act. Ablation: 4 extra REG (icons out of order) / 1 extra REG + Act (0x5bbe).
#[test]
fn h922_c61_fo127() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-126-end.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(text) = common::trace_text("fo-127.jsonl") else { return };
    let tr = parse_trace(&text).unwrap();
    let r = replay(&hdb, &gam, &tr, Rules::all());
    assert_eq!((r.regs.matched, r.regs.extra.len()), (253, 0), "{:?}", r.regs.extra);
    assert_eq!(r.lifecycle_other.extra.len(), 0);
    let r = replay(&hdb, &gam, &tr, Rules { h922_dbox_show_queued: false, ..Rules::all() });
    assert_eq!(r.regs.extra.len(), 4, "{:?}", r.regs.extra);
    let r = replay(&hdb, &gam, &tr, Rules { c61_one_player: false, ..Rules::all() });
    assert_eq!(r.regs.extra.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), ["0x5bbe"]);
    assert_eq!(r.lifecycle_other.extra.len(), 1);
}
