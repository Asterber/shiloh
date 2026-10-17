//! Bug report (H2210): "the first conversation with Smolnikoff — a grey screen". Smolnikoff's
//! lair View 0x1549 (reached by Explorable 0xaa75, C173) has the background AssetRef 0x655d = XN/25949.XMV (the original
//! opens it: `traces/fo-186.jsonl` 148.075 mv_new of owner 0x655d; shot `traces/fo-186.jsonl.shots/k12.png` shows the
//! room). That file is the game's only movie with a compressed header (moov → cmov, dcom 'zlib', cmvd); the port's
//! QuickTime reader found no mvhd, the player failed ("not a QuickTime movie") and the movie area stayed the plain fill
//! under the dialogue box.
use std::path::PathBuf;
use xf::front::{script, settle_timers, Session};
use xf::hdb::Hdb;
use xf::media::qt::{Handler, Movie};
use xf::media::MoviePlayer;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Distinct RGB values of an area (the plain fill has 1).
fn colours(rgb: &[u8]) -> usize {
    let mut v: Vec<[u8; 3]> = rgb.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    v.sort();
    v.dedup();
    v.len()
}

/// XN/25949.XMV: the inflated header is a 7.5-s 'jpeg' movie, 38 samples; its frames decode. Every movie file of the
/// game parses (the header read by ranges, as the web build does).
#[test]
fn h2210_compressed_movie_header() {
    let d = root().join("data/InstalledGame");
    if !d.join("XN/25949.XMV").exists() {
        return;
    }
    let f = d.join("XN/25949.XMV");
    let m = Movie::open(&f).unwrap();
    assert_eq!((m.timescale, m.duration), (600, 4484));
    let v: Vec<_> = m.tracks.iter().filter(|t| t.handler == Handler::Video).collect();
    assert_eq!(v.len(), 1);
    assert_eq!((&v[0].fourcc, v[0].samples.len(), v[0].width, v[0].height), (b"jpeg", 38, 600, 240));
    assert_eq!(Movie::open_header(&f).unwrap().tracks.len(), m.tracks.len());
    let mut p = MoviePlayer::open(&f).unwrap();
    assert_eq!(p.seek(0), Ok(true));
    assert!(colours(&p.rgb) > 1000, "a picture: {}", colours(&p.rgb));
    let (mut n, mut bad) = (0, Vec::new());
    for dir in ["", "XN", "XV", "XG", "XA", "XS"] {
        let Ok(rd) = std::fs::read_dir(d.join(dir)) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if ["xmv", "nmv", "mov"].contains(&ext.as_str()) {
                if Movie::open_header(&p).is_err() {
                    bad.push(format!("{dir}/{}", e.file_name().to_string_lossy()));
                }
                n += 1;
            }
        }
    }
    assert!(n > 1000, "{n} movie files");
    bad.sort();
    // these two are a bare 'mdat' (size 0 = to the end of the file) with no movie header at all — not decoded here
    assert_eq!(bad, ["XG/94122.xmv", "XG/94215.xmv"]);
}

/// fo-185-end.x, the fo-186 walk (k0..k10) and Explorable 0xaa75 (k11), then Smolnikoff's CharView (314,204): View
/// 0x1549 with its background 0x655d drawn under the dialogue box — not the flat movie-area fill.
#[test]
fn h2210_smolnikoff_lair_is_drawn() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-185-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let steps = "p:570,210;p:582,210;p:172,210;p:70,210;p:70,210;p:326,185;p:320,186;p:339,218;p:397,205;p:70,210;\
                 p:320,294;p:324,233;p:314,204";
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        settle_timers(&mut s, 100, 900_000);
    }
    assert_eq!(s.r.w.view, Some(0x1549));
    assert_eq!(s.r.background, Some(0x655d));
    let fb = s.render();
    // the movie area's top band (20..620 × 90..280), above the dialogue box
    let band: Vec<u8> = (90..280).flat_map(|y| fb[(y * 640 + 20) * 3..(y * 640 + 620) * 3].to_vec()).collect();
    assert!(colours(&band) > 1000, "the lair is drawn: {} colours", colours(&band));
}
