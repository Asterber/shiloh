//! H2060–H2068: the original's Alt hotkeys (static FUN_0046c390, H1868; NOT traced) on a real New Game —
//! `front::altkeys` through `front::host::alt`, the way both fronts call it.
use std::path::PathBuf;
use xf::front::altkeys::{AltOut, PREF_KEYS};
use xf::front::host;
use xf::front::intro::Start;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn leak(p: PathBuf) -> Option<&'static Hdb> {
    p.exists().then(|| &*Box::leak(Box::new(Hdb::open(&p).unwrap())))
}

fn game(rules: Rules) -> Option<Session<'static>> {
    let d = root().join("data/InstalledGame");
    let (Some(h), Some(g)) = (leak(d.join("XFiles.hdb")), leak(d.join("XFiles.gam"))) else { return None };
    let mut s = Session::with_rules(h, g, &d, rules);
    s.boot(Start::NewGame);
    settle_timers(&mut s, 100, 60_000);
    Some(s)
}

fn shown(s: &Session, layout: u32) -> bool {
    s.r.w.layouts().iter().any(|l| l.0 == layout)
}

#[test]
fn h2062_alt_letters_flip_the_prefs() {
    let Some(mut s) = game(Rules::all()) else { return };
    for (vk, var, name) in PREF_KEYS {
        let was = s.r.w.vars.get(var).unwrap();
        assert_eq!(host::alt(&mut s, vk, false), AltOut::Pref(name, was == 0), "{name}");
        assert_eq!(s.r.w.vars.get(var), Some((was == 0) as i32), "{name}");
        host::alt(&mut s, vk, false);
        assert_eq!(s.r.w.vars.get(var), Some(was), "{name} back");
    }
    // C86: Artificial Intuition is on in a New Game; Alt+A turns it off and the next New Game keeps that (host prefs)
    assert_eq!(host::alt(&mut s, 0x41, false), AltOut::Pref("Artificial Intuition", false));
    let g = s.r.w.gam;
    s.reload(g, false);
    assert_eq!(s.r.w.vars.get(0x369), Some(0));
    // the rule off: nothing
    let Some(mut s) = game(Rules { h2060_alt_keys: false, ..Rules::all() }) else { return };
    assert_eq!(host::alt(&mut s, 0x41, false), AltOut::None);
    assert_eq!(s.r.w.vars.get(0x369), Some(1));
}

#[test]
fn h2063_alt_p_pauses_until_a_key() {
    let Some(mut s) = game(Rules::all()) else { return };
    let (now, posts) = (s.r.now, s.r.posts.len());
    assert_eq!(host::alt(&mut s, 0x50, false), AltOut::Pause(true));
    s.tick(10);
    assert!(s.alt.paused && s.r.frozen && shown(&s, 0xf5e8));
    // the timers stand still: a minute of game clock posts nothing of the game's own
    let p0 = s.r.posts.len();
    for _ in 0..600 {
        s.tick(100);
    }
    assert_eq!(s.r.posts.len(), p0, "posts while paused");
    assert!(p0 >= posts);
    // any key resumes (and goes no further)
    let l = host::key(&mut s, 0x41);
    assert!(l.contains("pause ended"), "{l}");
    s.tick(10);
    assert!(!s.alt.paused && !s.r.frozen && !shown(&s, 0xf5e8));
    assert!(s.r.now > now);
    // Alt+P twice: paused, resumed
    host::alt(&mut s, 0x50, false);
    assert_eq!(host::alt(&mut s, 0x50, false), AltOut::Pause(false));
    s.tick(10);
    assert!(!s.r.frozen && !shown(&s, 0xf5e8));
    // a click resumes too
    host::alt(&mut s, 0x50, false);
    assert!(host::button(&mut s, host::Button::Left, Some((320, 240))).unwrap().contains("pause ended"));
    assert!(!s.r.frozen);
}

#[test]
fn h2064_alt_s_opens_the_save_screen_without_the_menu() {
    let Some(mut s) = game(Rules::all()) else { return };
    assert_eq!(host::alt(&mut s, 0x53, false), AltOut::SaveScreen);
    s.tick(10);
    assert!(shown(&s, 0x14269) && !s.r.menu_open);
    // a second Alt+S over it: nothing
    assert_eq!(host::alt(&mut s, 0x53, false), AltOut::None);
    // Esc = the screen's Cancel (H403): back to the game, no menu (not opened from it)
    host::key(&mut s, 0x1b);
    s.tick(10);
    assert!(!shown(&s, 0x14269) && !s.r.menu_open);
}

#[test]
fn h2066_state_box_and_other_keys() {
    let Some(mut s) = game(Rules::all()) else { return };
    assert_eq!(host::alt(&mut s, 0x49, false), AltOut::None, "Alt+I without Shift");
    assert_eq!(host::alt(&mut s, 0x49, true), AltOut::StateBox);
    let t = s.alt.msgbox.clone().unwrap();
    // New Game: iCurrNode 1, the four scores 100 (XFiles.gam)
    assert_eq!(t, "Node 1\nScores: Asta 100, Loss 100, Paranoia 100, X-Track 100");
    // the box holds the game; a key closes it
    let now = s.now;
    s.tick(500);
    assert_eq!(s.now, now);
    let a = s.render();
    assert!(host::key(&mut s, 0x20).contains("state box closed"));
    assert_ne!(a, s.render());
    // volume, FPS, hotspots, captions
    assert_eq!(host::alt(&mut s, 0x30, false), AltOut::Volume(0));
    assert_eq!(host::alt(&mut s, 0x39, false), AltOut::Volume(100));
    assert_eq!(host::alt(&mut s, 0x46, true), AltOut::Fps(true));
    s.tick(25);
    assert_eq!(s.alt.fps.as_ref().unwrap().fps(), 40);
    assert_eq!(host::alt(&mut s, 0x46, true), AltOut::Fps(false));
    assert_eq!(host::alt(&mut s, 0x48, true), AltOut::Hotspots(true));
    assert!(s.overlay);
    let cc = s.r.w.closed_captioning();
    assert_eq!(host::alt(&mut s, 0x43, false), AltOut::Captions(!cc));
    assert_eq!(host::alt(&mut s, 0x58, false), AltOut::Exit);
    assert_eq!(host::alt(&mut s, 0x4c, false), AltOut::None, "Alt+L: an empty function");
}

#[test]
fn h2060_intro_takes_only_alt_x() {
    let d = root().join("data/InstalledGame");
    let (Some(h), Some(g)) = (leak(d.join("XFiles.hdb")), leak(d.join("XFiles.gam"))) else { return };
    let mut s = Session::with_rules(h, g, &d, Rules::all());
    s.boot(Start::Intro);
    if !s.intro_playing() {
        return;
    }
    assert_eq!(host::alt(&mut s, 0x41, false), AltOut::None);
    assert_eq!(host::alt(&mut s, 0x50, false), AltOut::None);
    assert_eq!(host::alt(&mut s, 0x58, false), AltOut::Exit);
    assert!(!s.intro_playing());
}
