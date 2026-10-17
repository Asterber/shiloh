//! Text scrolling (H1810–H1815): the dialogue box's own scroller (FUN_00481ec0 and its arrow / held-button
//! handlers, H1810–H1813 — static, not traced) and the FBI computer's MainScroll items (H791) held down
//! (IsIItemHeld, H184 / H1814), plus the mouse wheel (port choice, H1815).
mod common;
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::dbox;

const LOGIN: &str = "p:70,210;p:570,210;p:570,210;p:251,281;p:320,210;p:83,282;p:320,210;p:466,300;p:320,210;p:70,210;p:320,210;p:412,147;p:320,210;p:171,271;p:320,210;p:187,223;p:320,210;p:570,210;p:2,92;p:70,210;p:570,210;p:318,200";
/// fo-114's ING phone search: Cook's Field Notes in panel 0xe3d9.. (H892), down arrow 0xe3df at (309,456)
fn cook_notes() -> Option<String> {
    Some(format!(";r:106,54;p:68,110;p:338,300;t:{};k:13", cook_number()?))
}
/// The number typed into fo-114's ING phone search: phone-directory entry 4 (text AssetRef 0x106cf, H575), read from
/// the user's install as its last 10 digits; None without the game data.
fn cook_number() -> Option<String> {
    let [n] = common::game_texts([0x106cf])?;
    let d: Vec<char> = n.chars().filter(char::is_ascii_digit).collect();
    assert!(d.len() >= 10, "a phone number");
    Some(d[d.len() - 10..].iter().collect())
}
/// fo-056's 20 steps from fo-054-end.x: Rudy's box at (185,285) with more rows than the talk panel holds (H304)
const RUDY: &str = "p:105,190;h:117,336,57a3;p:505,187;h:499,350,5686;p:536,92;p:570,210;p:287,209;p:220,370;p:314,209;\
    p:70,210;p:470,209;p:215,212;p:424,286;p:2,2;p:460,183;p:70,210;p:475,210;p:193,210;p:322,210;p:308,201";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// `save` loaded, `steps` applied (settled and rendered after each: the front measures the computer's panels, H791)
fn session(save: &str, steps: &str) -> Option<Session<'static>> {
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
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
        let _ = s.render();
    }
    Some(s)
}

fn dump(name: &str, fb: &[u8]) {
    if let Some(d) = std::env::var_os("XF_SCROLL_DUMP") {
        front::write_ppm(&PathBuf::from(d).join(name), fb).unwrap();
    }
}

fn mid(r: [i16; 4]) -> (i32, i32) {
    ((r[0] as i32 + r[2] as i32) / 2, (r[1] as i32 + r[3] as i32) / 2)
}

fn press_release(s: &mut Session, (x, y): (i32, i32)) {
    s.hover_at(x, y);
    s.press_at(x, y);
    s.release_at(Some((x, y)));
    s.r.run(100_000);
}

/// Pure model (H1810 / H892): rows = lines plus half-line gaps; the largest first row leaves the text's bottom inside
/// the view; page down = the view less a line; page up from near the top = the top.
#[test]
fn h1810_scroll_model() {
    // Rudy-like: entries of 1, 1, 2, 2 lines at 15 px, gaps 7: 15 7 15 7 15 15 7 15 15 = 111 px in a 90-px view
    let rows = dbox::scroll_rows(&[1, 1, 2, 2], 15);
    assert_eq!(rows, vec![15, 7, 15, 7, 15, 15, 7, 15, 15]);
    let max = dbox::scroll_max(&rows, 90);
    assert_eq!(max, 2, "111 − (15 + 7) = 89 ≤ 90");
    assert_eq!(dbox::scroll_max(&dbox::scroll_rows(&[1, 2], 15), 90), 0, "fits: no scroller");
    use dbox::Scroll::*;
    assert_eq!(dbox::scroll_step(&rows, 90, 15, 0, max, LineDown), 1);
    assert_eq!(dbox::scroll_step(&rows, 90, 15, 2, max, LineDown), 2, "clamped");
    assert_eq!(dbox::scroll_step(&rows, 90, 15, 0, max, LineUp), 0);
    assert_eq!(dbox::scroll_step(&rows, 90, 15, 0, max, PageDown), 2);
    assert_eq!(dbox::scroll_step(&rows, 90, 15, 2, max, PageUp), 0);
    let long = dbox::scroll_rows(&[2; 10], 15);
    let lmax = dbox::scroll_max(&long, 90);
    let p = dbox::scroll_step(&long, 90, 15, 0, lmax, PageDown);
    let off = |k: usize| long[..k].iter().sum::<i32>();
    assert!(off(p) <= 75 && off(p + 1) > 75, "page down by at most the view less a line: {p}");
}

