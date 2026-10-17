//! Subtitle packs (H950–H955): `xf-prep --en-subs` scans an English copy of the game, keeps the captions of the movies
//! the RU data has under the same name and duration, and the Session shows them over the RU movies.
//! The RU tests use data/InstalledGame (skipped without it); the English DVD test uses the folder `XF_EN_DIR`
//! (e.g. the files of an English DVD Edition; skipped without it). No game text is part of the tests.
mod common;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use xf::front::{self, Session};
use xf::hdb::Hdb;
use xf::media::subs::{self, Match, Pack};

fn data() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    d.join("XFiles.hdb").exists().then_some(d)
}

fn en_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("XF_EN_DIR")?);
    d.is_dir().then_some(d)
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("xf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("English/MININST/XV")).unwrap();
    d
}

/// AssetRef 0x5b8a = XV/23434.xmv (whole file, the stab video), an RU movie that kept English text tracks (H341)
const STAB: u32 = 0x5b8a;

/// White pixels in the caption strip under the View (y 330..350)
fn strip_white(fb: &[u8]) -> usize {
    (330..350)
        .flat_map(|y| (20..620).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let o = (y * front::SCREEN_W + x) * 3;
            fb[o..o + 3] == [255, 255, 255]
        })
        .count()
}

/// H953 / H955: a stand-in "English" dir (a disc root with `English/…/XV/23434.XMV`, here a copy of the RU movie that
/// kept its text tracks) → scan finds its visible captions, build matches the RU movie of the same name and duration,
/// another movie under that name is left out (duration differs), and the pack round-trips through its file.
#[test]
fn h955_scan_build_pack() {
    let Some(d) = data() else { return };
    let root = tmp("ensubs");
    std::fs::copy(d.join("XV/23434.xmv"), root.join("English/MININST/XV/23434.XMV")).unwrap();
    std::fs::copy(d.join("XV/38448.xmv"), root.join("English/MININST/XV/41314.XMV")).unwrap();
    assert_eq!(subs::english_root(&root), root.join("English"));
    let (src, st) = subs::scan(&subs::english_root(&root));
    assert_eq!((st.movies, st.with_text, st.dup), (2, 2, 0));
    let s = &src["xv/23434.xmv"];
    assert_eq!(s.rel, "MININST/XV/23434.XMV");
    // the three sound captions (FNV-1a 64 of the texts, as tests/captions.rs)
    let texts: Vec<u64> = s.cues.iter().map(|c| common::fnv64(&c.text)).collect();
    assert_eq!(texts, [0xb59da570db3a3a36, 0xb38145c7df02c32a, 0x30fbc29450bc4c24]);
    assert_eq!((s.cues[0].start, s.cues[0].end), (1386, 2833));
    let (pack, m) = subs::build(&d, &src, "en");
    assert_eq!(m["xv/23434.xmv"], Match::Used);
    assert!(matches!(m["xv/41314.xmv"], Match::DurationDiffers(..)), "{:?}", m["xv/41314.xmv"]);
    assert_eq!(pack.movies.len(), 1);
    let file = subs::pack_path(&root.join("cache"), "en");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, pack.to_json()).unwrap();
    let back = Pack::load(&file).unwrap();
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(back, pack);
    assert_eq!(back.cues(&d.join("XV/23434.xmv")).unwrap(), s.cues.as_slice());
}

/// H955: the Session shows a pack's cue for the playing movie (movie-file time) instead of its own text tracks, in the
/// caption strip, only while captions are on.
#[test]
fn h955_pack_drawn_over_movie() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut pack = Pack { lang: "en".into(), ..Default::default() };
    pack.movies.insert(
        "xv/23434.xmv".into(),
        subs::PackMovie {
            src: "XV/23434.XMV".into(),
            dur_ms: 27_000,
            cues: vec![xf::media::captions::Cue { start: 3000, end: 5000, text: "PACK LINE".into() }],
        },
    );
    let mut s = Session::with_rules(&h, &g, &d, front::rules_hyp());
    s.r.new_game();
    s.sub_pack = Some(Arc::new(pack));
    s.r.set_closed_captioning(true);
    let t0 = s.now;
    s.play_preview(STAB);
    let at = |s: &mut Session, t: u64| {
        while s.now < t0 + t {
            s.tick(50);
        }
        s.render()
    };
    at(&mut s, 2000);
    assert!(s.captions().is_empty(), "the pack replaces the movie's own text tracks (its 1.4–2.8 s effect caption)");
    let fb = at(&mut s, 4000);
    let caps = s.captions();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].text, "PACK LINE");
    assert_eq!(caps[0].track, [0, 240, 600, 260], "the strip under the picture");
    if front::text::arial12().is_some() {
        assert!(strip_white(&fb) > 20, "white text in the strip");
    }
    s.r.set_closed_captioning(false);
    assert!(s.captions().is_empty());
}

/// FNV-1a 64 of XV/19706.XMV's visible cues ("start end text\n" each) from the DVD Edition
const H19706: &str = "dbb63765247752b9";

/// H951–H954 on an English DVD Edition (`XF_EN_DIR`, skipped without it): the scan's totals, the 19706 composite
/// (layered draft tracks → the corrected wording), and the RU coverage.
#[test]
fn h954_en_dvd_coverage() {
    let (Some(en), Some(d)) = (en_dir(), data()) else { return };
    let (src, st) = subs::scan(&subs::english_root(&en));
    // 2200: XN/25949.XMV's zlib-compressed movie header (cmov) is parsed since H2210
    assert_eq!((st.movies, st.with_text, st.dup, st.conflicts), (2200, 1526, 315, 0));
    assert_eq!(src.len(), 1144);
    assert_eq!(src.values().map(|s| s.cues.len()).sum::<usize>(), 2617);
    let m = &src["xv/19706.xmv"];
    assert_eq!(m.cues.len(), 9);
    // the front-most tracks hold the corrections (H952); checked by a hash so that no game text is in the source
    let all: String = m.cues.iter().map(|c| format!("{} {} {}\n", c.start, c.end, c.text)).collect();
    assert_eq!(format!("{:016x}", xf::media::pcmcache::fnv64(all.as_bytes())), H19706);
    let (pack, matched) = subs::build(&d, &src, "en");
    assert_eq!(matched.len(), 1144);
    assert_eq!(pack.movies.len(), 1144, "every captioned English movie has an RU movie of the same name and duration");
    let p = Path::new("XV/23434.xmv");
    assert!(pack.cues(p).is_some_and(|c| !c.is_empty()));
}
