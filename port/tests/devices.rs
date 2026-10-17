//! The player's devices in the closed loop (H165–H174, static hypotheses — not traced): inventory panel (prefs 'H'),
//! PDA / phone inventory clicks, dialling, the top-right menu object, right click, key focus. Save fo-2c9-door.x (View
//! 0x2c9, inventory PDA 0x442b + phone 0x442f, Pref_cInventory 'H', RegCurrNode var 0x4c3 = 1).
mod common;
use std::path::PathBuf;
use xf::front::{settle, settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::devices::{MenuChoice, MENU_OWNER, PANEL_OWNER, PDA, PHONE};
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn with_session(rules: Rules, f: impl FnOnce(&mut Session)) {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle(&mut s, 100, 900_000);
    f(&mut s);
}

/// The phone table's first number (text AssetRef 0x106c5, the exe's phone book entry 0) as dialled locally: its last 7
/// digits; None without the game data.
fn first_local_number() -> Option<String> {
    let [n] = common::game_texts([0x106c5])?;
    assert_eq!(n.len(), 10, "a 10-digit number");
    Some(n[3..].to_string())
}

fn shown(s: &Session, layout: u32) -> bool {
    s.r.w.layouts().iter().any(|l| l.0 == layout)
}

fn hit(s: &Session, x: i32, y: i32) -> Option<u32> {
    s.r.hit(x, y).map(|c| c.owner)
}

/// H170: no inventory rect before the panel was shown — the bottom strip is the panel; a hover there places PDA
/// (10,420)–(32,460) and phone (37,420)–(51,460); a hover above hides it again. Without H170 the items are placed at
/// their Activation (H150).
#[test]
fn h170_inventory_panel() {
    with_session(Rules::all(), |s| {
        assert_eq!(hit(s, 40, 440), Some(PANEL_OWNER));
        s.hover_at(40, 440);
        settle(s, 100, 10_000);
        assert!(s.r.inv_panel);
        assert_eq!(hit(s, 40, 440), Some(PHONE));
        assert_eq!(hit(s, 20, 440), Some(PDA));
        assert_eq!(hit(s, 300, 440), Some(PANEL_OWNER));
        s.hover_at(300, 200);
        assert!(!s.r.inv_panel);
        assert_eq!(hit(s, 40, 440), Some(PANEL_OWNER));
    });
    with_session(Rules { h170_inv_panel: false, ..Rules::all() }, |s| {
        assert_eq!(hit(s, 40, 440), Some(PHONE));
    });
}

/// H165 / H173: PDA click → OnIntInvClick (selection cleared, panel hidden) → layout 0x84d8 (node var 1 ≤ 7).
#[test]
fn h165_pda_click_opens_0x84d8() {
    with_session(Rules::all(), |s| {
        s.hover_at(20, 440);
        settle(s, 100, 10_000);
        assert_eq!(s.click_at(20, 440).map(|c| c.owner), Some(PDA));
        settle(s, 100, 60_000);
        assert!(shown(s, 0x84d8));
        assert_eq!(s.r.inv_selected, None);
        assert!(!s.r.inv_panel);
    });
}

/// H165 / H166: phone click → layout 0x3bc1, PhoneInit, focus 0x9a5a; keypad clicks fill the number, key presses go
/// through PhoneKeyPress (H174); Send → timer 0x16 → PhoneSend: the local 7 digits of the table's first entry →
/// iCallTriggerID 0x1173 := 0x106fc, iCallActionID 0x1174 := 0x106fe.
#[test]
fn h166_phone_dialling() {
    let Some(local) = first_local_number() else { return };
    with_session(Rules::all(), |s| {
        s.hover_at(40, 440);
        settle(s, 100, 10_000);
        assert_eq!(s.click_at(40, 440).map(|c| c.owner), Some(PHONE));
        settle(s, 100, 60_000);
        assert!(shown(s, 0x3bc1));
        assert_eq!(s.r.w.dev.focus, Some(0x9a5a));
        assert_eq!(s.r.w.dev.number, "");
        // keypad: the first four digits by click (the middle column's 2nd row three times, then 0 at (330,446)), the
        // last three by key
        for (x, y) in [(330, 373), (330, 373), (330, 373), (330, 446)] {
            assert!(s.click_at(x, y).is_some());
            settle(s, 100, 10_000);
        }
        assert_eq!(s.r.w.dev.number, local[..4]);
        for k in local[4..].chars() {
            assert!(s.r.key_press(k as u32));
            settle(s, 100, 10_000);
        }
        assert_eq!(s.r.w.dev.number, local);
        assert_eq!(s.click_at(280, 300).map(|c| c.owner), Some(0xdf4b)); // send
        settle_timers(s, 50, 10_000);
        assert_eq!(s.r.w.vars.get(0x1173), Some(0x106fc));
        assert_eq!(s.r.w.vars.get(0x1174), Some(0x106fe));
    });
}

/// H169: (600,10) = the top-right engine object → in-game menu; "Save" shows the save screen 0x14269 (H140).
#[test]
fn h169_top_right_menu() {
    with_session(Rules::all(), |s| {
        assert_eq!(hit(s, 600, 10), Some(MENU_OWNER));
        s.click_at(600, 10);
        assert!(s.r.menu_open);
        s.r.menu_choose(MenuChoice::Save);
        settle(s, 100, 60_000);
        assert!(!s.r.menu_open);
        assert!(shown(s, 0x14269));
    });
}

/// H172: right click on the PDA = Double Click (event 1) → the same list as the click (0x84d8), no selection.
#[test]
fn h172_right_click_pda() {
    with_session(Rules::all(), |s| {
        s.hover_at(20, 440);
        settle(s, 100, 10_000);
        assert_eq!(s.r.right_click(20, 440).map(|c| c.owner), Some(PDA));
        settle(s, 100, 60_000);
        assert!(shown(s, 0x84d8));
        assert_eq!(s.r.inv_selected, None);
    });
}

/// fo-037 k0–k9 in the closed loop: hover + phone click, the seven keypad keys of the first entry's local number, Send.
fn dial_first_entry(s: &mut Session) {
    let local = first_local_number().unwrap();
    s.hover_at(40, 440);
    settle(s, 100, 10_000);
    assert_eq!(s.click_at(40, 440).map(|c| c.owner), Some(PHONE));
    settle(s, 100, 60_000);
    for (x, y) in [(330, 373), (330, 373), (330, 373), (330, 446), (272, 337), (272, 409), (272, 337)] {
        assert!(s.click_at(x, y).is_some());
        settle(s, 100, 10_000);
    }
    assert_eq!(s.r.w.dev.number, local);
    assert_eq!(s.click_at(280, 300).map(|c| c.owner), Some(0xdf4b));
    settle_timers(s, 50, 10_000);
}

/// H200 (fo-037 k11 / k12, post-hoc): after the phone's use (OnIntInvClick clears the selection before the panel's
/// queued state 0) both inventory objects leave the bar; the next show places them as before — PDA (10,420)–(32,460),
/// phone (37,420)–(51,460) — so (67,440) is the panel (traced: nothing) and (20,440) the PDA. Ablation: the clicked phone
/// stays placed and the PDA lands at (56,420).
#[test]
fn h200_bar_order_after_phone_use() {
    for on in [true, false] {
        with_session(Rules { h200_deselect_before_panel_hide: on, ..Rules::all() }, |s| {
            s.hover_at(40, 440);
            settle(s, 100, 10_000);
            assert_eq!(s.click_at(40, 440).map(|c| c.owner), Some(PHONE));
            settle(s, 100, 60_000);
            s.hover_at(2, 2); // the pointer leaves the strip (H361: the next hover at (67,440) is an entry)
            assert_eq!(s.click_at(2, 2).map(|c| c.owner), Some(0x9a5a)); // close the phone (fo-037 k10)
            settle(s, 100, 60_000);
            s.hover_at(67, 440);
            settle(s, 100, 10_000);
            assert!(s.r.inv_panel);
            if on {
                assert_eq!(hit(s, 67, 440), Some(PANEL_OWNER));
                assert_eq!(hit(s, 20, 440), Some(PDA));
                assert_eq!(hit(s, 40, 440), Some(PHONE));
            } else {
                assert_eq!(hit(s, 67, 440), Some(PDA));
            }
        });
    }
}

/// H166 corrected (fo-037 J3, trace-fed replay reproduces it): the call's action list runs from the ring's end, not from an
/// event 6 — ring 0x15b78 (condition on RandGen's value: an input here, as traced 1) plays 0x15b73; its media end
/// (Deactivation) fires trigger 0x15b9c → DoActionList[0x15b87, 0x15b88] → three ring-list stops + DoActionListID[0x1173,
/// 0x1174] → AL 0x106fe with trigger 0x106fc (fo-037 469.44–477.38). Nothing of it before the ring ends.
#[test]
fn h166_call_runs_at_ring_end() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.uneval.insert(0x15b78, true);
    s.r.load_game();
    settle(&mut s, 100, 900_000);
    let p0 = s.r.posts.len();
    dial_first_entry(&mut s);
    let ring_end_list = [0x15b9f, 0x15b8b, 0x15b8d, 0x15b8f, 0x15b91];
    assert!(s.r.media.contains(&0x15b73), "the ring plays");
    assert!(!s.r.posts[p0..].iter().any(|m| m.action == 0x15b9f));
    let t0 = s.now;
    while s.now - t0 < 60_000 && !s.idle() {
        s.tick(100);
    }
    let posts: Vec<(u32, u32)> = s.r.posts[p0..].iter().map(|m| (m.action, m.trigger)).collect();
    let i = posts.iter().position(|p| p.0 == 0x15b9f).expect("ring end → 0x15b9f");
    assert_eq!(posts[i].1, 0x15b9c);
    assert_eq!(posts[i..i + 5].iter().map(|p| p.0).collect::<Vec<_>>(), ring_end_list);
    let call: Vec<u32> = posts[i + 5..].iter().filter(|p| p.1 == 0x106fc).map(|p| p.0).collect();
    assert_eq!(
        call,
        [0x11fa9, 0x11faa, 0x12349, 0x10701, 0x11fb6, 0x11fab, 0x1234d, 0x1234a, 0x1234b, 0x1234f, 0x1234e]
    );
}

