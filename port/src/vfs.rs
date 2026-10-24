//! Game data from several places at once: an installed game folder, the game's disc images (`.iso`, `.mdf`, raw
//! `.bin` / `.img`, read by [`crate::iso`]), copies of disc folders — merged into one virtual game folder (port
//! feature). `xf --data CD1.iso --data CD2.iso …`, `xf --data DIR` where DIR holds the images, or an installed folder
//! plus images.
//!
//! How the sources become one folder:
//! - Each source is searched for *game roots*: directories that hold the game's files (`XFiles.hdb`, `XFiles.gam`,
//!   `NAV*.NMV`, `X7.PFF`, …) or its media folders (`XV`, `XN`, `XS`, `XG`, `XT`), at most three levels deep. On the
//!   EN DVD Edition these are `English/MININST` (the files an install copies: XFILES.HDB / .GAM / .EXE, the DLLs,
//!   `XT/`, part of `XV/`), `English/MEDINST` and `English/` (the movies). An installed folder is its own game root.
//! - The game roots are layered: installed folders first (in the order given), then the images / disc folders —
//!   `MININST` before `MEDINST` before the others, discs in volume-label order (`XFILES1` < `XFILES2` …). A file is
//!   taken from the first layer that has it; a directory lists the union of its layers (case-insensitive names, as the
//!   game's own lookup: the HDB writes `nav1.nmv`, the disc has `NAV1.NMV`).
//! - A disc (image or folder copy) whose game roots lack `XFiles.hdb` is searched for a Gentee setup program
//!   (`*.exe` in its root folder and game roots) whose installer archive holds it ([`crate::gentee`]); the game's
//!   files in that archive (the HDB's file group) become a layer of their own, first on that disc. They are read
//!   out of the archive on demand, in memory: nothing is installed or written, no cache.
//! - The result is mounted at the first source's path ([`mount`]); every core read of a path below it goes through
//!   the layers ([`VfsFs`], installed with [`install`]), everything else (saves, fonts, the cache) to `std::fs`.
//!
//! Files are read lazily by offset from the image: a read returns only the requested file or range (a movie is never
//! read beyond what the player asks for; the window reads each movie file whole, as from an installed folder).
//! Images and folders are read-only; nothing is written into the game data.
use crate::gentee::Setup;
use crate::io::{Entry, Fs, StdFs};
use crate::iso::Image;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};

/// One layer of a [`DataSet`]: a real directory or a directory inside an image.
#[derive(Clone)]
pub enum Layer {
    /// a real directory (canonical path)
    Dir(PathBuf),
    /// a directory of an image: the image, its node, the directory's path inside the image (for messages)
    Iso(Arc<Image>, usize, String),
    /// the game files inside a disc's installer archive ([`crate::gentee`]): the archive, the entries that are the
    /// game's (the group of XFiles.hdb), where the installer is (for messages)
    Setup(Arc<Setup>, Arc<Vec<usize>>, String),
}

impl Layer {
    fn describe(&self) -> String {
        match self {
            Layer::Dir(d) => d.display().to_string(),
            Layer::Iso(img, _, rel) => {
                let r = if rel.is_empty() { "/".to_string() } else { format!("/{rel}") };
                format!("{} [{}]{r}", img.path.display(), img.label)
            }
            Layer::Setup(_, _, at) => format!("{at} (inside the installer)"),
        }
    }
}

/// What a relative path names in one layer.
enum Hit {
    File(PathBuf),
    Dir(PathBuf),
    IsoFile(Arc<Image>, usize),
    IsoDir(Arc<Image>, usize),
    SetupFile(Arc<Setup>, usize),
    /// a folder of an installer archive: the archive, its entries, the folder's path (`/`-separated, lower case)
    SetupDir(Arc<Setup>, Arc<Vec<usize>>, String),
}

/// The merged game data of several sources.
pub struct DataSet {
    /// the virtual game folder (absolute): paths below it are read from the layers
    pub root: PathBuf,
    pub layers: Vec<Layer>,
    /// the images opened (for messages / the disc list)
    pub images: Vec<Arc<Image>>,
    /// game roots left out: another language's folder on the same disc (the EN DVD's `Francais/`)
    pub skipped: Vec<String>,
    /// the sources it was opened from (H2372: the folder left out is opened from them, [`mount_skipped`])
    pub sources: Vec<PathBuf>,
}

/// File-name extensions taken as disc images when a source folder is scanned for them.
pub const IMAGE_EXTS: [&str; 4] = ["iso", "mdf", "bin", "img"];

/// Files of an installed game / a disc's install folder that mark a game root.
const ROOT_FILES: [&str; 12] = [
    "xfiles.hdb",
    "xfiles.gam",
    "xfiles.exe",
    "x7.pff",
    "x.pff",
    "navm.nmv",
    "nav1.nmv",
    "nav2.nmv",
    "nav3.nmv",
    "nav4.nmv",
    "nav5.nmv",
    "nav7.nmv",
];
/// Media folders that mark a game root.
const ROOT_DIRS: [&str; 5] = ["xv", "xn", "xs", "xg", "xt"];

