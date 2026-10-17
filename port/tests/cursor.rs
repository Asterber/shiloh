//! The cursor (front::cursor): H330 animation, H331 crosshair, H307 shapes.
use std::path::PathBuf;
use xf::front::{self, cursor, script, Session};
use xf::hdb::Hdb;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Load `save`, settle, put the pointer at (x, y) with the game cursor drawn.
fn session(save: &str, x: i32, y: i32) -> Option<Session<'static>> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, front::rules_hyp());
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    for i in script::parse(&format!("m:{x},{y}")).unwrap() {
        script::apply(&mut s, &i);
    }
    s.game_cursor = true;
    Some(s)
}

fn px(fb: &[u8], x: i32, y: i32) -> [u8; 3] {
    let o = (y as usize * front::SCREEN_W + x as usize) * 3;
    [fb[o], fb[o + 1], fb[o + 2]]
}

/// The cursor layer at the session's time: the pixels (in a 48×48 box around (x, y)) where the frame with the cursor
/// differs from the frame without it, with their colours (the View behind may move with time).
fn layer(s: &mut Session, x: i32, y: i32) -> Vec<((i32, i32), [u8; 3])> {
    s.game_cursor = false;
    let plain = s.render();
    s.game_cursor = true;
    let with = s.render();
    (y - 24..y + 24)
        .flat_map(|yy| (x - 24..x + 24).map(move |xx| (xx, yy)))
        .filter(|&(xx, yy)| px(&with, xx, yy) != px(&plain, xx, yy))
        .map(|p| (p, px(&with, p.0, p.1)))
        .collect()
}

/// pixels of the two layers that differ (position or colour)
fn diff(a: &[((i32, i32), [u8; 3])], b: &[((i32, i32), [u8; 3])]) -> usize {
    let sa: std::collections::HashSet<_> = a.iter().collect();
    let sb: std::collections::HashSet<_> = b.iter().collect();
    sa.symmetric_difference(&sb).count()
}

/// H330: the frame shown t ms after the activation, looped
#[test]
fn h330_frame_at() {
    let eye = [2000, 120, 1000, 1000];
    let at = |t| cursor::frame_at(&eye, t);
    assert_eq!(
        [at(0), at(1999), at(2000), at(2119), at(2120), at(3120), at(4119), at(4120), at(6120)],
        [0, 0, 1, 1, 2, 3, 3, 0, 1]
    );
    assert_eq!(cursor::frame_at(&[100; 5], 730), 2);
    assert_eq!(cursor::frame_at(&[], 5), 0);
    assert_eq!(cursor::frame_at(&[0, 0], 5), 0);
}

/// H330: the PFF header's second table = the frame durations of the animated cursors' files
#[test]
fn h330_pff_durations() {
    let xg = root().join("data/InstalledGame/XG");
    let dur = |f: &str| xf::media::pict::Pff::open(&xg.join(f)).ok().map(|p| p.durations());
    let Some(eye) = dur("07093.pff") else { return };
    assert_eq!(eye, [2000, 120, 1000, 1000]);
    assert_eq!(dur("17477.pff").unwrap(), [100; 5]);
    let mut bx = vec![100; 11];
    bx.push(1000);
    assert_eq!(dur("25538.pff").unwrap(), bx);
    assert_eq!(dur("58092.pff").unwrap(), [100; 4]);
}

/// H330: frames of the cursors — the eye / fist / blue X / stiletto are animations, the hands and lips one still each
#[test]
fn h330_cursor_frames() {
    let Some(mut s) = session("fo-062-end.x", 500, 160) else { return };
    let n = |s: &mut Session, id| cursor::frames(s, id).len();
    assert_eq!([n(&mut s, 0x1bfc), n(&mut s, 0x4454), n(&mut s, 0x63c5), n(&mut s, 0xe2ef)], [4, 5, 12, 4]);
    assert_eq!([n(&mut s, 0x11c), n(&mut s, 0x11d), n(&mut s, 0x11e), n(&mut s, 0x122), n(&mut s, 0x7c8c)], [1; 5]);
}

