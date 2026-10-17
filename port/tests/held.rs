//! Held inventory values in the closed loop (H360–H363): using an item on a CharView (reaction / refusal
//! player / dialogue box), dropping the held object when the pointer enters the inventory strip, the drop's reset of
//! iCurrInventory 0x15a, the cursor picture's Deactivation. Post-hoc checks against fo-060 / fo-061 / fo-065 and the
//! Cook scene of fo-056-end.x (View 0x319, CharView 0x55e9, dialogue box open).
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// What one step did.
#[derive(Debug, Default)]
struct Step {
    hit: Option<u32>,
    posts: Vec<u32>,
    regs: Vec<u32>,
    plays: Vec<u32>,
}

/// Runs `steps` from `save` (`m:X,Y` = pointer moved, `p:X,Y` = moved + click); after each step the session settles and
/// 1.2 s more pass (the H170 panel hide after an inventory Enable). Returns the steps and the session state at the end
/// through `end`.
fn run(save: &str, steps: &str, rules: Rules, end: impl FnOnce(&Session)) -> Option<Vec<Step>> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let mut out = Vec::new();
    for st in steps.split(';') {
        let (p0, r0, pl0) = (s.r.posts.len(), s.r.regs.len(), s.r.plays.len());
        let (kind, xy) = st.split_at(2);
        let v: Vec<i32> = xy.split(',').map(|x| x.parse().unwrap()).collect();
        let hit = match kind {
            "m:" => s.hover_at(v[0], v[1]),
            _ => {
                s.hover_at(v[0], v[1]);
                s.click_at(v[0], v[1])
            }
        };
        settle_timers(&mut s, 100, 900_000);
        for _ in 0..12 {
            s.tick(100);
        }
        out.push(Step {
            hit: hit.map(|c| c.owner),
            posts: s.r.posts[p0..].iter().map(|m| m.action).collect(),
            regs: s.r.regs[r0..].to_vec(),
            plays: s.r.plays[pl0..].iter().map(|p| p.1).collect(),
        });
    }
    end(&s);
    Some(out)
}

const COOK: u32 = 0x55e9;
/// fo-056-end.x bar (after the hover at (20,440)): lockpick 0x4427 (value 0xc) at (91..121), flashlight 0x441e (8) at
/// (161..191), case files 0x7afb (4) at (336..366); Cook's f2c region (263,169)–(353,234)
const LOCKPICK: &str = "m:20,440;p:106,440";
const FLASHLIGHT: &str = "m:20,440;p:176,440";
const CASE_FILES: &str = "m:20,440;p:351,440";

/// H360: the lockpick (value 0xc → refusal slot 4) on Cook, who does not react to it (his Mouse Click trigger 0x62d9 tests
/// 0x15a against 0x260 / 0x3f0 / 0x3ef only): his refusal Conversation 0x10697's f38 list [0x4d4a, 0x10695 (kind 4 →
/// empty), 0x4d52, 0x4d48, **0x4d4e**, …] → AssetRef 0x4d4e played after the Mouse Click's 3 posts. Off: nothing.
#[test]
fn h360_item_on_cook_plays_refusal() {
    let steps = format!("{LOCKPICK};p:308,201");
    let Some(on) = run("fo-056-end.x", &steps, Rules::all(), |_| {}) else { return };
    assert_eq!(on[1].hit, Some(0x4427));
    assert_eq!(on[2].hit, Some(COOK));
    assert_eq!(on[2].posts, vec![0x769d, 0x7c4e, 0x7c4c]);
    assert_eq!(on[2].plays, vec![0x4d4e]);
    let off = run("fo-056-end.x", &steps, Rules { h360_char_view_held_click: false, ..Rules::all() }, |_| {}).unwrap();
    assert_eq!(off[2].posts, vec![0x769d, 0x7c4e, 0x7c4c]);
    assert!(off[2].plays.is_empty() && off[2].regs.is_empty());
}

