//! Where the player's copy of the game is (port feature): the order `xf`, `xf-serve` and `xf-prep` look for the game
//! data, so that a release starts with a double-click and no command-line flags.
//!
//! First hit wins ([`Search::candidates`]):
//! 1. `--data` (repeatable), else `XF_DATA` (a path list) — explicit: an error there ends the program, as before.
//! 2. Paths given as bare arguments (what drag and drop does: Windows and macOS start a program with the dropped
//!    files' paths), treated like `--data`.
//! 3. The source tree's `../data/InstalledGame` when it holds the game (developers and tests; a release binary built
//!    elsewhere never has it).
//! 4. The sources remembered in the settings file ([`crate::front::settings::DATA`]) from the last run that found the
//!    data itself — when they still exist. Not in headless runs (they never read the settings file, H1750).
//! 5. Next to the program ([`beside_dirs`]: the executable's own folder, its real path, and the current folder unless
//!    that is the home folder): an installed game folder (it holds `XFiles.hdb`), disc images (`.iso`, `.mdf`, `.bin`,
//!    `.img` holding the game's files), or a sub-folder that is one of those — one level deep, nothing else
//!    ([`scan`]). Several found: the most complete first, the English DVD Edition before others.
//! 6. Nothing: the window asks (a file / folder dialog); headless runs and the terminal programs stop with
//!    instructions ([`HELP`]).
//!
//! Steps 2, 5 and 6 are remembered ([`Origin::remember`]) once the data opens; `--data` / `XF_DATA` are not (like the
//! other command-line flags they hold for that run only, H1750), so a one-off `--data` never replaces the player's
//! copy.
use crate::vfs::{DataSet, IMAGE_EXTS};
use std::path::{Path, PathBuf};

/// Where the data sources came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `--data`
    Flag,
    /// `XF_DATA`
    Env,
    /// bare path arguments (drag and drop)
    Args,
    /// the source tree's `../data/InstalledGame`
    Legacy,
    /// the settings file
    Remembered,
    /// found next to the program
    Beside,
    /// chosen in the window's dialog
    Picked,
}

impl Origin {
    /// Whether these sources are written to the settings file once the data opens.
    pub fn remember(self) -> bool {
        matches!(self, Origin::Args | Origin::Beside | Origin::Picked)
    }

    /// Whether an error with these sources ends the program (the player named them) instead of trying the next step.
    pub fn explicit(self) -> bool {
        matches!(self, Origin::Flag | Origin::Env | Origin::Args | Origin::Picked)
    }

    pub fn what(self) -> &'static str {
        match self {
            Origin::Flag => "--data",
            Origin::Env => "XF_DATA",
            Origin::Args => "the path given to the program",
            Origin::Legacy => "the source tree's data/InstalledGame",
            Origin::Remembered => "remembered from the last run",
            Origin::Beside => "found next to the program",
            Origin::Picked => "chosen in the dialog",
        }
    }
}

/// Data sources to try, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub sources: Vec<PathBuf>,
    pub origin: Origin,
    /// one line: what was found (for the log)
    pub why: String,
}

/// The inputs of the search (each program fills what it has).
#[derive(Clone, Debug, Default)]
pub struct Search {
    /// `--data`
    pub flags: Vec<PathBuf>,
    /// `XF_DATA`
    pub env: Vec<PathBuf>,
    /// bare path arguments
    pub args: Vec<PathBuf>,
    /// the source tree's `../data/InstalledGame`
    pub legacy: Option<PathBuf>,
    /// the settings file's [`crate::front::settings::DATA`]
    pub remembered: Vec<PathBuf>,
    /// the folders to look in ([`beside_dirs`])
    pub beside: Vec<PathBuf>,
}

