//! The English DVD Edition as the only game data (H2300–H2309, static + port runs, NOT traced): `XF_EN_DIR` = the
//! user's own EN DVD image (`.iso` / `.mdf`) or its files; skipped without it. No game text is part of the tests.
mod common;
use std::path::PathBuf;
use std::sync::OnceLock;
use xf::exetables::{Build, ExeTables};
use xf::front::Session;
use xf::hdb::Hdb;
use xf::world::Rules;

/// The EN data root (mounted once per test binary), None without `XF_EN_DIR`.
fn en() -> Option<PathBuf> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        // the backend that reads disc images, before the first file read
        xf::vfs::install();
        let mut src = xf::vfs::env_sources("XF_EN_DIR");
        // the DVD's files (a folder holding English/MININST …): its layers, merged
        if src.len() == 1 && src[0].is_dir() && !src[0].join("XFiles.hdb").exists() {
            src = xf::media::lang::layers(&src[0]);
        }
        if src.is_empty() {
            eprintln!("skipping: XF_EN_DIR (the English DVD Edition image or files) not set");
            return None;
        }
        let root = xf::vfs::data_root(&src, &mut |_| {}).expect("the EN data set");
        Some(root)
    })
    .clone()
}

fn hdb(root: &std::path::Path, name: &str) -> &'static Hdb {
    Box::leak(Box::new(Hdb::open(root.join(name)).unwrap()))
}

/// H2300: the EN exe is identified and every table has the RU 1.00 shape (and, the HDBs sharing their objIds, the same
/// values except the string pointers)
#[test]
fn en_exe_tables() {
    let Some(root) = en() else { return };
    let h = hdb(&root, "XFiles.hdb");
    let t = &h.exe;
    assert_eq!(t.build, Some(Build::EnDvd));
    assert_eq!((t.ing_records.len(), t.media_keys.len(), t.cheats.len(), t.phones.len()), (60, 16, 26, 13));
    assert_eq!((t.ing_replies.len(), t.media_replies.len(), t.note_replies.len()), (18, 10, 22));
    assert_eq!((t.cursors.len(), t.note_days.len(), t.volume_bars.len(), t.ext_kinds.len()), (12, 23, 4, 45));
    assert!(t.ext_kinds.iter().any(|(e, k)| e == "NMV" && *k == 0x16));
    let pc = &t.pc;
    assert_eq!((pc.ing_phones.len(), pc.sbase_dots.len(), pc.note_letters.len(), pc.recipients.len()), (12, 31, 7, 9));
    assert!(pc.enclosures.len() == 9 && pc.enclosures.iter().all(|e| e.len() == 11 && e.ends_with(".DAT")));
    // H2301: the two subjects are AssetRefs of the HDB in this build
    assert!(pc.article_subject.is_empty() && pc.field_notes.is_empty());
    let (a, f) = (pc.article_subject_asset.unwrap(), pc.field_notes_asset.unwrap());
    assert!(h.has(xf::world::class::ASSET_REF, a) && h.has(xf::world::class::ASSET_REF, f));
    // the pointers into the HDB hold
    assert!(t.phones.iter().all(|&(a, _, _)| h.has(xf::world::class::ASSET_REF, a)));
    assert!(t.smol_pick_views.iter().flatten().all(|&v| h.has(xf::world::class::VIEW, v)));
    // same values as the RU 1.00 tables (when the RU install is there too)
    if let Some(d) = common::data() {
        let ru = ExeTables::parse(&std::fs::read(d.join("XFiles.exe")).unwrap()).unwrap();
        assert_eq!((&ru.ing_records, &ru.cheats, &ru.phones, &ru.note_days), (&t.ing_records, &t.cheats, &t.phones, &t.note_days));
        assert_eq!((&ru.ext_kinds, &ru.cursors, &ru.volume_bars), (&t.ext_kinds, &t.cursors, &t.volume_bars));
        assert_eq!((&ru.pc.mail_rows_pc, &ru.pc.enclosures, &ru.pc.ing_phones), (&pc.mail_rows_pc, &pc.enclosures, &pc.ing_phones));
    }
}