/// The files the core needs and where it reads them (see [`DataSet::check`]).
pub const ESSENTIAL: [(&str, bool, &str); 5] = [
    ("XFiles.hdb", true, "the game database"),
    ("XFiles.gam", true, "the initial game state (New Game)"),
    ("XFiles.exe", false, "the exe's tables (computer searches, phone, cheats, e-mail replies) and cursors"),
    ("XFilesT.dll", false, "the game's string table (interface texts)"),
    ("Jrn.ttr", false, "the journal / document fonts (Jrn.ttr, Hcd.ttr)"),
];

fn lower(s: &std::ffi::OsStr) -> String {
    s.to_string_lossy().to_lowercase()
}

/// Whether `rel` has only normal components (no `..`, root, prefix).
fn plain(rel: &Path) -> bool {
    rel.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// The real path of `rel` below the directory `dir`, letter case ignored per component; None when missing or (with
/// symlinks resolved) outside `dir`.
fn dir_lookup(dir: &Path, rel: &Path) -> Option<PathBuf> {
    let direct = dir.join(rel);
    let p = if direct.exists() {
        direct
    } else {
        let mut at = dir.to_path_buf();
        for c in rel.components() {
            let Component::Normal(n) = c else { continue };
            let want = lower(n);
            let hit = std::fs::read_dir(&at).ok()?.flatten().find(|e| lower(&e.file_name()) == want)?;
            at = hit.path();
        }
        at
    };
    let c = dunce::canonicalize(p).ok()?;
    c.starts_with(dir).then_some(c)
}

impl DataSet {
    /// The sources (folders and image files) merged; Err with a message when a source cannot be read or holds no
    /// game files.
    pub fn open(sources: &[PathBuf]) -> Result<DataSet, String> {
        DataSet::open_lang(sources, None)
    }

    /// [`DataSet::open`] taking the language folder `lang` (a [`crate::media::lang::GameLang`] code: `fr` = the
    /// DVD's `Francais/`) on a disc with several (H2371); None or a language the disc lacks: as [`DataSet::open`].
    pub fn open_lang(sources: &[PathBuf], lang: Option<&str>) -> Result<DataSet, String> {
        let first = sources.first().ok_or("no data source")?;
        let root = std::path::absolute(first).map_err(|e| format!("{}: {e}", first.display()))?;
        let mut installed: Vec<Layer> = Vec::new();
        // (sort key, layer) of the discs: images and disc folders
        let mut discs: Vec<((u8, String, usize, usize), Layer)> = Vec::new();
        let mut images = Vec::new();
        let mut seen_img = HashSet::new();
        let mut skipped: Vec<String> = Vec::new();
        let mut open_image = |p: &Path,
                              images: &mut Vec<Arc<Image>>,
                              discs: &mut Vec<_>,
                              skipped: &mut Vec<String>|
         -> Result<(), String> {
            let canon = dunce::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()))?;
            if !seen_img.insert(canon.clone()) {
                return Ok(());
            }
            let img = Arc::new(Image::open(&canon).map_err(|e| e.to_string())?);
            let (roots, other) = one_family(iso_roots(&img), lang);
            for f in other {
                skipped.push(format!("{} [{}]/{f}", img.path.display(), img.label));
            }
            // the game's own files only inside the disc's installer: read from the installer archive
            let has_hdb = roots.iter().any(|(node, _)| img.child(*node, "xfiles.hdb").is_some());
            let setups = if has_hdb { Vec::new() } else { iso_setups(&img, &roots) };
            if roots.is_empty() && setups.is_empty() {
                return Err(format!("{} (volume {}): no game files on this disc image", p.display(), img.label));
            }
            let n = images.len();
            for (node, rel) in roots {
                discs
                    .push(((prio(&rel), img.label.to_lowercase(), n, depth(&rel)), Layer::Iso(img.clone(), node, rel)));
            }
            for (setup, ents, rel) in setups {
                let at = format!("{} [{}]/{rel}", img.path.display(), img.label);
                discs.push(((0, img.label.to_lowercase(), n, depth(&rel)), Layer::Setup(setup, ents, at)));
            }
            images.push(img);
            Ok(())
        };
        for (si, s) in sources.iter().enumerate() {
            let meta = std::fs::metadata(s).map_err(|e| format!("{}: {e}", s.display()))?;
            if meta.is_file() {
                open_image(s, &mut images, &mut discs, &mut skipped)?;
                continue;
            }
            let d = dunce::canonicalize(s).map_err(|e| format!("{}: {e}", s.display()))?;
            let roots = dir_roots(&d);
            let own = is_installed(&d, &roots);
            let mut imgs: Vec<PathBuf> = std::fs::read_dir(&d)
                .map_err(|e| format!("{}: {e}", d.display()))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .is_some_and(|x| IMAGE_EXTS.contains(&x.to_string_lossy().to_lowercase().as_str()))
                })
                .collect();
            // a folder of disc folders (`X-Files-DE/disk1/*.iso … disk7/*.iso`): the images one level down, as the
            // scan next to the program finds them
            if roots.is_empty() && imgs.is_empty() {
                imgs = sub_images(&d);
            }
            imgs.sort();
            if roots.is_empty() && imgs.is_empty() {
                return Err(format!(
                    "{}: no game files (XFiles.hdb) and no disc images (.iso / .mdf / .bin / .img) here or in its \
                     sub-folders",
                    d.display()
                ));
            }
            if own {
                // an installed folder: its own root only (the folder as the game sees it)
                installed.push(Layer::Dir(d.clone()));
                // a copy of a disc with media at its root and the game's files only in the installer
                if dir_lookup(&d, Path::new("xfiles.hdb")).is_none() {
                    for (setup, ents, rel) in dir_setups(&d, &[]) {
                        installed.push(Layer::Setup(setup, ents, d.join(&rel).display().to_string()));
                    }
                }
            } else if imgs.is_empty() {
                // a copy of a disc (or of several, one per sub-folder): the game roots found in it
                let label = d.file_name().map(lower).unwrap_or_default();
                let rels = roots
                    .into_iter()
                    .map(|r| (r.clone(), r.strip_prefix(&d).unwrap_or(&r).to_string_lossy().replace('\\', "/")));
                let (keep, other) = one_family(rels.collect(), lang);
                for f in other {
                    skipped.push(format!("{}/{f}", d.display()));
                }
                if !keep.iter().any(|(r, _)| dir_lookup(r, Path::new("xfiles.hdb")).is_some()) {
                    for (setup, ents, rel) in dir_setups(&d, &keep) {
                        let at = d.join(&rel).display().to_string();
                        discs.push(((0, label.clone(), 1000 + si, depth(&rel)), Layer::Setup(setup, ents, at)));
                    }
                }
                for (r, rel) in keep {
                    discs.push(((prio(&rel), label.clone(), 1000 + si, depth(&rel)), Layer::Dir(r)));
                }
            }
            // a folder of disc images: the images only (its sub-folders are not searched)
            for p in imgs {
                // a file with an image extension that is not an ISO 9660 image (a .bin of another kind) is skipped
                if crate::iso::is_image(&p) {
                    open_image(&p, &mut images, &mut discs, &mut skipped)?;
                }
            }
        }
        discs.sort_by(|a, b| a.0.cmp(&b.0));
        let mut layers = installed;
        layers.extend(discs.into_iter().map(|x| x.1));
        if layers.is_empty() {
            return Err(format!("{}: no game files found", root.display()));
        }
        Ok(DataSet { root, layers, images, skipped, sources: sources.to_vec() })
    }

    fn hits(&self, rel: &Path) -> Vec<Hit> {
        if !plain(rel) {
            return Vec::new();
        }
        let mut v = Vec::new();
        for l in &self.layers {
            match l {
                Layer::Dir(d) => {
                    if let Some(p) = dir_lookup(d, rel) {
                        v.push(if p.is_dir() { Hit::Dir(p) } else { Hit::File(p) });
                    }
                }
                Layer::Iso(img, node, _) => {
                    if let Some(i) = img.lookup(*node, rel) {
                        v.push(if img.node(i).is_dir {
                            Hit::IsoDir(img.clone(), i)
                        } else {
                            Hit::IsoFile(img.clone(), i)
                        });
                    }
                }
                Layer::Setup(s, ents, _) => {
                    let key = slash_lower(rel);
                    if key.is_empty() {
                        v.push(Hit::SetupDir(s.clone(), ents.clone(), key));
                    } else if let Some(&i) = ents.iter().find(|&&i| s.entries[i].name.to_lowercase() == key) {
                        v.push(Hit::SetupFile(s.clone(), i));
                    } else if ents.iter().any(|&i| s.entries[i].name.to_lowercase().starts_with(&format!("{key}/"))) {
                        v.push(Hit::SetupDir(s.clone(), ents.clone(), key));
                    }
                }
            }
        }
        v
    }

    /// The first layer's file at `rel` (a directory there is not a file).
    fn file(&self, rel: &Path) -> std::io::Result<Hit> {
        for h in self.hits(rel) {
            match h {
                Hit::File(_) | Hit::IsoFile(..) | Hit::SetupFile(..) => return Ok(h),
                // a directory in an earlier layer hides a file of the same name in a later one
                _ => break,
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{}: not in the game data", self.root.join(rel).display()),
        ))
    }

    pub fn read(&self, rel: &Path) -> std::io::Result<Vec<u8>> {
        match self.file(rel)? {
            Hit::File(p) => std::fs::read(p),
            Hit::IsoFile(img, i) => img.read(i),
            Hit::SetupFile(s, i) => s.read(i),
            _ => unreachable!(),
        }
    }

    pub fn size(&self, rel: &Path) -> std::io::Result<u64> {
        match self.file(rel)? {
            Hit::File(p) => Ok(std::fs::metadata(p)?.len()),
            Hit::IsoFile(img, i) => Ok(img.node(i).size),
            Hit::SetupFile(s, i) => Ok(s.entries[i].size),
            _ => unreachable!(),
        }
    }

    pub fn read_at(&self, rel: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.file(rel)? {
            Hit::File(p) => StdFs.read_at(&p, off, buf),
            Hit::IsoFile(img, i) => img.read_at(i, off, buf),
            Hit::SetupFile(s, i) => s.read_at(i, off, buf),
            _ => unreachable!(),
        }
    }

    pub fn is_file(&self, rel: &Path) -> bool {
        self.file(rel).is_ok()
    }

    /// Whether `rel` names a directory in some layer.
    pub fn is_dir(&self, rel: &Path) -> bool {
        matches!(self.hits(rel).first(), Some(Hit::Dir(_) | Hit::IsoDir(..) | Hit::SetupDir(..)))
    }

    /// The union of the directory `rel` in every layer (first layer wins a name, letter case ignored); entry paths are
    /// virtual (below [`DataSet::root`]).
    pub fn read_dir(&self, rel: &Path) -> std::io::Result<Vec<Entry>> {
        let hits = self.hits(rel);
        let base = self.root.join(rel);
        let mut names: HashMap<String, usize> = HashMap::new();
        let mut out: Vec<Entry> = Vec::new();
        let mut any = false;
        let top = rel.as_os_str().is_empty();
        let mut add = |name: String, is_dir: bool, out: &mut Vec<Entry>| {
            let k = name.to_lowercase();
            // a disc's install folders are layers of their own, not sub-folders of the game folder
            if top && is_dir && (k == "mininst" || k == "medinst") {
                return;
            }
            if !names.contains_key(&k) {
                names.insert(k, out.len());
                out.push(Entry { path: base.join(&name), name, is_dir });
            }
        };
        for h in hits {
            match h {
                Hit::Dir(d) => {
                    any = true;
                    for e in std::fs::read_dir(&d)?.flatten() {
                        let is_dir = e.path().is_dir();
                        add(e.file_name().to_string_lossy().into_owned(), is_dir, &mut out);
                    }
                }
                Hit::IsoDir(img, i) => {
                    any = true;
                    for &c in &img.node(i).children {
                        let n = img.node(c);
                        add(n.name.clone(), n.is_dir, &mut out);
                    }
                }
                Hit::SetupDir(s, ents, key) => {
                    any = true;
                    let pre = if key.is_empty() { String::new() } else { format!("{key}/") };
                    for &i in ents.iter() {
                        let name = &s.entries[i].name;
                        let head = name.get(..pre.len()).filter(|h| h.eq_ignore_ascii_case(&pre));
                        let Some(rest) = head.and_then(|_| name.get(pre.len()..)) else {
                            continue;
                        };
                        match rest.split_once('/') {
                            Some((dir, _)) => add(dir.to_string(), true, &mut out),
                            None => add(rest.to_string(), false, &mut out),
                        }
                    }
                }
                _ => {}
            }
        }
        if !any {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{}: no such directory in the game data", base.display()),
            ));
        }
        Ok(out)
    }

    /// One line per layer: where the files come from.
    pub fn describe(&self) -> Vec<String> {
        self.layers.iter().map(Layer::describe).collect()
    }

    /// Problems with the merged data: (fatal, message). Fatal = the game cannot start (no HDB / GAM). When an
    /// essential file is missing and a disc holds an installer archive (`*.CAB`), the message says so.
    pub fn check(&self) -> Vec<(bool, String)> {
        let mut out = Vec::new();
        let cab = self.images.iter().any(|img| has_cab(img, img.root(), 2));
        for (f, fatal, what) in ESSENTIAL {
            if self.is_file(Path::new(f)) {
                continue;
            }
            let hint = if cab {
                " — it is not on the discs as a plain file (only inside the installer's .CAB archive): install the game once \
                 and pass the installed folder first, e.g. --data INSTALLED --data CD1.iso …"
            } else if !self.images.is_empty() {
                " — on a CD set it is on the first disc (in MININST): give the image of disc 1 too"
            } else {
                ""
            };
            out.push((fatal, format!("{f} ({what}) not found in the game data{hint}")));
        }
        // each CD of a set holds its own bank of navigation movies at its root (NAV1.NMV on disc 1 … NAV7.NMV on
        // disc 7): one missing means that disc was not given, and the scenes filmed on it would stay black (checked
        // only when discs are read; an installed folder is taken as it is)
        let missing: Vec<u32> = (1..=7)
            .filter(|n| !self.images.is_empty() && !self.is_file(Path::new(&format!("NAV{n}.NMV"))))
            .collect();
        if !missing.is_empty() {
            let list = |f: &dyn Fn(u32) -> String| missing.iter().map(|&n| f(n)).collect::<Vec<_>>().join(", ");
            out.push((
                false,
                format!(
                    "{} not found: the movies of disc(s) {} are missing (those scenes stay black) — give every disc's \
                     image, e.g. --data CD1.iso … --data CD7.iso",
                    list(&|n| format!("NAV{n}.NMV")),
                    list(&|n| n.to_string())
                ),
            ));
        }
        out
    }
}

