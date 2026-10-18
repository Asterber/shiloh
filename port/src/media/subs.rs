//! Subtitle packs (H950–H959, port feature): the English captions of the user's own English copy of the game (the DVD
//! Edition or the retail CDs), taken once from the text tracks of its movies by `xf-prep --en-subs DIR` and stored in the
//! user cache dir next to the QDMC cache (`<cache>/subs-v1/en.json`), then shown over the RU build's movies of the same
//! file name. The port ships no game text: the pack is made locally from the user's files.
//!
//! What a pack holds per movie: the captions a viewer of the English movie sees, as cues in movie-**file** time — the
//! composite of all its text tracks as QuickTime draws them (H342: every track fills its rect with black and draws its
//! text, back to front; all English text tracks share the strip (0,240)–(600,260), so the **front-most** track that has
//! a sample at a time is what shows — [`visible_cues`], H952). Blank samples show nothing.
//!
//! Matching (H953): an English movie is used for the RU movie of the same directory and file name (`XV/23434.XMV` ↔
//! `XV/23434.xmv`, `pcmcache::key`), only when both durations agree within [`DUR_TOL_MS`] (otherwise the cue times
//! would not fit the RU movie's picture and sound).
//!
//! File: JSON `{"format": "xf-subs", "version": 1, "lang": "en", "movies": {"xv/23434.xmv": {"src": "XV/23434.XMV",
//! "dur_ms": 27000, "cues": [[start_ms, end_ms, "text"], …]}, …}}`. Read through [`crate::io`] (the web front fetches it
//! from xf-serve's `/cache/`).
use super::captions::{text_captions, Cue};
use super::qt::{Handler, Movie};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Sub-directory of the cache dir (bump when the layout changes).
pub const SUBDIR: &str = "subs-v1";
/// Duration difference (ms) up to which an English movie is taken as the same cut as the RU one.
pub const DUR_TOL_MS: u64 = 40;

/// Languages a pack can be in (`<cache>/subs-v1/<lang>.json`)
pub const LANGS: [&str; 2] = ["en", "ru"];
/// `auto` takes the first of these that exists: the RU build's player reads Russian best, English is the source
pub const AUTO_ORDER: [&str; 2] = ["ru", "en"];

pub fn is_lang(l: &str) -> bool {
    LANGS.contains(&l)
}

/// H2305: the languages whose pack `cache` holds (none without a cache dir)
pub fn packs_in(cache: Option<&Path>) -> Vec<&'static str> {
    LANGS.into_iter().filter(|l| cache.is_some_and(|c| crate::io::is_file(&pack_path(c, l)))).collect()
}

/// `<cache>/subs-v1/<lang>.json`
pub fn pack_path(cache: &Path, lang: &str) -> PathBuf {
    cache.join(SUBDIR).join(format!("{lang}.json"))
}