/// H360: the flashlight (value 8 → slot 10: no refusal) on Cook → DisplayDBox as with nothing held: the box (0x565d)
/// and its rows are built. Off: nothing. Timing: the selection's H451 comment (Cook's slot 2 0x4d52, 2 s
/// after the selection) plays and ends while this harness settles, so the click comes after it. AR3 (the same click
/// 3.8 s after the selection, fo-073 k2) is REFUTED on the original — the click fell inside 0x4d52 and was swallowed
/// (H452); see tests/gun.rs `h451_h452_fo073_*`. The slot-10 box itself is not observed on the original yet.
#[test]
fn h360_item_without_refusal_opens_dialogue() {
    let steps = format!("{FLASHLIGHT};p:308,201");
    let mut shown = false;
    let Some(on) = run("fo-056-end.x", &steps, Rules::all(), |s| shown = s.r.w.dbox_layout().is_some()) else { return };
    assert_eq!(on[2].hit, Some(COOK));
    assert!(on[2].regs.contains(&0x565d));
    assert!(shown);
    let off = run("fo-056-end.x", &steps, Rules { h360_char_view_held_click: false, ..Rules::all() }, |s| {
        shown = s.r.w.dbox_layout().is_some()
    })
    .unwrap();
    assert!(!off[2].regs.contains(&0x565d) && !shown);
}

/// H360 reaction test (FUN_004d1f70): Cook reacts to the case files (0x7c4c: 0x15a == 0x3ef) — no refusal player, no box
/// from the click; his trigger's actions run (the case-files scene).
#[test]
fn h360_reacting_char_view() {
    let steps = format!("{CASE_FILES};p:308,201");
    let Some(on) = run("fo-056-end.x", &steps, Rules::all(), |_| {}) else { return };
    assert_eq!(on[1].hit, Some(0x7afb));
    assert!(on[2].posts.starts_with(&[0x769d, 0x7c4e, 0x7c4c]));
    assert!(!on[2].plays.contains(&0x4d4e));
    assert!(!on[2].regs.contains(&0x565d));
}

/// fo-065 (`traces/fo-065.jsonl`, from fo-064-end.x) k13: Skinner 0x58cb clicked with the badge held (value 1, slot 4,
/// no reaction) → the original registers **0x4e1f** (124.850) = slot 4 of 0x58cb's refusal list 0x106b2;
/// k14: the move from the View into the strip drops the held badge (held_drop 128.577), the click
/// selects it again: 6 posts (0x11330 / 0x11331 roll-out + 0x7cc6 OnInvClick, 0x13730 / 2 / 4) and REG 0x7c8c / 0xbb1c
/// as on the original. Off (H360 / H361): k13 no player, k14 only the 2 roll-out posts (the old "click on the selected
/// item does nothing").
#[test]
fn h360_h361_fo065_k13_k14() {
    let steps = "m:20,440;p:316,440;p:327,218;p:570,210;p:70,210;p:70,210;p:570,210;p:157,275;p:320,210;p:246,309;\
                 p:570,210;p:570,210;p:70,210;p:341,219;p:316,440";
    let Some(on) = run("fo-064-end.x", steps, Rules::all(), |_| {}) else { return };
    assert_eq!(on[13].hit, Some(0x58cb));
    assert_eq!(on[13].posts, vec![0x748e, 0x7490, 0x1166b]);
    assert_eq!(on[13].plays, vec![0x4e1f]);
    assert_eq!(on[14].posts, vec![0x11330, 0x11331, 0x7cc6, 0x13730, 0x13732, 0x13734]);
    assert_eq!(on[14].regs, vec![0x7c8c, 0xbb1c]);
    let off = run(
        "fo-064-end.x",
        steps,
        Rules { h360_char_view_held_click: false, h361_panel_roll_in_drop: false, h362_drop_resets_inv_var: false, ..Rules::all() },
        |_| {},
    )
    .unwrap();
    assert!(off[13].plays.is_empty());
    assert_eq!(off[14].posts, vec![0x11330, 0x11331]);
}

