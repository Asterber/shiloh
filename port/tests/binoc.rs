//! The binoculars switch the View's background (H1420; C230 `traces/fo-198.jsonl`): at the railyard pole
//! top (View 0x21ba, list 0x23fd = [0x1ec7, 0x1ec6]) selecting the binoculars starts 0x1ec6 (its start trigger 0x7dfa:
//! 0x421 := 0x1ec6, 0x13b9 bRYBoxcarEnabled := 1); the Esc drop of the binoculars brings 0x1ec7 back (trigger 0x11fe4:
//! 0x421 := 0x1ec7). Ablation: H1420 off.
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::devices::MenuChoice;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn settle(s: &mut Session) {
    settle_timers(s, 100, 900_000);
    for _ in 0..12 {
        s.tick(100);
    }
}

/// (background before, after the binoculars, after the Esc drop; 0x421 after the binoculars, after the drop; 0x13b9
/// after the binoculars; frames before / with the binoculars differ)
fn run(rules: Rules) -> Option<(Option<u32>, Option<u32>, Option<u32>, Option<i32>, Option<i32>, Option<i32>, bool)> {
    let d = root().join("data/InstalledGame");
    let save2 = root().join("data/bot/fo-198-end.x");
    if !d.join("XFiles.hdb").exists() || !save2.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    // fo-198-end.x: the pole top 0x21ba, binoculars not held (dropped by the save's Esc), 0x13b9 already 1 — take the
    // boxcar flag back to 0 so the start trigger's write is seen
    let g = Hdb::open(&save2).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle(&mut s);
    assert_eq!(s.r.w.view, Some(0x21ba), "pole top");
    assert_eq!(s.r.w.dev.held, None);
    s.r.w.vars.set_native(0x13b9, Some(0), 1);
    let bg0 = s.r.background;
    let f0 = s.render();
    s.hover_at(20, 440);
    settle(&mut s);
    s.hover_at(246, 439);
    s.click_at(246, 439);
    settle(&mut s);
    assert_eq!(s.r.w.dev.held, Some(1), "binoculars held");
    assert!(
        !rules.h1420_binoc_view_swap || !s.r.w.dev.held_overlay,
        "H1772: kind 2 (the View pair 0x1ec7 / 0x1ec6): no mask, the movie is the binocular view"
    );
    let bg1 = s.r.background;
    let (v1, b1) = (s.r.w.vars.get(0x421), s.r.w.vars.get(0x13b9));
    let f1 = s.render();
    s.r.open_menu();
    s.r.menu_choose(MenuChoice::Resume);
    settle(&mut s);
    assert_eq!(s.r.w.dev.held, None, "dropped");
    Some((bg0, bg1, s.r.background, v1, s.r.w.vars.get(0x421), b1, f0 != f1))
}

#[test]
fn h1420_binoculars_switch_the_pole_top_background() {
    let Some((bg0, bg1, bg2, v1, v2, b1, differs)) = run(Rules::all()) else { return };
    assert_eq!(bg0, Some(0x1ec7), "RR_5S_WIDE before");
    assert_eq!(bg1, Some(0x1ec6), "the binocular view 0x1ec6");
    assert_eq!(v1, Some(0x1ec6), "0x7dfa: 0x421 := 0x1ec6");
    assert_eq!(b1, Some(1), "0x160c6: bRYBoxcarEnabled := 1");
    assert!(differs, "the front shows the binocular background");
    assert_eq!(bg2, Some(0x1ec7), "the drop brings RR_5S_WIDE back");
    assert_eq!(v2, Some(0x1ec7), "0x11fe7: 0x421 := 0x1ec7");

    let Some((_, bg1, _, _, _, b1, _)) = run(Rules { h1420_binoc_view_swap: false, ..Rules::all() }) else { return };
    assert_eq!(bg1, Some(0x1ec7), "ablation: no switch");
    assert_eq!(b1, Some(0), "ablation: no boxcar");
}

/// H1772 (static, not traced): the binoculars in a View without a binocular special (Cook's office 0x319, fo-056-end.x:
/// kind 0) draw the mask 0x87c1 (600×275) centred on the pointer, multiplying the band: the centre unchanged, the
/// mask's black corners dark; the pictures outside the band untouched.
#[test]
fn h1772_binocular_mask_in_a_plain_view() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-056-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle(&mut s);
    s.hover_at(20, 440);
    settle(&mut s);
    let (x, y) = xf::front::click_point(&s, 0x553d).expect("the binoculars in the bar");
    s.hover_at(x, y);
    s.click_at(x, y);
    settle(&mut s);
    assert_eq!((s.r.w.dev.held, s.r.w.dev.held_overlay), (Some(1), true), "binoculars held, kind 0");
    s.hover_at(320, 210);
    let masked = s.render();
    s.r.w.dev.held_overlay = false;
    let plain = s.render();
    let px = |fb: &[u8], x: usize, y: usize| fb[(y * 640 + x) * 3..(y * 640 + x) * 3 + 3].to_vec();
    assert_eq!(px(&masked, 160, 210), px(&plain, 160, 210), "inside the left lens: unchanged");
    assert_eq!(px(&masked, 22, 92), vec![0, 0, 0], "the mask's corner: black");
    assert!(
        plain[(92 * 640 + 22) * 3..(92 * 640 + 22) * 3 + 3].iter().any(|&c| c > 0),
        "the plain View is not black there"
    );
    assert_eq!(px(&masked, 320, 400), px(&plain, 320, 400), "below the band: untouched");
}