fn has_cab(img: &Image, dir: usize, depth: usize) -> bool {
    img.node(dir).children.iter().any(|&c| {
        let n = img.node(c);
        (!n.is_dir && n.name.to_lowercase().ends_with(".cab") && n.size > 4096)
            || (n.is_dir && depth > 0 && has_cab(img, c, depth - 1))
    })
}

/// `rel` with `/` between its components, lower case (an installer archive's names).
fn slash_lower(rel: &Path) -> String {
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(lower(n)),
            _ => None,
        })
        .collect();
    parts.join("/")
}

/// The entries of an installer archive that are the game's: the group of XFiles.hdb; None when it has no HDB.
fn game_entries(s: &Setup) -> Option<Vec<usize>> {
    let hdb = s.entries.iter().find(|e| e.name.eq_ignore_ascii_case("xfiles.hdb"))?;
    Some((0..s.entries.len()).filter(|&i| s.entries[i].group == hdb.group).collect())
}

/// Smallest program file taken as a possible installer.
const SETUP_MIN: u64 = 64 << 10;

fn is_exe(name: &str) -> bool {
    name.to_lowercase().ends_with(".exe")
}

type SetupLayer = (Arc<Setup>, Arc<Vec<usize>>, String);

/// The installer archives with the game's files on an image: the setup programs in its root folder and in its
/// game roots (rel = the program's path inside the image).
fn iso_setups(img: &Arc<Image>, roots: &[(usize, String)]) -> Vec<SetupLayer> {
    let mut dirs: Vec<(usize, String)> = vec![(img.root(), String::new())];
    dirs.extend(roots.iter().filter(|r| r.0 != img.root()).cloned());
    let mut out = Vec::new();
    for (dir, rel) in dirs {
        for &c in &img.node(dir).children {
            let n = img.node(c);
            if n.is_dir || !is_exe(&n.name) || n.size < SETUP_MIN {
                continue;
            }
            let Ok(s) = Setup::open(Box::new(crate::gentee::IsoSource(img.clone(), c))) else { continue };
            if let Some(ents) = game_entries(&s) {
                let r = if rel.is_empty() { n.name.clone() } else { format!("{rel}/{}", n.name) };
                out.push((Arc::new(s), Arc::new(ents), r));
            }
        }
    }
    out
}

