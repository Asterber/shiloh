//! Port feature (not the original's): the game language — voice-over and the game's own text — switched between the
//! installed RU 1.00 data and an English data set (e.g. an English DVD Edition image), live (H1830–H1840, RE_NOTES
//! "Game language").
//!
//! The RU HDB stays the one the core runs on (its object graph = the traces'; saves and the e2e stay valid). Only the
//! language resources are taken from the English data set, by file name (same keys in both, H1832):
//! - movies and sounds `XV/`, `XG/`, `XS/` (`xmv` / `amv` / `dmv` / `mus`): video frames + sound of the playing media
//!   ([`EnIndex::lang_path`]); the core's media lengths (`Session::duration`) and `.HOT` hotspots stay the RU files';
//! - the text documents `XT/*.xtx` (journal, notes, computer files);
//! - the string table of `XfilesT.dll` (dialogue lines, PDA notes, item names, ING texts; same ids, [`strings`]).
//!
//! Not swapped: `x.pff` / `X7.PFF` / XG pictures (English in both; the RU `x.pff` matches the RU HDB's preferences
//! screen, H1834), NAV*.NMV / XN (identical). The `.ttr` game fonts differ (H1835): `front::doc::face_lang` takes the EN one.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The game language: which data set's voice-over and texts play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GameLang {
    /// the installed RU 1.00 data (the default; nothing swapped)
    #[default]
    Ru,
    /// the English data set (`--en-data DIR`)
    En,
    /// the German retail CD set as the game data (H2323; never a data set laid over another)
    De,
    /// the EN DVD Edition's `Francais/` (H2373): the game data, or laid over the disc's English data (H2374)
    Fr,
}

impl GameLang {
    pub fn parse(s: &str) -> Option<GameLang> {
        match s.to_ascii_lowercase().as_str() {
            "ru" | "rus" | "russian" => Some(GameLang::Ru),
            "en" | "eng" | "english" => Some(GameLang::En),
            "de" | "deu" | "ger" | "german" | "deutsch" => Some(GameLang::De),
            "fr" | "fra" | "fre" | "french" | "francais" | "fran\u{e7}ais" => Some(GameLang::Fr),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            GameLang::Ru => "ru",
            GameLang::En => "en",
            GameLang::De => "de",
            GameLang::Fr => "fr",
        }
    }

    /// The language's name in English (the settings' labels, H2380)
    pub fn name(self) -> &'static str {
        match self {
            GameLang::Ru => "Russian",
            GameLang::En => "English",
            GameLang::De => "German",
            GameLang::Fr => "French",
        }
    }
}

/// H2380: the edition a data set of `lang` comes from, by the build of its exe ([`crate::exetables::identify_head`];
/// None: no known exe): the RU 1.00 build, the EN DVD Edition's 2.05 (its English and French folders, H2373), the
/// German CDs' 1.00.19; the English CDs carry the RU 1.00 exe (H2329).
pub fn edition(lang: GameLang, build: Option<crate::exetables::Build>) -> String {
    use crate::exetables::Build;
    match (lang, build) {
        (GameLang::Ru, _) => "RU 1.00".into(),
        (_, Some(Build::EnDvd)) => "EN DVD Edition 2.05".into(),
        (GameLang::En, Some(Build::Ru100)) => "EN CD 1.00".into(),
        (_, Some(Build::De10019)) | (GameLang::De, None) => "DE CD 1.00.19".into(),
        (l, _) => l.code().to_ascii_uppercase(),
    }
}

/// H2380: a game language's label in the settings: language — edition (source), e.g. "French — EN DVD Edition 2.05
/// (XFILES1.mdf, Francais)"; the source only by file / folder names ([`source_name`]), never a full path.
pub fn label(lang: GameLang, build: Option<crate::exetables::Build>, source: &str) -> String {
    let e = edition(lang, build);
    if source.is_empty() {
        format!("{} \u{2014} {e}", lang.name())
    } else {
        format!("{} \u{2014} {e} ({source})", lang.name())
    }
}

/// H2380: the source of a data set as the user gave it, by names only: the image file's or the folder's name (the
/// first of several and how many more), plus the language folder taken from it when not the disc's default (`folder`,
/// e.g. `Francais`).
pub fn source_name(sources: &[PathBuf], folder: Option<&str>) -> String {
    let name = |p: &PathBuf| {
        p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| {
            std::path::absolute(p).ok().and_then(|a| a.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default()
        })
    };
    let mut s = match sources {
        [] => String::new(),
        [one] => name(one),
        [first, rest @ ..] => format!("{} +{}", name(first), rest.len()),
    };
    if let Some(f) = folder.filter(|f| !f.is_empty()) {
        if !s.is_empty() {
            s += ", ";
        }
        s += f;
    }
    s
}

