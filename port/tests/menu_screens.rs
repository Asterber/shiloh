//! Issue Asterber/shiloh#2 (the menu and the Options screen): H2340 — the screens the menu opens (Options here) show
//! over the live menu and the game stays paused under them (static, R29
//! `traces/fo-086.jsonl`): no game clock, no sound of its players, no caption / video text over the screen, no click to
//! the paused video; H2341 — the first menu's LEFTmain.mov intro shows alone (the original's shot
//! `traces/fo-226.jsonl.burst/c049.9.png`). Skipped without the game data.
use std::path::PathBuf;
use xf::front::host::{self, Button};
use xf::front::intro::Start;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn session(start: Start, rules: Rules) -> Option<Session<'static>> {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        eprintln!("data missing: skipped");
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(d.join("XFiles.gam")).unwrap()));
    let mut s = Session::with_rules(h, g, &d, rules);
    s.boot(start);
    Some(s)
}

fn ticks(s: &mut Session, n: usize) {
    for _ in 0..n {
        s.tick(100);
    }
}

fn press(s: &mut Session, b: Button, x: i32, y: i32) -> Option<String> {
    let l = host::button(s, b, Some((x, y)));
    host::release(s, b, Some((x, y)));
    s.tick(100);
    l
}

const OPENING: u32 = 0xfba5;
const OPTIONS: u32 = 0xc24a;
const OPTIONS_ITEM: (i32, i32) = (555, 293);
const RETURN_TO_MENU: (i32, i32) = (512, 460);

/// New Game, 1 s into the opening movie (0xfba5), Esc → Options.
fn options_during_the_opening(rules: Rules) -> Option<Session<'static>> {
    let mut s = session(Start::NewGame, rules)?;
    ticks(&mut s, 10);
    assert_eq!(s.r.playing(), Some(OPENING));
    host::key(&mut s, 0x1b);
    assert!(s.r.menu_open);
    press(&mut s, Button::Left, OPTIONS_ITEM.0, OPTIONS_ITEM.1);
    assert!(s.r.w.layouts().iter().any(|l| l.0 == OPTIONS), "Options shown");
    assert!(!s.r.menu_open);
    Some(s)
}

/// H2340: the game's clock, its movie and its sound stand still under Options; "Return to main menu" brings the menu
/// back and the menu's Return runs the game on from where it stopped.
#[test]
fn h2340_the_game_stays_paused_under_options() {
    let Some(mut s) = options_during_the_opening(Rules::all()) else { return };
    let t = s.now;
    ticks(&mut s, 50);
    assert_eq!(s.now, t, "the game's clock stops under Options");
    assert_eq!(s.r.playing(), Some(OPENING), "the opening movie does not run on");
    assert!(s.audio_cues().is_empty(), "no sound of the paused game");
    // the right button does not skip the paused video, the left one reaches the screen's items
    press(&mut s, Button::Right, 320, 200);
    assert_eq!(s.r.playing(), Some(OPENING));
    press(&mut s, Button::Left, RETURN_TO_MENU.0, RETURN_TO_MENU.1);
    assert!(s.r.menu_open, "\"Return to main menu\" → the menu");
    assert_eq!(s.now, t);
    s.menu_choose(xf::world::devices::MenuChoice::Resume);
    ticks(&mut s, 10);
    assert!(s.now > t, "the menu's Return runs the game on");
    assert_eq!(s.r.playing(), Some(OPENING));
}

/// H2340: the opening's video text (the location title) is not drawn over Options — the screen looks the
/// same as one opened with no movie running.
#[test]
fn h2340_no_video_text_over_options() {
    let Some(mut s) = options_during_the_opening(Rules::all()) else { return };
    ticks(&mut s, 30);
    let during = s.render();
    // the same screen with nothing playing: opened after the opening's end
    let Some(mut t) = session(Start::NewGame, Rules::all()) else { return };
    ticks(&mut t, 600);
    host::key(&mut t, 0x1b);
    press(&mut t, Button::Left, OPTIONS_ITEM.0, OPTIONS_ITEM.1);
    ticks(&mut t, 30);
    let after = t.render();
    let differ = (0..during.len() / 3).filter(|&i| during[i * 3..i * 3 + 3] != after[i * 3..i * 3 + 3]).count();
    assert_eq!(differ, 0, "{differ} pixels differ: something of the paused game is drawn over Options");
}

/// Rule off: the port before — the game ran on under Options.
#[test]
fn h2340_rule_off_the_game_runs_on() {
    let Some(mut s) = options_during_the_opening(Rules { h2340_menu_screen_pauses: false, ..Rules::all() }) else {
        return;
    };
    let t = s.now;
    ticks(&mut s, 50);
    assert!(s.now > t);
}