/// Movie-time boundaries (ms) of every text sample of every text track (edit lists applied), sorted, deduplicated.
fn boundaries(movie: &Movie) -> Vec<u64> {
    let mts = movie.timescale.max(1) as u64;
    let mut out = vec![0, movie.duration_ms()];
    for tr in movie.tracks.iter().filter(|t| t.handler == Handler::Text) {
        let ts = tr.timescale.max(1) as u64;
        let edits: Vec<(u64, i64, u64)> = if tr.edits.is_empty() {
            vec![(0, 0, tr.duration * mts / ts)]
        } else {
            let mut at = 0;
            tr.edits
                .iter()
                .map(|e| {
                    let x = (at, e.media_time, e.dur);
                    at += e.dur;
                    x
                })
                .collect()
        };
        for (at, mt0, dur) in edits {
            out.push(at * 1000 / mts);
            out.push((at + dur) * 1000 / mts);
            if mt0 < 0 {
                continue;
            }
            let (m0, m1) = (mt0 as u64, mt0 as u64 + dur * ts / mts);
            for &s0 in &tr.times {
                if s0 > m0 && s0 < m1 {
                    out.push(at * 1000 / mts + (s0 - m0) * 1000 / ts);
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// HYPOTHESIS H952 — the captions a viewer sees in `movie` (movie-file time): between each two sample boundaries, the
/// text of the front-most text track there ([`text_captions`] is back to front), blank = nothing; equal neighbours merged.
pub fn visible_cues(movie: &Movie, data: &[u8]) -> Vec<Cue> {
    let b = boundaries(movie);
    let mut out: Vec<Cue> = Vec::new();
    for w in b.windows(2) {
        let (a, z) = (w[0], w[1]);
        if z <= a {
            continue;
        }
        let caps = text_captions(movie, data, (a + z) / 2);
        let text = caps.last().map(|c| c.text.replace("\r\n", "\n").replace('\r', "\n")).unwrap_or_default();
        let text = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
        if text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(c) if c.end == a && c.text == text => c.end = z,
            _ => out.push(Cue { start: a, end: z, text }),
        }
    }
    out
}

/// One movie of a pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackMovie {
    /// the source movie's path relative to the English dir (information only)
    pub src: String,
    pub dur_ms: u64,
    pub cues: Vec<Cue>,
}

/// A subtitle pack: language + cues per movie key (`xv/23434.xmv`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pack {
    pub lang: String,
    pub movies: BTreeMap<String, PackMovie>,
}

impl Pack {
    pub fn to_json(&self) -> String {
        let movies: serde_json::Map<String, serde_json::Value> = self
            .movies
            .iter()
            .map(|(k, m)| {
                let cues: Vec<serde_json::Value> =
                    m.cues.iter().map(|c| serde_json::json!([c.start, c.end, c.text])).collect();
                (k.clone(), serde_json::json!({ "src": m.src, "dur_ms": m.dur_ms, "cues": cues }))
            })
            .collect();
        let v = serde_json::json!({ "format": "xf-subs", "version": 1, "lang": self.lang, "movies": movies });
        serde_json::to_string(&v).unwrap()
    }

    pub fn parse(b: &[u8]) -> Result<Pack, String> {
        let v: serde_json::Value = serde_json::from_slice(b).map_err(|e| e.to_string())?;
        if v["format"] != "xf-subs" || v["version"] != 1 {
            return Err("not an xf-subs v1 pack".into());
        }
        let mut p = Pack { lang: v["lang"].as_str().unwrap_or("").to_string(), movies: BTreeMap::new() };
        for (k, m) in v["movies"].as_object().ok_or("no movies")? {
            let cues = m["cues"]
                .as_array()
                .ok_or("cues")?
                .iter()
                .filter_map(|c| {
                    Some(Cue { start: c[0].as_u64()?, end: c[1].as_u64()?, text: c[2].as_str()?.to_string() })
                })
                .collect();
            let src = m["src"].as_str().unwrap_or("").to_string();
            p.movies.insert(k.to_ascii_lowercase(), PackMovie { src, dur_ms: m["dur_ms"].as_u64().unwrap_or(0), cues });
        }
        Ok(p)
    }

    /// Read a pack ([`crate::io`]).
    pub fn load(path: &Path) -> Result<Pack, String> {
        let b = crate::io::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Pack::parse(&b).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Pick the pack for a `--subs` / page choice: a language (`en`, `ru`) = that pack (Err when missing); `auto` = the
    /// first of [`AUTO_ORDER`] present in the cache dir (`Ok(None)` when none); `off` = none. The Russian pack is
    /// made by `xf-prep --ru-subs TABLE` ([`translate`]).
    pub fn pick(cache: &Path, mode: &str) -> Result<Option<Pack>, String> {
        Pack::pick_for(cache, mode, super::lang::GameLang::Ru)
    }

    /// [`Pack::pick`] for game data in `data_lang` (H2305): over English data `auto` takes no pack (the movies' own
    /// English captions); a language asked for by name is still loaded.
    pub fn pick_for(cache: &Path, mode: &str, data_lang: super::lang::GameLang) -> Result<Option<Pack>, String> {
        match mode {
            "auto" if data_lang == super::lang::GameLang::En => Ok(None),
            // H2323: over German data auto = the English pack when there is one
            "auto" if data_lang == super::lang::GameLang::De => Ok(Pack::load(&pack_path(cache, "en")).ok()),
            "off" => Ok(None),
            "auto" => Ok(AUTO_ORDER.iter().find_map(|l| Pack::load(&pack_path(cache, l)).ok())),
            l if is_lang(l) => Pack::load(&pack_path(cache, l)).map(Some),
            _ => Err(format!("subtitle language {mode:?}: expected en, ru, auto or off")),
        }
    }

    /// Cues for the movie file `file` (key = parent dir + file name, lower case).
    pub fn cues(&self, file: &Path) -> Option<&[Cue]> {
        self.movies.get(&super::pcmcache::key(file)).map(|m| m.cues.as_slice())
    }
}

/// An English movie with captions found by [`scan`].
#[derive(Clone, Debug)]
pub struct Source {
    pub key: String,
    /// relative to the scanned dir
    pub rel: String,
    pub dur_ms: u64,
    pub cues: Vec<Cue>,
}

// through crate::io: a folder, or a disc image mounted by crate::vfs
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(mut v) = crate::io::read_dir(dir) else { return };
    v.sort_by(|a, b| a.path.cmp(&b.path));
    for e in v {
        if e.is_dir {
            walk(&e.path, out);
        } else {
            out.push(e.path);
        }
    }
}

/// The English root inside `dir`: `dir/English` when it exists (the DVD Edition's disc root holds `English/` and
/// `Francais/`), else `dir` itself.
pub fn english_root(dir: &Path) -> PathBuf {
    if let Ok(rd) = crate::io::read_dir(dir) {
        for e in rd {
            if e.is_dir && e.name.eq_ignore_ascii_case("english") {
                return e.path;
            }
        }
    }
    dir.to_path_buf()
}

/// Counts of [`scan`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanStats {
    /// QuickTime files read
    pub movies: usize,
    /// … with text tracks
    pub with_text: usize,
    /// files whose key was taken already (the DVD's `MININST/XV`, `MEDINST/XV` and `XV` repeat files)
    pub dup: usize,
    /// … whose captions differ from the first one's
    pub conflicts: usize,
}

/// Every QuickTime movie under `dir` (recursively) with at least one visible caption, keyed like the RU data
/// (`xv/23434.xmv`); the first one (path order) wins when two files share a key.
pub fn scan(dir: &Path) -> (BTreeMap<String, Source>, ScanStats) {
    let mut files = Vec::new();
    walk(dir, &mut files);
    let mut out: BTreeMap<String, Source> = BTreeMap::new();
    let mut st = ScanStats::default();
    for f in files {
        let Ok(head) = Movie::open_header(&f) else { continue };
        st.movies += 1;
        if !super::captions::has_captions(&head) {
            continue;
        }
        st.with_text += 1;
        let Ok(data) = crate::io::read(&f) else { continue };
        let Some(m) = Movie::parse(&data) else { continue };
        let cues = visible_cues(&m, &data);
        if cues.is_empty() {
            continue;
        }
        let key = super::pcmcache::key(&f);
        if let Some(first) = out.get(&key) {
            st.dup += 1;
            st.conflicts += (first.cues != cues) as usize;
            continue;
        }
        let rel = f.strip_prefix(dir).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        out.insert(key.clone(), Source { key, rel, dur_ms: m.duration_ms(), cues });
    }
    (out, st)
}

/// How an RU movie fared against the English sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Match {
    /// same name, durations within [`DUR_TOL_MS`]: in the pack
    Used,
    /// same name, durations differ (RU ms, EN ms): left out
    DurationDiffers(u64, u64),
}

