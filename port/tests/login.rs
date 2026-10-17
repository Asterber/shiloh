//! The FBI computer login in the closed loop (H191 / H260, static hypotheses; the route to the screen is fo-045's, traced:
//! `traces/fo-045.md` k0–k21 from `data/bot/fo-040-end.x`, LoginInit[0x43] at k21, LoginClick[2] / [1] at k22 / k23).
//! The typed login (user / password texts read from the game, AssetRefs 0xe231 / 0x10d33), the right-click bypass on the
//! welcome picture, a wrong password.
mod common;
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

/// fo-045 k0..k21 (`traces/fo-045.jsonl.marks`): the last click (318,200) on Explorable 0x96e3 shows 0x3b99
const ROUTE: [(i32, i32); 22] = [
    (70, 210),
    (570, 210),
    (570, 210),
    (251, 281),
    (320, 210),
    (83, 282),
    (320, 210),
    (466, 300),
    (320, 210),
    (70, 210),
    (320, 210),
    (412, 147),
    (320, 210),
    (171, 271),
    (320, 210),
    (187, 223),
    (320, 210),
    (570, 210),
    (2, 92),
    (70, 210),
    (570, 210),
    (318, 200),
];
const LOGIN: u32 = 0x3b99; // the password screen
const WELCOME: u32 = 0x3b01; // the FBI menu
const CONNECT_VAR: u32 = 0x9ff; // IFL_bPasswordFBIConnect
const LOGGED_IN: u32 = 0x1352; // IFL_bHasLoggedInToFBIComputer

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn settle(s: &mut Session) {
    settle_timers(s, 100, 900_000);
}

fn at_login(rules: Rules, f: impl FnOnce(&mut Session)) {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-040-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle(&mut s);
    for (x, y) in ROUTE {
        s.hover_at(x, y);
        assert!(s.click_at(x, y).is_some(), "route click {x},{y}");
        settle(&mut s);
    }
    assert!(shown(&s, LOGIN), "fo-045 k21 shows the login screen");
    f(&mut s);
}

fn shown(s: &Session, layout: u32) -> bool {
    s.r.w.layouts().iter().any(|l| l.0 == layout)
}

fn typ(s: &mut Session, t: &str) {
    for c in t.chars() {
        assert!(s.r.key_press(c.to_ascii_uppercase() as u32), "key {c:?} not taken (focus {:?})", s.r.w.dev.focus);
        settle(s);
    }
}

fn var(s: &Session, v: u32) -> Option<i32> {
    s.r.w.vars.get(v)
}

/// The login's user and password (text AssetRefs 0xe231 / 0x10d33 that PswdConnect compares), the wrong password used
/// below (the PowerBook's password text 0x10d2e); None without the game data.
fn login_texts() -> Option<[String; 3]> {
    common::game_texts([0xe231, 0x10d33, 0x10d2e])
}

/// H260: LoginInit 'C' focuses the name field; typing the user text, Tab, the password, Enter → PswdConnect succeeds
/// (0x9ff := 1, 0x1352 := 1), the Connect list's conditional type 5s hide 0x3b99 and show 0x3b01.
#[test]
fn h260_typed_login() {
    let Some([user, pass, _]) = login_texts() else { return };
    // H570: the saves have Pref_bArtificialIntuition on → name pre-filled, focus in the password field (fo-069 k22)
    at_login(Rules::all(), |s| {
        assert_eq!(s.r.w.dev.focus, Some(0xc63c));
        typ(s, &pass);
        assert!(s.r.key_press(0xd));
        settle(s);
        assert_eq!(var(s, CONNECT_VAR), Some(1));
        assert_eq!(var(s, LOGGED_IN), Some(1));
        assert!(shown(s, WELCOME));
    });
    at_login(Rules { h570_login_art_intuit: false, ..Rules::all() }, |s| {
        assert_eq!(s.r.w.dev.focus, Some(0xc63b));
        typ(s, &user);
        assert!(s.r.key_press(9));
        settle(s);
        assert_eq!(s.r.w.dev.focus, Some(0xc63c));
        typ(s, &pass);
        assert!(s.r.key_press(0xd));
        settle(s);
        assert_eq!(var(s, CONNECT_VAR), Some(1));
        assert_eq!(var(s, LOGGED_IN), Some(1));
        assert!(!shown(s, LOGIN));
        assert!(shown(s, WELCOME));
    });
    // without H260 the name text 0xe231 is unknown → the check fails, the screen stays
    at_login(Rules { h260_login_text: false, ..Rules::all() }, |s| {
        s.hover_at(354, 233);
        s.click_at(354, 233);
        settle(s);
        typ(s, &user);
        s.r.key_press(9);
        settle(s);
        typ(s, &pass);
        s.r.key_press(0xd);
        settle(s);
        assert_eq!(var(s, CONNECT_VAR), Some(0));
        assert!(shown(s, LOGIN));
    });
}