/// [`iso_setups`] for a copy of a disc in the folder `d` (rel = the program's path below `d`).
fn dir_setups(d: &Path, roots: &[(PathBuf, String)]) -> Vec<SetupLayer> {
    let mut dirs: Vec<PathBuf> = vec![d.to_path_buf()];
    dirs.extend(roots.iter().map(|r| r.0.clone()).filter(|r| r != d));
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if !is_exe(&name) || !p.is_file() || p.metadata().map_or(0, |m| m.len()) < SETUP_MIN {
                continue;
            }
            let Ok(src) = crate::gentee::FileSource::open(&p) else { continue };
            let Ok(s) = Setup::open(Box::new(src)) else { continue };
            if let Some(ents) = game_entries(&s) {
                let r = p.strip_prefix(d).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.push((Arc::new(s), Arc::new(ents), r));
            }
        }
    }
    out
}

/// Layer order inside one disc: `MININST` (what an install copies), `MEDINST`, the rest.
fn prio(rel: &str) -> u8 {
    let last = rel.rsplit('/').next().unwrap_or("").to_lowercase();
    match last.as_str() {
        "mininst" => 0,
        "medinst" => 1,
        _ => 2,
    }
}

/// The family of a game root: its parent for an install folder (`English/MININST` → `English`), else itself.
fn family(rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((parent, _)) if prio(rel) < 2 => parent.to_string(),
        None if prio(rel) < 2 => String::new(),
        _ => rel.to_string(),
    }
}