/// H202 (pre-registration AC2, core prediction): End 0x9a53 (380,300) during the ring → DoActionList[0x9a5b, 0x9a5f]:
/// 0x1363 "IFL_bPhoneEndCall" := 0x25d and the three ring lists stopped (mode 3) → the ring's Deactivation trigger
/// 0x15b9c runs 0x15b9f with its condition (0x1363 = 0x25e) false → no DoActionListID, no AL 0x106fe.
#[test]
fn h202_end_call_during_ring() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.uneval.insert(0x15b78, true);
    s.r.load_game();
    settle(&mut s, 100, 900_000);
    dial_first_entry(&mut s);
    assert!(s.r.media.contains(&0x15b73));
    let p0 = s.r.posts.len();
    assert_eq!(s.click_at(380, 300).map(|c| c.owner), Some(0x9a53));
    let t0 = s.now;
    while s.now - t0 < 60_000 && !s.idle() {
        s.tick(100);
    }
    assert!(!s.r.media.contains(&0x15b73));
    let posts: Vec<u32> = s.r.posts[p0..].iter().map(|m| m.action).collect();
    assert!(posts.contains(&0x15ba9));
    assert!(!posts.contains(&0x15b91) && !s.r.posts[p0..].iter().any(|m| m.trigger == 0x106fc), "{posts:x?}");
    assert!(s.r.conds.iter().any(|c| c.0 == 0x15b9f && c.2 == Some(false)));
}

