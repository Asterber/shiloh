//! The French edition on the EN DVD Edition image (its `Francais/` folder, H2370–H2376; static comparison + port runs,
//! NOT traced): `XF_EN_DIR` = the user's own DVD image (`.iso` / `.mdf`); skipped without it, or when the image has no
//! French folder. No game text is part of the tests.
mod common;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use xf::exetables::Build;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::media::lang::{EnData, GameLang};
use xf::vfs::DataSet;
use xf::world::Rules;

/// The DVD image, None (with a note) without `XF_EN_DIR` or when it is not a multi-language image.
fn image() -> Option<Vec<PathBuf>> {
    let src = xf::vfs::env_sources("XF_EN_DIR");
    if src.len() != 1 || !src[0].is_file() {
        eprintln!("skipping: XF_EN_DIR (the EN DVD Edition image) not set");
        return None;
    }
    Some(src)
}

/// The disc's French folder as the game data, mounted below the image's path (`<image>/fr-data`) — once per binary.
fn fr() -> Option<PathBuf> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        xf::vfs::install();
        let src = image()?;
        let mut ds = DataSet::open_lang(&src, Some("fr")).expect("the DVD image");
        if ds.skipped_langs() != ["en"] {
            eprintln!("skipping: no French folder on this image");
            return None;
        }
        ds.root = ds.root.join("fr-data");
        Some(xf::vfs::mount(ds).root.clone())
    })
    .clone()
}

/// The disc's English folder as the game data (the default) and its French folder laid over it (H2374): (data root,
/// the French data set's root).
fn en_fr() -> Option<(PathBuf, PathBuf)> {
    static ROOTS: OnceLock<Option<(PathBuf, PathBuf)>> = OnceLock::new();
    ROOTS
        .get_or_init(|| {
            xf::vfs::install();
            let src = image()?;
            let root = xf::vfs::data_root(&src, &mut |_| {}).expect("the EN data set");
            let over = xf::vfs::mount_skipped(&root, &mut |l| eprintln!("{l}"))?;
            Some((root, over))
        })
        .clone()
}

fn hdb(p: PathBuf) -> &'static Hdb {
    Box::leak(Box::new(Hdb::open(p).unwrap()))
}

/// H2370 / H2373: the French folder holds the EN DVD's program, database and initial state byte for byte (so the
/// 2.05 tables, H2300, hold) and a French string table (scGameLanguage "french"); only the language resources differ.
#[test]
fn fr_same_build_french_strings() {
    let Some(src) = image() else { return };
    let en = DataSet::open(&src).unwrap();
    let fr = DataSet::open_lang(&src, Some("fr")).unwrap();
    if fr.skipped_langs() != ["en"] {
        return;
    }
    for f in ["XFiles.exe", "XFiles.hdb", "XFiles.gam", "Jrn.ttr", "Dlg.ttr"] {
        let (a, b) = (en.read(Path::new(f)).unwrap(), fr.read(Path::new(f)).unwrap());
        assert!(a == b, "{f} differs");
    }
    let t = xf::media::lang::dll_lang(&xf::pe::string_table(&fr.read(Path::new("XFilesT.dll")).unwrap()).unwrap());
    assert_eq!(t, Some(GameLang::Fr));
    for f in ["XFilesT.dll", "x.pff", "X7.PFF"] {
        assert!(en.read(Path::new(f)).unwrap() != fr.read(Path::new(f)).unwrap(), "{f} the same");
    }
    // H2375: two movies of the English folder are not in the French one under their names
    assert!(en.is_file(Path::new("XV/19668.xmv")) && !fr.is_file(Path::new("XV/19668.xmv")));
    assert!(en.is_file(Path::new("XV/56001.xmv")) && !fr.is_file(Path::new("XV/56001.xmv")));
}