/// Build the pack for the RU data dir `ru` (its top-level files and direct sub-directories, as the engine indexes
/// them) from the English `sources`: (pack, per RU movie key the outcome).
pub fn build(ru: &Path, sources: &BTreeMap<String, Source>, lang: &str) -> (Pack, BTreeMap<String, Match>) {
    let lib = super::MediaLib::new(ru);
    let mut pack = Pack { lang: lang.to_string(), movies: BTreeMap::new() };
    let mut out = BTreeMap::new();
    for (key, s) in sources {
        let Some(f) = lib.find(key) else { continue };
        let Ok(m) = Movie::open_header(f) else { continue };
        let ru_ms = m.duration_ms();
        if ru_ms.abs_diff(s.dur_ms) > DUR_TOL_MS {
            out.insert(key.clone(), Match::DurationDiffers(ru_ms, s.dur_ms));
            continue;
        }
        pack.movies.insert(key.clone(), PackMovie { src: s.rel.clone(), dur_ms: s.dur_ms, cues: s.cues.clone() });
        out.insert(key.clone(), Match::Used);
    }
    (pack, out)
}

/// A translation table: JSON object `{"English cue text": "translated text", …}` (the user's own file, never in git —
/// the game text and its translation stay in the user's cache).
pub fn parse_table(b: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let v: serde_json::Value = serde_json::from_slice(b).map_err(|e| e.to_string())?;
    let o = v.as_object().ok_or("a translation table is a JSON object {\"English\": \"translation\"}")?;
    o.iter()
        .map(|(k, t)| Ok((k.clone(), t.as_str().ok_or_else(|| format!("{k:?}: translation is not a string"))?.to_string())))
        .collect()
}

