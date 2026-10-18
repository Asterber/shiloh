//! The German retail CD set as the only game data (H2320–H2329, static + port runs, NOT traced): `XF_DE_DIR` = the
//! user's own 7 disc images — a folder holding them (or holding one folder per disc, `disk1/*.iso … disk7/*.iso`), or
//! the image paths separated as in `PATH`; skipped without it. No game text is part of the tests.
mod common;
use std::path::PathBuf;
use std::sync::OnceLock;
use xf::exetables::{Build, ExeTables};
use xf::front::Session;
use xf::hdb::Hdb;
use xf::media::lang::GameLang;
use xf::world::Rules;

/// The German data root (mounted once per test binary), None without `XF_DE_DIR`.
fn de() -> Option<PathBuf> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        xf::vfs::install();
        let src = xf::vfs::env_sources("XF_DE_DIR");
        if src.is_empty() {
            eprintln!("skipping: XF_DE_DIR (the German CD images) not set");
            return None;
        }
        Some(xf::vfs::data_root(&src, &mut |_| {}).expect("the German data set"))
    })
    .clone()
}

fn hdb(root: &std::path::Path, name: &str) -> &'static Hdb {
    Box::leak(Box::new(Hdb::open(root.join(name)).unwrap()))
}

/// H2320: the German exe (1.00.19) is identified and every table has the RU 1.00 shape and — the German HDB sharing
/// the RU / EN objIds — the RU values
#[test]
fn de_exe_tables() {
    let Some(root) = de() else { return };
    let d = xf::io::read(&root.join("XFiles.exe")).unwrap();
    assert_eq!(xf::exetables::identify(&d), Ok(Build::De10019));
    let h = hdb(&root, "XFiles.hdb");
    let t = &h.exe;
    assert_eq!(t.build, Some(Build::De10019));
    assert_eq!((t.ing_records.len(), t.media_keys.len(), t.cheats.len(), t.phones.len()), (60, 16, 26, 13));
    assert_eq!((t.ing_replies.len(), t.media_replies.len(), t.note_replies.len()), (18, 10, 22));
    assert_eq!((t.cursors.len(), t.note_days.len(), t.volume_bars.len(), t.ext_kinds.len()), (12, 23, 4, 45));
    // H51 / H2303: the extension table the movie kinds come from (the cause of the stalled opening without it)
    assert!(t.ext_kinds.iter().any(|(e, k)| e == "NMV" && *k == 0x16));
    assert!(t.ext_kinds.iter().all(|(e, _)| !e.is_empty() && e.bytes().all(|b| b.is_ascii_uppercase())));
    let pc = &t.pc;
    assert_eq!((pc.ing_phones.len(), pc.sbase_dots.len(), pc.note_letters.len(), pc.recipients.len()), (12, 31, 7, 9));
    assert!(pc.enclosures.len() == 9 && pc.enclosures.iter().all(|e| e.len() == 11 && e.ends_with(".DAT")));
    // H2322: the two subjects are the EN build's AssetRefs
    assert!(pc.article_subject.is_empty() && pc.field_notes.is_empty());
    assert_eq!((pc.article_subject_asset, pc.field_notes_asset), (Some(0x172a1), Some(0x10767)));
    assert!(h.has(xf::world::class::ASSET_REF, 0x172a1) && h.has(xf::world::class::ASSET_REF, 0x10767));
    // the ids point into the HDB
    assert!(t.phones.iter().all(|&(a, _, _)| h.has(xf::world::class::ASSET_REF, a)));
    assert!(t.suicide_notes.iter().all(|&a| h.has(xf::world::class::ASSET_REF, a)));
    assert!(t.smol_pick_views.iter().flatten().all(|&v| h.has(xf::world::class::VIEW, v)));
    let layouts = t.mel_shot_layouts.iter().chain(&t.suicide_note_layouts);
    assert!(layouts.into_iter().all(|&l| h.has(xf::world::class::IFACE_LAYOUT, l)));
    // the same values as the RU 1.00 tables, every table (when the RU install is there too)
    if let Some(d) = common::data() {
        let mut ru = ExeTables::parse(&std::fs::read(d.join("XFiles.exe")).unwrap()).unwrap();
        // the build and the RU literals (H2301) aside
        ru.build = t.build;
        ru.pc.article_subject = String::new();
        ru.pc.field_notes = String::new();
        ru.pc.article_subject_asset = pc.article_subject_asset;
        ru.pc.field_notes_asset = pc.field_notes_asset;
        assert_eq!(&ru, t);
    }
}

/// H2324 (= H2303 on German data): New Game, nothing done — the opening's two media start together and the first
/// View's hotspots come at 21 s, then Cook's idle loop at 51 s, as on the RU data (before: one after the other,
/// hotspots at 36 s, the idle loop after 66 s)
#[test]
fn de_new_game_opening() {
    let Some(root) = de() else { return };
    let (h, g) = (hdb(&root, "XFiles.hdb"), hdb(&root, "XFiles.gam"));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    s.boot(xf::front::intro::Start::NewGame);
    let (mut first_hot, mut both, mut idle_loop) = (None, false, None);
    while s.now < 60_000 && idle_loop.is_none() {
        s.tick(33);
        both |= s.r.started().len() >= 2;
        if first_hot.is_none() && !s.r.w.hotspots.is_empty() {
            first_hot = Some(s.now);
        }
        if first_hot.is_some() && s.r.background != Some(0x6453) && s.r.background.is_some() {
            idle_loop = Some(s.now);
        }
    }
    assert!(both, "the opening's two media never ran together");
    let t = first_hot.expect("the first View's hotspots");
    assert!(t < 25_000, "hotspots at {t} ms");
    let t = idle_loop.expect("the next background within 60 s");
    assert!(t < 55_000, "next background at {t} ms");
}

