//! The game's ending in the closed loop (C220, `traces/fo-220.jsonl`; H1560 / H1561 = C270): from the last story save
//! `data/bot/fo-219-end.x` (View 0xacfb, the dead keyguard) the key, the AI icon + 'lure Mulder', the stiletto from the
//! bar, Cook at the blast door stabbed (hotspot 462) — then sc122AD, sc127A, the epilogue sc125A, the credits, EndGame →
//! the main menu (no Save item), and New game from there. The clicks at the original's button-down times (relative to
//! the key's: 49.459, AI 56.963, lure 57.816, stiletto 64.701, Cook 110.326).
use std::path::PathBuf;
use xf::front::host::{self, HostReq};
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const TICK: u64 = 20;

/// Run the clock on to `t` ms after `t0` (the menu stops the game's clock: stop there).
fn run_to(s: &mut Session, t0: u64, t: u64) {
    while s.now < t0 + t && !s.r.menu_open {
        s.tick(TICK.min(t0 + t - s.now));
    }
}

/// when the player of `asset` first started (state 1)
fn played_at(s: &Session, asset: u32) -> Option<u64> {
    s.r.media_log.iter().find(|m| m.1 == asset && m.2 == 1).map(|m| m.0)
}

fn transition_seen(s: &Session, view: u32) -> bool {
    s.r.transitions.iter().any(|t| t.0 == view)
}

#[test]
fn c220_key_lure_stiletto_cook_credits_main_menu_new_game() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-219-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 60_000);
    assert_eq!(s.r.w.view, Some(0xacfb));
    let t0 = s.now;

    // the key: INCOMPC 0xbf77 first, then View 0x387e (C220: +6.5 s)
    s.hover_at(335, 200);
    assert_eq!(s.click_at(335, 200).map(|c| c.owner), Some(0xacfe));
    run_to(&mut s, t0, 7504);
    assert!(played_at(&s, 0xbf77).is_some());
    assert_eq!(s.r.w.view, Some(0x387e));

    // the AI icon, then the ActionIcon 'lure Mulder' inside Timer 13's 8 s → View 0xb321 at once
    s.hover_at(562, 63);
    s.click_at(562, 63);
    run_to(&mut s, t0, 8357);
    s.hover_at(560, 370);
    assert_eq!(s.click_at(560, 370).map(|c| c.owner), Some(0x5ffe));
    run_to(&mut s, t0, 8357 + 100);
    assert_eq!(s.r.w.view, Some(0xb321));
    let lure = t0 + 8357;

    // the stiletto from the bar while the lure videos play (H1543: 0x15a := 0x17)
    run_to(&mut s, t0, 15242 - 1300);
    s.hover_at(20, 440);
    run_to(&mut s, t0, 15242);
    s.hover_at(456, 440);
    assert_eq!(s.click_at(456, 440).map(|c| c.owner), Some(0x63eb));
    run_to(&mut s, t0, 15242 + 400);
    s.hover_at(300, 380);
    assert_eq!(s.r.w.vars.get(0x15a), Some(0x17), "the stiletto held");
    // H1560 / H1561: the nav stills block 0xb321's Activation until the lure chain ends (original +19.7 s after the
    // click: trigger 0xb324's 0x115aa / 0xb333 → sc122B 0xc0d6)
    let sc122b = played_at(&s, 0xc0d6);
    assert!(sc122b.is_none(), "sc122B already at the stiletto click");

    // View 0xb340 (Cook at the blast door) after sc122B (original 97.291 = key +47.83 s)
    run_to(&mut s, t0, 60867);
    let sc122b = played_at(&s, 0xc0d6).expect("sc122B played");
    assert!((19_000..21_500).contains(&(sc122b - lure)), "sc122B at lure +{} ms", sc122b - lure);
    assert_eq!(s.r.w.view, Some(0xb340));

    // the stiletto click on Cook (hotspot 462, the stiletto reaction) → kill movie sc122AD
    s.hover_at(153, 190);
    // H2350 (fo-220 110.329: REG + Activation of 0xe2ef at the button-down): the animated stiletto over hotspot 462
    assert_eq!(xf::front::cursor::shape(&s), Some(xf::front::cursor::Shape::Game(0xe2ef)), "the stiletto cursor");
    let c = s.click_at(153, 190).expect("Cook's hotspot");
    assert_eq!(c.owner.wrapping_sub(xf::world::moviehot::MOVIE_HOT_TAG), 462);
    let cook = s.now;
    run_to(&mut s, cook, 1000);
    assert!(played_at(&s, 0x5b8a).is_some());
    assert!(!transition_seen(&s, 0xc0de), "no Scully-shot View");

    // the ending: FBI exterior night, sc127A, sc125A, the credits, EndGame → the main menu
    run_to(&mut s, cook, 220_000);
    // the credits (XV/19666.xmv) play and draw
    assert_eq!(s.r.playing(), Some(0x4cd2));
    assert!(!s.r.menu_open);
    let fb = s.render();
    assert!(fb.iter().filter(|&&b| b > 16).count() > 1000, "the credits frame is drawn");
    run_to(&mut s, cook, 300_000);
    let at = |a: u32| played_at(&s, a).unwrap_or_else(|| panic!("{a:#x} not played")) - cook;
    let (fbi, sc127a, sc125a, credits) = (at(0xfba6), at(0x158d9), at(0x5500), at(0x4cd2));
    eprintln!("after the click: FBI {fbi}, sc127A {sc127a}, sc125A {sc125a}, credits {credits}, menu {}", s.now - cook);
    assert!(fbi < sc127a && sc127a < sc125a && sc125a < credits);
    // the original: sc125A +134.3 s, the credits +172.5 s, the main menu +267.6 s after the click
    assert!((131_000..138_000).contains(&sc125a), "sc125A at +{sc125a}");
    assert!((167_000..176_000).contains(&credits), "credits at +{credits}");
    assert!(s.r.menu_open, "the main menu after the credits");
    assert!(!s.r.game_running, "no game to save");
    assert!(s.now - cook > 255_000, "the menu only after the credits (+{})", s.now - cook);
    assert!(xf::front::menu::MenuUi::item_at(570, 199, s.r.game_running).is_none(), "no Save item");
    let fb = s.render();
    assert!(fb.iter().any(|&b| b != 0), "the menu renders");

    // New game from the main menu
    s.hover_at(570, 101);
    s.menu_click(570, 101);
    assert_eq!(host::request(&mut s), Some(HostReq::New));
    let fresh: &'static Hdb = Box::leak(Box::new(Hdb::open(d.join("XFiles.gam")).unwrap()));
    s.reload(fresh, false);
    settle_timers(&mut s, 100, 900_000);
    assert!(!s.r.menu_open);
    assert!(s.r.game_running);
    assert_eq!(s.r.w.view, Some(0x2b3), "the new game's first View");
}
