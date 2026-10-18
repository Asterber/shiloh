//! xf-prep — one-time preparation of the user's game copy for the port: decodes the 'QDMC' (QDesign Music) sound
//! tracks, which the port does not decode itself, to PCM with the user's own ffmpeg and stores them in the PCM cache
//! (`xf::media::pcmcache`). Nothing is written into the data dir.
//!
//!   xf-prep [--data DIR] [--cache DIR] [--ffmpeg PATH] [--force]
//!   xf-prep --en-subs EN_DIR [--data DIR] [--cache DIR]
//!   xf-prep --ru-subs TABLE.json [--cache DIR]
//!   xf-prep --subs-texts [--cache DIR]
//!
//! `--en-subs EN_DIR` (H955) instead makes the English subtitle pack: EN_DIR = the user's English copy of the game (the
//! DVD Edition's disc root or its `English/` folder, or an English CD / install); the captions of its movies' text
//! tracks (`xf::media::subs`) for every movie of the data dir with the same name and duration go to
//! `<cache>/subs-v1/en.json` (no ffmpeg needed; nothing is written into either game dir). Then `xf --subs en`.
//!
//! `--ru-subs TABLE.json` (H956) makes the Russian pack `<cache>/subs-v1/ru.json` from the English one (`en.json`, made
//! by `--en-subs` or copied from another machine): TABLE.json is the user's own translation, a JSON object
//! `{"English cue text": "Russian text", ...}`; every cue of en.json gets its translation, movies and timings unchanged.
//! Cue texts missing in the table stay English and are listed. `--subs-texts` prints the distinct English cue texts of
//! en.json as a JSON object with empty values: the template of such a table. Neither needs the data dir or ffmpeg; the
//! table and both packs hold game text and stay out of git. Then `xf --subs ru`.
//!
//! `--data` default `XF_DATA`, else ../data/InstalledGame; `--cache` default `XF_CACHE`, else the platform user cache
//! dir (`pcmcache::default_dir`); `--ffmpeg` default `XF_FFMPEG`, else `ffmpeg` from PATH. Scans the data dir and its
//! direct sub-directories (what the engine indexes) for QuickTime files whose first sound track is QDMC, and runs
//! `ffmpeg -ignore_editlist 1 -i FILE -map 0:a:0 -f s16le -c:a pcm_s16le -` for each (rate and channels kept): the cache
//! holds the whole media of the track; the engine cuts it to the track's length and applies the edit list itself.
//! Idempotent: an entry whose source size and hash match is kept (`--force` redoes it).
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use xf::media::audio::{self, Pcm};
use xf::media::pcmcache;
use xf::media::qt::Movie;
use xf::media::subs;

fn die(m: &str) -> ! {
    eprintln!("xf-prep: {m}");
    std::process::exit(2)
}

/// Files of `dir` and of its direct sub-directories, sorted (through xf::io: a folder or mounted disc images).
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = xf::io::read_dir(dir) else { return out };
    for e in rd {
        if e.is_dir {
            if let Ok(sub) = xf::io::read_dir(&e.path) {
                out.extend(sub.into_iter().filter(|f| !f.is_dir).map(|f| f.path));
            }
        } else {
            out.push(e.path);
        }
    }
    out.sort();
    out
}

