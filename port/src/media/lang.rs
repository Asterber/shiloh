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
}

impl GameLang {
    pub fn parse(s: &str) -> Option<GameLang> {
        match s.to_ascii_lowercase().as_str() {
            "ru" | "rus" | "russian" => Some(GameLang::Ru),
            "en" | "eng" | "english" => Some(GameLang::En),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            GameLang::Ru => "ru",
            GameLang::En => "en",
        }
    }
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

    /// The `XfilesT.dll` string table of the English data set (empty if none).
    pub fn strings(&self) -> HashMap<u32, String> {
        match self.find("xfilest.dll") {
            Some(p) => crate::io::read(p).ok().and_then(|d| crate::pe::string_table(&d)).unwrap_or_default(),
            None => HashMap::new(),
        }
    }
}

/// H2305 (port): the language of the game data the session runs on (`--data`): the exe beside the HDB tells (RU 1.00
/// → Russian, the EN DVD Edition → English); an unknown exe → Russian when the `XfilesT.dll` strings hold Cyrillic,
/// English when they hold none, Russian (the port's default) without strings.
pub fn data_lang(h: &crate::hdb::Hdb, dir: &Path) -> GameLang {
    use crate::exetables::Build;
    match h.exe.build {
        Some(Build::Ru100) => GameLang::Ru,
        Some(Build::EnDvd) => GameLang::En,
        None => {
            let t = crate::pe::text_dll_strings(dir);
            let cyr = t.values().any(|s| s.chars().any(|c| ('\u{410}'..='\u{44f}').contains(&c)));
            if t.is_empty() || cyr {
                GameLang::Ru
            } else {
                GameLang::En
            }
        }
    }
}

/// The language of an HDB's own data by the exe beside it alone (no file reads; Russian unless the EN DVD Edition's)
pub fn hdb_lang(h: &crate::hdb::Hdb) -> GameLang {
    if h.exe.build == Some(crate::exetables::Build::EnDvd) {
        GameLang::En
    } else {
        GameLang::Ru
    }
}

/// The English data set as the session uses it: the file index + its string table.
#[derive(Clone, Debug, Default)]
pub struct EnData {
    pub index: EnIndex,
    pub strings: std::sync::Arc<HashMap<u32, String>>,
}

impl EnData {
    /// Err when `dir` holds no English data (no `XfilesT.dll`, no `XV/` movies).
    pub fn open(dir: &Path) -> Result<EnData, String> {
        let index = EnIndex::new(dir);
        let strings = index.strings();
        if strings.is_empty() || !index.index.keys().any(|k| k.starts_with("xv/")) {
            return Err(format!("{}: no English game data (XfilesT.dll + XV/ movies; the DVD's English/ or an installed English game)", dir.display()));
        }
        Ok(EnData { index, strings: std::sync::Arc::new(strings) })
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
        assert_eq!(GameLang::parse("fr"), None);
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