/// The folder names of a language's family on a multi-language disc (H2371): the EN DVD's `English/` and
/// `Francais/`.
fn family_names(lang: &str) -> &'static [&'static str] {
    match lang.to_ascii_lowercase().as_str() {
        "fr" | "french" | "francais" => &["francais", "french", "fran\u{e7}ais"],
        "de" | "german" => &["deutsch", "german"],
        _ => &["english"],
    }
}

/// The language code of a family folder's name (`Francais` → `fr`), None for another name.
pub fn family_lang(name: &str) -> Option<&'static str> {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
    ["en", "fr", "de"].into_iter().find(|l| family_names(l).contains(&last.as_str()))
}

/// One title per disc: a disc with several families of game roots (the EN DVD's `English/` and `Francais/`, each
/// with its own MININST / MEDINST) gives the one of `lang` (H2371), else the English one, else the disc root's, else
/// the first by name. Returns the roots kept and the families left out.
fn one_family<T>(roots: Vec<(T, String)>, lang: Option<&str>) -> (Vec<(T, String)>, Vec<String>) {
    let mut fams: Vec<String> = roots.iter().map(|r| family(&r.1)).collect();
    fams.sort();
    fams.dedup();
    // a family inside another (`English` and `English/Extras`) is part of it
    let tops: Vec<String> = fams
        .iter()
        .filter(|f| !fams.iter().any(|g| g != *f && (g.is_empty() || f.starts_with(&format!("{g}/")))))
        .cloned()
        .collect();
    if tops.len() <= 1 {
        return (roots, Vec::new());
    }
    let named = |l: &str| {
        let names = family_names(l);
        tops.iter().find(|f| names.contains(&f.rsplit('/').next().unwrap_or("").to_lowercase().as_str())).cloned()
    };
    let pick = lang.and_then(named).or_else(|| named("en")).unwrap_or_else(|| tops[0].clone());
    let inside = |f: &str| f == pick || f.starts_with(&format!("{pick}/"));
    let other = tops.iter().filter(|f| **f != pick).cloned().collect();
    (roots.into_iter().filter(|r| inside(&family(&r.1))).collect(), other)
}

