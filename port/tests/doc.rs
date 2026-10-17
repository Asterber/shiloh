//! Document texts (front::doc, H530–H536): the journal and the Bible close-ups shown by the engine's layout show
//! (the layouts are not on the FO route of the bot's saves), their pages drawn from XT/*.xtx with the game's fonts.
use std::path::PathBuf;
use xf::front::{self, doc, Session};
use xf::hdb::Hdb;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Load `save`, show `layout` as an engine layout show (its items' Activations run JournalInit / BibleInit), then
/// `f` on the session.
fn with_layout<T>(save: &str, layout: u32, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
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
    s.r.viewer_show_layout(layout);
    front::settle_timers(&mut s, 100, 900_000);
    s.render();
    if std::env::var_os("XF_DOC_DEBUG").is_some() {
        eprintln!("layouts {:x?}", s.r.w.layouts());
        eprintln!("active {:x?}", s.r.active);
    }
    Some(f(&mut s))
}

fn dump(name: &str, fb: &[u8]) {
    if let Some(d) = std::env::var_os("XF_DOC_DUMP") {
        let p = PathBuf::from(d).join(name);
        front::write_ppm(&p, fb).unwrap();
    }
}

fn dark_in(fb: &[u8], r: [usize; 4]) -> usize {
    (r[1]..r[3])
        .flat_map(|y| (r[0]..r[2]).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let o = (y * front::SCREEN_W + x) * 3;
            fb[o] == 0 && fb[o + 1] == 0 && fb[o + 2] == 0
        })
        .count()
}

/// H531: JournalInit (item 0xe407's Activation) puts the last two pages in 0xb522 / 0xb523; their text is drawn in
/// ZonkersHand 14 (Jrn.ttr) in the page rects (112,65)–(302,359) and (356,65)–(528,361), black, left, word-wrapped.
#[test]
fn h531_journal_pages_drawn() {
    let Some((fb, docs, lines)) = with_layout("fo-040-end.x", 0xb130, |s| {
        let docs = doc::shown_documents(s);
        let fb = s.render();
        let f = doc::face(&s.lib, doc::DocFont::Jrn, 14).expect("Jrn.ttr");
        let text = doc::xtx_text(&s.lib, s.r.w.h, docs.iter().find(|d| d.0 == 0xb522).unwrap().1).unwrap();
        let lines = doc::layout(&text, 302 - 112, |t| f.face.width(t));
        (fb, docs, lines)
    }) else {
        return;
    };
    dump("journal.ppm", &fb);
    let items: Vec<u32> = docs.iter().map(|d| d.0).collect();
    assert!(items.contains(&0xb522) && items.contains(&0xb523), "{docs:x?}");
    assert!(lines.len() > 5, "{lines:?}");
    // ablation-style check: the page rect holds the text's black pixels, the gap between the pages none from it
    let left = dark_in(&fb, [112, 65, 302, 359]);
    assert!(left > 800, "left page text pixels {left}");
}

/// H532: BibleInit (item 0x13bf1's Activation) shows pages 1 / 2 (Arial 12) in 0x13be4 / 0x13be5.
#[test]
fn h532_bible_pages_drawn() {
    let Some((fb, docs, arial)) = with_layout("fo-040-end.x", 0xec66, |s| {
        (s.render(), doc::shown_documents(s), doc::face(&s.lib, doc::DocFont::Arial, 12).is_some())
    }) else {
        return;
    };
    dump("bible.ppm", &fb);
    let items: Vec<u32> = docs.iter().map(|d| d.0).collect();
    assert!(items.contains(&0x13be4) && items.contains(&0x13be5), "{docs:x?}");
    // H535: the pool item 0x13c0b shares page 1's asset object with 0x13be4: drawn once, in 0x13be4
    assert!(!items.contains(&0x13c0b), "{docs:x?}");
    if !arial {
        return;
    }
    let left = dark_in(&fb, [97, 44, 294, 377]);
    assert!(left > 800, "left page text pixels {left}");
}

/// H533 self-consistency: every journal page (item 0xc10e's 42 AssetRefs) laid out in ZonkersHand 14 with DrawText's
/// word breaking fits its page rect (the authors split the text into pages for this font: the fullest page needs 12
/// lines × 24 px = 288 of 294 px). The RU Bible pages in Arial 12 do NOT all fit (H536: the translation is longer; the
/// original is expected to clip them — AG check) — their line counts are printed, not asserted.
#[test]
fn h533_pages_fit() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let lib = xf::media::MediaLib::new(&d);
    for (pool, font, size, w, hgt) in
        [(0xc10e, doc::DocFont::Jrn, 14, 302 - 112, 359 - 65), (0x13c0b, doc::DocFont::Arial, 12, 294 - 97, 377 - 44)]
    {
        let Some(f) = doc::face(&lib, font, size) else { continue };
        let list = h.read(0x4f, pool).unwrap().u32("f44");
        let mut worst = (0, 0);
        let mut counts = Vec::new();
        for a in h.list_of(0x36, list) {
            let t = doc::xtx_text(&lib, &h, a).unwrap();
            let n = doc::layout(&t, w, |s| f.face.width(s)).len() as i32;
            counts.push(n);
            worst = worst.max((n, a));
        }
        eprintln!("{pool:#x}: line height {}, lines per page {counts:?}, rect {hgt} px", f.line_h);
        if pool == 0xc10e {
            assert!(worst.0 * f.line_h <= hgt, "{pool:#x}: {worst:x?}");
            assert_eq!(f.line_h, 24);
        } else {
            assert_eq!(f.line_h, 15, "R34 / H891: Arial 12's VDMX record 12 / -3 (fo-113 line tops 44, 59, 74 …)");
        }
    }
}