/// fo-060 k11 (`traces/fo-060.jsonl` 161.645–161.716, from fo-059-end.x): badge held, the gun added by the drawer
/// (k10, panel shown 1 s then hidden), then (347,440) from the View: the drop takes the badge out of the bar, the panel
/// places every item in activation order (…, badge 301, gun 336) → the click selects the **gun** (13 posts, REG 0x1bf6 /
/// 0x1bb1 — the original's). Without H361 the badge keeps (301,420) and the bar cursor runs on from the previous
/// show (k10's 1 s Enable show), so the other items land beyond x = 640 and (347,440) hits the bare panel (frozen
/// pred.json without the 1 s hide: the PDA, 39 posts — H212's mismatch).
#[test]
fn h361_fo060_k11_gun_after_drop() {
    let steps = "m:20,440;p:21,440;p:239,322;p:326,234;p:161,210;p:215,212;p:318,309;p:236,178;p:201,174;p:316,440;\
                 p:300,268;p:347,440";
    let Some(on) = run("fo-059-end.x", steps, Rules::all(), |_| {}) else { return };
    assert_eq!(on[11].hit, Some(0x4433));
    assert_eq!(on[11].posts.len(), 13);
    assert_eq!(on[11].regs, vec![0x1bf6, 0x1bb1]);
    let off = run("fo-059-end.x", steps, Rules { h361_panel_roll_in_drop: false, ..Rules::all() }, |_| {}).unwrap();
    assert_eq!(off[11].hit, Some(xf::world::devices::PANEL_OWNER));
    assert!(off[11].posts.is_empty());
}

/// fo-061 k2 / k3 (from fo-060-end.x): after the badge click the pointer stays in the strip (20,440) — no panel on the
/// original (no panel_state), and the click (382,440) reaches nothing (0 posts). Without H361 the core showed the panel
/// on any hover inside it and predicted the PDA (39 posts).
#[test]
fn h361_fo061_no_panel_without_entry() {
    let steps = "m:20,440;p:316,440;m:20,440;p:382,440";
    let Some(on) = run("fo-060-end.x", steps, Rules::all(), |_| {}) else { return };
    assert_eq!(on[1].hit, Some(0x440d));
    assert!(on[3].posts.is_empty());
    let off = run("fo-060-end.x", steps, Rules { h361_panel_roll_in_drop: false, ..Rules::all() }, |_| {}).unwrap();
    assert_eq!(off[3].posts.len(), 39);
}

/// A player's deselect (H361 + H362): lockpick selected, the pointer leaves the strip and comes back → held object
/// dropped, selection cleared, 0x15a back to Inventory_None (0); the next click on Cook talks (box 0x565d) instead of the
/// refusal 0x4d4e. Without H362 the value stays held (refusal again); without H361 nothing is dropped.
#[test]
fn h361_h362_deselect_by_leaving_the_strip() {
    let steps = format!("{LOCKPICK};m:300,200;m:300,440;p:308,201");
    let mut state = (None, None, None);
    let grab = |s: &Session| (s.r.inv_selected, s.r.w.dev.held, s.r.w.vars.get(0x15a));
    let Some(on) = run("fo-056-end.x", &steps, Rules::all(), |s| state = grab(s)) else { return };
    assert_eq!(on[3].hit, Some(xf::world::devices::PANEL_OWNER));
    assert!(on[4].regs.contains(&0x565d) && !on[4].plays.contains(&0x4d4e));
    assert_eq!(state, (None, None, Some(0)));
    let off = run("fo-056-end.x", &steps, Rules { h362_drop_resets_inv_var: false, ..Rules::all() }, |s| state = grab(s)).unwrap();
    assert_eq!(off[4].plays, vec![0x4d4e]);
    assert_eq!(state, (None, None, Some(0xc)));
    let off = run("fo-056-end.x", &steps, Rules { h361_panel_roll_in_drop: false, ..Rules::all() }, |s| state = grab(s)).unwrap();
    assert_eq!(off[4].plays, vec![0x4d4e]);
    assert_eq!(state, (Some(0x4427), Some(0), Some(0xc)));
}