/// Whether the file key `rel` (`xv/19812.xmv`, as written in an AssetRef blob, any case) is a language resource taken
/// from the English data set.
pub fn is_lang_key(rel: &str) -> bool {
    let k = rel.to_ascii_lowercase();
    let Some((dir, name)) = k.split_once('/') else { return false };
    let ext = name.rsplit_once('.').map(|x| x.1).unwrap_or("");
    match dir {
        "xv" | "xg" | "xs" => matches!(ext, "xmv" | "amv" | "dmv" | "mus"),
        "xt" => ext == "xtx",
        _ => false,
    }
}

/// H2374: whether the file key `rel` is one of the picture containers (`x.pff`, `X7.PFF`, `XG/*.pff`): swapped with
/// the language between the EN DVD's English and French data (the same HDB, H2373) — the French ones hold the French
/// interface pictures; never between RU and English data (the RU `x.pff` fits the RU HDB, H1834).
pub fn is_pic_key(rel: &str) -> bool {
    let k = rel.to_ascii_lowercase().replace('\\', "/");
    k == "x.pff" || k == "x7.pff" || (k.starts_with("xg/") && k.ends_with(".pff"))
}

/// H2374: whether the picture containers swap between data of `a` and a data set of `b` laid over it: only between
/// English and French (one HDB); RU and German keep their own (other HDBs, H1834 / H2325).
pub fn swaps_pictures(a: GameLang, b: GameLang) -> bool {
    matches!((a, b), (GameLang::En, GameLang::Fr) | (GameLang::Fr, GameLang::En))
}

/// The layers of an English data set, first wins: an installed English game dir is a single layer (like the RU
/// install); the DVD Edition's `English/` holds the hard-disk part in `MININST/` (XFILES.HDB, XFILEST.DLL, `XT/`, part of
/// `XV/` …), `MEDINST/XV` and the rest on the disc root (`XV/`, `XN/`, `XG/`, `XS/`) — repeats are the same files (H950).
pub fn layers(dir: &Path) -> Vec<PathBuf> {
    // `dir/English` when present (the disc root holds `English/` and `Francais/`); through `crate::io` (the web's files)
    let root = crate::io::read_dir(dir)
        .ok()
        .and_then(|rd| rd.into_iter().find(|e| e.is_dir && e.name.eq_ignore_ascii_case("english")))
        .map_or_else(|| dir.to_path_buf(), |e| e.path);
    let mut v = Vec::new();
    if let Ok(rd) = crate::io::read_dir(&root) {
        for want in ["mininst", "medinst"] {
            if let Some(e) = rd.iter().find(|e| e.is_dir && e.name.eq_ignore_ascii_case(want)) {
                v.push(e.path.clone());
            }
        }
    }
    v.push(root);
    v
}

/// The English data set: a case-insensitive file index over its [`layers`] (top-level files + one sub-directory level,
/// as [`super::MediaLib`]).
#[derive(Clone, Debug, Default)]
pub struct EnIndex {
    pub root: PathBuf,
    index: HashMap<String, PathBuf>,
}

