//! Bug report: "in View 0x963, hotspot 0x1c66 leads to a view that is just a grey fill".
//! Nav 0x1c66 (warehouse interior, View 0x963) → View 0xe92 (Morley's cigarette butt close-up), whose only
//! background is AssetRef 0xe2e "Navs:WarehouseR:Interior:22:Sc20fF_morly.mov" = nav2.nmv at 33001 (the original builds
//! it: `traces/fo-111.jsonl` 244.472 trig_register / mv_new of owner 0xe2e, step 9 of fo-111). That movie time is
//! track 1 sample 2, whose sample description (stsc id 2) is a 'cvid' entry in a track whose first entry is 'jpeg'
//! (H1940): the port decoded every sample with the first entry's codec, the JPEG decoder failed ("first two bytes are
//! not an SOI marker") and the View stayed the empty movie-area fill. The same cvid sample is in NAV2..NAV6.NMV track 1
//! (sample 2) and NAV4.NMV track 1 sample 28.
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::media::{MediaLib, MoviePlayer};
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Distinct RGB values (a flat fill has 1; the dark, near-grey butt close-up has ~180).
fn colours(rgb: &[u8]) -> usize {
    let mut v: Vec<[u8; 3]> = rgb.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    v.sort();
    v.dedup();
    v.len()
}

/// The sample codecs come from stsc's description ids; the cvid stills decode; every .nmv still AssetRef decodes.
#[test]
fn h1940_mixed_codec_track_samples_decode() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    for (f, cvid) in
        [("NAV2.NMV", &[2][..]), ("NAV3.NMV", &[2]), ("NAV4.NMV", &[2, 28]), ("NAV5.NMV", &[2]), ("NAV6.NMV", &[2])]
    {
        let mut p = MoviePlayer::open(&d.join(f)).unwrap();
        let t = &p.movie.tracks[1];
        let got: Vec<usize> = (0..t.samples.len()).filter(|&s| t.sample_fourcc(s) == *b"cvid").collect();
        assert_eq!(got, cvid, "{f}");
        assert_eq!(p.movie.tracks[0].sample_codecs.len(), 0, "{f}: track 0 is all 'jpeg'");
        assert_eq!(p.seek(33001), Ok(true), "{f}");
        assert!(colours(&p.rgb) > 50, "{f}: the cvid still is a picture");
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let lib = MediaLib::new(&d);
    let mut players: std::collections::HashMap<String, MoviePlayer> = Default::default();
    let mut n = 0;
    for &(a, _) in &h.class(0x35).unwrap().objects {
        let Some(c) = lib.resolve(&h, a) else { continue };
        if !c.name.to_ascii_lowercase().ends_with(".nmv") || !c.is_still() {
            continue;
        }
        let p = players.entry(c.name.clone()).or_insert_with(|| MoviePlayer::open(&c.file).unwrap());
        assert_eq!(p.seek(c.start as u64), Ok(true), "AssetRef {a:#x} {} at {}", c.name, c.start);
        n += 1;
    }
    assert!(n > 1200, "{n} .nmv stills");
}

/// fo-110-end.x, the fo-111 prefix to View 0x963 (k1..k8), then nav 0x1c66 (k9 `p:162,308`): View 0xe92 with background
/// 0xe2e drawn — not the flat movie-area fill.
#[test]
fn h1940_butt_close_up_is_drawn() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-110-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let steps = "p:320,312;p:263,318;m:20,440;p:141,440;p:301,209;p:320,144;p:598,210;p:320,210;p:570,210;p:162,308";
    for st in steps.split(';') {
        let (kind, xy) = st.split_at(2);
        let v: Vec<i32> = xy.split(',').map(|x| x.parse().unwrap()).collect();
        s.hover_at(v[0], v[1]);
        if kind == "p:" {
            s.click_at(v[0], v[1]);
        }
        settle_timers(&mut s, 100, 900_000);
    }
    assert_eq!((s.r.w.view, s.r.background), (Some(0xe92), Some(0xe2e)));
    let frame = s.render();
    // the movie area of the 640×480 screen: (20, 90)–(620, 330)
    let area: Vec<u8> = (90..330).flat_map(|y| frame[(y * 640 + 20) * 3..(y * 640 + 620) * 3].to_vec()).collect();
    assert!(colours(&area) > 50, "View 0xe92 is a flat fill ({} colours)", colours(&area));
}