impl Search {
    /// The candidates of steps 1–4 in the order they are tried (the scan next to the program, step 5, is
    /// [`Search::resolve`]'s). Steps 1–2 give at most one (the first that has sources); the automatic steps follow
    /// only when there is none.
    pub fn candidates(&self, log: &mut dyn FnMut(String)) -> Vec<Found> {
        let one =
            |sources: &[PathBuf], origin: Origin| vec![Found { sources: sources.to_vec(), origin, why: show(sources) }];
        if !self.flags.is_empty() {
            if !self.args.is_empty() {
                // `xf --data A B.iso`: the bare paths join `--data`
                let mut s = self.flags.clone();
                s.extend(self.args.iter().cloned());
                return one(&s, Origin::Flag);
            }
            return one(&self.flags, Origin::Flag);
        }
        if !self.env.is_empty() {
            if !self.args.is_empty() {
                log(format!("XF_DATA is set: the paths given ({}) are not used", show(&self.args)));
            }
            return one(&self.env, Origin::Env);
        }
        if !self.args.is_empty() {
            return one(&self.args, Origin::Args);
        }
        let mut out = Vec::new();
        if let Some(l) = self.legacy.as_ref().filter(|l| is_install_dir(l)) {
            out.extend(one(std::slice::from_ref(l), Origin::Legacy));
        }
        if !self.remembered.is_empty() {
            if self.remembered.iter().all(|p| p.exists()) {
                out.extend(one(&self.remembered, Origin::Remembered));
            } else {
                log(format!("the remembered game data {} is gone", show(&self.remembered)));
            }
        }
        out
    }

    /// Steps 1–5: the first candidate that opens ([`resolve`]); the folders next to the program are scanned only
    /// when the steps before give nothing usable.
    pub fn resolve(&self, log: &mut dyn FnMut(String)) -> Result<(PathBuf, Found), Option<(Found, String)>> {
        match resolve(self.candidates(log), log) {
            Err(None) => resolve(scan(&self.beside, log), log),
            r => r,
        }
    }
}

fn show(p: &[PathBuf]) -> String {
    p.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
}

/// Instructions for a player when no game data was found.
pub const HELP: &str = "The X-Files Game data was not found. This port needs your own copy of the game:\n\
  - put the disc image(s) (.iso / .mdf / .bin / .img) or the installed game's folder next to the program, or\n\
  - drop the disc image or the game folder onto the program, or\n\
  - start it with --data PATH (an installed game folder, a disc image, or a folder of disc images).\n\
The window also asks for it when started with a double-click. Once found, the place is remembered.";

/// Whether `d` is an installed game folder: it holds `XFiles.hdb` (any letter case).
pub fn is_install_dir(d: &Path) -> bool {
    has_file(d, "xfiles.hdb")
}

fn has_file(d: &Path, lower: &str) -> bool {
    std::fs::read_dir(d)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().to_lowercase() == lower && e.path().is_file())
}

fn is_image_name(p: &Path) -> bool {
    p.extension().is_some_and(|x| IMAGE_EXTS.contains(&x.to_string_lossy().to_lowercase().as_str()))
}

/// The disc images directly in `d` that hold game files (other files with those extensions are left out), sorted.
fn game_images(d: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(d)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_image_name(p) && p.is_file() && crate::iso::is_image(p))
        .filter(|p| DataSet::open(std::slice::from_ref(p)).is_ok())
        .collect();
    v.sort();
    v
}

/// The folders to look for the game in: the executable's own (real path: on macOS a double-clicked binary runs with
/// the home folder as its current folder), then the current folder unless it is the home folder or a drive's root
/// (scanning those would read unrelated folders, and on macOS ask for access to Documents / Desktop).
pub fn beside_dirs() -> Vec<PathBuf> {
    let exe = std::env::current_exe().ok().and_then(|e| dunce::canonicalize(e).ok());
    let cwd = std::env::current_dir().ok().and_then(|d| dunce::canonicalize(d).ok());
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .and_then(|h| dunce::canonicalize(h).ok());
    // Windows: a program started from a shortcut or a drop may run in C:\Windows\System32 — never scanned
    let windir = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("windir"))
        .map(PathBuf::from)
        .and_then(|w| dunce::canonicalize(w).ok());
    let cwd = cwd.filter(|c| !windir.as_ref().is_some_and(|w| c.starts_with(w)));
    dirs_to_scan(exe.as_deref().and_then(Path::parent), cwd.as_deref(), home.as_deref())
}