/// A pack in language `lang` made from the English pack `en` by replacing every cue's text through `table`; same movies
/// and timings. Texts missing in the table stay English; they are returned (sorted, once each).
pub fn translate(en: &Pack, table: &BTreeMap<String, String>, lang: &str) -> (Pack, Vec<String>) {
    let mut missing = std::collections::BTreeSet::new();
    let mut out = Pack { lang: lang.to_string(), movies: BTreeMap::new() };
    for (k, m) in &en.movies {
        let cues = m
            .cues
            .iter()
            .map(|c| match table.get(&c.text) {
                Some(t) => Cue { start: c.start, end: c.end, text: t.clone() },
                None => {
                    missing.insert(c.text.clone());
                    c.clone()
                }
            })
            .collect();
        out.movies.insert(k.clone(), PackMovie { src: m.src.clone(), dur_ms: m.dur_ms, cues });
    }
    (out, missing.into_iter().collect())
}

/// The distinct cue texts of `pack` in first-seen order: the list to translate.
pub fn texts(pack: &Pack) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    pack.movies.values().flat_map(|m| m.cues.iter()).filter(|c| seen.insert(c.text.clone())).map(|c| c.text.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::captions::TextDesc;
    use crate::media::qt::{Edit, Track};

    /// A text track over `data`: samples (media time, text) appended to `data`, timescale 600, given edits.
    fn text_track(data: &mut Vec<u8>, samples: &[(u64, &str)], dur: u64, edits: Vec<Edit>, layer: i16) -> Track {
        let mut offs = Vec::new();
        for (_, t) in samples {
            offs.push((data.len() as u64, 2 + t.len() as u32));
            data.extend_from_slice(&(t.len() as u16).to_be_bytes());
            data.extend_from_slice(t.as_bytes());
        }
        Track {
            handler: Handler::Text,
            fourcc: *b"text",
            width: 600,
            height: 20,
            timescale: 600,
            duration: dur,
            edits,
            samples: offs,
            times: samples.iter().map(|s| s.0).collect(),
            snd: None,
            chunks: Vec::new(),
            enabled: true,
            layer,
            tx: 0,
            ty: 240,
            track_w: 600,
            track_h: 20,
            text: Some(TextDesc {
                display_flags: 0,
                justify: 1,
                bg: [0; 3],
                text_box: [2, 2, 598, 18],
                line_h: 16,
                ascent: 12,
                font_id: 3,
                face: 0,
                size: 12,
                fg: [255; 3],
                font_name: "Geneva".into(),
            }),
            sample_codecs: Vec::new(),
        }
    }

    /// H952: the front-most track (lowest layer, then the later track) is what shows; its blank sample hides the
    /// tracks behind; the edit list places the media (an empty edit shows nothing); equal neighbours merge.
    #[test]
    fn h952_front_most_track_wins() {
        let mut d = Vec::new();
        // back: a draft over 0..6 s
        let a = text_track(&mut d, &[(0, "The draft line."), (1800, "Bye")], 3600, vec![], 0);
        // front (later track): empty 0..1 s, then its media 0..1 s = the correction; then a blank sample 1..2 s
        let e = |dur, media_time| Edit { dur, media_time };
        let b = text_track(&mut d, &[(0, "The fixed line."), (600, " ")], 1200, vec![e(600, -1), e(1200, 0)], 0);
        // a track behind everything (layer 1) that only shows where nothing is in front of it: never here
        let c = text_track(&mut d, &[(0, "hidden")], 3600, vec![], 1);
        let m = Movie { timescale: 600, duration: 3600, tracks: vec![a, b, c] };
        let v = visible_cues(&m, &d);
        let t: Vec<(u64, u64, &str)> = v.iter().map(|c| (c.start, c.end, c.text.as_str())).collect();
        assert_eq!(
            t,
            vec![(0, 1000, "The draft line."), (1000, 2000, "The fixed line."), (3000, 6000, "Bye")],
            "2–3 s: the front track's blank sample covers the draft"
        );
    }

    /// H952: lines are trimmed, CR breaks lines, blank lines dropped.
    #[test]
    fn h952_text_normalised() {
        let mut d = Vec::new();
        let a = text_track(&mut d, &[(0, "  ( SPEAKER: )\r Hello.  \r\r")], 600, vec![], 0);
        let m = Movie { timescale: 600, duration: 600, tracks: vec![a] };
        assert_eq!(visible_cues(&m, &d), vec![Cue { start: 0, end: 1000, text: "( SPEAKER: )\nHello.".into() }]);
    }

    #[test]
    fn pack_json_round_trip() {
        let mut p = Pack { lang: "en".into(), movies: BTreeMap::new() };
        p.movies.insert(
            "xv/1.xmv".into(),
            PackMovie {
                src: "XV/1.XMV".into(),
                dur_ms: 5000,
                cues: vec![Cue { start: 10, end: 900, text: "a \"b\"\nc’".into() }],
            },
        );
        let q = Pack::parse(p.to_json().as_bytes()).unwrap();
        assert_eq!(p, q);
        assert_eq!(q.cues(Path::new("/x/XV/1.XMV")).unwrap()[0].end, 900);
        assert!(q.cues(Path::new("/x/XN/1.XMV")).is_none());
        assert!(Pack::parse(b"{\"format\":\"other\"}").is_err());
    }
    #[test]
    fn h956_translate_keeps_timings_and_reports_missing() {
        let mut en = Pack { lang: "en".into(), movies: BTreeMap::new() };
        en.movies.insert(
            "xv/1.xmv".into(),
            PackMovie {
                src: "XV/1.XMV".into(),
                dur_ms: 5000,
                cues: vec![
                    Cue { start: 0, end: 10, text: "one".into() },
                    Cue { start: 10, end: 20, text: "two".into() },
                    Cue { start: 20, end: 30, text: "one".into() },
                ],
            },
        );
        let t = parse_table("{\"one\": \"\u{43e}\u{434}\u{438}\u{43d}\"}".as_bytes()).unwrap();
        let (ru, missing) = translate(&en, &t, "ru");
        let c = &ru.movies["xv/1.xmv"].cues;
        assert_eq!((c[0].start, c[0].end, c[0].text.as_str()), (0, 10, "\u{43e}\u{434}\u{438}\u{43d}"));
        assert_eq!(c[1].text, "two");
        assert_eq!(c[2].text, c[0].text);
        assert_eq!(missing, vec!["two".to_string()]);
        assert_eq!(texts(&en), vec!["one".to_string(), "two".to_string()]);
        // the pack file round-trips with its language
        assert_eq!(Pack::parse(ru.to_json().as_bytes()).unwrap(), ru);
        assert_eq!(ru.lang, "ru");
        assert!(parse_table(b"[1]").is_err());
    }

    #[test]
    fn h956_pick_modes() {
        let d = std::env::temp_dir().join(format!("xf-subs-pick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(SUBDIR)).unwrap();
        assert!(Pack::pick(&d, "auto").unwrap().is_none());
        assert!(Pack::pick(&d, "ru").is_err());
        assert!(Pack::pick(&d, "off").unwrap().is_none());
        assert!(Pack::pick(&d, "de").is_err());
        let mk = |l: &str| Pack { lang: l.into(), movies: BTreeMap::new() };
        std::fs::write(pack_path(&d, "en"), mk("en").to_json()).unwrap();
        assert_eq!(Pack::pick(&d, "auto").unwrap().unwrap().lang, "en");
        std::fs::write(pack_path(&d, "ru"), mk("ru").to_json()).unwrap();
        assert_eq!(Pack::pick(&d, "auto").unwrap().unwrap().lang, "ru");
        assert_eq!(Pack::pick(&d, "en").unwrap().unwrap().lang, "en");
        // H2305: over English game data auto takes no pack (the movies' own captions)
        let en = crate::media::lang::GameLang::En;
        assert!(Pack::pick_for(&d, "auto", en).unwrap().is_none());
        assert_eq!(Pack::pick_for(&d, "ru", en).unwrap().unwrap().lang, "ru");
        let _ = std::fs::remove_dir_all(&d);
    }
}