impl EnIndex {
    pub fn new(dir: &Path) -> EnIndex {
        let mut index = HashMap::new();
        for layer in layers(dir) {
            let lib = super::MediaLib::new(&layer);
            for (k, p) in lib.entries() {
                index.entry(k.clone()).or_insert_with(|| p.clone());
            }
        }
        EnIndex { root: dir.to_path_buf(), index }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn find(&self, rel: &str) -> Option<&PathBuf> {
        self.index.get(&rel.to_ascii_lowercase())
    }

    /// The English file for the RU file key `rel` when it is a language resource ([`is_lang_key`]) and present.
    pub fn lang_path(&self, rel: &str) -> Option<&PathBuf> {
        if is_lang_key(rel) {
            self.find(rel)
        } else {
            None
        }
    }

    /// H2374: this data set's picture file for the key `rel` when it is a picture container ([`is_pic_key`]) and
    /// present.
    pub fn pic_path(&self, rel: &str) -> Option<&PathBuf> {
        if is_pic_key(rel) {
            self.find(rel)
        } else {
            None
        }
    }

    /// The `XfilesT.dll` string table of the English data set (empty if none).
    pub fn strings(&self) -> HashMap<u32, String> {
        match self.find("xfilest.dll") {
            Some(p) => crate::io::read(p).ok().and_then(|d| crate::pe::string_table(&d)).unwrap_or_default(),
            None => HashMap::new(),
        }
    }
}

/// H2305 / H2329 (port): the language of the game data the session runs on (`--data`), by its `XfilesT.dll` string
/// table: Cyrillic strings → Russian; else string 1091 (scGameLanguage) "german" → German, "french" → French (H2373); else
/// English. The exe is not
/// the sign: the English retail CDs carry the very exe (and HDB) of RU 1.00 (H2329), and the RU DLL keeps
/// scGameLanguage "english". Without a string table: by the exe (EN DVD → English, German 1.00.19 → German, else
/// Russian, the port's default). Cached per folder (read once).
pub fn data_lang(h: &crate::hdb::Hdb, dir: &Path) -> GameLang {
    use crate::exetables::Build;
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, Option<GameLang>>>> = std::sync::OnceLock::new();
    let by_dll = *CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(dir.to_path_buf())
        .or_insert_with(|| dll_lang(&crate::pe::text_dll_strings(dir)));
    by_dll.unwrap_or(match h.exe.build {
        Some(Build::EnDvd) => GameLang::En,
        Some(Build::De10019) => GameLang::De,
        _ => GameLang::Ru,
    })
}

/// [`data_lang`] from a string table alone (None: no strings)
pub fn dll_lang(t: &HashMap<u32, String>) -> Option<GameLang> {
    if t.is_empty() {
        None
    } else if t.values().any(|s| s.chars().any(|c| ('\u{410}'..='\u{44f}').contains(&c))) {
        Some(GameLang::Ru)
    } else if t.get(&1091).is_some_and(|s| s.trim().eq_ignore_ascii_case("german")) {
        Some(GameLang::De)
    } else if t.get(&1091).is_some_and(|s| s.trim().eq_ignore_ascii_case("french")) {
        Some(GameLang::Fr)
    } else {
        Some(GameLang::En)
    }
}

/// The language of an HDB's own data by the exe beside it alone (no file reads; Russian unless the EN DVD Edition's or
/// the German build's)
pub fn hdb_lang(h: &crate::hdb::Hdb) -> GameLang {
    match h.exe.build {
        Some(crate::exetables::Build::EnDvd) => GameLang::En,
        Some(crate::exetables::Build::De10019) => GameLang::De,
        _ => GameLang::Ru,
    }
}

impl GameLang {
    /// H2306 / H2323: the code page of this language's text files (`XT/*.xtx`, the computer's files): cp1251 for
    /// Russian, cp1252 for English, German and French (H2373)
    pub fn cp1252(self) -> bool {
        self != GameLang::Ru
    }
}

/// The data set laid over the game data as the session uses it (H1836: English over RU data; H2374: the EN DVD's
/// French folder over its English one): the file index + its string table + its language.
#[derive(Clone, Debug)]
pub struct EnData {
    pub index: EnIndex,
    pub strings: std::sync::Arc<HashMap<u32, String>>,
    /// the data set's language, by its string table ([`dll_lang`]; English without one)
    pub lang: GameLang,
    /// H2380: the build of its exe by the PE header ([`crate::exetables::identify_head`]; None: none / unknown)
    pub build: Option<crate::exetables::Build>,
    /// H2380: where it comes from, by names ([`source_name`]: `XFILES1.mdf, Francais`); set by the front
    pub source: String,
}

impl Default for EnData {
    fn default() -> EnData {
        EnData { index: EnIndex::default(), strings: Default::default(), lang: GameLang::En, build: None, source: String::new() }
    }
}

impl EnData {
    /// Err when `dir` holds no English data (no `XfilesT.dll`, no `XV/` movies).
    pub fn open(dir: &Path) -> Result<EnData, String> {
        let index = EnIndex::new(dir);
        let strings = index.strings();
        if strings.is_empty() || !index.index.keys().any(|k| k.starts_with("xv/")) {
            return Err(format!("{}: no English game data (XfilesT.dll + XV/ movies; the DVD's English/ or an installed English game)", dir.display()));
        }
        let lang = dll_lang(&strings).unwrap_or(GameLang::En);
        let build = index
            .find("xfiles.exe")
            .and_then(|p| crate::io::read_range(p, 0, 4096).ok())
            .and_then(|h| crate::exetables::identify_head(&h));
        Ok(EnData { index, strings: std::sync::Arc::new(strings), lang, build, source: String::new() })
    }

    /// [`EnData::open`] with its [`EnData::source`]
    pub fn open_from(dir: &Path, source: String) -> Result<EnData, String> {
        EnData::open(dir).map(|e| EnData { source, ..e })
    }