/// H2303: New Game on the EN data alone — the opening's two media start together (H51's media kinds come from the
/// exe's extension table): the first View's hotspots come at 21 s, as on the RU data, not after both media one by one.
#[test]
fn en_new_game_opening() {
    let Some(root) = en() else { return };
    let (h, g) = (hdb(&root, "XFiles.hdb"), hdb(&root, "XFiles.gam"));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    s.boot(xf::front::intro::Start::NewGame);
    let mut first_hot = None;
    while s.now < 40_000 && first_hot.is_none() {
        s.tick(33);
        if !s.r.w.hotspots.is_empty() {
            first_hot = Some(s.now);
        }
    }
    let t = first_hot.expect("the first View's hotspots");
    assert!(t < 25_000, "hotspots at {t} ms");
}

/// H2304: a RU 1.00 save loads on the EN data (the HDBs share their objIds) and the dialogue box's line click plays the
/// line (fo-028-end.x, the RU test of H135 in `dbox.rs`).
#[test]
fn en_ru_save_dialogue_click() {
    let Some(root) = en() else { return };
    let save = common::root().join("data/bot/fo-028-end.x");
    if !save.exists() {
        return;
    }
    let h = hdb(&root, "XFiles.hdb");
    let g: &'static Hdb = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    s.r.load_game();
    xf::front::settle(&mut s, 100, 900_000);
    assert!(s.r.w.dbox_layout().is_some_and(|l| l.list == 0x590d));
    let r0 = s.r.regs.len();
    let c = s.click_at(345, 311).expect("hit");
    assert_eq!(c.owner, 0x590e);
    xf::front::settle(&mut s, 100, 900_000);
    assert_eq!(s.r.regs[r0..].to_vec(), vec![0x4e75, 0x4e77]);
    assert_eq!(s.r.w.conv_history, vec![0x590e]);
}

/// H2306: the EN XT texts are cp1252 — decoded, none holds Cyrillic, and the accented Latin letters some hold survive
#[test]
fn en_texts_cp1252() {
    let Some(root) = en() else { return };
    let h = hdb(&root, "XFiles.hdb");
    let lib = xf::media::MediaLib::new(&root);
    let c = h.classes.iter().find(|c| c.id == xf::world::class::ASSET_REF).unwrap();
    let (mut n, mut accented) = (0, 0);
    for &(id, _) in &c.objects {
        let Some(t) = xf::front::doc::xtx_text(&lib, h, id) else { continue };
        n += 1;
        assert!(!t.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c)), "{id:#x} decoded to Cyrillic");
        accented += t.chars().any(|c| ('\u{c0}'..='\u{ff}').contains(&c)) as usize;
    }
    assert!(n > 100 && accented > 0, "{n} texts, {accented} with accented letters");
}

/// H2305: English game data offers English only — no Russian game language, no subtitle pack (the movies' own
/// captions), an English data set over it is not used, F4 has nothing to switch; a remembered Russian choice is fitted.
#[test]
fn en_only_languages() {
    let Some(root) = en() else { return };
    let (h, g) = (hdb(&root, "XFiles.hdb"), hdb(&root, "XFiles.gam"));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    use xf::media::lang::GameLang;
    assert_eq!((s.data_lang, s.game_lang), (GameLang::En, GameLang::En));
    assert_eq!(s.game_langs(), vec!["en"]);
    assert_eq!(s.next_game_lang(), None);
    assert!(s.set_game_lang("ru").is_err());
    assert!(s.set_game_lang("en").is_ok() && s.r.w.lang_strings.is_none());
    assert_eq!(s.sub_langs(&["en", "ru"]), vec!["auto"]);
    s.set_en_data(Some(std::sync::Arc::new(xf::media::lang::EnData::default())));
    assert!(s.en_data.is_none() && s.game_langs() == vec!["en"]);
    let mut set = xf::front::settings::Settings::default();
    set.set("subs", "ru").unwrap();
    set.fit(&s, &["en", "ru"], false);
    assert_eq!((set.get("game_lang"), set.get("subs")), ("en", "auto"));
    let ch = xf::front::settings::choices(&s, &["ru"]);
    assert!(ch.iter().all(|(_, v)| !v.contains(&"ru")));
    // the RU data: Russian, its packs offered
    if let Some(d) = common::data() {
        let (h, g) = (hdb(&d, "XFiles.hdb"), hdb(&d, "XFiles.gam"));
        let s = Session::with_rules(h, g, &d, Rules::all());
        assert_eq!((s.data_lang, s.game_langs()), (GameLang::Ru, vec!["ru"]));
        assert_eq!(s.sub_langs(&["ru"]), vec!["auto", "ru", "off"]);
        assert_eq!(s.sub_langs(&[]), vec!["auto"]);
    }
}