/// HYPOTHESIS H550 (a blocker reported in the window): in live play RandGen draws a value, so one of the three
/// ring lists (0x133d4 = 0 / 0x133d5 = 1 / 0x15b78 > 1) plays and the call's AL 0x106fe runs at the ring's end (H202,
/// C56 / C58). Ablation: with `live_rand` None the value is unknown, the ring conditions fall back to `uneval` (false) —
/// no ring, no call (what the window did).
#[test]
fn h550_live_rand_rings_and_calls() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let call_posts = |live: Option<u32>| {
        let mut s = Session::with_rules(&h, &g, &d, Rules::all());
        s.r.w.live_rand = live;
        s.r.load_game();
        settle(&mut s, 100, 900_000);
        let (p0, e0) = (s.r.posts.len(), s.r.execs.len());
        dial_first_entry(&mut s);
        let t0 = s.now;
        while s.now - t0 < 60_000 && !s.idle() {
            s.tick(100);
        }
        let ring = s.r.execs[e0..].iter().copied().find(|a| [0x133d4, 0x133d5, 0x15b78].contains(a));
        let call = s.r.posts[p0..].iter().filter(|p| p.trigger == 0x106fc).count();
        (ring, call, s.r.w.vars.get(0x1175))
    };
    let mut rings = Vec::new();
    for seed in 0..12 {
        let (ring, call, v) = call_posts(Some(seed));
        let v = v.expect("RandGen value drawn");
        assert!((0..=3).contains(&v));
        assert_eq!(ring, Some([0x133d4, 0x133d5, 0x15b78, 0x15b78][v as usize]), "seed {seed}: value {v}");
        assert_eq!(call, 11, "seed {seed}: AL 0x106fe's 11 posts after the ring");
        rings.push(ring);
    }
    assert!(rings.iter().collect::<std::collections::HashSet<_>>().len() >= 2);
    let (ring, call, v) = call_posts(None);
    assert_eq!((ring, call, v), (None, 0, None), "ablation: value unknown → no ring, no call");
}

/// H550 (FUN_00468df0): one LCG step from the seed, the range's both ends included, bounds swapped when reversed
#[test]
fn h550_rand_gen_value() {
    use xf::world::rand_gen_value;
    assert_eq!(rand_gen_value(0, 0, 3), 12345 % 4);
    assert_eq!(rand_gen_value(0, 3, 0), 12345 % 4);
    assert_eq!(rand_gen_value(1, 0, 3), (0x41c64e6du32.wrapping_add(0x3039) % 4) as i32);
    assert_eq!(rand_gen_value(5, 10, 12), 10 + (5u32.wrapping_mul(0x41c64e6d).wrapping_add(0x3039) % 3) as i32);
    assert_eq!(rand_gen_value(9, 7, 7), 7);
}
