//! H280 (runner): a sound player does not hold the one-player FIFO (A3). A bug reported in the window: New Game →
//! View 0x4d5, the stereo 0x10dc4 starts the music 0xcc6b (XS/52331.mus, 87.7 s), the full-screen item 0x109c0 is
//! dismissed, then the ringing phone 0x681f is clicked: the call video 0x57f2 (play 0xdb3f) must start at once — under
//! A3 it waited behind the music and started ~85 s later, in whatever View the player had walked to.
use std::path::PathBuf;
use xf::front::{self, Session};
use xf::hdb::Hdb;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// window-like clock: `ms` of 16-ms ticks
fn run(s: &mut Session, ms: u64) {
    let t0 = s.now;
    while s.now - t0 < ms {
        s.tick(16);
    }
}

fn click(s: &mut Session, x: i32, y: i32) -> Option<u32> {
    s.hover_at(x, y);
    s.click_at(x, y).map(|c| c.owner)
}

/// Returns (ms from the phone click until 0x57f2 plays, View then).
fn phone_after_music(h280: bool) -> Option<(u64, Option<u32>)> {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut rules = front::rules_hyp();
    rules.h280_sound_no_hold = h280;
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.new_game();
    front::settle_timers(&mut s, 16, 900_000); // the intro
    assert_eq!(s.r.w.view, Some(0x2b3));
    assert_eq!(click(&mut s, 161, 210), Some(0x1432c)); // forward → 0x2d7
    run(&mut s, 3000);
    assert_eq!(click(&mut s, 215, 212), Some(0x6e33)); // → 0x4d5
    run(&mut s, 3000); // Timer 5 (2 s): the ring 0xcc41 starts, the phone 0x681f is registered
    assert_eq!(s.r.w.view, Some(0x4d5));
    assert_eq!(click(&mut s, 520, 229), Some(0x10dc4)); // the stereo: music 0xcc6b + full-screen item 0x109c0
    run(&mut s, 1000);
    assert!(s.r.media.contains(&0xcc6b), "{:x?}", s.r.media);
    assert_eq!(s.r.started().contains(&0xcc6b), h280, "{:x?}", s.r.media); // A3: queued behind the ring
    assert_eq!(click(&mut s, 556, 281), Some(0x109c0)); // dismiss
    run(&mut s, 1000);
    assert_eq!(click(&mut s, 556, 281), Some(0x681f)); // the ringing phone
    let t0 = s.now;
    while s.now - t0 < 120_000 {
        s.tick(16);
        if s.r.playing() == Some(0x57f2) {
            return Some((s.now - t0, s.r.w.view));
        }
        if s.now - t0 == 2000 {
            click(&mut s, 2, 92); // the user walks on (left → 0x4d4) when nothing happens
        }
    }
    panic!("the call video 0x57f2 never played (h280 {h280})");
}

#[test]
fn h280_phone_call_not_held_by_music() {
    let Some((ms, view)) = phone_after_music(true) else { return };
    eprintln!("H280: call video {ms} ms after the click");
    assert!(ms < 1000, "call video after {ms} ms");
    assert_eq!(view, Some(0x4d5));
    // ablation: A3 (one FIFO for sound and video) — the call waits for the 87.7 s music, the user has left the View
    let (ms, view) = phone_after_music(false).unwrap();
    eprintln!("A3: call video {ms} ms after the click, View {view:x?}");
    assert!(ms > 60_000, "call video after {ms} ms");
    assert_ne!(view, Some(0x4d5));
}