/// H260 / H191: a wrong password → 0x9ff := 0, 0x1352 unchanged, the message box (0x1079f), the login stays.
#[test]
fn h260_wrong_password() {
    let Some([user, pass, wrong]) = login_texts() else { return };
    assert!(!wrong.eq_ignore_ascii_case(&pass));
    at_login(Rules::all(), |s| {
        let before = var(s, LOGGED_IN);
        typ(s, &user);
        s.r.key_press(9);
        settle(s);
        typ(s, &wrong);
        s.r.key_press(0xd);
        settle(s);
        assert_eq!(var(s, CONNECT_VAR), Some(0));
        assert_eq!(var(s, LOGGED_IN), before);
        assert!(shown(s, LOGIN));
        assert!(!shown(s, WELCOME));
        // the original's failed-login branch (FUN_004be800): the message box 0xead3 (engine show) with item 0x1079f
        // enabled (< 3 failures); its item 0x107b7's Activation runs SetIItemFocus[0] (action 0x16cc5) after
        // LoginInit 'C' focused 0xc63b
        assert!(shown(s, 0xead3));
        assert_eq!(s.r.w.dev.focus, Some(0));
    });
}

/// Bypass (static: item 0xc683 (98,44)–(114,64) over one letter of the welcome picture SRN_splash password.pic; TL 0xc685 =
/// trigger 0xc684 event 1 (Double Click = right click, C53 / H172) → list 0xc686 = [type 5 hide 0x3b99, type 5 show
/// 0x3b01]): the menu without PswdConnect (0x9ff, 0x1352 untouched).
#[test]
fn h260_welcome_o_right_click_bypass() {
    at_login(Rules::all(), |s| {
        let (c, l) = (var(s, CONNECT_VAR), var(s, LOGGED_IN));
        s.hover_at(106, 54);
        assert_eq!(s.r.right_click(106, 54).map(|c| c.owner), Some(0xc683));
        settle(s);
        assert!(!shown(s, LOGIN));
        assert!(shown(s, WELCOME));
        assert_eq!((var(s, CONNECT_VAR), var(s, LOGGED_IN)), (c, l));
    });
}

/// Every "extra" of a replay (the core predicted something the original did not do), per category.
pub fn extras(r: &xf::replay::Report) -> Vec<(&'static str, usize)> {
    [
        ("regs", r.regs.extra.len()),
        ("posts", r.posts.extra.len()),
        ("execs", r.execs.extra.len()),
        ("transitions", r.transitions.extra.len()),
        ("hotspots", r.hotspots.extra.len()),
        ("lifecycle", r.lifecycle.extra.len()),
        ("lifecycle_other", r.lifecycle_other.extra.len()),
        ("lifecycle_iface", r.lifecycle_iface.extra.len()),
        ("lifecycle_expl", r.lifecycle_expl.extra.len()),
        ("lifecycle_cursor", r.lifecycle_cursor.extra.len()),
        ("lifecycle_misc", r.lifecycle_misc.extra.len()),
        ("hide_order", r.hide_order.extra.len()),
        ("regs_expl", r.regs_expl.extra.len()),
    ]
    .into_iter()
    .filter(|x| x.1 > 0)
    .collect()
}

/// fo-045 (traced, `traces/fo-045.md`): LoginInit[0x43] 1199.62, LoginClick[2] 1216.61, LoginClick[1] 1229.10 — with H260
/// the replay keeps 0 extra and the same numbers (no PswdConnect / typed key in the trace).
#[test]
fn h260_fo045_login_clicks_replay() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-040-end.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(text) = common::trace_text("fo-045.jsonl") else { return };
    let trace = xf::replay::parse_trace(&text).unwrap();
    let r = xf::replay::replay(&hdb, &gam, &trace, Rules::all());
    assert_eq!(extras(&r), vec![]);
    assert_eq!((r.posts.matched, r.posts.trace), (313, 313));
    assert_eq!((r.execs.matched, r.execs.trace), (121, 121));
    assert_eq!((r.lifecycle_iface.matched, r.lifecycle_iface.trace), (92, 92));
    assert!(r.unimplemented.is_empty(), "{:?}", r.unimplemented);
    let off = xf::replay::replay(&hdb, &gam, &trace, Rules { h260_login_text: false, ..Rules::all() });
    assert_eq!((off.posts.matched, off.execs.matched, off.lifecycle_iface.matched), (313, 121, 92));
}