/// H2341: the first menu's opening (C290) shows LEFTmain.mov alone over (0,0)–(472,480): no MIDloop.mov rect at
/// (61,103)–(449,413), no TOPro.mov rect at (57,0)–(409,60) over it — pixels of the original's shot
/// `traces/fo-226.jsonl.burst/c049.9.png` (LEFTmain ≈ 2.6 s in; the port's whole frame there differs from the shot by
/// 0.04 / 255 on average).
#[test]
fn h2341_first_menu_shows_leftmain_alone() {
    let Some(mut s) = session(Start::Menu, Rules::all()) else { return };
    ticks(&mut s, 26);
    assert!(s.r.menu_open && s.menu.first);
    let fb = s.render();
    let px = |x: usize, y: usize| [fb[(y * 640 + x) * 3], fb[(y * 640 + x) * 3 + 1], fb[(y * 640 + x) * 3 + 2]];
    // (x, y, the shot's RGB): inside MIDloop's rect, inside TOPro's rect
    for (x, y, want) in
        [(200, 200, [15, 33, 41]), (250, 380, [19, 40, 47]), (120, 30, [3, 19, 23]), (300, 40, [7, 33, 43])]
    {
        let got = px(x, y);
        assert!(got.iter().zip(want).all(|(&g, w)| (g as i32 - w).abs() <= 3), "({x},{y}): {got:?}, the shot {want:?}");
    }
}

/// H2344: during the first menu's opening (C290: LEFTmain.mov 0→5400, then RIGHT.mov 0→5000, two blocking waits inside
/// the menu opening routine FUN_0044e060 with no message pump) the menu takes no pointer input: no item / TOPro hover, no
/// hover sound, no click. The input is held and handled at the opening's end, in order (`traces/fo-224.jsonl`,
/// fo-226 / fo-227 / fo-228 / fo-242: no wnd_mouse between the menu's start and RIGHT.mov's end + 0.02 s; fo-226: the
/// bot's New click at 59.8, during RIGHT.mov's opening, reached the window proc at 62.367 and started the New Game).
#[test]
fn h2344_first_menu_opening_takes_no_pointer_input() {
    use xf::front::menu::{IDLE_MS, LEFT, LEFT_INTRO_MS, MID_SOUND, RIGHT};
    use xf::world::devices::MenuChoice;
    let Some(mut s) = session(Start::Menu, Rules::all()) else { return };
    let assets = |s: &mut Session| s.audio_cues().iter().map(|c| c.asset).collect::<Vec<_>>();
    ticks(&mut s, 20);
    assert!(s.r.menu_open && s.menu.first);
    // LEFTmain plays alone: the logo area (TOPro's rect) and the items do not react
    for (x, y) in [(200, 30), (570, 101), (555, 293)] {
        assert_eq!(host::motion(&mut s, Some((x, y))), host::Pointer::Arrow, "({x},{y})");
        s.tick(100);
        assert_eq!(assets(&mut s), vec![LEFT.0], "({x},{y}): LEFTmain's sound alone");
        assert!(s.menu.hover.is_none() && s.menu.top.is_none(), "({x},{y})");
    }
    // a click on New: nothing yet
    assert_eq!(host::button(&mut s, Button::Left, Some((570, 101))), None);
    host::release(&mut s, Button::Left, Some((570, 101)));
    s.tick(100);
    assert!(s.r.menu_open && s.menu.request.is_none() && s.menu.chosen.is_none() && s.menu.sounds.is_empty());
    // RIGHT.mov's opening (the items fading in): still no hover; its opening range sounds, not an item's
    ticks(&mut s, 40);
    let since = s.menu.clock - s.menu.opened;
    assert!((LEFT_INTRO_MS..LEFT_INTRO_MS + IDLE_MS).contains(&since), "{since}");
    host::motion(&mut s, Some((555, 145)));
    s.tick(100);
    assert!(s.menu.hover.is_none());
    let cues = s.audio_cues();
    assert_eq!(cues.iter().map(|c| c.asset).collect::<Vec<_>>(), vec![MID_SOUND, RIGHT.0]);
    assert_eq!(cues.iter().find(|c| c.asset == RIGHT.0).unwrap().start, 0, "RIGHT.mov's opening range, not Load's");
    assert!(s.r.menu_open && s.menu.request.is_none());
    // the opening's end delivers the held click on New (the New Game request)
    while s.menu.opening() {
        s.tick(100);
    }
    s.tick(100);
    assert_eq!(s.menu.request, Some(MenuChoice::New));
}

/// H2344: a later opening (the in-game Esc menu) takes the pointer at once.
#[test]
fn h2344_esc_menu_hover_at_once() {
    let Some(mut s) = session(Start::NewGame, Rules::all()) else { return };
    ticks(&mut s, 10);
    host::key(&mut s, 0x1b);
    s.tick(100);
    assert!(s.r.menu_open && !s.menu.first && !s.menu.opening());
    assert_eq!(host::motion(&mut s, Some((570, 101))), host::Pointer::Hand);
    assert!(s.menu.hover.is_some());
}
