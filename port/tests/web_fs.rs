//! The web front's file access, natively: an `xf::io` backend that serves exactly what the browser worker can fetch
//! from xf-serve (`port/web/src/lib.rs` WebFs + `www/worker.js url()`): relative `data/<path>` (the game dir),
//! `fonts/<name>` (xf-serve `--fonts DIR`) and nothing else — no absolute path, no system font dir. The program-drawn
//! texts (phone display, books, the Bible, FBI computer, note pad) must render with it as in the window (bug
//! report, H1020). Own test binary: `xf::io::install` is once per process.
mod common;
use std::path::{Path, PathBuf};
use xf::front::{self, doc, script, Session};
use xf::hdb::Hdb;
use xf::io::{Entry, Fs};

/// The number typed into fo-114's ING phone search: phone-directory entry 4 (text AssetRef 0x106cf, H575), read from
/// the session's HDB as its last 10 digits.
fn cook_number(s: &Session) -> String {
    let d: Vec<char> = common::text_of(&s.r.w, 0x106cf).chars().filter(char::is_ascii_digit).collect();
    assert!(d.len() >= 10, "a phone number");
    d[d.len() - 10..].iter().collect()
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The fonts dir a server deploy mounts at /fonts: a copy of macOS /System/Library/Fonts/Supplemental.
fn fonts_dir() -> Option<PathBuf> {
    let d = std::env::var_os("XF_TEST_WEB_FONTS").map(PathBuf::from).unwrap_or_else(|| "/System/Library/Fonts/Supplemental".into());
    d.join("Arial.ttf").is_file().then_some(d)
}

/// Only what xf-serve serves: `data/…` → game dir, `save/…` → data/bot (the test's saves), `fonts/<name>` → fonts dir.
struct WebLike {
    game: PathBuf,
    saves: PathBuf,
    fonts: PathBuf,
}

impl WebLike {
    fn map(&self, p: &Path) -> std::io::Result<PathBuf> {
        let s = p.to_string_lossy();
        let nf = || std::io::Error::new(std::io::ErrorKind::NotFound, format!("{s}: not served"));
        let (base, rel) = if let Some(r) = s.strip_prefix("data/") {
            (&self.game, r)
        } else if s == "data" {
            (&self.game, "")
        } else if let Some(r) = s.strip_prefix("save/") {
            (&self.saves, r)
        } else if let Some(r) = s.strip_prefix("fonts/") {
            if r.contains('/') {
                return Err(nf());
            }
            (&self.fonts, r)
        } else {
            return Err(nf());
        };
        Ok(if rel.is_empty() { base.clone() } else { base.join(rel) })
    }
}

impl Fs for WebLike {
    fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.map(p)?)
    }
    fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>> {
        let real = self.map(p)?;
        Ok(std::fs::read_dir(&real)?
            .flatten()
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                Entry { is_dir: e.path().is_dir(), path: p.join(&name), name }
            })
            .collect())
    }
    fn size(&self, p: &Path) -> std::io::Result<u64> {
        Ok(std::fs::metadata(self.map(p)?)?.len())
    }
    fn read_at(&self, p: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        let b = self.read(p)?;
        let s = (off as usize).min(b.len());
        let n = buf.len().min(b.len() - s);
        buf[..n].copy_from_slice(&b[s..s + n]);
        Ok(n)
    }
}

fn setup() -> bool {
    let game = root().join("data/InstalledGame");
    let Some(fonts) = fonts_dir() else { return false };
    if !game.join("XFiles.hdb").is_file() {
        return false;
    }
    xf::io::install(Box::new(WebLike { game, saves: root().join("data/bot"), fonts }));
    // as web/src/lib.rs xf_init
    front::text::set_web_font_dir("fonts");
    true
}

