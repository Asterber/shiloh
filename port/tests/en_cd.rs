//! The English retail CD set as the game data (H2329, static + port runs, NOT traced): `XF_EN_CD_DIR` = the user's
//! own CD images (a folder holding them, one folder per disc, or the image paths separated as in `PATH`; CD 1 is
//! enough); skipped without it. No game text is part of the tests.
use std::path::PathBuf;
use std::sync::OnceLock;
use xf::exetables::Build;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::media::lang::GameLang;
use xf::world::Rules;

fn en_cd() -> Option<PathBuf> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        xf::vfs::install();
        let mut src = xf::vfs::env_sources("XF_EN_CD_DIR");
        // a folder of disc folders with deeper images (`cd1/X_Files_The_Game_CD1/*.iso`): the images themselves
        if let [one] = src.as_slice() {
            if one.is_dir() {
                let mut v = Vec::new();
                for e in walk(one, 3) {
                    if e.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")) {
                        v.push(e);
                    }
                }
                if !v.is_empty() {
                    v.sort();
                    src = v;
                }
            }
        }
        if src.is_empty() {
            eprintln!("skipping: XF_EN_CD_DIR (the English CD images) not set");
            return None;
        }
        Some(xf::vfs::data_root(&src, &mut |_| {}).expect("the English CD data set"))
    })
    .clone()
}

fn walk(d: &std::path::Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() && depth > 0 {
            out.extend(walk(&p, depth - 1));
        } else if p.is_file() {
            out.push(p);
        }
    }
    out
}

/// H2329: the English CDs carry the RU 1.00 exe (the RU tables, Build::Ru100) but English data: the string table
/// decides the language — English, cp1252 texts, no Russian packs
#[test]
fn en_cd_is_english_on_the_ru_exe() {
    let Some(root) = en_cd() else { return };
    let h: &'static Hdb = Box::leak(Box::new(Hdb::open(root.join("XFiles.hdb")).unwrap()));
    let g: &'static Hdb = Box::leak(Box::new(Hdb::open(root.join("XFiles.gam")).unwrap()));
    assert_eq!(h.exe.build, Some(Build::Ru100));
    let s = Session::with_rules(h, g, &root, Rules::all());
    assert_eq!((s.data_lang, s.game_lang), (GameLang::En, GameLang::En));
    assert_eq!(s.sub_langs(&["en", "ru"]), vec!["auto"]);
    let lib = xf::media::MediaLib::new(&root);
    let c = h.classes.iter().find(|c| c.id == xf::world::class::ASSET_REF).unwrap();
    let mut n = 0;
    for &(id, _) in &c.objects {
        let Some(t) = xf::front::doc::xtx_text(&lib, h, id) else { continue };
        n += 1;
        assert!(!t.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c)), "{id:#x} decoded to Cyrillic");
    }
    assert!(n > 100, "{n} texts");
}