/// H330: over an eye region (fo-062 end, (500,160)) the cursor blinks: frame 0 for 2 s, then the blink frame for 120 ms,
/// … looped (4120 ms) — frames rendered at different times show different cursor pictures; the loop returns to frame 0.
#[test]
fn h330_eye_cursor_animates() {
    let Some(mut s) = session("fo-062-end.x", 500, 160) else { return };
    assert_eq!(front::ui::cursor_id(&s), Some(0x1bfc));
    let t0 = s.now;
    let f0 = layer(&mut s, 500, 160);
    s.now = t0 + 1000;
    let f0b = layer(&mut s, 500, 160);
    s.now = t0 + 2050;
    let f1 = layer(&mut s, 500, 160);
    s.now = t0 + 4120 + 500;
    let f0c = layer(&mut s, 500, 160);
    assert!(f0.len() > 150, "{}", f0.len());
    assert_eq!(diff(&f0, &f0b), 0, "still frame 0 at 1 s");
    assert!(diff(&f0, &f1) > 100, "blink frame at 2.05 s: {}", diff(&f0, &f1));
    assert_eq!(diff(&f0, &f0c), 0, "looped back to frame 0");
}

/// H330: the animation restarts when the cursor becomes current again (a new activation rebuilds its frames)
#[test]
fn h330_restart_on_activation() {
    let Some(mut s) = session("fo-062-end.x", 40, 150) else { return };
    assert_eq!(front::ui::cursor_id(&s), Some(0x4454));
    let t0 = s.now;
    let f0 = layer(&mut s, 40, 150);
    s.now = t0 + 250; // fist frame 2
    let f2 = layer(&mut s, 40, 150);
    assert!(diff(&f0, &f2) > 20, "{}", diff(&f0, &f2));
    s.mouse = Some((320, 450)); // off: crosshair
    s.render();
    s.mouse = Some((40, 150));
    let again = layer(&mut s, 40, 150); // activated at t0 + 250 → frame 0
    assert_eq!(diff(&f0, &again), 0);
}

/// H331: over nothing the null cursor = XFiles.exe cursor 0x81, a crosshair with its hot spot (7,7) at the pointer
#[test]
fn h331_crosshair_over_nothing() {
    let Some(mut s) = session("fo-054-end.x", 320, 450) else { return };
    assert_eq!(cursor::shape(&s), Some(cursor::Shape::Win(cursor::WIN_CROSSHAIR)));
    let fb = s.render();
    assert_eq!(px(&fb, 320, 450), [0, 0, 0], "black centre");
    assert_eq!(px(&fb, 320, 447), [255, 255, 255], "white bar above");
    assert_eq!(px(&fb, 317, 450), [255, 255, 255], "white bar left");
    assert_eq!(px(&fb, 319, 447), [0, 0, 0], "black outline");
    // no pointer → nothing drawn
    s.mouse = None;
    assert_eq!(cursor::shape(&s), None);
}

/// fo-176-end.x (View 0x664a): `steps` applied with the rule set `rules`, then the cursor shape.
fn shape_after(rules: xf::world::Rules, steps: &str) -> Option<cursor::Shape> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-176-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, rules);
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    s.game_cursor = true;
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        for _ in 0..20 {
            s.tick(50);
        }
    }
    cursor::shape(&s)
}

/// C301 (`traces/fo-253.jsonl`): the gun held — the gunsight 0x84 over the band (the held gun hit) and nothing else;
/// over Cook's Explorable (no reaction to the gun) its own fist 0x4454; another item over the band: the crosshair.
/// Ablation: C301 off → the crosshair with the gun.
#[test]
fn c301_gunsight_with_the_gun() {
    use cursor::Shape::{Game, Win};
    let gun = "m:20,440;p:351,440;m:450,150";
    let Some(sh) = shape_after(front::rules_hyp(), gun) else { return };
    assert_eq!(sh, Win(cursor::WIN_GUNSIGHT));
    assert_eq!(shape_after(front::rules_hyp(), &format!("{gun};m:236,209")), Some(Game(0x4454)));
    assert_eq!(shape_after(front::rules_hyp(), "m:20,440;p:316,440;m:450,150"), Some(Win(cursor::WIN_CROSSHAIR)));
    let off = xf::world::Rules { c301_gunsight_cursor: false, ..front::rules_hyp() };
    assert_eq!(shape_after(off, gun), Some(Win(cursor::WIN_CROSSHAIR)));
}

/// C302 (`traces/fo-253.jsonl`): item 0x440d held over Cook's Explorable 0xbaa3 (no Mouse Click reaction to it) → the
/// Explorable's own cursor 0x4454, not the item's 0x7c8c. Ablation: C302 off → the item's cursor.
#[test]
fn c302_no_item_cursor_without_a_reaction() {
    use cursor::Shape::Game;
    let item = "m:20,440;p:316,440;m:236,209";
    let Some(sh) = shape_after(front::rules_hyp(), item) else { return };
    assert_eq!(sh, Game(0x4454));
    let off = xf::world::Rules { c302_item_cursor_reaction: false, ..front::rules_hyp() };
    assert_eq!(shape_after(off, item), Some(Game(0x7c8c)));
}
