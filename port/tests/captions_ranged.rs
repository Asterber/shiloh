//! Closed captions (H340) of movies read by ranges, as the web front reads them (`xf::io::ranged`: the worker fetches
//! sample ranges from xf-serve, a movie's file is never held whole). Bug report: no English captions in the browser on
//! the EN DVD Edition (F3 / "Subtitles: on" showed an empty strip) — the text samples were read from the empty
//! whole-file buffer. Own test binary: `xf::io::install` is once per process.
mod common;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use xf::front::{self, Session};
use xf::hdb::Hdb;
use xf::io::{Entry, Fs};
use xf::media::MoviePlayer;

/// The disc-image backend, but asking the core for range reads (as the web front's `WebFs`).
struct Ranged(xf::vfs::VfsFs);

impl Fs for Ranged {
    fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
        self.0.read(p)
    }
    fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>> {
        self.0.read_dir(p)
    }
    fn size(&self, p: &Path) -> std::io::Result<u64> {
        self.0.size(p)
    }
    fn read_at(&self, p: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read_at(p, off, buf)
    }
    fn ranged(&self) -> bool {
        true
    }
    fn is_file(&self, p: &Path) -> bool {
        self.0.is_file(p)
    }
}

fn install() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| assert!(xf::io::install(Box::new(Ranged(xf::vfs::VfsFs)))));
    assert!(xf::io::ranged());
}

/// The EN DVD Edition (`XF_EN_DIR`: the image or its files) mounted under the ranged backend, None without it.
fn en() -> Option<PathBuf> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    install();
    ROOT.get_or_init(|| {
        let mut src = xf::vfs::env_sources("XF_EN_DIR");
        if src.len() == 1 && src[0].is_dir() && !src[0].join("XFiles.hdb").exists() {
            src = xf::media::lang::layers(&src[0]);
        }
        if src.is_empty() {
            eprintln!("skipping: XF_EN_DIR (the English DVD Edition image or files) not set");
            return None;
        }
        if !xf::vfs::needs_vfs(&src) {
            return Some(src[0].clone());
        }
        let ds = xf::vfs::DataSet::open(&src).expect("the EN data set");
        Some(xf::vfs::mount(ds).root.clone())
    })
    .clone()
}

/// The RU stab video (XV/23434.xmv, two English text tracks): a ranged player shows the same captions as one holding
/// the whole file.
#[test]
fn ranged_player_reads_text_samples() {
    install();
    let Some(d) = common::data() else { return };
    let f = d.join("XV/23434.xmv");
    let bytes = std::fs::read(&f).unwrap();
    let m = xf::media::qt::Movie::parse(&bytes).unwrap();
    let mut p = MoviePlayer::open(&f).unwrap();
    for t in [2000, 10_000, 20_000] {
        let want = xf::media::captions::text_captions(&m, &bytes, t);
        assert!(want.iter().any(|c| !c.text.trim().is_empty()), "a caption at {t} ms");
        assert_eq!(p.captions_at(t), want, "{t} ms");
    }
}

/// H2305 + the bug report: on the EN DVD Edition alone, New Game's opening (Willmore greets Cook, media 0x4d64) shows
/// its own English captions once the captions are on (F3), read by ranges.
#[test]
fn en_opening_captions_ranged() {
    let Some(root) = en() else { return };
    let h: &'static Hdb = Box::leak(Box::new(Hdb::open(root.join("XFiles.hdb")).unwrap()));
    let g: &'static Hdb = Box::leak(Box::new(Hdb::open(root.join("XFiles.gam")).unwrap()));
    let mut s = Session::with_rules(h, g, &root, front::rules_hyp());
    s.boot(front::intro::Start::NewGame);
    s.r.set_closed_captioning(true);
    let mut seen = Vec::new();
    while s.now < 20_000 {
        s.tick(100);
        for c in s.captions() {
            if !c.text.trim().is_empty() && !seen.contains(&c.text) {
                seen.push(c.text);
            }
        }
    }
    assert!(seen.len() >= 3, "English captions in the opening: {seen:?}");
    assert!(seen.iter().all(|t| t.is_ascii()), "{seen:?}");
}
