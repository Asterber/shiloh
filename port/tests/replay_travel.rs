//! Lock-step replay of the bot traces fo-026..fo-029, fo-161 / fo-162 (every hypothesis on, loaded saves): regression guards for the
//! rules fitted on them (RE_NOTES H123..H129, H1181).
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

/// H123 (fo-027 / fo-028, PDA map layout 0x84da): items 0x84ee / 0x84f4 / 0x8500 share AssetRef 0x4926 — one registration,
/// one Activation, one Deactivation (after the last holder's removal). Ablation: 2 extra REG, 4 extra Act/Deact.
#[test]
fn h123_shared_asset_object() {
    for name in ["fo-027", "fo-028"] {
        let Some(r) = run("fo-2c9-door", name, Rules::all()) else { return };
        assert_eq!((r.regs.extra.len(), r.lifecycle_other.extra.len(), r.hide_order.extra.len()), (0, 0, 0), "{name}");
        let off = run("fo-2c9-door", name, Rules { h123_shared_asset_object: false, ..Rules::all() }).unwrap();
        assert_eq!((off.regs.extra.len(), off.lifecycle_other.extra.len(), off.hide_order.extra.len()), (2, 4, 2), "{name}");
    }
}

/// H125 (C40; fo-027 / fo-028): SwapDisc(0x6af, 0x2a21, 0x2a23, 0x2a24, 0x4c3) — REG Node / Location / ViewPoint, transition
/// 0x2a24 kind 0, REG View; old View / hotspots / ViewPoint 0x2b2 / Location 0x12f / Node 0x12d / looping player 0x76bf
/// deactivated, new Node / Location / ViewPoint / View activated; 0x2a24's hotspots, Explorable 0x15a4f, background 0x84b6,
/// CharView 0x590c (TL 0 twice: the H74 wrapper) follow. Ablation: transition 4/5, lifecycle 39/48.
#[test]
fn h125_swap_disc_travel() {
    for (name, regs) in [("fo-027", 206), ("fo-028", 205)] { // 175 / 174 without H140..H143 (save screen), 168 / 167 also without H126
        let Some(r) = run("fo-2c9-door", name, Rules::all()) else { return };
        assert_eq!((r.transitions.matched, r.transitions.trace, r.transitions.extra.len()), (5, 5, 0), "{name}");
        assert_eq!((r.lifecycle.matched, r.lifecycle.trace, r.lifecycle.extra.len()), (48, 48, 0), "{name}");
        assert_eq!((r.hotspots.matched, r.hotspots.trace), (18, 18), "{name}");
        assert_eq!((r.regs.matched, r.regs.extra.len(), r.reg_tl_bad.len()), (regs, 0, 0), "{name}");
        assert_eq!((r.regs_expl.matched, r.lifecycle_expl.matched, r.lifecycle_expl.trace), (4, 8, 8), "{name}");
        assert_eq!(r.lifecycle_other.extra.len(), 0, "{name}");
        for o in ["Deact 0x12f", "Deact 0x12d", "Deact 0x76bf", "Act 0x6af", "Act 0x2a21"] {
            assert!(r.lifecycle_other.missing.iter().all(|x| x.1 != o), "{name}: {o} not predicted");
        }
        let off = run("fo-2c9-door", name, Rules { h125_swap_disc: false, ..Rules::all() }).unwrap();
        assert_eq!((off.transitions.matched, off.lifecycle.matched), (4, 39), "{name}");
    }
}

/// H126 (static: DisplayDBox, FUN_00481660, and SetStartView's tail; post-hoc fo-027 / fo-028 / fo-029): the dialogue box of CharView 0x590c —
/// DisplayDBox (fo-027 / fo-028) or the Load of a save with VCGameState +0x1a4 = 0x590c (fo-029) builds VCConversationList
/// 0x590d (TL 0), its enabled Conversations 0x590e / 0x5914 / 0x591a (TL f3c) and their assets 0x4503 / 0x4505 / 0x4507;
/// fo-029: the CharView's Deactivation removes them. Ablation: the 4 TL-unknown registrations and the misses come back.
#[test]
fn h126_dialog_box() {
    for (save, name, other) in [("fo-2c9-door", "fo-027", 6), ("fo-2c9-door", "fo-028", 6), ("fo-028-end", "fo-029", 14)] {
        let Some(r) = run(save, name, Rules::all()) else { return };
        assert_eq!((r.reg_tl_unknown.len(), r.reg_tl_bad.len(), r.regs.extra.len()), (0, 0, 0), "{name}");
        assert_eq!((r.lifecycle_misc.matched, r.lifecycle_misc.trace, r.lifecycle_misc.extra.len()), (other, other, 0), "{name}"); // "other classes"
        assert_eq!(r.lifecycle_other.extra.len(), 0, "{name}");
        for o in ["Act 0x4503", "Act 0x4505", "Act 0x4507"] {
            assert!(r.lifecycle_other.missing.iter().all(|x| x.1 != o), "{name}: {o}");
        }
        assert!(r.unimplemented.iter().all(|x| !x.1.contains("DisplayDBox")), "{name}");
        let off = run(save, name, Rules { h126_dialog_box: false, ..Rules::all() }).unwrap();
        assert_eq!(off.reg_tl_unknown.len(), 4, "{name}");
    }
}