/// H1810: Rudy's box (fo-056 k20 state) — the scroller is there, the down arrow scrolls one row, the lines and their
/// hit rects move up by it, the thumb moves down the bar; the last line (cut at the panel before) comes into view and
/// is clickable; the up arrow goes back; the box's next showing starts at the top again.
#[test]
fn h1810_dbox_arrows_scroll() {
    let Some(mut s) = session("fo-054-end.x", RUDY) else { return };
    let l0 = s.r.w.dbox_layout().expect("box");
    let sc0 = l0.scroll.clone().expect("scroller: the rows do not fit");
    assert_eq!(sc0.first, 0);
    assert!(sc0.max >= 1, "{sc0:?}");
    assert_eq!(sc0.up, [417, 316, 429, 326]);
    assert_eq!(sc0.down, [417, 394, 429, 404]);
    assert_eq!(sc0.thumb[1], 326, "thumb at the bar's top");
    let fb0 = s.render();
    dump("rudy-0.ppm", &fb0);
    press_release(&mut s, mid(sc0.down));
    let l1 = s.r.w.dbox_layout().unwrap();
    let sc1 = l1.scroll.clone().unwrap();
    assert_eq!(sc1.first, 1);
    assert_eq!(sc1.offset, 15, "one text line");
    assert!(sc1.thumb[1] > sc0.thumb[1], "thumb moved down: {:?}", sc1.thumb);
    // the second line moved up by 15 px
    let y0 = l0.tops[1];
    let k = l1.lines.iter().position(|x| x.0 == l0.lines[1].0).unwrap();
    assert_eq!(l1.tops[k], y0 - 15);
    let fb1 = s.render();
    dump("rudy-1.ppm", &fb1);
    assert_ne!(fb0, fb1, "the text moved");
    // to the end: the last line is wholly inside the panel and is hit at its middle
    for _ in 0..10 {
        press_release(&mut s, mid(sc0.down));
    }
    let l2 = s.r.w.dbox_layout().unwrap();
    let sc2 = l2.scroll.clone().unwrap();
    assert_eq!(sc2.first, sc2.max, "clamped at the end");
    assert_eq!(sc2.thumb[3], sc2.bar[3], "thumb at the bar's bottom");
    let (last, r) = *l2.lines.last().unwrap();
    assert!(r[3] <= l2.panel[3], "last line inside the panel: {r:?}");
    assert_eq!(s.r.hit(mid(r).0, mid(r).1).map(|c| c.owner), Some(last));
    // a line scrolled out above the panel is not hit there
    assert!(l2.lines.iter().all(|x| x.1[1] >= l2.panel[1]));
    dump("rudy-end.ppm", &s.render());
    // back up
    press_release(&mut s, mid(sc0.up));
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, sc2.max - 1);
}

/// H1812: the bar below the thumb pages down, above it pages up; the thumb dragged to the bar's bottom shows the end
#[test]
fn h1812_dbox_bar_and_thumb() {
    let Some(mut s) = session("fo-054-end.x", RUDY) else { return };
    let sc = s.r.w.dbox_layout().unwrap().scroll.unwrap();
    press_release(&mut s, (mid(sc.bar).0, sc.bar[3] as i32 - 2));
    let paged = s.r.w.dbox_layout().unwrap().scroll.unwrap();
    assert!(paged.first > 0, "{paged:?}");
    let t = paged.thumb;
    press_release(&mut s, (mid(sc.bar).0, sc.bar[1] as i32 + 1));
    if t[1] > sc.bar[1] + 1 {
        assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, 0, "page up to the top (H892)");
    }
    // drag the thumb from the top to the bottom
    let (x, y) = mid(sc.thumb);
    s.hover_at(x, y);
    s.press_at(x, y);
    s.hover_at(x + 5, sc.bar[3] as i32 + 20);
    let dragged = s.r.w.dbox_layout().unwrap().scroll.unwrap();
    s.release_at(Some((x + 5, sc.bar[3] as i32 + 20)));
    assert_eq!(dragged.first, dragged.max);
    // after the button-up the pointer's moves do not drag
    s.hover_at(x, y);
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, dragged.max);
}

