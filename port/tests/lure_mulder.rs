//! C308 / H2291 (fo-264, fo-265): luring Mulder into the iso chamber by hand, played through the front's
//! input path (front::host, 16-ms ticks — the window / web path) from the bot's fo-219-end.x (View 0xacfb, the dead
//! keyguard): the key, the chase through the base (each chase ViewPoint re-arms Timer 13; the right nav clears 0x857),
//! the Explorable 0xb2fc → View 0xb2fe 'trap Mulder' (movie XV/66733, Timer 10 = 9 s) and its red button = movie
//! hotspot 451 (screen (414,255)–(465,321), 0.633–2.567 s of the movie): a click inside the window → 0xb321 'Mulder
//! trapped' (C308); a click after it hits nothing and Timer 10 plays the gravestone (C309, fo-265).
use std::path::PathBuf;
use xf::front::host::{self, Button};
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn tick(s: &mut Session, ms: u64) {
    let mut t = 0;
    while t < ms {
        s.tick(16);
        t += 16;
    }
}

fn click(s: &mut Session, x: i32, y: i32) {
    host::motion(s, Some((x, y)));
    host::button(s, Button::Left, Some((x, y)));
    tick(s, 48);
    host::release(s, Button::Left, Some((x, y)));
}

/// fo-264's chase after the key, one click per View (0.7 s after each transition, as the bot): (point, View reached)
const CHASE: [((i32, i32), u32); 13] = [
    ((570, 210), 0x3885),
    ((320, 210), 0x38c7),
    ((70, 210), 0x38c0),
    ((301, 210), 0x406d),
    ((404, 210), 0x38d6),
    ((570, 210), 0x38dd),
    ((213, 211), 0x3909),
    ((309, 210), 0x394b),
    ((260, 209), 0x39eb),
    ((320, 210), 0x3a59),
    ((320, 210), 0x3993),
    ((570, 210), 0x3985),
    ((570, 210), 0xb2fe),
];

/// fo-219-end.x → the key → the chase → View 0xb2fe; None without the data.
fn to_trap<'a>(h: &'a Hdb, g: &'a Hdb) -> Option<Session<'a>> {
    let mut s = Session::with_rules(h, g, &root().join("data/InstalledGame"), Rules::all());
    s.r.w.live_rand = Some(0);
    s.load_game();
    settle_timers(&mut s, 100, 900_000);
    assert_eq!(s.r.w.view, Some(0xacfb), "fo-219-end.x: the dead keyguard");
    // the key (fo-264 114.481): INCOMPC 0xbf77, then 0x387e (0xe5ed / 0xe5ee: 0x857 := 1, Timer 13 = 8 s)
    click(&mut s, 335, 200);
    for _ in 0..800 {
        if s.r.w.view == Some(0x387e) {
            break;
        }
        tick(&mut s, 16);
    }
    assert_eq!(s.r.w.view, Some(0x387e), "the key → 0x387e");
    assert_eq!((s.r.w.vars.get(0x856), s.r.w.vars.get(0x857)), (Some(1), Some(1)), "Mulder chasing");
    tick(&mut s, 700);
    for (k, &((x, y), view)) in CHASE.iter().enumerate() {
        click(&mut s, x, y);
        assert_eq!(s.r.w.view, Some(view), "chase step {k} ({x},{y})");
        assert!(!s.r.posts.iter().any(|m| m.action == 0x138e0), "no Alien Blast (step {k})");
        if view != 0xb2fe {
            tick(&mut s, 700);
        }
    }
    Some(s)
}

fn data() -> Option<(Hdb, Hdb)> {
    let h = Hdb::open(root().join("data/InstalledGame/XFiles.hdb")).ok()?;
    let g = Hdb::open(root().join("data/bot/fo-219-end.x")).ok()?;
    Some((h, g))
}

/// C308 (fo-264): the red button (hotspot 451) 1.5 s after the turn → cViewState 't', the View removed → 0xb2fe's
/// Deactivation Set View 0xb321, no gravestone.
#[test]
fn c308_red_button_in_its_window_traps_mulder() {
    let Some((h, g)) = data() else { return };
    let mut s = to_trap(&h, &g).unwrap();
    assert_eq!(s.r.w.vars.get(0x856), Some(0), "0xb351: the chase is over at 'trap Mulder'");
    tick(&mut s, 1500);
    click(&mut s, 440, 290);
    tick(&mut s, 200);
    assert_eq!(s.r.w.view, Some(0xb321), "Mulder trapped");
    assert!(s.r.posts.iter().any(|m| m.action == 0xb338), "0xb338 (0x5d7 = 't') Set View");
    tick(&mut s, 9000);
    assert!(!s.r.posts.iter().any(|m| m.action == 0xb30f), "no gravestone from Timer 10");
}

/// C309 (fo-265): the same click 4.6 s after the turn (after hotspot 451's window) hits nothing; Timer 10 (9 s) plays
/// the gravestone (0xb30d's 0x16c91 / 0xb30f).
#[test]
fn c309_red_button_after_its_window_does_nothing() {
    let Some((h, g)) = data() else { return };
    let mut s = to_trap(&h, &g).unwrap();
    tick(&mut s, 4600);
    click(&mut s, 440, 290);
    tick(&mut s, 200);
    assert_eq!(s.r.w.view, Some(0xb2fe), "the late click hits nothing");
    tick(&mut s, 4500);
    assert!(s.r.posts.iter().any(|m| m.action == 0xb30f), "Timer 10 → the gravestone");
    assert!(!s.r.posts.iter().any(|m| m.action == 0xb338), "no 'Mulder trapped'");
}