/// H122 (static: the type-3 Disable's InterfaceItem case, FUN_004201a0; post-hoc): a type-3 Disable of a built InterfaceItem removes it at
/// once — fo-030 0xe6bd / 0xe6c4 / 0x1107d (0x855f, 0x84f6, 0x84f2 + assets), fo-029 0x1199f (0x8a34 / 0x11745), fo-026.
/// Ablation: the Deacts come at the layout's hide (early matches; fo-029 / fo-030 H58 hide order 2 / 5 extra).
#[test]
fn h122_iitem_disable() {
    let off = Rules { h122_iitem_disable: false, ..Rules::all() };
    for (save, name, hide_extra_off) in [("fo-2c9-door", "fo-026", 0), ("fo-028-end", "fo-029", 2), ("fo-029-end", "fo-030", 5)] {
        let Some(r) = run(save, name, Rules::all()) else { return };
        let early = |r: &Report| r.lifecycle_iface.early.len() + r.lifecycle_other.early.len() + r.hide_order.early.len();
        assert_eq!((r.hide_order.extra.len(), r.lifecycle_iface.extra.len(), r.lifecycle_other.extra.len(), early(&r)), (0, 0, 0, 0), "{name}");
        let o = run(save, name, off).unwrap();
        assert_eq!(o.hide_order.extra.len(), hide_extra_off, "{name}");
        assert!(early(&o) > 0, "{name}");
    }
    let r = run("fo-029-end", "fo-030", Rules::all()).unwrap();
    assert_eq!(r.hide_order.matched, 95); // 62 without H140..H143 (the save screen's hide)
}

/// H1181 (C151 / C152; fo-161, fo-162): the two fax inventory objects 0x130b2 / 0xa186 share the icon AssetRefs 0xa182 /
/// 0xa184 / 0xd545 — registered and activated once (the second fax, and the Load's second activation, only re-post them);
/// 0x130b2's removal leaves them, 0xa186's deactivates them (fo-162 265.285). Ablation: 3 extra REG / 3 extra Act each,
/// and fo-162's three Deacts missing.
#[test]
fn h1181_shared_icon_asset() {
    for (save, name) in [("fo-160-end", "fo-161"), ("fo-161-end", "fo-162")] {
        let Some(r) = run(save, name, Rules::all()) else { return };
        assert_eq!((r.regs.extra.len(), r.lifecycle_other.extra.len()), (0, 0), "{name}");
        let off = run(save, name, Rules { h1181_shared_icon_asset: false, ..Rules::all() }).unwrap();
        assert_eq!((off.regs.extra.len(), off.lifecycle_other.extra.len()), (3, 3), "{name}");
        if name == "fo-162" {
            assert_eq!(r.lifecycle_other.matched, off.lifecycle_other.matched + 3);
        }
    }
}

/// H1500 (H1462, fo-206 66.008 / 69.721): InitEmailRead's Disable of IN BOX 0x116cc meets an item not yet activated —
/// nothing removed, its Deactivation comes at the read view's hide. Ablation: 1 H58 hide-order extra (Deact 66.008).
#[test]
fn h1500_iitem_disable_needs_active() {
    let Some(r) = run("fo-203-end", "fo-206", Rules::all()) else { return };
    assert_eq!((r.hide_order.matched, r.hide_order.extra.len()), (246, 0));
    let off = run("fo-203-end", "fo-206", Rules { h1500_iitem_disable_needs_active: false, ..Rules::all() }).unwrap();
    assert_eq!((off.hide_order.matched, off.hide_order.extra.len()), (245, 1));
}

/// H1408 (fo-162 54.556: H601's queued mouse command after the CharTeleport fires event 0 on CharView 0xa0eb, then the box
/// player REG 0x108a1, the IdeaIcons, the answers' assets): the replay runs the native CharView click on a traced Mouse
/// Click. Ablation: 17 fewer registrations, 24 fewer AssetRef Act / Deact, box state commands 3 → 1; no extra either way.
#[test]
fn h1408_replay_char_view_click() {
    let Some(r) = run("fo-161-end", "fo-162", Rules::all()) else { return };
    let off = run("fo-161-end", "fo-162", Rules { h1408_replay_char_view_click: false, ..Rules::all() }).unwrap();
    assert_eq!((r.regs.extra.len(), r.lifecycle_other.extra.len()), (0, 0));
    assert_eq!(r.regs.matched, off.regs.matched + 17);
    assert_eq!(r.lifecycle_other.matched, off.lifecycle_other.matched + 24);
    assert_eq!((r.dbox_states.matched, off.dbox_states.matched), (3, 1));
}

/// H1409 (fo-066 205.95: line 0xdcdc's f30 0xdcdf = XV/56543.xmv 0..3332, f34 0xdce1 = the same file 3333..5933; the
/// original registers 0xdcdf only, its Deactivation 212.00 ≈ 5.93 s after): an f34 asset in the f30 asset's movie file
/// gets no player. Ablation: 0xdce1 REG + Act extra.
#[test]
fn h1409_conv_tail_merge() {
    let Some(r) = run("fo-065-end", "fo-066", Rules::all()) else { return };
    assert_eq!((r.regs.extra.len(), r.lifecycle_other.extra.len()), (0, 0));
    let off = run("fo-065-end", "fo-066", Rules { h1409_conv_tail_merge: false, ..Rules::all() }).unwrap();
    assert_eq!((off.regs.extra.len(), off.lifecycle_other.extra.len()), (1, 1));
}