/// H1813: a held down arrow repeats its step until the button goes up
#[test]
fn h1813_dbox_arrow_held_repeats() {
    let Some(mut s) = session("fo-054-end.x", RUDY) else { return };
    let sc = s.r.w.dbox_layout().unwrap().scroll.unwrap();
    let (x, y) = mid(sc.down);
    s.hover_at(x, y);
    s.press_at(x, y);
    s.r.run(100_000);
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, 1);
    s.tick(front::SCROLL_REPEAT_FIRST_MS - 10);
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, 1, "no repeat yet");
    s.tick(20);
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, 2.min(sc.max));
    s.release_at(Some((x, y)));
    let at = s.r.w.dbox_layout().unwrap().scroll.unwrap().first;
    s.tick(1000);
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, at, "stops at the button-up");
}

/// H1815 (port choice): the wheel over the box steps its scroller; off the box it does nothing here
#[test]
fn h1815_wheel_dbox() {
    let Some(mut s) = session("fo-054-end.x", RUDY) else { return };
    let l = s.r.w.dbox_layout().unwrap();
    let (x, y) = mid(l.panel);
    assert!(s.wheel(x, y, 1));
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, 1);
    assert!(s.wheel(x, y, -1));
    assert_eq!(s.r.w.dbox_layout().unwrap().scroll.unwrap().first, 0);
    assert!(s.wheel(x, y, 5));
    let sc = s.r.w.dbox_layout().unwrap().scroll.unwrap();
    assert_eq!(sc.first, sc.max, "clamped");
}

/// H1814 / H184: holding the ING Field Notes' down arrow 0xe3df repeats MainScroll 1 through the item's Timer trigger
/// 0x103e0 (IsIItemHeld while the button is down: 250 ms, then every 100 ms) until the end of the text; after the
/// button-up the scroll stays
#[test]
fn h1814_computer_arrow_held_repeats() {
    let Some(cook) = cook_notes() else { return };
    let Some(mut s) = session("fo-040-end.x", &format!("{LOGIN}{cook}")) else { return };
    let max = s.r.w.pc.scroll_max.expect("measured");
    assert!(max >= 3, "{max}");
    s.hover_at(309, 456);
    s.press_at(309, 456);
    s.r.run(100_000);
    let _ = s.render();
    assert_eq!(s.r.w.pc.scroll, 1, "the click's own step");
    for _ in 0..20 {
        s.tick(100);
        let _ = s.render();
    }
    assert_eq!(s.r.w.pc.scroll, max, "repeated while held");
    s.release_at(Some((309, 456)));
    // up arrow 0xe3de held for 0.6 s: a few steps, then the button-up stops them
    s.hover_at(309, 375);
    s.press_at(309, 375);
    for _ in 0..6 {
        s.tick(100);
        let _ = s.render();
    }
    s.release_at(Some((309, 375)));
    let at = s.r.w.pc.scroll;
    assert!(at < max - 1, "{at}");
    for _ in 0..10 {
        s.tick(100);
        let _ = s.render();
    }
    assert_eq!(s.r.w.pc.scroll, at, "no step after the button-up");
}

/// H1814: a whole click (down + up at once) is not a held button — no repeat
#[test]
fn h1814_click_does_not_repeat() {
    let Some(cook) = cook_notes() else { return };
    let Some(mut s) = session("fo-040-end.x", &format!("{LOGIN}{cook};p:309,456")) else { return };
    assert_eq!(s.r.w.pc.scroll, 1);
    for _ in 0..10 {
        s.tick(100);
        let _ = s.render();
    }
    assert_eq!(s.r.w.pc.scroll, 1);
}

/// H1815 (port choice): the wheel over the computer clicks the nearest line-down / line-up item (MainScroll 1 / 0)
#[test]
fn h1815_wheel_computer() {
    let Some(cook) = cook_notes() else { return };
    let Some(mut s) = session("fo-040-end.x", &format!("{LOGIN}{cook}")) else { return };
    assert!(s.wheel(200, 410, 2));
    let _ = s.render();
    assert_eq!(s.r.w.pc.scroll, 2);
    assert!(s.wheel(200, 410, -1));
    let _ = s.render();
    assert_eq!(s.r.w.pc.scroll, 1);
}