/// H2373: French data alone — the language by its string table, the EN DVD's tables; New Game's opening as on the RU /
/// EN data (the first View's hotspots before 25 s).
#[test]
fn fr_new_game_opening() {
    let Some(root) = fr() else { return };
    let (h, g) = (hdb(root.join("XFiles.hdb")), hdb(root.join("XFiles.gam")));
    assert_eq!(h.exe.build, Some(Build::EnDvd));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    assert_eq!((s.data_lang, s.game_lang), (GameLang::Fr, GameLang::Fr));
    assert_eq!(s.game_langs(), vec!["fr"]);
    // H2375: no captions of its own beyond a few English sound effects: the English pack is the one offered
    assert_eq!(s.sub_langs(&["en", "ru"]), vec!["auto", "en", "off"]);
    assert!(s.no_subtitles(&[]));
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

/// H2373 / H2376: a RU 1.00 save loads on the French data; the dialogue box's lines are the French strings with their
/// accented letters (not turned Cyrillic), and a line click plays the line as on RU / EN data.
#[test]
fn fr_save_dialogue_accents() {
    let Some(root) = fr() else { return };
    let save = common::root().join("data/bot/fo-028-end.x");
    if !save.exists() {
        return;
    }
    let mut s = Session::with_rules(hdb(root.join("XFiles.hdb")), hdb(save), &root, Rules::all());
    s.r.load_game();
    xf::front::settle(&mut s, 100, 900_000);
    assert!(s.r.w.dbox_layout().is_some_and(|l| l.list == 0x590d));
    let lines = s.r.w.dbox_line_texts();
    assert_eq!(lines.len(), 3);
    let all: String = lines.iter().map(|l| l.2.as_str()).collect();
    assert!(!all.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c)), "Cyrillic in the lines");
    assert!(all.chars().any(|c| ('\u{c0}'..='\u{ff}').contains(&c)), "no accented letter");
    let r0 = s.r.regs.len();
    assert_eq!(s.click_at(345, 311).expect("hit").owner, 0x590e);
    xf::front::settle(&mut s, 100, 900_000);
    assert_eq!(s.r.regs[r0..].to_vec(), vec![0x4e75, 0x4e77]);
}

/// H2373: the French XT texts are cp1252 — decoded, none holds Cyrillic, many hold accented letters.
#[test]
fn fr_texts_cp1252() {
    let Some(root) = fr() else { return };
    let h = hdb(root.join("XFiles.hdb"));
    let lib = xf::media::MediaLib::new(&root);
    let c = h.classes.iter().find(|c| c.id == xf::world::class::ASSET_REF).unwrap();
    let (mut n, mut accented) = (0, 0);
    for &(id, _) in &c.objects {
        let Some(t) = xf::front::doc::xtx_text(&lib, h, id) else { continue };
        n += 1;
        assert!(!t.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c)), "{id:#x} decoded to Cyrillic");
        accented += t.chars().any(|c| ('\u{c0}'..='\u{ff}').contains(&c)) as usize;
    }
    assert!(n > 100 && accented > n / 2, "{n} texts, {accented} with accented letters");
}

/// H2374: the image as the game data — English (the default) with the French folder laid over it: the game language
/// switches to French and back (strings, movies, pictures from the French folder), the settings offer both.
#[test]
fn fr_over_en_switch() {
    let Some((root, over)) = en_fr() else { return };
    let (h, g) = (hdb(root.join("XFiles.hdb")), hdb(root.join("XFiles.gam")));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    let fr_set = EnData::open(&over).unwrap();
    assert_eq!(fr_set.lang, GameLang::Fr);
    s.set_en_data(Some(Arc::new(fr_set)));
    assert_eq!((s.data_lang, s.game_lang), (GameLang::En, GameLang::En));
    assert_eq!(s.game_langs(), vec!["en", "fr"]);
    assert_eq!(s.next_game_lang(), Some("fr"));
    let ch = xf::front::settings::choices(&s, &[]);
    assert!(ch.iter().any(|(k, v)| *k == "game_lang" && v == &vec!["en", "fr"]));
    // a movie of a dialogue line (a language resource) and a picture of x.pff
    let lib = xf::media::MediaLib::new(&root);
    let c = h.classes.iter().find(|c| c.id == xf::world::class::ASSET_REF).unwrap();
    let find = |want: &dyn Fn(&xf::media::Clip) -> bool| {
        c.objects.iter().find_map(|&(id, _)| lib.resolve(h, id).filter(|cl| want(cl)).map(|cl| (id, cl)))
    };
    let (_, movie) = find(&|cl| cl.name.to_ascii_lowercase().starts_with("xv/") && cl.ext() == "xmv").unwrap();
    let (pic, _) = find(&|cl| cl.name.eq_ignore_ascii_case("x.pff")).unwrap();
    let below = |p: &Path, d: &Path| p.starts_with(d);
    assert!(!below(&s.lang_file(&movie, GameLang::En), &over));
    assert!(!below(&s.clip(pic).unwrap().file, &over));
    s.set_game_lang("fr").unwrap();
    assert_eq!(s.r.w.lang_strings.as_ref().and_then(|t| t.get(&1091)).map(|x| x.as_str()), Some("french"));
    assert!(below(&s.lang_file(&movie, GameLang::Fr), &over));
    assert!(below(&s.clip(pic).unwrap().file, &over));
    assert_eq!(s.file_lang(GameLang::Fr), GameLang::Fr);
    s.set_game_lang("en").unwrap();
    assert!(s.r.w.lang_strings.is_none());
    assert!(!below(&s.clip(pic).unwrap().file, &over));
    // a French data set over French data is not used (as English over English, H2305)
    if let Some(froot) = fr() {
        let mut t = Session::with_rules(hdb(froot.join("XFiles.hdb")), g, &froot, Rules::all());
        t.set_en_data(Some(Arc::new(EnData::open(&over).unwrap())));
        assert!(t.overlays.is_empty() && t.game_langs() == vec!["fr"]);
    }
}

