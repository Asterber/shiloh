//! Closed captions (H340–H344): the text tracks of the installed movies, the caption strip drawn under the View while
//! Pref_bClosedCaptioning (var 0xa0d) is on, the icon bars' second start (H150 / H343), user .srt files (H344).
mod common;
use std::path::PathBuf;
use xf::front::{self, Session};
use xf::hdb::Hdb;
use xf::media::captions::{movie_cues, Cue};
use xf::media::qt::Movie;

fn data() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    d.join("XFiles.hdb").exists().then_some(d)
}

/// AssetRef 0x5b8a = XV/23434.xmv (whole file, the stab video): an RU movie that kept two English text tracks
const STAB: u32 = 0x5b8a;

/// The stab video's three sound captions (start, end in movie-file ms, FNV-1a 64 of the text: the game's text is not
/// spelled out here)
const STAB_CUES: [(u64, u64, u64); 3] =
    [(1386, 2832, 0xb59da570db3a3a36), (9733, 12766, 0xb38145c7df02c32a), (17955, 26800, 0x30fbc29450bc4c24)];

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

fn session<'a>(h: &'a Hdb, g: &'a Hdb, d: &std::path::Path) -> Session<'a> {
    let mut s = Session::with_rules(h, g, d, front::rules_hyp());
    s.r.new_game();
    s
}

fn at(s: &mut Session, t: u64) -> Vec<u8> {
    while s.now < t {
        s.tick(50);
    }
    s.render()
}

/// H340: XV/23434.xmv's text tracks — a blank 600×20 track over the whole movie and the sound captions, in movie-file time.
#[test]
fn h340_text_track_cues() {
    let Some(d) = data() else { return };
    let bytes = std::fs::read(d.join("XV/23434.xmv")).unwrap();
    let m = Movie::parse(&bytes).unwrap();
    let texts: Vec<_> = m.tracks.iter().filter_map(|t| t.text.as_ref().map(|x| (t, x))).collect();
    assert_eq!(texts.len(), 2);
    for (t, x) in &texts {
        assert_eq!((t.tx, t.ty, t.track_w, t.track_h), (0, 240, 600, 20));
        assert_eq!((x.justify, x.bg, x.fg, x.text_box, x.size, x.ascent), (1, [0; 3], [255; 3], [2, 2, 598, 18], 12, 12));
        assert_eq!(x.font_name, "Geneva");
    }
    let c: Vec<(u64, u64, u64)> =
        movie_cues(&m, &bytes).iter().map(|c: &Cue| (c.start, c.end, common::fnv64(&c.text))).collect();
    assert_eq!(c, STAB_CUES);
}

/// H341 / H342: with captions on, the stab video shows its first sound caption in white on the black strip under the
/// picture at 2.0 s (inside the cue 1.386–2.832 s), nothing at 4.0 s (blank sample); with captions off nothing at 2.0 s.
#[test]
fn h342_caption_drawn_inside_cue_only() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = session(&h, &g, &d);
    assert!(!s.r.w.closed_captioning(), "XFiles.gam: Pref_bClosedCaptioning = 0");
    s.r.set_closed_captioning(true);
    let t0 = s.now;
    s.play_preview(STAB);
    let inside = at(&mut s, t0 + 2000);
    let caps = s.captions();
    assert!(caps.iter().any(|c| common::fnv64(c.text.trim()) == STAB_CUES[0].2), "{caps:?}");
    let outside = at(&mut s, t0 + 4000);
    if front::text::arial12().is_some() {
        let n = strip_white(&inside);
        assert!(n > 60, "{n} white pixels in the strip inside the cue");
    }
    assert_eq!(strip_white(&outside), 0);
    // off: the same moment shows nothing
    let mut s = session(&h, &g, &d);
    let t0 = s.now;
    s.play_preview(STAB);
    let off = at(&mut s, t0 + 2000);
    assert!(s.captions().is_empty());
    assert_eq!(strip_white(&off), 0);
}

/// H343 / H150: switching captions on re-places the EmotionIcons from (·,340) to (·,351); off puts them back.
#[test]
fn h343_bar_shift() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = session(&h, &g, &d);
    front::settle(&mut s, 100, 60_000);
    let ys = |s: &Session| {
        [0x46fcu32, 0x4700, 0x6fde].map(|o| s.r.icon_bars.rects.get(&o).map(|r| (r.1[0], r.1[1])))
    };
    let before = ys(&s);
    assert!(before.iter().all(|r| r.is_some()), "New Game places the three EmotionIcons (H150): {before:?}");
    assert!(before.iter().all(|r| r.unwrap().1 == 340), "{before:?}");
    s.r.set_closed_captioning(true);
    let on = ys(&s);
    assert_eq!(on.map(|r| r.unwrap()), before.map(|r| (r.unwrap().0, 351)));
    s.r.set_closed_captioning(false);
    assert_eq!(ys(&s), before);
}

/// H344: a user .srt for the movie's file replaces its text tracks (movie-file time).
#[test]
fn h344_user_srt() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let dir = std::env::temp_dir().join(format!("xf-subs-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("XV")).unwrap();
    std::fs::write(dir.join("XV/23434.srt"), "1\n00:00:03,000 --> 00:00:05,000\nТест субтитров?\nWHO\n\n").unwrap();
    let mut s = session(&h, &g, &d);
    s.subs = Some(xf::media::MediaLib::new(&dir));
    s.r.set_closed_captioning(true);
    let t0 = s.now;
    s.play_preview(STAB);
    at(&mut s, t0 + 2000);
    assert!(s.captions().is_empty(), "the .srt replaces the text tracks");
    let fb = at(&mut s, t0 + 4000);
    let caps = s.captions();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].text, "Тест субтитров?\nWHO");
    assert_eq!(caps[0].track, [0, 224, 600, 260], "two lines: the strip grows one line upwards");
    if front::text::arial12().is_some() {
        assert!(strip_white(&fb) > 20);
    }
}