/// `p` with its longest existing prefix canonicalized (symlinks resolved), the rest appended as given.
fn resolved(p: &Path) -> PathBuf {
    let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut rest = Vec::new();
    let mut cur = p.as_path();
    loop {
        if let Ok(c) = cur.canonicalize() {
            return rest.iter().rev().fold(c, |a: PathBuf, x| a.join(x));
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                cur = parent;
            }
            _ => return p.clone(),
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // the file backend that also reads disc images (xf::vfs)
    xf::vfs::install();
    let mut data_src: Vec<PathBuf> = Vec::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut cache = pcmcache::dir();
    let mut ffmpeg = std::env::var_os("XF_FFMPEG").map(PathBuf::from).unwrap_or_else(|| "ffmpeg".into());
    let mut force = false;
    let mut en_subs: Option<PathBuf> = None;
    let mut ru_subs: Option<PathBuf> = None;
    let mut subs_texts = false;
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| die(&format!("{x}: value missing")));
        match x.as_str() {
            "--data" => data_src.push(val().into()),
            "--cache" => cache = Some(val().into()),
            "--ffmpeg" => ffmpeg = val().into(),
            "--force" => force = true,
            "--en-subs" => en_subs = Some(val().into()),
            "--ru-subs" => ru_subs = Some(val().into()),
            "--subs-texts" => subs_texts = true,
            "--licenses" => {
                print!("{}", xf::notices::LICENSES);
                return;
            }
            "-h" | "--help" => {
                eprintln!("usage: xf-prep [--data DIR] [--cache DIR] [--ffmpeg PATH] [--force]\n       xf-prep --en-subs EN_DIR [--data DIR] [--cache DIR]\n       xf-prep --ru-subs TABLE.json [--cache DIR]\n       xf-prep --subs-texts [--cache DIR]\n       xf-prep --licenses");
                return;
            }
            // a bare path: like --data (xf::locate)
            _ if !x.starts_with('-') => paths.push(x.into()),
            _ => die(&format!("unknown argument {x}")),
        }
    }
    let cache = cache.unwrap_or_else(|| die("no cache dir: pass --cache DIR or set XF_CACHE"));
    if ru_subs.is_some() || subs_texts {
        translate_subs(&cache, ru_subs.as_deref());
        return;
    }
    // `--data` repeatable: a folder, disc images or a folder of them (xf::vfs); without it XF_DATA, a bare path, the
    // place `xf` remembered, next to the program (xf::locate)
    let data = xf::locate::find_or_exit("xf-prep", &data_src, &paths, manifest.join("../data/InstalledGame"), false);
    if xf::vfs::find(&data).is_none() && !data.is_dir() {
        die(&format!("--data {}: not a directory", data.display()));
    }
    // the cache must not land in the user's game copy (checked before anything is created)
    let d = data.canonicalize().unwrap_or_else(|_| data.clone());
    let c = resolved(&cache);
    if c.starts_with(&d) {
        die(&format!("--cache {} is inside the data dir {}: choose a directory outside it", c.display(), d.display()));
    }
    if let Some(en) = &en_subs {
        prep_subs(&data, en, &cache);
        return;
    }
    let version = match Command::new(&ffmpeg).arg("-version").stdin(Stdio::null()).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("ffmpeg").to_string(),
        Ok(o) => die(&format!("{} -version failed: {}", ffmpeg.display(), String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => die(&format!(
            "ffmpeg not found ({}: {e}). Install ffmpeg (macOS: brew install ffmpeg) or pass --ffmpeg PATH / set XF_FFMPEG",
            ffmpeg.display()
        )),
    };
    eprintln!("xf-prep: data {}  cache {}  ({version})", data.display(), cache.display());
    let (mut made, mut kept, mut failed, mut scanned) = (0, 0, 0, 0);
    for f in files(&data) {
        // QuickTime files only: parse the header (a few reads); anything else is skipped
        let Ok(movie) = Movie::open_header(&f) else { continue };
        scanned += 1;
        let Some(t) = audio::sound_track(&movie) else { continue };
        if &t.fourcc != b"QDMC" {
            continue;
        }
        let rel = f.strip_prefix(&data).unwrap_or(&f).display().to_string();
        let bytes = xf::io::read(&f).unwrap_or_else(|e| die(&format!("{}: {e}", f.display())));
        let sd = t.snd.unwrap_or_else(|| die(&format!("{rel}: QDMC track without a sound description")));
        if !force && pcmcache::is_fresh(&cache, &f, &bytes) {
            kept += 1;
            println!("{rel:<16} cached");
            continue;
        }
        let out = Command::new(&ffmpeg)
            .args(["-nostdin", "-v", "error", "-ignore_editlist", "1", "-i"])
            .arg(&f)
            .args(["-map", "0:a:0", "-f", "s16le", "-c:a", "pcm_s16le", "-"])
            .stdin(Stdio::null())
            .output()
            .unwrap_or_else(|e| die(&format!("{}: {e}", ffmpeg.display())));
        if !out.status.success() || out.stdout.is_empty() {
            failed += 1;
            println!("{rel:<16} FAILED: {}", String::from_utf8_lossy(&out.stderr).trim());
            continue;
        }
        let pcm = Pcm {
            rate: sd.rate,
            channels: sd.channels.max(1),
            samples: out.stdout.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]])).collect(),
        };
        // the track's media length in sample frames (timescale = rate in every QDMC file of the game)
        let want =
            if t.timescale == sd.rate { t.duration } else { t.duration * sd.rate as u64 / t.timescale.max(1) as u64 };
        let got = pcm.frames() as u64;
        let placed = audio::place(&movie, pcm.clone());
        let note = if got == want { String::new() } else { format!("  NOTE: media {want} frames, ffmpeg gave {got}") };
        if let Err(e) = pcmcache::store(&cache, &f, &bytes, &pcm, &version) {
            die(&format!("{}: {e}", cache.display()));
        }
        made += 1;
        println!(
            "{rel:<16} {} Hz {} ch  media {:.3} s  movie {:.3} s  ({} edit(s)){note}",
            pcm.rate,
            pcm.channels,
            got as f64 / pcm.rate as f64,
            placed.frames() as f64 / pcm.rate as f64,
            t.edits.len()
        );
    }
    println!(
        "xf-prep: {scanned} QuickTime files scanned; QDMC: {made} converted, {kept} already cached, {failed} failed → {}",
        cache.join(pcmcache::SUBDIR).display()
    );
    if failed > 0 {
        std::process::exit(1);
    }
}