/// H2380: RU 1.00 data with the image as `--en-data`: every language of the disc is offered — Russian (the game
/// data), English (the image's default folder), French (its other folder) — each labelled with its edition and source
/// by names; F4 goes through all three; French strings come from the French folder, back to Russian.
#[test]
fn h2380_ru_with_en_image_offers_three() {
    let Some(src) = image() else { return };
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    xf::vfs::install();
    let en_root = xf::vfs::data_root(&src, &mut |_| {}).expect("the EN data set");
    let sets = xf::vfs::lang_sets(&d, Some(&en_root), &mut |l| eprintln!("{l}"));
    if sets.len() < 2 {
        eprintln!("skipping: no French folder on this image");
        return;
    }
    let img = src[0].file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(sets.iter().map(|s| s.1.clone()).collect::<Vec<_>>(), vec![img.clone(), format!("{img}, Francais")]);
    let (h, g) = (hdb(d.join("XFiles.hdb")), hdb(d.join("XFiles.gam")));
    let mut s = Session::with_rules(h, g, &d, Rules::all());
    s.data_source = xf::media::lang::source_name(&[d.clone()], None);
    for (root, source) in sets {
        assert!(s.add_lang_data(Arc::new(EnData::open_from(&root, source).unwrap())));
    }
    assert_eq!(s.game_langs(), vec!["ru", "en", "fr"]);
    let labels = s.game_lang_labels();
    assert_eq!(labels[0].1, "Russian \u{2014} RU 1.00 (InstalledGame)");
    assert_eq!(labels[1].1, format!("English \u{2014} EN DVD Edition 2.05 ({img})"));
    assert_eq!(labels[2].1, format!("French \u{2014} EN DVD Edition 2.05 ({img}, Francais)"));
    assert!(labels.iter().all(|l| !l.1.contains('/')), "no paths in the labels");
    let ch = xf::front::settings::choices(&s, &[]);
    assert!(ch.iter().any(|(k, v)| *k == "game_lang" && v == &vec!["ru", "en", "fr"]));
    // the subtitle choices stay RU data's (packs over Russian data only)
    assert_eq!(s.sub_langs(&["en", "ru"]), vec!["auto", "en", "ru", "off"]);
    assert_eq!(s.next_game_lang(), Some("en"));
    s.set_game_lang("fr").unwrap();
    assert_eq!(s.r.w.lang_strings.as_ref().and_then(|t| t.get(&1091)).map(|x| x.as_str()), Some("french"));
    assert_eq!(s.file_lang(GameLang::Fr), GameLang::Fr);
    assert_eq!(s.next_game_lang(), Some("ru"));
    // H2390 (was H1834's "RU keeps its own pictures"): French over RU data takes the French picture of each `x.pff`
    // entry RU holds as English does; H2402: the two entries RU retouched (H1833, same picture sizes) too
    let lib = xf::media::MediaLib::new(&d);
    let c = h.classes.iter().find(|c| c.id == xf::world::class::ASSET_REF).unwrap();
    let pics: Vec<u32> = c
        .objects
        .iter()
        .filter_map(|&(id, _)| lib.resolve(h, id).filter(|cl| cl.name.eq_ignore_ascii_case("x.pff")).map(|_| id))
        .collect();
    let mut kept = std::collections::BTreeSet::new();
    let mut taken = 0;
    for &pic in &pics {
        let cl = s.clip(pic).unwrap();
        if cl.file.starts_with(&d) {
            kept.insert(cl.start);
        } else {
            taken += 1;
        }
    }
    assert!(taken == pics.len() && kept.is_empty(), "{taken} of {} taken, kept {kept:?}", pics.len());
    assert_eq!(s.pic_lang(), GameLang::Fr);
    s.set_game_lang("en").unwrap();
    assert_eq!(s.pic_lang(), GameLang::Ru);
    assert!(pics.iter().all(|&p| s.clip(p).unwrap().file.starts_with(&d)), "English over RU: RU's pictures (H1834)");
    assert_eq!(s.r.w.lang_strings.as_ref().and_then(|t| t.get(&1091)).map(|x| x.as_str()), Some("english"));
    s.set_game_lang("ru").unwrap();
    assert!(s.r.w.lang_strings.is_none());
    assert_eq!(s.game_lang, GameLang::Ru);
    // the same language twice is not laid over again
    let again = xf::vfs::lang_sets(&d, Some(&en_root), &mut |_| {});
    assert!(!s.add_lang_data(Arc::new(EnData::open(&again[1].0).unwrap())));
}