/// A session as the web front builds it (root `data`), loaded from `save`.
fn session(save: &str) -> Option<Session<'static>> {
    if !setup() || !root().join("data/bot").join(save).is_file() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(Path::new("data/XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(Path::new("save").join(save)).unwrap()));
    let mut s = Session::with_rules(h, g, Path::new("data"), front::rules_hyp());
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    Some(s)
}

fn count(fb: &[u8], r: [usize; 4], c: [u8; 3]) -> usize {
    (r[1]..r[3]).flat_map(|y| (r[0]..r[2]).map(move |x| (x, y))).filter(|&(x, y)| fb[(y * front::SCREEN_W + x) * 3..][..3] == c).count()
}

const LOGIN: &str = "p:70,210;p:570,210;p:570,210;p:251,281;p:320,210;p:83,282;p:320,210;p:466,300;p:320,210;p:70,210;p:320,210;p:412,147;p:320,210;p:171,271;p:320,210;p:187,223;p:320,210;p:570,210;p:2,92;p:70,210;p:570,210;p:318,200";

/// `steps` with a rendered frame after each (the front measures the panel texts while drawing, H791)
fn run(s: &mut Session, steps: &str) {
    for i in script::parse(steps).unwrap() {
        script::apply(s, &i);
        front::settle_timers(s, 100, 900_000);
        let _ = s.render();
    }
}

/// All in one test (one session per process is enough; the io backend is global).
#[test]
fn web_fs_program_texts() {
    // Bible (Arial 12, doc.rs) — H532's check, through the web file access
    let Some(mut s) = session("fo-040-end.x") else { return };
    assert!(doc::face(&s.lib, doc::DocFont::Arial, 12).is_some(), "Arial not found through fonts/");
    assert!(doc::face(&s.lib, doc::DocFont::Jrn, 14).is_some(), "Jrn.ttr not found through data/");
    // the FBI computer's wrapped texts (e-mail reader, ING notes, phone directory rows) take doc::face too
    assert!(doc::face(&s.lib, doc::DocFont::Courier, 14).is_some() && doc::face(&s.lib, doc::DocFont::System, 16).is_some());
    s.r.viewer_show_layout(0xec66);
    front::settle_timers(&mut s, 100, 900_000);
    let fb = s.render();
    if let Some(d) = std::env::var_os("XF_DOC_DUMP") {
        front::write_ppm(&PathBuf::from(d).join("web-bible.ppm"), &fb).unwrap();
    }
    let left = count(&fb, [97, 44, 294, 377], [0, 0, 0]);
    assert!(left > 800, "Bible left page text pixels {left}");
    // the phone display (Courier New Bold 15, text.rs) — H865's fo-100 k08 count
    assert!(front::text::courier15_bold().is_some(), "Courier New Bold not found through fonts/");
    assert!(front::text::courier12().is_some() && front::text::arial12().is_some());
    let Some(mut s) = session("fo-2c9-door.x") else { return };
    let digits = "m:40,440;x:40,440;x:330,373;x:330,373;x:330,373;x:330,446;x:272,337;x:272,409;x:272,337";
    for i in script::parse(digits).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
        s.render();
    }
    let fb = s.render();
    if let Some(d) = std::env::var_os("XF_DOC_DUMP") {
        front::write_ppm(&PathBuf::from(d).join("web-phone.ppm"), &fb).unwrap();
    }
    assert_eq!(count(&fb, [273, 139, 390, 196], front::ui::PHONE_TEXT), 193);
    // the card found for the number (fo-100 k09): name lines + number
    for i in script::parse("x:375,250").unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
    }
    let fb = s.render();
    if let Some(d) = std::env::var_os("XF_DOC_DUMP") {
        front::write_ppm(&PathBuf::from(d).join("web-phone-card.ppm"), &fb).unwrap();
    }
    assert_eq!(count(&fb, [273, 139, 390, 196], front::ui::PHONE_TEXT), 824);
    // the FBI computer's MainScroll (H791 / H892, bug report "scrolling does not work" in the web): the scroll limits
    // are measured with doc::face while drawing — the media article and Cook's ING field notes scroll a line down
    let Some(mut s) = session("fo-040-end.x") else { return };
    run(&mut s, &format!("{LOGIN};r:106,54;p:68,136;t:tarakan;k:13"));
    assert!(s.r.w.pc.scroll_max.is_some_and(|m| m > 0), "media article measured: {:?}", s.r.w.pc.scroll_max);
    run(&mut s, "p:606,460");
    assert_eq!(s.r.w.pc.scroll, 1, "media article line down");
    let Some(mut s) = session("fo-040-end.x") else { return };
    let number = cook_number(&s);
    run(&mut s, &format!("{LOGIN};r:106,54;p:68,110;p:338,300;t:{number};k:13;p:309,456"));
    assert_eq!(s.r.w.pc.scroll, 1, "field notes line down");
}
