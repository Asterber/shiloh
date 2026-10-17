//! Bug report: "sometimes, in the Field Office, a slideshow of pictures from the apartments starts".
//! Cause (H981): View 0x2b3's last background 0x432 (the "empty" Field Office picture, the fallback when its other
//! backgrounds are disabled) is nav1.nmv 0..0 — the first still of the movie. `Clip::is_still` required start ≠ 0, so the
//! port played the background as the whole movie, looped: one Node-1 still per second (Field Office, Willmore's
//! apartment …). Every one of the 1212 .nmv AssetRefs is a still (1208 start == end ≠ 0, 4 at 0..0, no range).
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::media::MediaLib;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The four 0..0 .nmv AssetRefs are stills; a 0..0 movie loop (XN/25929.xmv 0x6549) stays a whole movie.
#[test]
fn h981_nmv_zero_is_a_still() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let lib = MediaLib::new(&d);
    for a in [0x432, 0x1040, 0xe398, 0x1d6b] {
        let c = lib.resolve(&h, a).unwrap();
        assert_eq!((c.start, c.end), (0, 0));
        assert!(c.is_still(), "{a:#x} {}", c.name);
    }
    assert!(!lib.resolve(&h, 0x6549).unwrap().is_still());
    assert!(!lib.resolve(&h, 0x4d64).unwrap().is_still()); // SC03C.mov, a whole movie
}

/// fo-031-end.x stands in View 0x2b3 with background 0x432: idle frames 1 s apart are identical (before the fix every
/// second showed another nav1.nmv still).
#[test]
fn h981_field_office_background_does_not_cycle() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-031-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 60_000);
    assert_eq!((s.r.w.view, s.r.background), (Some(0x2b3), Some(0x432)));
    let mut frames = Vec::new();
    for _ in 0..4 {
        s.tick(1000);
        frames.push(s.render());
    }
    assert!(frames.windows(2).all(|w| w[0] == w[1]), "the background changes while idle");
}
