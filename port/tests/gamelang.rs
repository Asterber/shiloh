//! Game language (H1830–H1840, port feature): the RU HDB keeps running the game; the voice-over, the movies and the
//! texts of string resources / XT documents come from the English data set when the game language is English
//! (`Session::set_game_lang`). The RU tests use data/InstalledGame and data/bot (skipped without them); the English data
//! set is the folder `XF_EN_DIR` (e.g. the files of an English DVD Edition; skipped without it). No game text is part of
//! the tests (only whether it is Cyrillic or not).
use std::path::PathBuf;
use std::sync::Arc;
use xf::front::{self, Session};
use xf::hdb::Hdb;
use xf::media::lang::{EnData, GameLang};
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn data() -> Option<PathBuf> {
    let d = root().join("data/InstalledGame");
    d.join("XFiles.hdb").exists().then_some(d)
}

fn en() -> Option<Arc<EnData>> {
    let d = PathBuf::from(std::env::var_os("XF_EN_DIR")?);
    if !d.is_dir() {
        return None;
    }
    Some(Arc::new(EnData::open(&d).expect("the English data set")))
}

fn cyrillic(t: &str) -> bool {
    t.chars().any(|c| ('\u{410}'..='\u{44f}').contains(&c))
}

fn click(s: &mut Session, x: i32, y: i32) {
    s.hover_at(x, y);
    s.click_at(x, y);
    front::settle_timers(s, 100, 900_000);
}

/// Without an English data set English is refused and the language stays RU.
#[test]
fn h1836_english_needs_data() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    assert!(s.set_lang(GameLang::En).is_err());
    assert_eq!(s.game_lang, GameLang::Ru);
    assert!(s.r.w.lang_strings.is_none());
    assert!(s.set_lang(GameLang::Ru).is_ok());
    assert_eq!(s.game_langs(), vec!["ru"]);
    assert!(s.set_game_lang("en").is_err());
}

/// H1831 / H1836: the dialogue box's lines (XfilesT.dll strings) switch at once; the core's run (lifecycle, registrations,
/// the box's rows and history) is the same in both languages; the RU texts come back on the switch back.
#[test]
fn h1836_dialogue_lines_switch_live_logic_unchanged() {
    let (Some(d), Some(en)) = (data(), en()) else { return };
    let save = root().join("data/bot/fo-033-end.x");
    if !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let run = |lang: GameLang| {
        let mut s = Session::with_rules(&h, &g, &d, Rules::all());
        s.set_en_data(Some(en.clone()));
        s.set_lang(lang).unwrap();
        s.r.load_game();
        front::settle_timers(&mut s, 100, 900_000);
        let texts0: Vec<String> = s.r.w.dbox_line_texts().into_iter().map(|x| x.2).collect();
        click(&mut s, 260, 311); // 0x5914
        click(&mut s, 260, 311); // 0x591a
        click(&mut s, 350, 200); // the lips: the HISTORY box
        let texts: Vec<String> = s.r.w.dbox_line_texts().into_iter().map(|x| x.2).collect();
        let lines: Vec<u32> = s.r.w.dbox_layout().map(|l| l.lines.iter().map(|x| x.0).collect()).unwrap_or_default();
        (texts0, texts, lines, s.r.lifecycle.clone(), s.r.regs.clone(), s.now)
    };
    let ru = run(GameLang::Ru);
    let enr = run(GameLang::En);
    assert_eq!(ru.2, vec![0x590e, 0x5914, 0x591a]);
    assert_eq!((&ru.2, &ru.3, &ru.4, ru.5), (&enr.2, &enr.3, &enr.4, enr.5), "the core runs the same");
    assert!(ru.0.iter().chain(&ru.1).all(|t| cyrillic(t)), "RU lines are Russian");
    assert!(enr.0.iter().chain(&enr.1).all(|t| !t.is_empty() && !cyrillic(t)), "EN lines are English");
    // live: the same session, switched while the box shows
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.set_en_data(Some(en.clone()));
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    let t = |s: &Session| s.r.w.dbox_line_texts().into_iter().map(|x| x.2).collect::<Vec<_>>();
    assert_eq!(t(&s), ru.0);
    assert_eq!(s.game_langs(), vec!["ru", "en"]);
    s.set_game_lang("en").unwrap(); // the settings' string API (H1755)
    assert_eq!(t(&s), enr.0);
    s.set_game_lang("ru").unwrap();
    s.set_lang(GameLang::En).unwrap();
    assert_eq!(t(&s), enr.0);
    s.set_lang(GameLang::Ru).unwrap();
    assert_eq!(t(&s), ru.0);
}