/// H2323: German game data — German is the only game language, F4 has nothing to switch, the text files are cp1252
/// (umlauts, no Cyrillic); subtitles: only an English pack is offered, and without one there is no subtitle option
#[test]
fn de_languages() {
    let Some(root) = de() else { return };
    let (h, g) = (hdb(&root, "XFiles.hdb"), hdb(&root, "XFiles.gam"));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    assert_eq!((s.data_lang, s.game_lang), (GameLang::De, GameLang::De));
    assert_eq!(xf::media::lang::hdb_lang(h), GameLang::De);
    assert_eq!(s.game_langs(), vec!["de"]);
    assert_eq!(s.next_game_lang(), None);
    assert!(s.set_game_lang("ru").is_err() && s.set_game_lang("en").is_err());
    assert!(s.set_game_lang("de").is_ok() && s.r.w.lang_strings.is_none());
    assert_eq!(s.sub_langs(&["en", "ru"]), vec!["auto", "en", "off"]);
    assert_eq!(s.sub_langs(&["ru"]), vec!["auto"]);
    assert!(s.no_subtitles(&["ru"]) && !s.no_subtitles(&["en"]));
    let ch = xf::front::settings::choices(&s, &[]);
    assert!(ch.contains(&("captions", vec!["game"])) && ch.contains(&("game_lang", vec!["de"])));
    let mut set = xf::front::settings::Settings::default();
    set.set("captions", "on").unwrap();
    set.set("subs", "ru").unwrap();
    set.fit(&s, &[], false);
    assert_eq!((set.get("game_lang"), set.get("subs"), set.get("captions")), ("de", "auto", "game"));
    // auto over German data: the English pack only
    let cache = std::env::temp_dir().join(format!("xf-de-packs-{}", std::process::id()));
    std::fs::create_dir_all(cache.join(xf::media::subs::SUBDIR)).unwrap();
    std::fs::write(xf::media::subs::pack_path(&cache, "ru"), br#"{"format":"xf-subs","version":1,"lang":"ru","movies":{}}"#)
        .unwrap();
    assert!(xf::media::subs::Pack::pick_for(&cache, "auto", GameLang::De).unwrap().is_none());
    std::fs::write(xf::media::subs::pack_path(&cache, "en"), br#"{"format":"xf-subs","version":1,"lang":"en","movies":{}}"#)
        .unwrap();
    assert_eq!(xf::media::subs::Pack::pick_for(&cache, "auto", GameLang::De).unwrap().unwrap().lang, "en");
    let _ = std::fs::remove_dir_all(&cache);
    // an English data set laid over the German data (same objIds, H2325) adds English
    s.set_en_data(Some(std::sync::Arc::new(xf::media::lang::EnData::default())));
    assert_eq!(s.game_langs(), vec!["de", "en"]);
    assert!(!s.no_subtitles(&[]));
    // the XT texts: cp1252, no Cyrillic, umlauts survive
    let lib = xf::media::MediaLib::new(&root);
    let c = h.classes.iter().find(|c| c.id == xf::world::class::ASSET_REF).unwrap();
    let (mut n, mut umlauts) = (0, 0);
    for &(id, _) in &c.objects {
        let Some(t) = xf::front::doc::xtx_text(&lib, h, id) else { continue };
        n += 1;
        assert!(!t.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c)), "{id:#x} decoded to Cyrillic");
        umlauts += t.chars().any(|c| "äöüÄÖÜß".contains(c)) as usize;
    }
    assert!(n > 100 && umlauts > 50, "{n} texts, {umlauts} with umlauts");
}

/// H2326: every movie AssetRef of the German HDB resolves to a file of the 7 discs and opens; H2327: the German
/// movies carry no German captions — a few dozen hold English sound-effect lines only
#[test]
fn de_movies_resolve() {
    let Some(root) = de() else { return };
    let h = hdb(&root, "XFiles.hdb");
    let lib = xf::media::MediaLib::new(&root);
    let (mut n, mut files) = (0, std::collections::BTreeSet::new());
    for &(id, _) in &h.class(xf::world::class::ASSET_REF).unwrap().objects {
        let r = h.read(xf::world::class::ASSET_REF, id).unwrap();
        let Some(a) = r
            .blob("asset")
            .and_then(|b| h.blob_bytes(b))
            .and_then(|b| xf::media::asset::AssetBlob::parse(b, r.u32("f40")))
        else {
            continue;
        };
        if !a.entries.iter().any(|e| [".xmv", ".nmv", ".amv", ".dmv"].iter().any(|x| e.file.to_lowercase().ends_with(x))) {
            continue;
        }
        n += 1;
        let c = lib.resolve(h, id).unwrap_or_else(|| panic!("{id:#x} does not resolve"));
        files.insert(c.file);
    }
    assert!(n > 3000, "{n} movie AssetRefs");
    let mut captioned = 0;
    for f in &files {
        let m = xf::media::qt::Movie::open(f).unwrap();
        if xf::media::captions::has_captions(&m) {
            captioned += 1;
            let data = xf::io::read(f).unwrap();
            assert!(xf::media::captions::movie_cues(&m, &data).iter().all(|c| c.text.is_ascii()), "{}", f.display());
        }
    }
    assert!(captioned < 50, "{captioned} movies with text tracks");
}
