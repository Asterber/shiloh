//! Robustness regressions found by a monkey-test driver: inputs a player can give in the window
//! that made the headless port panic, hang or grow without bound. Each test is the minimised reproducer of one
//! failure; see RE_NOTES §7 "Robustness (monkey)".
use std::path::PathBuf;
use xf::front::{self, Session};
use xf::hdb::Hdb;

fn data() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    d.join("XFiles.hdb").exists().then_some(d)
}

/// A movie whose frame is not 600×240 shown in the movie area (here through the port viewer's `--play`; the same
/// path draws the playing head, a looping player and the View background): the copy into the frame indexed the movie
/// as 600×240 and panicked on a smaller one (XV/56003.xmv is 140×144, XV/87345.xmv 502×64, XG/24465.xmv 80×60).
#[test]
fn render_movie_of_other_size() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    for a in [0xdac3u32, 0x15531, 0x5f91, 0x4cd2, 0xc1b3] {
        let mut s = Session::with_rules(&h, &g, &d, front::rules_hyp());
        s.play_preview(a);
        s.tick(100);
        let fb = s.render();
        assert_eq!(fb.len(), front::SCREEN_W * front::SCREEN_H * 3, "asset {a:#x}");
    }
}

/// A sound track counts every sound frame as a QuickTime sample: XS/80788.mus (a minute of QDMC music) has 4 276 224.
/// The parsed per-sample table (offset, size, time: ~100 MB) was kept by every cached MoviePlayer of such a file and
/// made the window's memory jump by hundreds of MB when music started (monkey fo-054-end seed 1, step 1179). Sound
/// tracks are read by chunks only; the decoded sound is unchanged.
#[test]
fn sound_track_has_no_per_sample_table() {
    let Some(d) = data() else { return };
    let p = d.join("XS/80788.mus");
    let Ok(b) = std::fs::read(&p) else { return };
    let m = xf::media::qt::Movie::parse(&b).unwrap();
    let t = m.tracks.iter().find(|t| t.handler == xf::media::qt::Handler::Sound).unwrap();
    assert!(t.samples.is_empty() && t.times.is_empty());
    assert_eq!(t.chunks.len(), 388);
    assert_eq!(t.chunks.iter().map(|c| c.1 as u64).sum::<u64>(), 4_276_224);
    // video tracks keep their table (XV/19937.xmv: a scene movie with sound)
    let v = xf::media::qt::Movie::parse(&std::fs::read(d.join("XV/19937.xmv")).unwrap()).unwrap();
    for t in &v.tracks {
        assert_eq!(t.samples.is_empty(), t.handler == xf::media::qt::Handler::Sound, "{:?}", t.handler);
    }
}

/// The decoded-sound cache kept every sound track the session ever played (the game's tracks decode to ~1.2 GB): it
/// now holds `sound_budget` bytes, dropping the oldest unused sounds; a sound still held elsewhere (a mixer voice) and
/// the one just decoded stay, and a dropped one decodes again the same.
#[test]
fn sound_cache_is_bounded() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, front::rules_hyp());
    s.sound_budget = 1;
    let sounds = |s: &Session| s.cache_stats().iter().find(|x| x.0 == "sounds").map_or(0, |x| x.1);
    // three scene movies with sound (the menu's MIDloop.amv, its click, a View movie)
    let assets = [front::menu::MID_SOUND, front::menu::CLICK_SOUND, 0x4d64];
    let first = s.sound(assets[0]).expect("MIDloop.amv sound");
    let held = first.clone(); // a mixer voice
    for &a in &assets[1..] {
        assert!(s.sound(a).is_some(), "{a:#x} has a sound track");
    }
    // the held one and the last decoded stay, the middle one went
    assert_eq!(sounds(&s), 2);
    drop(held);
    drop(first);
    let again = s.sound(assets[1]).expect("decoded again");
    assert!(again.samples.len() > 0);
    assert!(sounds(&s) <= 2);
}

/// The core's diagnostic histories grew for as long as the window ran (a scene's periodic timer: ~16 conditions /
/// posts a game second, monkey fo-047-end seed 5). The window and the web loop bound them with `trim_history`; the
/// state the game runs on is untouched.
#[test]
fn history_is_trimmed() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, front::rules_hyp());
    s.r.new_game();
    front::settle_timers(&mut s, 100, 120_000);
    let (view, active) = (s.r.w.view, s.r.active.clone());
    assert!(s.r.conds.len() > 100);
    s.r.trim_history(100);
    assert!(s.r.conds.len() <= 50 && s.r.posts.len() <= 100 && s.r.execs.len() <= 100 && s.r.lifecycle.len() <= 100);
    assert_eq!((s.r.w.view, s.r.active.clone()), (view, active));
    s.tick(1000);
    let _ = s.render();
}