fn depth(rel: &str) -> usize {
    rel.split('/').filter(|s| !s.is_empty()).count()
}

fn is_root_names(names: impl Iterator<Item = (String, bool)>) -> bool {
    names
        .into_iter()
        .any(|(n, is_dir)| if is_dir { ROOT_DIRS.contains(&n.as_str()) } else { ROOT_FILES.contains(&n.as_str()) })
}

/// The game roots of an image (node, path inside the image), at most three levels below its root.
fn iso_roots(img: &Image) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    fn walk(img: &Image, i: usize, rel: String, depth: usize, out: &mut Vec<(usize, String)>) {
        let kids = &img.node(i).children;
        if is_root_names(kids.iter().map(|&c| (img.node(c).name.to_lowercase(), img.node(c).is_dir))) {
            out.push((i, rel.clone()));
        }
        if depth == 0 {
            return;
        }
        for &c in kids {
            let n = img.node(c);
            // the media folders themselves are not searched (thousands of movies)
            if n.is_dir && !ROOT_DIRS.contains(&n.name.to_lowercase().as_str()) {
                let r = if rel.is_empty() { n.name.clone() } else { format!("{rel}/{}", n.name) };
                walk(img, c, r, depth - 1, out);
            }
        }
    }
    walk(img, img.root(), String::new(), 3, &mut out);
    out
}

/// The disc image files in the direct sub-folders of `d` (hidden folders left out).
fn sub_images(d: &Path) -> Vec<PathBuf> {
    let is_img = |p: &Path| {
        p.is_file() && p.extension().is_some_and(|x| IMAGE_EXTS.contains(&x.to_string_lossy().to_lowercase().as_str()))
    };
    let mut out = Vec::new();
    for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().starts_with('.') || !e.path().is_dir() {
            continue;
        }
        out.extend(std::fs::read_dir(e.path()).into_iter().flatten().flatten().map(|f| f.path()).filter(|p| is_img(p)));
    }
    out.sort();
    out
}

/// The game roots of a real directory (canonical paths), the directory itself first when it is one.
fn dir_roots(d: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(p: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        let es: Vec<(String, PathBuf, bool)> =
            rd.flatten().map(|e| (lower(&e.file_name()), e.path(), e.path().is_dir())).collect();
        if is_root_names(es.iter().map(|(n, _, d)| (n.clone(), *d))) {
            out.push(p.to_path_buf());
            // an installed folder: not searched further — unless it is a disc's language folder with its own
            // install folders (the DVD's `Francais/` copied or mounted alone, H2370)
            if depth == 3 && !es.iter().any(|(n, _, d)| *d && (n == "mininst" || n == "medinst")) {
                return;
            }
        }
        if depth == 0 {
            return;
        }
        for (n, sp, is_dir) in es {
            if is_dir && !ROOT_DIRS.contains(&n.as_str()) && !n.starts_with('.') {
                walk(&sp, depth - 1, out);
            }
        }
    }
    walk(d, 3, &mut out);
    out
}

/// Whether the folder `d` (with its game roots `roots`, [`dir_roots`]) is an installed game: its own root, and no
/// `MININST` / `MEDINST` game root in it (H2370: a disc's language folder holds the HDB only in `MININST`).
fn is_installed(d: &Path, roots: &[PathBuf]) -> bool {
    let inst = |r: &PathBuf| {
        r.parent() == Some(d) && r.file_name().is_some_and(|n| matches!(lower(n).as_str(), "mininst" | "medinst"))
    };
    roots.first().is_some_and(|r| r == d) && !roots.iter().any(inst)
}

/// The mounted data sets.
static MOUNTS: RwLock<Vec<Arc<DataSet>>> = RwLock::new(Vec::new());

/// Mount a data set at its root: core reads below [`DataSet::root`] go to its layers (once [`VfsFs`] is the
/// installed backend). A later mount at the same root replaces the earlier one.
pub fn mount(ds: DataSet) -> Arc<DataSet> {
    let ds = Arc::new(ds);
    let mut m = MOUNTS.write().unwrap_or_else(|e| e.into_inner());
    m.retain(|x| x.root != ds.root);
    m.push(ds.clone());
    // the longest root first (a mount inside another)
    m.sort_by_key(|x| std::cmp::Reverse(x.root.as_os_str().len()));
    ds
}