/// `--ru-subs TABLE` (Some) / `--subs-texts` (None): the Russian pack from en.json and a translation table, or the template.
fn translate_subs(cache: &Path, table: Option<&Path>) {
    let en_path = subs::pack_path(cache, "en");
    let en = subs::Pack::load(&en_path)
        .unwrap_or_else(|e| die(&format!("{e} (make en.json first: xf-prep --en-subs <English game dir>, or copy it)")));
    let Some(table) = table else {
        let t: serde_json::Map<String, serde_json::Value> =
            subs::texts(&en).into_iter().map(|k| (k, serde_json::Value::String(String::new()))).collect();
        println!("{}", serde_json::to_string_pretty(&t).unwrap());
        return;
    };
    let bytes = std::fs::read(table).unwrap_or_else(|e| die(&format!("{}: {e}", table.display())));
    let table = subs::parse_table(&bytes).unwrap_or_else(|e| die(&e));
    let (ru, missing) = subs::translate(&en, &table, "ru");
    for m in &missing {
        println!("not translated: {m}");
    }
    let path = subs::pack_path(cache, "ru");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, ru.to_json())
        .and_then(|_| std::fs::rename(&tmp, &path))
        .unwrap_or_else(|x| die(&format!("{}: {x}", path.display())));
    let cues: usize = ru.movies.values().map(|m| m.cues.len()).sum();
    println!(
        "xf-prep: Russian subtitles for {} movies ({cues} cues, {} distinct texts not in the table, left English) -> {}",
        ru.movies.len(),
        missing.len(),
        path.display()
    );
}

/// `--en-subs`: scan the English copy, match its captioned movies to the data dir's, write the pack.
fn prep_subs(data: &Path, en: &Path, cache: &Path) {
    // the English copy may be a disc image (the DVD's .iso / .mdf): mounted through xf::vfs
    let en = &xf::vfs::data_root_or_exit("xf-prep: --en-subs", &[en.to_path_buf()]);
    if xf::vfs::find(en).is_none() && !en.is_dir() {
        die(&format!("--en-subs {}: not a directory", en.display()));
    }
    let e = en.canonicalize().unwrap_or_else(|_| en.clone());
    if resolved(cache).starts_with(&e) {
        die(&format!(
            "--cache {} is inside the English dir {}: choose a directory outside it",
            cache.display(),
            e.display()
        ));
    }
    let root = subs::english_root(en);
    eprintln!("xf-prep: English subtitles from {} for {}", root.display(), data.display());
    let (src, st) = subs::scan(&root);
    let raw: usize = src.values().map(|s| s.cues.len()).sum();
    println!(
        "English: {} QuickTime files, {} with text tracks, {} with visible captions ({raw} cues); {} repeated files ({} with other captions, first kept)",
        st.movies, st.with_text, src.len(), st.dup, st.conflicts
    );
    if src.is_empty() {
        die("no movie with captions found: pass the English game's folder (the DVD's disc root or English/, or an English CD)");
    }
    let (pack, matched) = subs::build(data, &src, "en");
    let mut unmatched = 0;
    for (k, m) in &matched {
        if let subs::Match::DurationDiffers(ru, en) = m {
            unmatched += 1;
            println!("{k:<16} left out: duration {ru} ms here, {en} ms in the English movie");
        }
    }
    let missing = src.len() - matched.len();
    let path = subs::pack_path(cache, "en");
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).unwrap_or_else(|x| die(&format!("{}: {x}", d.display())));
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, pack.to_json())
        .and_then(|_| std::fs::rename(&tmp, &path))
        .unwrap_or_else(|x| die(&format!("{}: {x}", path.display())));
    let cues: usize = pack.movies.values().map(|m| m.cues.len()).sum();
    println!(
        "xf-prep: {} movies of the data dir get English subtitles ({cues} cues); {unmatched} left out (duration differs), {missing} English movies have no file of that name here → {}",
        pack.movies.len(),
        path.display()
    );
}
