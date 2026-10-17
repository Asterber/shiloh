//! C86 (fo-108, `traces/fo-108.jsonl`) / H1990: New Game takes the host's Preferences (the original's registry) into the
//! RegPreferences variables — Pref_bArtificialIntuition 0x369 = 1 where XFiles.gam has 0. Bug report "softlock at the
//! Tarakan": a port New Game kept the .gam's 0x369 = 0, so the wheelhouse's fingerprint video end (AL 0xf9c4) skipped
//! 0xfb30 (`0x369 == bTRUE` → Enable 0x184 = the 'call Amis' ActionIcon 0x600d at (560,370)); without it 0x8e7
//! bN3WillmoreCalledAmisAboutTarakanPrints stays 0 unless the player phones Amis (his number, AL 0x106fe), so Asta never
//! plays Mendoza's message (0x5e76's Activation needs 0x12fd + 0x8e6 + 0x12fc + 0x8e3 + 0x8e7) → no 0x112a coroner tag.
use std::path::PathBuf;
use xf::front::intro::Start;
use xf::front::{script, settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn leak(p: PathBuf) -> Option<&'static Hdb> {
    p.exists().then(|| &*Box::leak(Box::new(Hdb::open(&p).unwrap())))
}

/// C86's registry values (fo-108's save) and XFiles.gam's
const REGISTRY: [(u32, i32); 6] =
    [(0x369, 1), (0x3d5, b'H' as i32), (0x3d9, 1), (0x3de, b'W' as i32), (0x3e3, 1), (0xa0c, b'Y' as i32)];

#[test]
fn c86_new_game_takes_the_registry_prefs() {
    let d = root().join("data/InstalledGame");
    let (Some(h), Some(g)) = (leak(d.join("XFiles.hdb")), leak(d.join("XFiles.gam"))) else { return };
    assert_eq!(xf::world::C86_HOST_PREFS, REGISTRY);
    for on in [true, false] {
        let mut s = Session::with_rules(h, g, &d, Rules { c86_new_game_host_prefs: on, ..Rules::all() });
        s.boot(Start::NewGame);
        settle_timers(&mut s, 100, 60_000);
        for (v, x) in REGISTRY {
            let gam = g.read(0x53, v).unwrap().u32("f38") as i32;
            assert_ne!(gam, x, "{v:#x}: XFiles.gam differs from the registry");
            assert_eq!(s.r.w.vars.get(v), Some(if on { x } else { gam }), "{v:#x} rule {on}");
        }
        // the menu's New (C243 / H1702) — a new core from XFiles.gam — takes them again
        s.reload(g, false);
        assert_eq!(s.r.w.vars.get(0x369), Some(on as i32));
    }
}

/// The host's Preferences follow the game's own Options (H189 writes the variable): a New Game after them keeps the
/// player's choice. A Load: H2100 (on) — the host's value over the save's (fo-250 / fo-252); off (H1990) — the save's.
#[test]
fn c86_new_game_keeps_the_players_choice() {
    let d = root().join("data/InstalledGame");
    let (Some(h), Some(g), Some(save)) =
        (leak(d.join("XFiles.hdb")), leak(d.join("XFiles.gam")), leak(root().join("data/bot/fo-140-end.x")))
    else {
        return;
    };
    for h2100 in [true, false] {
        let mut s = Session::with_rules(h, g, &d, Rules { h2100_load_host_prefs: h2100, ..Rules::all() });
        s.boot(Start::NewGame);
        settle_timers(&mut s, 100, 60_000);
        s.r.w.set_prefs(&[(0x369, 0)]); // Options: Artificial Intuition off
        s.reload(g, false);
        assert_eq!(s.r.w.vars.get(0x369), Some(0));
        s.reload(save, true);
        let want = if h2100 { 0 } else { 1 };
        assert_eq!(s.r.w.vars.get(0x369), Some(want), "H2100 {h2100}: the host's (on) / the save's (off) value");
        s.reload(g, false);
        assert_eq!(s.r.w.vars.get(0x369), Some(want));
    }
}

/// fo-141's 52 steps (the core's route, lock-step on the original) from fo-140-end.x, ending with the ActionIcon click
/// (560,370) that calls Amis (C119).
const FO141: &str = "p:70,210;p:284,209;p:320,210;p:570,210;p:301,210;p:70,210;p:320,210;p:337,210;p:320,210;p:354,210;\
p:320,178;p:70,210;p:296,210;p:570,210;p:320,210;p:70,210;p:324,209;p:345,248;p:329,194;p:300,175;p:170,210;p:339,288;\
p:309,210;p:320,240;p:70,210;p:308,210;p:407,210;p:570,210;p:320,277;p:185,240;p:263,232;p:320,240;p:570,210;p:457,210;\
p:348,210;p:324,243;p:70,210;p:386,210;p:70,210;p:70,210;p:364,209;p:570,210;p:320,210;p:427,188;p:302,154;p:70,210;\
p:307,210;p:70,210;p:334,202;p:388,238;p:320,210;p:560,370";

/// the phone (owner click), Amis's local number on the keypad, Send (tests/devices.rs `dial_first_entry`)
const CALL_AMIS: &str = "o:442f;p:330,373;p:330,373;p:330,373;p:330,446;p:272,337;p:272,409;p:272,337;p:280,300";

fn tarakan(art_intuit: i32, extra: &str) -> Option<(Option<i32>, bool)> {
    let d = root().join("data/InstalledGame");
    let (h, save) = (leak(d.join("XFiles.hdb"))?, leak(root().join("data/bot/fo-140-end.x"))?);
    let mut s = Session::with_rules(h, save, &d, Rules::all());
    s.r.w.live_rand = Some(1); // H550: the phone's ring, as the window
    s.r.load_game();
    s.r.w.set_prefs(&[(0x369, art_intuit)]);
    settle_timers(&mut s, 100, 900_000);
    let steps = format!("{FO141};{extra}");
    for i in script::parse(&steps).unwrap() {
        script::apply(&mut s, &i);
        settle_timers(&mut s, 100, 900_000);
    }
    Some((s.r.w.vars.get(0x8e7), s.r.w.is_enabled(0x47, 0x600d)))
}

#[test]
fn h1990_call_amis_action_icon_needs_the_ai_pref() {
    // the original's (and the port's, C86) value: the ActionIcon calls Amis
    let Some((called, _)) = tarakan(1, "") else { return };
    assert_eq!(called, Some(1), "0x8e7 with Pref_bArtificialIntuition on (C119)");
    // the .gam's 0: 0xfb30 does not enable 0x184 — (560,370) hits nothing, 0x8e7 stays 0
    let (called, icon) = tarakan(0, "").unwrap();
    assert_eq!((called, icon), (Some(0), false));
    // not a softlock: the phone call to Amis (AL 0x106fe: 0x10bf && 0x13c0 → 0x11fac, its end 0x11fb2) sets it
    let (called, _) = tarakan(0, CALL_AMIS).unwrap();
    assert_eq!(called, Some(1), "0x8e7 by the phone");
}