/// [`beside_dirs`] from its inputs (tests).
pub fn dirs_to_scan(exe_dir: Option<&Path>, cwd: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = exe_dir.map(Path::to_path_buf).into_iter().collect();
    if let Some(c) = cwd {
        if Some(c) != home && c.parent().is_some() && !out.iter().any(|d| d == c) {
            out.push(c.to_path_buf());
        }
    }
    out
}

/// The game data next to the program: in each folder of `dirs` and its direct sub-folders, an installed game folder
/// or the disc images that hold game files (all images of one folder = one set). Each set is opened
/// ([`DataSet::open`]); sets without the database (`XFiles.hdb` / `XFiles.gam`) are left out. Best first: fewer
/// missing files, then the English DVD Edition, then more discs, then by path. Every set found is logged.
pub fn scan(dirs: &[PathBuf], log: &mut dyn FnMut(String)) -> Vec<Found> {
    let mut sets: Vec<Vec<PathBuf>> = Vec::new();
    let add = |d: &Path, sets: &mut Vec<Vec<PathBuf>>| {
        let set = if is_install_dir(d) { vec![d.to_path_buf()] } else { game_images(d) };
        if !set.is_empty() && !sets.contains(&set) {
            sets.push(set);
        }
    };
    for d in dirs {
        add(d, &mut sets);
        let mut subs: Vec<PathBuf> = std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        subs.sort();
        for s in subs {
            add(&s, &mut sets);
        }
    }
    // (missing files, not English, fewer discs, path) — smaller is better
    let mut ranked: Vec<((usize, bool, std::cmp::Reverse<usize>, PathBuf), Found)> = Vec::new();
    for set in sets {
        let ds = match DataSet::open(&set) {
            Ok(ds) => ds,
            Err(e) => {
                log(format!("next to the program: {} left out ({e})", show(&set)));
                continue;
            }
        };
        let probs = ds.check();
        if probs.iter().any(|p| p.0) {
            log(format!("next to the program: {} left out (no game database: {})", show(&set), probs[0].1));
            continue;
        }
        let en = is_english(&ds);
        let discs = ds.images.len();
        let mut why =
            vec![if probs.is_empty() { "complete".to_string() } else { format!("{} file(s) missing", probs.len()) }];
        if en {
            why.push("the English DVD Edition".into());
        }
        if discs > 0 {
            why.push(format!("{discs} disc image(s)"));
        } else {
            why.push("an installed game".into());
        }
        let found = Found { sources: set.clone(), origin: Origin::Beside, why: why.join(", ") };
        ranked.push(((probs.len(), !en, std::cmp::Reverse(discs), set[0].clone()), found));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0));
    let out: Vec<Found> = ranked.into_iter().map(|r| r.1).collect();
    for (i, f) in out.iter().enumerate() {
        log(format!(
            "next to the program: {} ({}){}",
            show(&f.sources),
            f.why,
            if i == 0 && out.len() > 1 { " — taken: the most complete" } else { "" }
        ));
    }
    out
}

/// Whether a data set is the English DVD Edition: its exe is that build, or its game files come from the DVD's
/// `English/` folder.
pub fn is_english(ds: &DataSet) -> bool {
    // a disc layer reads `IMAGE [LABEL]/English/MININST`: the part inside the image
    let from_english = ds
        .describe()
        .iter()
        .filter_map(|l| l.rsplit_once(']'))
        .any(|(_, rel)| rel.split(['/', '\\']).any(|c| c.eq_ignore_ascii_case("english")));
    from_english
        || ds
            .read(Path::new("XFiles.exe"))
            .ok()
            .and_then(|d| crate::exetables::identify(&d).ok())
            .is_some_and(|b| b == crate::exetables::Build::EnDvd)
}

/// Open the data sources ([`crate::vfs::data_root`]) and, when `need_db`, check that the game database is there
/// (`XFiles.hdb` and `XFiles.gam`): the game folder, or Err with a message for the player.
pub fn open(sources: &[PathBuf], need_db: bool, log: &mut dyn FnMut(String)) -> Result<PathBuf, String> {
    if let Some(p) = sources.iter().find(|p| !p.exists()) {
        return Err(format!("{}: not found", p.display()));
    }
    let root = crate::vfs::data_root(sources, log)?;
    if need_db {
        for f in ["XFiles.hdb", "XFiles.gam"] {
            if !crate::io::is_file(&root.join(f)) {
                return Err(format!("{}: no game data here ({f} not found)", show(sources)));
            }
        }
    }
    Ok(root)
}