/// The data set mounted at a root that `p` lies below, and `p` relative to it.
pub fn find(p: &Path) -> Option<(Arc<DataSet>, PathBuf)> {
    let m = MOUNTS.read().unwrap_or_else(|e| e.into_inner());
    if m.is_empty() {
        return None;
    }
    let abs;
    let p = if p.is_absolute() {
        p
    } else {
        abs = std::path::absolute(p).ok()?;
        &abs
    };
    m.iter().find_map(|ds| p.strip_prefix(&ds.root).ok().map(|r| (ds.clone(), r.to_path_buf())))
}

/// The file backend that serves the mounted data sets and passes everything else to `std::fs`.
pub struct VfsFs;

impl Fs for VfsFs {
    fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
        match find(p) {
            Some((ds, r)) => ds.read(&r),
            None => StdFs.read(p),
        }
    }
    fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>> {
        match find(p) {
            Some((ds, r)) => ds.read_dir(&r),
            None => StdFs.read_dir(p),
        }
    }
    fn size(&self, p: &Path) -> std::io::Result<u64> {
        match find(p) {
            Some((ds, r)) => ds.size(&r),
            None => StdFs.size(p),
        }
    }
    fn read_at(&self, p: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        match find(p) {
            Some((ds, r)) => ds.read_at(&r, off, buf),
            None => StdFs.read_at(p, off, buf),
        }
    }
    fn is_file(&self, p: &Path) -> bool {
        match find(p) {
            Some((ds, r)) => ds.is_file(&r),
            None => StdFs.is_file(p),
        }
    }
    /// The game data is read-only; other paths (the save slots) are written by `std::fs`.
    fn write(&self, p: &Path, bytes: &[u8]) -> std::io::Result<()> {
        match find(p) {
            Some(_) => Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{}: the game data is read-only", p.display()),
            )),
            None => StdFs.write(p, bytes),
        }
    }
}

static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Install [`VfsFs`] as the core's file backend (before the first read; again = no-op); false when another backend
/// is installed.
pub fn install() -> bool {
    use std::sync::atomic::Ordering;
    if INSTALLED.load(Ordering::SeqCst) {
        return true;
    }
    let ok = crate::io::install(Box::new(VfsFs));
    if ok {
        INSTALLED.store(true, Ordering::SeqCst);
    }
    ok
}

/// Whether the data sources need the merged view: more than one source, an image file, or a folder that is not an
/// installed game itself (images in it or in its sub-folders, a copy of a disc — or nothing, which [`DataSet::open`]
/// then reports). A single installed folder is read directly, as before.
pub fn needs_vfs(sources: &[PathBuf]) -> bool {
    match sources {
        [one] if one.is_dir() => {
            let Ok(c) = dunce::canonicalize(one) else { return true };
            !is_installed(&c, &dir_roots(&c))
        }
        _ => true,
    }
}

/// The game folder for the data sources (`--data` given once or more): a single installed folder as it is; otherwise
/// the merged data set, mounted (with [`VfsFs`] installed), its virtual root returned. `log` gets one line per layer
/// and the warnings; Err = cannot start (no source, unreadable, no HDB / GAM).
pub fn data_root(sources: &[PathBuf], log: &mut dyn FnMut(String)) -> Result<PathBuf, String> {
    data_root_lang(sources, None, log)
}

/// [`data_root`] taking the language folder `lang` on a multi-language disc ([`DataSet::open_lang`], H2371).
pub fn data_root_lang(sources: &[PathBuf], lang: Option<&str>, log: &mut dyn FnMut(String)) -> Result<PathBuf, String> {
    if !needs_vfs(sources) {
        return Ok(sources[0].clone());
    }
    let ds = DataSet::open_lang(sources, lang)?;
    if !install() {
        return Err("disc images: another file backend is installed".into());
    }
    for img in &ds.images {
        log(format!(
            "disc image {} — volume {}{}",
            img.path.display(),
            img.label,
            if img.joliet { " (Joliet)" } else { "" }
        ));
    }
    for l in ds.describe() {
        log(format!("  game files from {l}"));
    }
    for s in &ds.skipped {
        log(format!("  left out {s} (another language's folder on the disc)"));
    }
    let probs = ds.check();
    let ds = mount(ds);
    let mut fatal = Vec::new();
    for (f, m) in probs {
        if f {
            fatal.push(m);
        } else {
            log(format!("warning: {m}"));
        }
    }
    if !fatal.is_empty() {
        return Err(fatal.join("; "));
    }
    Ok(ds.root.clone())
}

impl DataSet {
    /// H2372: the languages of the language folders left out ([`DataSet::skipped`], e.g. `fr` for the EN DVD's
    /// `Francais/`), in order, once each.
    pub fn skipped_langs(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = Vec::new();
        for l in self.skipped.iter().filter_map(|s| family_lang(s)) {
            if !v.contains(&l) {
                v.push(l);
            }
        }
        v
    }
}

/// H2372: the language folders left out of the data set mounted at `root` ([`DataSet::skipped_langs`]); empty for a
/// folder read directly.
pub fn skipped_langs(root: &Path) -> Vec<&'static str> {
    find(root).filter(|(_, r)| r.as_os_str().is_empty()).map(|(ds, _)| ds.skipped_langs()).unwrap_or_default()
}