/// H1832 / H1836: an answer's movie plays the English file of the same name, its length (the core's media end) stays the
/// RU file's; a sound already playing finishes in its language, the next line starts in the new one.
#[test]
fn h1836_media_swap_pinned_per_start() {
    let (Some(d), Some(en)) = (data(), en()) else { return };
    let save = root().join("data/bot/fo-033-end.x");
    if !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.set_en_data(Some(en.clone()));
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    s.hover_at(260, 311);
    s.click_at(260, 311);
    for _ in 0..10 {
        s.tick(100);
    }
    let a = s.r.playing().expect("the answer plays");
    let c = s.clip(a).unwrap();
    assert!(c.name.to_ascii_lowercase().starts_with("xv/"), "{}", c.name);
    let enf = s.lang_file(&c, GameLang::En);
    assert_ne!(enf, c.file, "an English file of the same name");
    assert_eq!(s.lang_file(&c, GameLang::Ru), c.file);
    let dur_ru = s.duration(a);
    let cue = |s: &mut Session| s.audio_cues().into_iter().find(|x| x.asset == a).map(|x| x.pcm);
    let before = cue(&mut s).expect("its sound plays");
    s.set_lang(GameLang::En).unwrap();
    assert_eq!(s.duration(a), dur_ru, "the core's media length stays the RU file's");
    let after = cue(&mut s).unwrap();
    assert!(Arc::ptr_eq(&before, &after), "the playing line finishes in RU");
    let en_pcm = s.sound_in(a, GameLang::En).unwrap();
    assert!(!Arc::ptr_eq(&before, &en_pcm), "the English file's own sound");
    // the next line starts in English
    front::settle_timers(&mut s, 100, 900_000);
    s.hover_at(260, 311);
    s.click_at(260, 311);
    for _ in 0..10 {
        s.tick(100);
    }
    let b = s.r.playing().expect("the next answer plays");
    let pb = s.audio_cues().into_iter().find(|x| x.asset == b).map(|x| x.pcm).expect("its sound");
    assert!(Arc::ptr_eq(&pb, &s.sound_in(b, GameLang::En).unwrap()));
}

/// H1833: an XT document (.xtx) reads the English file; RU text is Cyrillic, EN text is not.
#[test]
fn h1836_xt_documents() {
    let (Some(d), Some(en)) = (data(), en()) else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.set_en_data(Some(en));
    // every kind-1 .xtx AssetRef whose RU text is Cyrillic: English when switched
    let assets: Vec<u32> = h.class(xf::world::class::ASSET_REF).unwrap().objects.iter().map(|o| o.0).collect();
    let (mut n, mut en_cyr) = (0, 0);
    for a in assets {
        let Some(ru) = xf::front::doc::xtx_text_lang(&s, a) else { continue };
        if !cyrillic(&ru) {
            continue;
        }
        s.set_lang(GameLang::En).unwrap();
        let t = xf::front::doc::xtx_text_lang(&s, a).expect("the English document");
        s.set_lang(GameLang::Ru).unwrap();
        assert_ne!(t, ru, "asset {a:#x}");
        en_cyr += cyrillic(&t) as usize;
        n += 1;
    }
    // an English article may quote Russian words (XT/55590.xtx 'RussianGangs')
    assert!(n > 100 && en_cyr * 50 < n, "{n} RU documents, {en_cyr} English ones with Cyrillic");
}
