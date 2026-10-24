//! The cold start (C240 / C241, fo-225 / fo-226; static side H1660 / H1661): the intro movies (FUN_004721a0), a click
//! skips the current one, then the main menu with no game running; its New starts the game up to View 0x2b3.
use std::path::PathBuf;
use xf::front::host::{self, Button, HostReq};
use xf::front::intro::{Start, MOVIES};
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::devices::MenuChoice;
use xf::world::Rules;

fn data() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    if d.join("XFiles.hdb").exists() {
        Some(d)
    } else {
        eprintln!("data missing: skipped");
        None
    }
}

fn px(fb: &[u8], x: usize, y: usize) -> u32 {
    fb[(y * 640 + x) * 3..(y * 640 + x) * 3 + 3].iter().map(|&b| b as u32).sum()
}

/// Bright pixels in a rect (x0..x1, y0..y1), step 4.
fn lit(fb: &[u8], x0: usize, x1: usize, y0: usize, y1: usize) -> usize {
    (y0..y1)
        .step_by(4)
        .flat_map(|y| (x0..x1).step_by(4).map(move |x| (x, y)))
        .filter(|&(x, y)| px(fb, x, y) > 90)
        .count()
}

/// Headless frames: the first logo (0xdac3, 140×144, centred on black), the other movies in order, then the menu; the
/// menu's New reaches View 0x2b3.
#[test]
fn h1660_logo_then_menu_then_new_game() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.boot(Start::Intro);
    assert_eq!(s.intro_movie(), Some(MOVIES[0]));
    assert!(!s.r.menu_open && !s.idle());
    s.tick(1500);
    let logo = s.render();
    assert!(lit(&logo, 250, 390, 170, 310) > 20, "the logo is drawn in the screen's centre");
    assert_eq!(lit(&logo, 0, 640, 0, 160) + lit(&logo, 0, 640, 320, 480), 0, "black around the logo");
    // the movies follow one another by their lengths; the game's queue does not run meanwhile
    let mut seen = vec![MOVIES[0]];
    let mut t = 1500;
    while s.intro_playing() && t < 200_000 {
        s.tick(500);
        t += 500;
        if let Some(m) = s.intro_movie() {
            if seen.last() != Some(&m) {
                seen.push(m);
            }
        }
    }
    assert_eq!(seen, MOVIES.to_vec());
    assert!((136_000..137_100).contains(&t), "intro length {t} ms (C240: the menu 136.6 s after the first logo)");
    assert!(s.r.menu_open && !s.r.game_running, "main menu, no game");
    assert_eq!(s.r.w.view, None);
    // H2344: past the opening (LEFTmain 5.4 s + RIGHT.mov 5.0 s), before which the menu takes no pointer input
    s.tick(11_000);
    let menu = s.render();
    assert!(lit(&menu, 480, 630, 100, 420) > 0, "menu labels drawn");
    // no game: the Save item is not there, Return keeps the menu
    assert_eq!(s.menu_hover(555, 195), None);
    host::key(&mut s, 0x1b);
    assert!(s.r.menu_open, "Esc (Return) with no game keeps the menu");
    // New → the host starts New Game (as the window / web hosts do) → the FBI intro → View 0x2b3
    host::motion(&mut s, Some((570, 101)));
    host::button(&mut s, Button::Left, Some((570, 101)));
    assert_eq!(host::request(&mut s), Some(HostReq::New));
    let g2: &Hdb = Box::leak(Box::new(Hdb::open(d.join("XFiles.gam")).unwrap()));
    s.reload(g2, false);
    assert!(!s.r.menu_open && s.r.game_running);
    settle_timers(&mut s, 100, 120_000);
    assert_eq!(s.r.w.view, Some(0x2b3));
}

/// FUN_00408cf0: a button down (left or right) ends the current movie only; keys other than Alt+X do nothing.
#[test]
fn h1660_click_skips_one_movie() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.boot(Start::Intro);
    s.tick(100);
    host::key(&mut s, 0x1b);
    host::key(&mut s, 0x20);
    host::key(&mut s, 0x0d);
    assert!(!host::end_playing(&mut s));
    host::motion(&mut s, Some((100, 100)));
    assert_eq!(s.intro_movie(), Some(MOVIES[0]), "C241: Esc / Space / Enter / moves do not skip");
    host::button(&mut s, Button::Left, Some((320, 240)));
    assert_eq!(s.intro_movie(), Some(MOVIES[1]));
    host::button(&mut s, Button::Right, None);
    assert_eq!(s.intro_movie(), Some(MOVIES[2]));
    host::button(&mut s, Button::Left, Some((10, 10)));
    host::button(&mut s, Button::Left, Some((10, 10)));
    assert!(!s.intro_playing() && s.r.menu_open, "past the last movie: the menu");
    // a click on the menu after the intro is the menu's
    assert_eq!(host::button(&mut s, Button::Left, Some((570, 101))).as_deref(), Some("menu: New"));
    assert_eq!(s.menu.request, Some(MenuChoice::New));
}

/// Alt+X during the intro: quit (FUN_004721a0 returns 0 → the program ends).
#[test]
fn h1660_alt_x_quits() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.boot(Start::Intro);
    assert!(host::intro_abort(&mut s));
    assert_eq!(host::request(&mut s), Some(HostReq::Quit));
    assert!(!host::intro_abort(&mut s));
}

/// `--skip-intro` and PrefSkipIntro (var 0x3e2 ≠ 0): the menu at once; `--new-game`: the old start.
#[test]
fn h1660_skip_intro() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.boot(Start::Menu);
    assert!(!s.intro_playing() && s.r.menu_open && !s.r.game_running);

    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.w.vars.set_native(xf::front::intro::VAR_SKIP_INTRO, Some(1), 0);
    s.boot(Start::Intro);
    assert!(!s.intro_playing() && s.r.menu_open, "PrefSkipIntro set: no intro");

    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.boot(Start::NewGame);
    assert!(!s.intro_playing() && !s.r.menu_open && s.r.game_running);
    settle_timers(&mut s, 100, 120_000);
    assert_eq!(s.r.w.view, Some(0x2b3));
}