/// The first candidate that opens: (game folder, its sources). An explicit candidate's error is returned at once
/// (Err(Some)); an automatic one's is logged and the next tried; Err(None) = nothing found.
pub fn resolve(cands: Vec<Found>, log: &mut dyn FnMut(String)) -> Result<(PathBuf, Found), Option<(Found, String)>> {
    for c in cands {
        match open(&c.sources, c.origin != Origin::Flag && c.origin != Origin::Env, log) {
            Ok(root) => return Ok((root, c)),
            Err(e) if c.origin.explicit() => return Err(Some((c, e))),
            Err(e) => log(format!("{} ({}): {e}", show(&c.sources), c.origin.what())),
        }
    }
    Err(None)
}

/// The sources as absolute paths (what the settings file keeps).
pub fn absolute(sources: &[PathBuf]) -> Vec<PathBuf> {
    sources
        .iter()
        .map(|p| dunce::canonicalize(p).or_else(|_| std::path::absolute(p)).unwrap_or_else(|_| p.clone()))
        .collect()
}

/// The remembered sources of the settings file at `config` (empty: none, or no file).
pub fn remembered(config: &Path) -> Vec<PathBuf> {
    use crate::front::settings::Settings;
    Settings::load(config, Settings::window_default()).0.data_paths()
}

/// Remember the sources in the settings file at `config` (the file's other settings kept).
pub fn remember(config: &Path, sources: &[PathBuf]) -> Result<(), String> {
    use crate::front::settings::Settings;
    let (mut set, _) = Settings::load(config, Settings::window_default());
    let abs = absolute(sources);
    if set.data_paths() == abs {
        return Ok(());
    }
    set.set_data_paths(&abs)?;
    set.save(config).map_err(|e| format!("{}: {e}", config.display()))
}

/// The terminal programs' search (`xf-serve`, `xf-prep`): `--data` sources, bare paths, the source tree's data
/// (`legacy`), the settings file's remembered place ([`crate::front::settings::config_path`]), next to the program.
/// The game folder; when `keep`, the sources are remembered (for the steps that are). Nothing found / an error: the
/// message and [`HELP`] on stderr, then exit 2 (after Enter when the console would vanish).
pub fn find_or_exit(tool: &str, flags: &[PathBuf], args: &[PathBuf], legacy: PathBuf, keep: bool) -> PathBuf {
    let cfg = crate::front::settings::config_path();
    let search = Search {
        flags: flags.to_vec(),
        env: crate::vfs::env_sources("XF_DATA"),
        args: args.to_vec(),
        legacy: Some(legacy),
        remembered: cfg.as_deref().map(remembered).unwrap_or_default(),
        beside: beside_dirs(),
    };
    let mut log = |l: String| eprintln!("{tool}: {l}");
    match search.resolve(&mut log) {
        Ok((root, f)) => {
            eprintln!("{tool}: game data {} ({}: {})", root.display(), f.origin.what(), f.why);
            if let (true, true, Some(c)) = (keep, f.origin.remember(), cfg) {
                match remember(&c, &f.sources) {
                    Ok(()) => eprintln!("{tool}: remembered in {}", c.display()),
                    Err(e) => eprintln!("{tool}: could not remember the game data: {e}"),
                }
            }
            root
        }
        Err(e) => {
            match e {
                Some((_, e)) => eprintln!("{tool}: {e}"),
                None => eprintln!("{tool}: {HELP}"),
            }
            pause_if_own_console();
            std::process::exit(2)
        }
    }
}

/// Windows: whether this process is alone on its console — started by a double-click (Explorer made the console for
/// it, and closes it when the program ends, with whatever it printed). Elsewhere false.
pub fn own_console() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
        }
        let mut ids = [0u32; 4];
        // SAFETY: the buffer holds `count` u32 entries
        let n = unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) };
        n == 1
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// When the console would vanish with the program ([`own_console`]): wait for Enter, so the player can read it.
pub fn pause_if_own_console() {
    if own_console() {
        eprintln!("\nPress Enter to close this window.");
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
}