/// H2380 (the web's loading path): the French data set as xf-serve serves it — a flat index of the mounted folder
/// (`XFILEST.DLL`, `XV/…` at its top, no `Francais/` / `MININST/` prefix; the disc's `RECYCLED/` left out) — opens as
/// French and is taken over the image's English data. The copy holds the string table and the exe header for real,
/// the other files listed empty (the web's index only lists them).
#[test]
fn h2380_flat_french_index() {
    let Some((root, over)) = en_fr() else { return };
    fn walk(p: &Path, rel: &str, depth: usize, out: &mut Vec<(String, PathBuf)>) {
        for e in xf::io::read_dir(p).unwrap_or_default() {
            let r = if rel.is_empty() { e.name.clone() } else { format!("{rel}/{}", e.name) };
            if e.is_dir {
                if depth > 0 && !xf::vfs::is_junk_dir(&e.name) {
                    walk(&e.path, &r, depth - 1, out);
                }
            } else {
                out.push((r, e.path));
            }
        }
    }
    let mut files = Vec::new();
    walk(&over, "", 3, &mut files);
    assert!(files.len() > 1000, "{} files", files.len());
    assert!(!files.iter().any(|f| f.0.to_ascii_lowercase().starts_with("recycled/")));
    assert!(files.iter().any(|f| f.0.eq_ignore_ascii_case("xfilest.dll")), "a flat index");
    let dir = std::env::temp_dir().join(format!("xf-flat-fr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, p) in &files {
        let to = dir.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        let body = match rel.to_ascii_lowercase().as_str() {
            "xfilest.dll" => xf::io::read(p).unwrap(),
            "xfiles.exe" => xf::io::read_range(p, 0, 4096).unwrap(),
            _ => Vec::new(),
        };
        std::fs::write(&to, body).unwrap();
    }
    let set = EnData::open_from(&dir, "XFILES1.mdf, Francais".into()).unwrap();
    assert_eq!((set.lang, set.build), (GameLang::Fr, Some(Build::EnDvd)));
    let (h, g) = (hdb(root.join("XFiles.hdb")), hdb(root.join("XFiles.gam")));
    let mut s = Session::with_rules(h, g, &root, Rules::all());
    assert!(s.add_lang_data(Arc::new(set)));
    assert_eq!(s.game_langs(), vec!["en", "fr"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The FBI computer's ING search screen (RU 1.00 `fo-040-end.x`, route of `tests/computer.rs`): the login, then ING.
const ING_ROUTE: &str = "p:70,210;p:570,210;p:570,210;p:251,281;p:320,210;p:83,282;p:320,210;p:466,300;p:320,210;p:70,210;p:320,210;p:412,147;p:320,210;p:171,271;p:320,210;p:187,223;p:320,210;p:570,210;p:2,92;p:70,210;p:570,210;p:318,200;r:106,54;p:68,110";
/// RU HDB: the ING screen's disabled Forward button (InterfaceItem, its picture) and the screen's background picture
const FORWARD_ITEM: u32 = 0x1148a;
const FORWARD_PIC: u32 = 0x11482;
const ING_BACKGROUND: u32 = 0x11480;

/// H2390: RU 1.00 data with the image's languages laid over, on the ING screen in `lang`: (Forward's hit rect, where
/// its picture is drawn and its width, the background picture's file, the RU data root).
fn ru_ing(lang: &str) -> Option<([i16; 4], (i32, i32, usize), PathBuf, PathBuf)> {
    let (mut s, d) = ru_session("fo-040-end.x", lang, ING_ROUTE)?;
    let rect = s.r.clickables().into_iter().find(|c| c.owner == FORWARD_ITEM).expect("the ING screen's Forward").rect;
    let (x, y, a, _) = s.region_pictures_of().into_iter().find(|p| p.2 == FORWARD_PIC).expect("Forward's picture");
    let w = s.picture(a).unwrap().width;
    let bg = s.clip(ING_BACKGROUND).unwrap().file;
    Some((rect, (x, y, w), bg, d))
}

/// RU 1.00 data with the image's languages laid over, the bot save `save` loaded in `lang`, `steps` run: (session, the
/// RU data root)
fn ru_session(save: &str, lang: &str, steps: &str) -> Option<(Session<'static>, PathBuf)> {
    let src = image()?;
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    let save = d.join("../bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    xf::vfs::install();
    let en_root = xf::vfs::data_root(&src, &mut |_| {}).expect("the EN data set");
    let sets = xf::vfs::lang_sets(&d, Some(&en_root), &mut |_| {});
    if sets.len() < 2 {
        return None;
    }
    let (h, g) = (hdb(d.join("XFiles.hdb")), hdb(save));
    let mut s = Session::with_rules(h, g, &d, xf::front::rules_hyp());
    for (root, source) in sets {
        assert!(s.add_lang_data(Arc::new(EnData::open_from(&root, source).unwrap())));
    }
    s.set_game_lang(lang).unwrap();
    s.r.load_game();
    xf::front::settle_timers(&mut s, 100, 900_000);
    for i in xf::front::script::parse(steps).unwrap() {
        xf::front::script::apply(&mut s, &i);
        xf::front::settle_timers(&mut s, 100, 900_000);
    }
    Some((s, d))
}

/// H2390: on RU data in French the ING buttons were laid out for the French pictures (the French string table's
/// scGameLanguage moves them, H650) while the RU pictures were drawn — Forward's picture narrower than its new place.
/// Now French over RU data draws the French interface pictures, so each button's picture fills its rect; in English
/// and Russian the RU layout and pictures stay.
#[test]
fn h2390_ing_buttons_french_over_ru() {
    for lang in ["fr", "en", "ru"] {
        let Some((rect, (x, y, w), bg, d)) = ru_ing(lang) else { return };
        assert_eq!((x, y), (rect[0] as i32, rect[1] as i32), "{lang}: Forward drawn at its rect");
        assert!((w as i32 - (rect[2] - rect[0]) as i32).abs() <= 2, "{lang}: picture {w} wide in {rect:?}");
        assert_eq!(!bg.starts_with(&d), lang == "fr", "{lang}: the background from {}", bg.display());
    }
}

/// C131's route (`traces/fo-149.md`, from `data/bot/fo-144-end.x`): the desk, the answering machine, the home computer,
/// the password, E-Mail, Amis's mail, its fingerprint attachment, the FBI database, Search
const PRINTS_ROUTE: &str =
    "p:300,211;p:70,210;p:320,210;p:259,210;p:513,275;p:318,200;t:shiloh;k:13;p:68,192;p:390,247;p:381,428;p:338,373;p:557,442";
/// RU HDB: bN4WillmoreKnowsAboutCooksPrintsOnTarakan (set by that search on the original, C131)
const KNOWS_COOKS_PRINTS: u32 = 0x6af;

/// H2392: the Tarakan prints' search from the e-mail attachment runs in every game language over RU data. In French the
/// RU HDB's relocation of that screen's Search button has its right edge left of its left one (the EN HDB's is wider),
/// so the button could not be clicked.
#[test]
fn h2392_prints_search_every_language() {
    for lang in ["fr", "en", "ru"] {
        let Some((s, _)) = ru_session("fo-144-end.x", lang, PRINTS_ROUTE) else { return };
        assert_eq!(s.r.w.vars.get(KNOWS_COOKS_PRINTS), Some(1), "{lang}: the search ran");
    }
}

/// RU HDB: the FBI computer's E-mail screen background (InterfaceItem picture; the French x.pff's entry 530 in French)
const MAIL_BACKGROUND: u32 = 0x10ef0;

/// H2400: the E-mail screen's background in French over RU data — the French picture's lower bands come after a 1-bit
/// QuickTime logo BitMap, which the decoder read as a PixMap and stopped (black below y 192). The left column and the
/// frame's bottom are drawn now, in every game language.
#[test]
fn h2400_mail_background_whole() {
    let route = ING_ROUTE.rsplit_once(';').unwrap().0.to_string() + ";p:68,192";
    for lang in ["fr", "en", "ru"] {
        let Some((mut s, _)) = ru_session("fo-040-end.x", lang, &route) else { return };
        let (x, y, a, _) =
            s.region_pictures_of().into_iter().find(|p| p.2 == MAIL_BACKGROUND).expect("the E-mail screen's background");
        let p = s.picture(a).unwrap();
        assert_eq!((x, y, p.width, p.height), (0, 0, 640, 480), "{lang}");
        for (px, py) in [(40, 300), (40, 470), (320, 476)] {
            let o = (py * p.width + px) * 3;
            assert!(p.rgb[o..o + 3].iter().any(|&c| c > 60), "{lang}: black at ({px}, {py})");
        }
    }
}

/// RU HDB: the compose screen's Send button (layout 0x9516), the ING record's birth-date label and its value, the
/// help screen's text panel of the evidence icons (layout 0xc51c)
const COMPOSE_SEND: u32 = 0x118e0;
const BIRTH_LABEL: u32 = 0x8ae1;
const BIRTH_VALUE: u32 = 0x8aea;
const HELP_EVIDENCE_TEXT: u32 = 0xc79e;
/// RU HDB: Cook's name, the ING screen's key word (the text stays in the user's data)
const COOK: u32 = 0xed7a;

/// H2401: French over RU data takes the 2.05 HDB's layout where the RU HDB has none for the French pictures — the
/// compose screen's Send button (Forward from Cook's record), the record's birth-date label and value, the help
/// screen's evidence text panel (taller and wider); English and Russian keep the RU layout.
#[test]
fn h2401_layout_of_the_french_pictures() {
    let Some((s, _)) = ru_session("fo-040-end.x", "ru", "") else { return };
    let cook = common::text_of(&s.r.w, COOK);
    let base = ING_ROUTE.rsplit_once(';').unwrap().0.to_string();
    let record = format!("{base};p:68,110;p:338,373;t:{};k:13", cook.to_lowercase());
    for lang in ["fr", "en", "ru"] {
        let fr = lang == "fr";
        let Some((s, _)) = ru_session("fo-040-end.x", lang, &record) else { return };
        let rects = &s.r.w.px.item_rects;
        let want = |r: [i16; 4]| fr.then_some(r);
        assert_eq!(rects.get(&BIRTH_LABEL).copied(), want([329, 101, 435, 117]), "{lang}: the birth-date label");
        assert_eq!(rects.get(&BIRTH_VALUE).copied(), want([455, 100, 609, 116]), "{lang}: the birth date");
        let (s, _) = ru_session("fo-040-end.x", lang, &format!("{record};p:383,442")).unwrap();
        let send = s.r.clickables().into_iter().find(|c| c.owner == COMPOSE_SEND).expect("the compose screen's Send").rect;
        assert_eq!(send, if fr { [20, 366, 158, 389] } else { [43, 365, 144, 388] }, "{lang}: Send");
        let (s, _) = ru_session("fo-056-end.x", lang, "k:27;p:555,340;p:30,350").unwrap();
        let help = s.r.w.px.item_rects.get(&HELP_EVIDENCE_TEXT).copied();
        assert_eq!(help, want([18, 139, 379, 322]), "{lang}: the help text panel");
    }
}