    /// H2380: its label in the settings ([`label`])
    pub fn label(&self) -> String {
        label(self.lang, self.build, &self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h1836_lang_keys() {
        assert!(is_lang_key("XV/19812.xmv"));
        assert!(is_lang_key("xs/67048.AMV"));
        assert!(is_lang_key("XT/80249.xtx"));
        assert!(is_lang_key("XG/49587.xmv"));
        assert!(!is_lang_key("XV/19812.HOT"));
        assert!(!is_lang_key("XG/52648.pff"));
        assert!(!is_lang_key("x.pff"));
        assert!(!is_lang_key("NAV1.NMV"));
        assert!(!is_lang_key("XN/71100.xmv"));
        assert!(!is_lang_key("Dlg.ttr"));
        assert_eq!(GameLang::parse("EN"), Some(GameLang::En));
        assert_eq!(GameLang::parse("ru"), Some(GameLang::Ru));
        assert_eq!(GameLang::parse("fr"), Some(GameLang::Fr));
        assert_eq!(GameLang::parse("it"), None);
        assert_eq!(GameLang::Fr.code(), "fr");
        assert!(GameLang::Fr.cp1252());
        assert!(is_pic_key("X.PFF") && is_pic_key("x7.pff") && is_pic_key("XG/52648.pff") && !is_pic_key("XV/1.xmv"));
        assert!(swaps_pictures(GameLang::En, GameLang::Fr) && swaps_pictures(GameLang::Fr, GameLang::En));
        assert!(!swaps_pictures(GameLang::Ru, GameLang::En) && !swaps_pictures(GameLang::De, GameLang::En));
        assert_eq!(GameLang::parse("German"), Some(GameLang::De));
        assert_eq!(GameLang::De.code(), "de");
        assert!(GameLang::De.cp1252() && GameLang::En.cp1252() && !GameLang::Ru.cp1252());
        // H2329: the string table decides — Cyrillic, then scGameLanguage (1091)
        let t = |v: &[(u32, &str)]| v.iter().map(|&(k, s)| (k, s.to_string())).collect::<HashMap<u32, String>>();
        assert_eq!(dll_lang(&t(&[])), None);
        assert_eq!(dll_lang(&t(&[(1091, "english"), (5, "\u{416}")])), Some(GameLang::Ru));
        assert_eq!(dll_lang(&t(&[(1091, "german")])), Some(GameLang::De));
        assert_eq!(dll_lang(&t(&[(1091, "english")])), Some(GameLang::En));
        assert_eq!(dll_lang(&t(&[(1091, "french")])), Some(GameLang::Fr));
    }

    /// H2380: the settings' labels — language, edition, source by names only.
    #[test]
    fn h2380_labels() {
        use crate::exetables::Build;
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(source_name(&[p("dvd/XFILES1.mdf")], None), "XFILES1.mdf");
        assert_eq!(source_name(&[p("dvd/XFILES1.mdf")], Some("Francais")), "XFILES1.mdf, Francais");
        assert_eq!(source_name(&[p("a/disk1.iso"), p("b/disk2.iso"), p("c/disk3.iso")], None), "disk1.iso +2");
        assert_eq!(source_name(&[p("x/InstalledGame/")], None), "InstalledGame");
        assert_eq!(label(GameLang::Ru, Some(Build::Ru100), "InstalledGame"), "Russian \u{2014} RU 1.00 (InstalledGame)");
        assert_eq!(label(GameLang::Fr, Some(Build::EnDvd), ""), "French \u{2014} EN DVD Edition 2.05");
        assert_eq!(edition(GameLang::En, Some(Build::Ru100)), "EN CD 1.00");
        assert_eq!(edition(GameLang::De, None), "DE CD 1.00.19");
        assert_eq!(edition(GameLang::En, None), "EN");
    }

    /// The DVD layout: MININST first, then MEDINST, then the disc root.
    #[test]
    fn h1836_dvd_layers() {
        let d = std::env::temp_dir().join(format!("xf-lang-layers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let e = d.join("English");
        for (p, b) in [
            ("MININST/XFILEST.DLL", "dll"),
            ("MININST/XV/1.XMV", "mini"),
            ("MEDINST/XV/2.XMV", "med"),
            ("XV/1.XMV", "root1"),
            ("XV/3.XMV", "root3"),
            ("MININST/XT/9.XTX", "t"),
        ] {
            std::fs::create_dir_all(e.join(p).parent().unwrap()).unwrap();
            std::fs::write(e.join(p), b).unwrap();
        }
        let ix = EnIndex::new(&d);
        let rd = |k: &str| std::fs::read_to_string(ix.find(k).unwrap()).unwrap();
        assert_eq!(rd("xv/1.xmv"), "mini");
        assert_eq!(rd("XV/2.xmv"), "med");
        assert_eq!(rd("xv/3.xmv"), "root3");
        assert_eq!(rd("xt/9.xtx"), "t");
        assert!(ix.lang_path("xfilest.dll").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