/// H2372: the language folder `lang` of the same sources as a second data set, mounted below the first one's root
/// (`<root>/<lang>.lang`, a name no game folder has): the data set laid over the game data by the game-language
/// switch. Returns its virtual root.
pub fn mount_lang(sources: &[PathBuf], lang: &str) -> Result<PathBuf, String> {
    let mut ds = DataSet::open_lang(sources, Some(lang))?;
    if ds.skipped.is_empty() || ds.skipped_langs().contains(&lang) {
        return Err(format!("{lang}: not a language folder of these discs"));
    }
    ds.root = ds.root.join(format!("{lang}.lang"));
    if !install() {
        return Err("disc images: another file backend is installed".into());
    }
    Ok(mount(ds).root.clone())
}

/// H2374: the first language folder left out of the data mounted at `root` (a multi-language disc: the EN DVD's
/// `Francais/`) mounted as a data set of its own ([`mount_lang`]) — the one the game-language switch lays over the
/// game data. None when nothing was left out (a failure is logged).
pub fn mount_skipped(root: &Path, log: &mut dyn FnMut(String)) -> Option<PathBuf> {
    let (ds, _) = find(root).filter(|(_, r)| r.as_os_str().is_empty())?;
    let lang = *ds.skipped_langs().first()?;
    match mount_lang(&ds.sources, lang) {
        Ok(d) => Some(d),
        Err(e) => {
            log(format!("the disc's {lang} folder: {e}"));
            None
        }
    }
}

/// H2380: every language folder left out of the data mounted at `root`, each mounted as a data set of its own
/// ([`mount_lang`]): (language code, the folder's name as on the disc — `Francais` —, its virtual root). Empty when
/// nothing was left out (a failure is logged and skipped).
pub fn mount_skipped_all(root: &Path, log: &mut dyn FnMut(String)) -> Vec<(&'static str, String, PathBuf)> {
    let Some((ds, _)) = find(root).filter(|(_, r)| r.as_os_str().is_empty()) else { return Vec::new() };
    let mut out = Vec::new();
    for lang in ds.skipped_langs() {
        let folder = ds
            .skipped
            .iter()
            .filter(|s| family_lang(s) == Some(lang))
            .map(|s| s.rsplit(['/', '\\']).next().unwrap_or("").to_string())
            .next()
            .unwrap_or_default();
        match mount_lang(&ds.sources, lang) {
            Ok(d) => out.push((lang, folder, d)),
            Err(e) => log(format!("the disc's {lang} folder: {e}")),
        }
    }
    out
}

/// H2380: every language data set of the discs given, for the game-language switch: the `--en-data` game folder
/// `en` (its default language folder, English) and the language folders left out of it, then those left out of the
/// game data `data` (the EN DVD's `Francais/`) — each once, with its source by names
/// ([`crate::media::lang::source_name`]: `XFILES1.mdf, Francais`). The data sets of the game data's own language are
/// dropped later (`Session::add_lang_data`).
pub fn lang_sets(data: &Path, en: Option<&Path>, log: &mut dyn FnMut(String)) -> Vec<(PathBuf, String)> {
    use crate::media::lang::source_name;
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    if let Some(en) = en {
        out.push((en.to_path_buf(), source_name(&sources_of(en), None)));
        for (_, folder, d) in mount_skipped_all(en, log) {
            out.push((d, source_name(&sources_of(en), Some(&folder))));
        }
    }
    for (_, folder, d) in mount_skipped_all(data, log) {
        if !out.iter().any(|o| o.0 == d) {
            out.push((d, source_name(&sources_of(data), Some(&folder))));
        }
    }
    out
}

/// H2380: a recycle bin folder left on a disc's master (`Francais/RECYCLED/`: old install files under other names,
/// H2370) — never indexed, served or read as game files.
pub fn is_junk_dir(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "recycled" | "recycler" | "$recycle.bin")
}

/// H2380: the sources the game folder `root` was made of (the data set mounted there: the images / folders given),
/// else `root` itself (a folder read directly) — for the settings' labels ([`crate::media::lang::source_name`]).
pub fn sources_of(root: &Path) -> Vec<PathBuf> {
    match find(root).filter(|(_, r)| r.as_os_str().is_empty()) {
        Some((ds, _)) => ds.sources.clone(),
        None => vec![root.to_path_buf()],
    }
}

/// The data sources of an environment variable (`XF_DATA`, `XF_EN_DATA`): one path, or several separated as in
/// `PATH` (`:` on Unix, `;` on Windows). Empty when unset.
pub fn env_sources(var: &str) -> Vec<PathBuf> {
    std::env::var_os(var)
        .map(|v| std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()).collect())
        .unwrap_or_default()
}

/// [`data_root`] for the command line tools: the game folder, the layers logged to stderr as `<tool>: …`; the
/// program ends (exit 2) when the data cannot be used.
pub fn data_root_or_exit(tool: &str, sources: &[PathBuf]) -> PathBuf {
    data_root(sources, &mut |l| eprintln!("{tool}: {l}")).unwrap_or_else(|e| {
        eprintln!("{tool}: {e}");
        std::process::exit(2)
    })
}
