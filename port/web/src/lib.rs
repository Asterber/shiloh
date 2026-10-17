//! Web front of the port: the core's [`Session`] compiled to wasm32-unknown-unknown and driven from a Web Worker
//! (`www/worker.js`). Plain `extern "C"` ABI, no wasm-bindgen: the worker calls the `xf_*` exports; the module imports
//! `env.xf_js_fetch` / `env.xf_js_take` (synchronous XMLHttpRequest with a Range header — allowed in workers) and
//! `env.xf_js_log`; the save slots go through `env.xf_js_save` (xf-serve's `/saves/`, else the browser's IndexedDB —
//! the worker decides, `www/worker.js`).
//!
//! Game files are read through [`xf::io`] with [`WebFs`]: the file list comes from the server's `/data-index`, whole
//! files are fetched once per read (the core caches what it decodes), range reads go through a block cache.
//! Input and the mixer are the SDL window's ([`xf::front::host`]); one tick = the window loop's iteration (tick,
//! audio cues → mixer, render).
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use xf::front::host::{self, Button, Mix, Pointer};
use xf::front::{Session, SCREEN_H, SCREEN_W};
use xf::hdb::Hdb;
use xf::io::{Entry, Fs};

#[link(wasm_import_module = "env")]
extern "C" {
    /// GET `path` (`data/…` → /data/…, `fonts/…` → /fonts/…): `n` < 0 the whole file, else bytes `off..off+n` (Range).
    /// Returns the byte count (kept by the JS side until [`xf_js_take`]) or −1 (no such file / HTTP error).
    fn xf_js_fetch(path: *const u8, path_len: usize, off: f64, n: f64) -> f64;
    /// Copy the bytes of the last successful [`xf_js_fetch`] to `dst`.
    fn xf_js_take(dst: *mut u8);
    /// A log line (level 0 info, 1 error).
    fn xf_js_log(p: *const u8, len: usize, level: u32);
    /// A save slot file `name` (`slots.txt` / `<NAME>.x`, H402): `op` 0 = read (byte count, bytes kept until
    /// [`xf_js_take`]), 1 = write `data` (0), 2 = size; −1 = no such file / failed.
    fn xf_js_save(op: u32, name: *const u8, name_len: usize, data: *const u8, data_len: usize) -> f64;
}

/// The save slots' directory in the [`WebFs`] namespace: `saves/<file>` goes to [`xf_js_save`].
const SAVES: &str = "saves";

fn save_file(p: &Path) -> Option<&str> {
    p.strip_prefix(SAVES).ok()?.to_str().filter(|f| !f.is_empty() && !f.contains('/'))
}

fn save_op(op: u32, file: &str, data: &[u8]) -> std::io::Result<Vec<u8>> {
    let got = unsafe { xf_js_save(op, file.as_ptr(), file.len(), data.as_ptr(), data.len()) };
    if got < 0.0 {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("saves/{file}: failed")));
    }
    let mut b = Vec::new();
    if op == 0 {
        b.resize(got as usize, 0);
        if !b.is_empty() {
            unsafe { xf_js_take(b.as_mut_ptr()) };
        }
    } else if op == 2 {
        b = (got as u64).to_le_bytes().to_vec();
    }
    Ok(b)
}

fn log(s: &str, level: u32) {
    unsafe { xf_js_log(s.as_ptr(), s.len(), level) }
}

fn fetch(p: &Path, off: u64, n: Option<usize>) -> std::io::Result<Vec<u8>> {
    let s = p.to_string_lossy();
    let got = unsafe { xf_js_fetch(s.as_ptr(), s.len(), off as f64, n.map_or(-1.0, |n| n as f64)) };
    if got < 0.0 {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{s}: not served")));
    }
    let mut b = vec![0u8; got as usize];
    if !b.is_empty() {
        unsafe { xf_js_take(b.as_mut_ptr()) };
    }
    Ok(b)
}

/// Range reads are cached in blocks of this size (one request per run of missing blocks).
const BLOCK: u64 = 256 * 1024;
/// At most this many blocks kept (64 MiB); the least recently used go first.
const MAX_BLOCKS: usize = 256;

#[derive(Default)]
struct Blocks {
    map: HashMap<(PathBuf, u64), (Vec<u8>, u64)>,
    clock: u64,
}

/// The game files served by xf-serve: `data/<path>` as listed by /data-index, plus `fonts/<name>`; H1837: `en/<path>`
/// as listed by /en-index (the English data set, added later by [`xf_en_data`]).
pub struct WebFs {
    /// file path → size
    files: RwLock<HashMap<PathBuf, u64>>,
    /// directory path → entries
    dirs: RwLock<HashMap<PathBuf, Vec<Entry>>>,
    blocks: Mutex<Blocks>,
}

/// The installed [`WebFs`] (H1837: [`xf_en_data`] adds the English files to it).
static WEB_FS: OnceLock<&'static WebFs> = OnceLock::new();

impl WebFs {
    /// `index`: lines `<size>\t<relative path>` (xf-serve /data-index); files go under `root`.
    pub fn new(root: &Path, index: &str) -> WebFs {
        let fs = WebFs { files: Default::default(), dirs: Default::default(), blocks: Mutex::new(Blocks::default()) };
        fs.add(root, index);
        fs
    }

    /// Add the files of `index` (lines `<size>\t<relative path>`) under `root`.
    pub fn add(&self, root: &Path, index: &str) {
        let mut files = self.files.write().unwrap();
        let mut dirs = self.dirs.write().unwrap();
        for l in index.lines() {
            let Some((size, rel)) = l.split_once('\t') else { continue };
            let Ok(size) = size.parse::<u64>() else { continue };
            let mut parent = root.to_path_buf();
            let parts: Vec<&str> = rel.split('/').collect();
            for (i, part) in parts.iter().enumerate() {
                let path = parent.join(part);
                let is_dir = i + 1 < parts.len();
                let list = dirs.entry(parent.clone()).or_default();
                if !list.iter().any(|e| e.name == *part) {
                    list.push(Entry { name: part.to_string(), path: path.clone(), is_dir });
                }
                parent = path;
            }
            files.insert(parent, size);
        }
    }

    /// A file of the index by its path relative to the root, case-insensitively (`xfiles.hdb` → `XFiles.hdb`).
    pub fn find(&self, root: &Path, rel: &str) -> Option<PathBuf> {
        let want = root.join(rel).to_string_lossy().to_ascii_lowercase();
        self.files.read().unwrap().keys().find(|p| p.to_string_lossy().to_ascii_lowercase() == want).cloned()
    }
}

impl Fs for WebFs {
    fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
        if let Some(f) = save_file(p) {
            return save_op(0, f, &[]);
        }
        fetch(p, 0, None)
    }
    fn is_file(&self, p: &Path) -> bool {
        match save_file(p) {
            Some(f) => save_op(2, f, &[]).is_ok(),
            None => self.files.read().unwrap().contains_key(p),
        }
    }
    fn write(&self, p: &Path, bytes: &[u8]) -> std::io::Result<()> {
        match save_file(p) {
            Some(f) => save_op(1, f, bytes).map(|_| ()),
            None => Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the game files are read-only")),
        }
    }
    fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>> {
        self.dirs.read().unwrap().get(p).cloned().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such directory"))
    }
    fn size(&self, p: &Path) -> std::io::Result<u64> {
        if let Some(f) = save_file(p) {
            let b = save_op(2, f, &[])?;
            return Ok(u64::from_le_bytes(b[..8].try_into().unwrap()));
        }
        let known = self.files.read().unwrap().get(p).copied();
        match known {
            Some(n) => Ok(n),
            None => Ok(fetch(p, 0, None)?.len() as u64),
        }
    }
    fn read_at(&self, p: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        let size = self.size(p)?;
        let end = (off + buf.len() as u64).min(size);
        if off >= end {
            return Ok(0);
        }
        let (b0, b1) = (off / BLOCK, (end - 1) / BLOCK);
        let mut bl = self.blocks.lock().unwrap();
        // fetch each run of missing blocks with one request
        let mut b = b0;
        while b <= b1 {
            if bl.map.contains_key(&(p.to_path_buf(), b)) {
                b += 1;
                continue;
            }
            let mut e = b;
            while e < b1 && !bl.map.contains_key(&(p.to_path_buf(), e + 1)) {
                e += 1;
            }
            let from = b * BLOCK;
            let to = ((e + 1) * BLOCK).min(size);
            let data = fetch(p, from, Some((to - from) as usize))?;
            if (data.len() as u64) < to - from {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "short range response"));
            }
            for k in b..=e {
                let s = ((k - b) * BLOCK) as usize;
                let t = (s + BLOCK as usize).min(data.len());
                bl.clock += 1;
                let c = bl.clock;
                bl.map.insert((p.to_path_buf(), k), (data[s..t].to_vec(), c));
            }
            b = e + 1;
        }
        let mut n = 0usize;
        for k in b0..=b1 {
            bl.clock += 1;
            let c = bl.clock;
            let Some(blk) = bl.map.get_mut(&(p.to_path_buf(), k)) else { break };
            blk.1 = c;
            let bs = k * BLOCK;
            let s = off.max(bs) - bs;
            let t = end.min(bs + blk.0.len() as u64) - bs;
            if s < t {
                buf[n..n + (t - s) as usize].copy_from_slice(&blk.0[s as usize..t as usize]);
                n += (t - s) as usize;
            }
        }
        // evict the least recently used blocks
        if bl.map.len() > MAX_BLOCKS {
            let mut ages: Vec<(u64, (PathBuf, u64))> = bl.map.iter().map(|(k, v)| (v.1, k.clone())).collect();
            ages.sort_by_key(|x| x.0);
            let drop = bl.map.len() - MAX_BLOCKS;
            for (_, k) in ages.into_iter().take(drop) {
                bl.map.remove(&k);
            }
        }
        Ok(n)
    }
    fn ranged(&self) -> bool {
        true
    }
}

/// The installed backend: the leaked [`WebFs`] ([`WEB_FS`]).
struct FsRef(&'static WebFs);

impl Fs for FsRef {
    fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
        self.0.read(p)
    }
    fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>> {
        self.0.read_dir(p)
    }
    fn size(&self, p: &Path) -> std::io::Result<u64> {
        self.0.size(p)
    }
    fn read_at(&self, p: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read_at(p, off, buf)
    }
    fn ranged(&self) -> bool {
        self.0.ranged()
    }
    fn is_file(&self, p: &Path) -> bool {
        self.0.is_file(p)
    }
    fn write(&self, p: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.0.write(p, bytes)
    }
}

struct State {
    s: Session<'static>,
    mix: Mix,
    /// H1791–H1793: the sink's queue depth and latency (front::avsync)
    pump: xf::front::avsync::Pump,
    rgba: Vec<u8>,
    pcm16: Vec<i16>,
    pcm: Vec<f32>,
    logged: usize,
    shown_lines: Vec<(u32, [i16; 4], String)>,
    /// XFiles.gam (the menu's New starts from it)
    gam: PathBuf,
    /// the menu's Quit was chosen ([`xf_quit`])
    quit: bool,
    /// the player's settings (H1750; the page keeps them in localStorage and sends each through [`xf_set`])
    set: xf::front::settings::Settings,
    /// H2305: the subtitle packs xf-serve's cache holds (looked up once; only over Russian game data)
    packs: Vec<&'static str>,
    /// the menu screen's "Port settings" button was clicked (H1751): the page opens its dialog ([`xf_settings_req`])
    settings_req: bool,
    /// H2100: a Save wrote the host's Preferences into `set` — the page keeps them ([`xf_prefs_saved`])
    prefs_saved: bool,
    /// the pointer is over that button
    menu_hover: bool,
    /// [`xf_settings`]'s text
    text: String,
}

/// The menu's New / Quit and the load screen's Load (H403 / H404), as the window's `host_request` (bin/xf.rs): a new
/// core from XFiles.gam or the save file (leaked: it lives as long as the worker).
fn host_request(st: &mut State) {
    // H2100 / H2084: a Save wrote the running game's Preferences to the host's (the original's registry)
    if std::mem::take(&mut st.s.prefs_saved) {
        st.set.set_host_prefs(&st.s.host_prefs);
        st.prefs_saved = true;
    }
    let Some(req) = host::request(&mut st.s) else { return };
    let (path, load) = match req {
        host::HostReq::Quit => {
            log("menu: Quit", 0);
            st.quit = true;
            return;
        }
        host::HostReq::New => (st.gam.clone(), false),
        host::HostReq::Load(p) => (p, true),
    };
    match Hdb::open(&path) {
        Ok(g) => {
            let g: &'static Hdb = Box::leak(Box::new(g));
            log(&format!("{} {}", if load { "load" } else { "new game from" }, path.display()), 0);
            st.s.reload(g, load);
            if let Some(on) = st.set.captions() {
                st.s.r.set_closed_captioning(on);
            }
        }
        Err(e) => log(&format!("{}: {e}", path.display()), 1),
    }
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|st| st.borrow_mut().as_mut().map(f))
}

#[no_mangle]
pub extern "C" fn xf_alloc(n: usize) -> *mut u8 {
    let mut v = vec![0u8; n.max(1)];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

#[no_mangle]
pub unsafe extern "C" fn xf_free(p: *mut u8, n: usize) {
    drop(Vec::from_raw_parts(p, n.max(1), n.max(1)));
}

unsafe fn str_arg<'a>(p: *const u8, n: usize) -> &'a str {
    if n == 0 {
        return "";
    }
    std::str::from_utf8(std::slice::from_raw_parts(p, n)).unwrap_or("")
}

/// Start: `index` = xf-serve /data-index, `trace` = a trace of the original (system-value conditions, as the window
/// takes them from traces/fo-009.jsonl; may be empty), `hyp` = every hypothesis rule (the window's default), `start`
/// (H1660 / H1661) = 0 the original's cold start (the intro movies, then the main menu), 1 the main menu at once, 2 New
/// Game at once (the test mode). Returns 1 on success.
#[no_mangle]
pub unsafe extern "C" fn xf_init(
    index: *const u8,
    index_len: usize,
    trace: *const u8,
    trace_len: usize,
    hyp: u32,
    start: u32,
) -> u32 {
    std::panic::set_hook(Box::new(|i| log(&format!("panic: {i}"), 1)));
    let root = Path::new("data");
    let fs: &'static WebFs = Box::leak(Box::new(WebFs::new(root, str_arg(index, index_len))));
    let (Some(hdb), Some(gam)) = (fs.find(root, "XFiles.hdb"), fs.find(root, "XFiles.gam")) else {
        log("XFiles.hdb / XFiles.gam not in the server's data dir (xf-serve --data DIR)", 1);
        return 0;
    };
    let _ = WEB_FS.set(fs);
    xf::io::install(Box::new(FsRef(fs)));
    // QDMC music: the PCM cache xf-serve serves under /cache/ (xf-prep, see port/README.md)
    xf::media::pcmcache::set_dir("cache");
    // Courier New / Arial (UI, phone, computer, the Bible): xf-serve's /fonts (`--fonts DIR`) — every system font load
    // of the core searches this dir (front::text::font_dirs; the system font dirs are not reachable from the page)
    xf::front::text::set_web_font_dir("fonts");
    let open = |p: &Path| match Hdb::open(p) {
        Ok(h) => Some(&*Box::leak(Box::new(h))),
        Err(e) => {
            log(&e.to_string(), 1);
            None
        }
    };
    let (Some(h), Some(g)) = (open(&hdb), open(&gam)) else { return 0 };
    let rules = if hyp != 0 { xf::front::rules_hyp() } else { xf::front::rules() };
    let mut s = Session::with_rules(h, g, root, rules);
    let trace = str_arg(trace, trace_len);
    if !trace.is_empty() {
        match xf::replay::parse_trace(trace) {
            Ok(t) => {
                s.r.uneval = xf::world::run::Runner::uneval_from_trace(&t, h);
                log("system-value conditions from /trace.jsonl", 0);
            }
            Err(e) => log(&format!("trace: {e}"), 1),
        }
    }
    s.game_cursor = true; // H307
    // H955 / H956: the subtitle pack (xf-prep --en-subs / --ru-subs) when xf-serve's cache has one; F3 shows it. The page
    // picks the language (`xf_subs`, default auto = ru, else en) right after init.
    set_pack(&mut s, "auto", false);
    // H550: live play draws RandGen values (the phone's ring, H202); seed = the game clock (ms, varies with the player)
    s.r.w.live_rand = Some(0);
    s.boot(match start {
        1 => xf::front::intro::Start::Menu,
        2 => xf::front::intro::Start::NewGame,
        _ => xf::front::intro::Start::Intro,
    });
    // H402: the save slots — xf-serve's /saves/ (or the browser's IndexedDB) through WebFs
    s.set_saves(xf::save::Slots::open(Path::new(SAVES)));
    // H2305: the packs on offer (none over English data: the movies' own captions); the game language = the data's
    let packs = if s.data_lang == xf::media::lang::GameLang::Ru {
        let cache = Path::new("cache");
        xf::media::subs::LANGS.into_iter().filter(|l| xf::io::fs().size(&xf::media::subs::pack_path(cache, l)).is_ok()).collect()
    } else {
        Vec::new()
    };
    let mut set = xf::front::settings::Settings::default();
    let _ = set.set("game_lang", s.game_lang.code());
    let st = State {
        s,
        mix: Mix { rate: 48000, volume: 0.01, ..Default::default() },
        pump: xf::front::avsync::Pump::new(48000, 4800),
        rgba: vec![255; SCREEN_W * SCREEN_H * 4],
        pcm16: Vec::new(),
        pcm: Vec::new(),
        logged: 0,
        shown_lines: Vec::new(),
        gam,
        quit: false,
        set,
        packs,
        settings_req: false,
        prefs_saved: false,
        menu_hover: false,
        text: String::new(),
    };
    STATE.with(|x| *x.borrow_mut() = Some(st));
    1
}

/// One iteration of the window loop: advance the clock `dt` ms (the page loop clamps it to 250 like the window), sounds follow the cues, compose the frame.
/// Returns a pointer to the frame, 640×480 RGBA, or null when the frame equals the previous one (nothing to draw: a
/// still scene costs no canvas upload; [`xf_frame`] gives the frame anyway).
#[no_mangle]
pub extern "C" fn xf_tick(dt: u32) -> *const u8 {
    with(|st| {
        advance(st, dt);
        let mut fb = st.s.render();
        // H1751: the "Port settings" button on the game's menu screen (the page's settings dialog)
        if st.s.r.menu_open {
            xf::front::settings::draw_menu_button(&mut fb, st.menu_hover);
        }
        let mut changed = false;
        for (d, s) in st.rgba.chunks_exact_mut(4).zip(fb.chunks_exact(3)) {
            if d[..3] != *s {
                d[..3].copy_from_slice(s);
                changed = true;
            }
        }
        if changed { st.rgba.as_ptr() } else { std::ptr::null() }
    })
    .unwrap_or(std::ptr::null())
}

/// H1731: [`xf_tick`] without composing a frame — the loop of a hidden tab whose game keeps running ("pause when
/// hidden" off): the clock, the host requests, the sounds and the log, nothing drawn.
#[no_mangle]
pub extern "C" fn xf_step(dt: u32) {
    with(|st| advance(st, dt));
}

/// The clock `dt` ms on, then what the window loop does after `tick` (host requests, log lines). The voices follow the
/// cues in [`xf_pump`] (H1795: placed where they are heard in step with the picture).
fn advance(st: &mut State, dt: u32) {
    st.s.tick(dt as u64);
    host_request(st);
    let lines = st.s.r.w.dbox_line_texts();
    if lines != st.shown_lines {
        for (i, (c, r, t)) in lines.iter().enumerate() {
            log(&format!("dialogue line {}: {:#x} click at ({}, {}): {t}", i + 1, c, (r[0] + r[2]) / 2, (r[1] + r[3]) / 2), 0);
        }
        st.shown_lines = lines;
    }
    let all: Vec<&String> = st.s.r.log.iter().chain(&st.s.log).collect();
    for l in all.iter().skip(st.logged) {
        log(l, 0);
    }
    // robustness: printed lines are dropped and the core's histories bounded (a long session grew without end)
    st.s.r.log.clear();
    st.s.log.clear();
    st.logged = 0;
    st.s.r.trim_history(100_000);
}

/// H1734: the save slots read again (`slots.txt` through WebFs) — after the page imported slot files into the store
/// the engine reads (the game keeps the slot names from its start, `Session::set_saves`).
#[no_mangle]
pub extern "C" fn xf_saves_reload() {
    with(|st| st.s.set_saves(xf::save::Slots::open(Path::new(SAVES))));
}

/// The last composed frame (640×480 RGBA), changed or not.
#[no_mangle]
pub extern "C" fn xf_frame() -> *const u8 {
    with(|st| st.rgba.as_ptr()).unwrap_or(std::ptr::null())
}

fn at(x: i32, y: i32, inside: u32) -> Option<(i32, i32)> {
    (inside != 0 && (0..SCREEN_W as i32).contains(&x) && (0..SCREEN_H as i32).contains(&y)).then_some((x, y))
}

/// Pointer moved to picture point (x, y) (`inside` = 0: off the picture). Returns 0 = hide the page's pointer (the
/// game's cursor is in the frame), 1 = hand, 2 = arrow.
#[no_mangle]
pub extern "C" fn xf_motion(x: i32, y: i32, inside: u32) -> u32 {
    with(|st| {
        let p = at(x, y, inside);
        st.menu_hover = st.s.r.menu_open && p.is_some_and(|(x, y)| xf::front::settings::menu_button_at(x, y));
        if st.menu_hover {
            return 1;
        }
        match host::motion(&mut st.s, p) {
        Pointer::Hidden => 0,
        Pointer::Hand => 1,
        Pointer::Arrow => 2,
        }
    })
    .unwrap_or(2)
}

/// The mouse wheel at (x, y): `lines` notches, < 0 = up (H1815, a port convenience: the dialogue box / the computer's
/// scrolled texts).
#[no_mangle]
pub extern "C" fn xf_wheel(lines: i32, x: i32, y: i32, inside: u32) {
    with(|st| {
        if let Some(l) = host::wheel(&mut st.s, at(x, y, inside), lines) {
            log(&l, 0);
        }
    });
}

/// Mouse button `b` (0 left, 2 right — DOM `MouseEvent.button`) pressed at (x, y).
#[no_mangle]
pub extern "C" fn xf_button(b: u32, x: i32, y: i32, inside: u32) {
    let b = match b {
        0 => Button::Left,
        2 => Button::Right,
        _ => return,
    };
    with(|st| {
        let p = at(x, y, inside);
        if b == Button::Left && st.s.r.menu_open && p.is_some_and(|(x, y)| xf::front::settings::menu_button_at(x, y)) {
            st.settings_req = true;
            return;
        }
        if let Some(l) = host::button(&mut st.s, b, p) {
            log(&l, 0);
        }
        host_request(st);
    });
}

/// Load the subtitle pack `mode` (en / ru / auto / off, `xf::media::subs::Pack::pick`) from xf-serve's cache into the
/// session; `on` also switches the captions on (the page's choice of a language). Returns 1 when a pack is in use.
fn set_pack(s: &mut Session, mode: &str, on: bool) -> u32 {
    match xf::media::subs::Pack::pick_for(Path::new("cache"), mode, s.data_lang) {
        Ok(Some(p)) => {
            log(&format!("{} subtitles for {} movies (F3: captions on / off)", p.lang, p.movies.len()), 0);
            s.sub_pack = Some(std::sync::Arc::new(p));
            if on {
                s.r.set_closed_captioning(true);
            }
            1
        }
        Ok(None) => {
            s.sub_pack = None;
            0
        }
        Err(e) => {
            if mode != "auto" {
                log(&format!("subtitles {mode}: {e}"), 1);
            }
            s.sub_pack = None;
            0
        }
    }
}

/// The subtitle language by name: `en`, `ru`, `auto` (ru, else en) or `off`; the page's select calls it after init
/// (`on` = 0) and on change (`on` = 1: picking a language also switches the captions on). Returns 1 when a pack is in use.
#[no_mangle]
pub unsafe extern "C" fn xf_subs(p: *const u8, n: usize, on: u32) -> u32 {
    let mut mode = str_arg(p, n).to_string();
    with(|st| {
        // H2305: a choice the data does not offer (e.g. a remembered Russian pack over English data) = auto
        if !st.s.sub_langs(&st.packs).contains(&mode.as_str()) {
            mode = "auto".into();
        }
        // H1750: the settings follow (xf_settings reports it)
        let _ = st.set.set("subs", &mode);
        set_pack(&mut st.s, &mode, on != 0 && xf::media::subs::is_lang(&mode))
    })
    .unwrap_or(0)
}

/// Mouse button `b` (0 left, 2 right) released at (x, y): a dragged IdeaIcon is dropped there (C107).
#[no_mangle]
pub extern "C" fn xf_release(b: u32, x: i32, y: i32, inside: u32) {
    let b = match b {
        0 => Button::Left,
        2 => Button::Right,
        _ => return,
    };
    with(|st| {
        if let Some(l) = host::release(&mut st.s, b, at(x, y, inside)) {
            log(&l, 0);
        }
        host_request(st);
    });
}

/// A key by DOM `KeyboardEvent.key`: Space / Enter end the playing media (as in the window), F1 toggles the overlay,
/// F3 the closed captions, other keys go to the game as virtual-key codes (front::host::vk).
#[no_mangle]
pub unsafe extern "C" fn xf_key(p: *const u8, n: usize) {
    let k = str_arg(p, n).to_string();
    with(|st| {
        if (k == " " || k == "Enter") && host::end_playing(&mut st.s) {
            return;
        }
        // F4 = the next game language (H1836, as the window: RU / EN with the English data set, xf_en_data; nothing with
        // one language, H2305) — the settings follow
        if k == "F4" {
            if let Some(next) = st.s.next_game_lang() {
                let _ = st.set.set("game_lang", next);
                if let Some(l) = st.set.apply_key("game_lang", &mut st.s) {
                    log(&l, 0);
                }
            }
            return;
        }
        // F1 = hotspots, F3 = closed captions on / off (H343, as the window) — the settings follow (H1750)
        if k == "F1" || k == "F3" {
            let key = if k == "F1" { "overlay" } else { "captions" };
            let on = if k == "F1" { !st.s.overlay } else { !st.s.r.w.closed_captioning() };
            let _ = st.set.set(key, if on { "on" } else { "off" });
            if let Some(l) = st.set.apply_key(key, &mut st.s) {
                log(&l, 0);
            }
            return;
        }
        if let Some(code) = host::vk(&k) {
            log(&host::key(&mut st.s, code), 0);
            host_request(st);
        }
    });
}

/// H2060 (H1868): an Alt hotkey of the original — `vk` its virtual-key code ('0'–'9', 'A'–'Z'), `shift` ≠ 0 = Shift
/// held too ([`host::alt`]). Returns what the page does: 0 nothing, 1 done in the engine, 2 Alt+X (exit — ignored in
/// the browser), 3 the captions / hotspots changed (the page asks for the settings again), 0x100 + percent = the
/// volume (H2061: the engine's mixer has it already; the page keeps it and moves its slider).
#[no_mangle]
pub extern "C" fn xf_alt(vk: u32, shift: u32) -> u32 {
    with(|st| {
        let r = host::alt(&mut st.s, vk, shift != 0);
        let d = r.describe();
        if !d.is_empty() {
            log(&d, 0);
        }
        use xf::front::altkeys::AltOut;
        let onoff = |b: bool| if b { "on" } else { "off" };
        let code = match r {
            AltOut::None => 0,
            AltOut::Exit => 2,
            AltOut::Volume(v) => {
                let _ = st.set.set("volume", &v.to_string());
                st.mix.volume = v as f32 / 100.0;
                0x100 + v
            }
            AltOut::Captions(on) => {
                let _ = st.set.set("captions", onoff(on));
                3
            }
            AltOut::Hotspots(on) => {
                let _ = st.set.set("overlay", onoff(on));
                3
            }
            _ => 1,
        };
        host_request(st);
        code
    })
    .unwrap_or(0)
}

/// H1837: the English data set — `index` = xf-serve /en-index (its files are fetched from /en/…). Returns 1 when the
/// session has it (the game language can then be switched, [`xf_game_lang`]), 0 if not (no index, not English data).
#[no_mangle]
pub unsafe extern "C" fn xf_en_data(index: *const u8, index_len: usize) -> u32 {
    let index = str_arg(index, index_len);
    let Some(fs) = WEB_FS.get() else { return 0 };
    if index.is_empty() {
        return 0;
    }
    let root = Path::new("en");
    fs.add(root, index);
    match xf::media::lang::EnData::open(root) {
        Ok(en) => {
            log(&format!("English data set: {} files, {} strings (F4: game language)", en.index.len(), en.strings.len()), 0);
            with(|st| st.s.set_en_data(Some(std::sync::Arc::new(en))));
            1
        }
        Err(e) => {
            log(&format!("English data set: {e}"), 1);
            0
        }
    }
}

/// H1836: the game language, 0 = RU, 1 = EN (the settings' "game language"). Returns 1 when set, 0 if not (EN without
/// the English data set).
#[no_mangle]
pub extern "C" fn xf_game_lang(lang: u32) -> u32 {
    use xf::media::lang::GameLang;
    let l = if lang == 1 { GameLang::En } else { GameLang::Ru };
    let mut ok = 0;
    with(|st| match st.s.set_lang(l) {
        Ok(()) => ok = 1,
        Err(e) => log(&format!("game language: {e}"), 1),
    });
    ok
}

/// Output sample rate and volume (0..=1).
#[no_mangle]
pub extern "C" fn xf_audio(rate: u32, volume: f32) {
    with(|st| {
        st.mix.rate = rate.max(1);
        st.mix.volume = volume.clamp(0.0, 1.0);
        st.pump.rate = rate.max(1);
    });
}

/// H1791: the queue depth the pump keeps in the sink, frames (the page picks it for its sink: AudioWorklet or the
/// ScriptProcessor fallback).
#[no_mangle]
pub extern "C" fn xf_pump_target(frames: u32) {
    with(|st| st.pump.target = frames.max(1));
}

/// H1791–H1795: one audio tick after [`xf_tick`] from the sink's last report ([`xf::front::avsync::Report`]:
/// `consumed` frames taken from its queue, the next frame at the graph's output `lead_s` after the report, `block_s`
/// taken at once, the context's own latency `out_s`, the report `age_s` old): the voices follow the session's cues and
/// the frames to send are mixed ([`xf::front::avsync::web_audio`]). Returns their number (0: nothing to send); the
/// samples: [`xf_pcm`]. The latency estimate becomes the picture delay (`Session::av_delay`).
#[no_mangle]
pub extern "C" fn xf_pump(consumed: f64, lead_s: f64, block_s: f64, out_s: f64, age_s: f64) -> u32 {
    with(|st| {
        let r = xf::front::avsync::Report { consumed: consumed.max(0.0) as u64, lead_s, block_s, out_s, age_s };
        xf::front::avsync::web_audio(&mut st.s, &mut st.mix, &mut st.pump, r, &mut st.pcm16);
        st.pcm.clear();
        st.pcm.extend(st.pcm16.iter().map(|&x| x as f32 / 32768.0));
        (st.pcm16.len() / 2) as u32
    })
    .unwrap_or(0)
}

/// The samples of the last [`xf_pump`]: interleaved stereo f32.
#[no_mangle]
pub extern "C" fn xf_pcm() -> *const f32 {
    with(|st| st.pcm.as_ptr()).unwrap_or(std::ptr::null())
}

/// The pump's latency estimate (mix → heard), ms; the picture delay is this less one display frame.
#[no_mangle]
pub extern "C" fn xf_audio_latency() -> f64 {
    with(|st| st.pump.latency_s.unwrap_or(0.0) * 1000.0).unwrap_or(0.0)
}

/// The player option "long timers" (C191 / H1203; `xf --long-timers`): 0 = wine (default — a timer above 65535 ms, the
/// Gordon's Hauling bomb, never fires), 1 = nt (it runs: the bomb ends the game 120 s after the manifest). Takes effect
/// for timers armed (or re-armed by a resume) from now on; New / Load keep it.
#[no_mangle]
pub extern "C" fn xf_long_timers(nt: u32) {
    with(|st| {
        let v = if nt != 0 { xf::world::LongTimers::Nt } else { xf::world::LongTimers::Wine };
        st.s.r.w.rules.set_long_timers(v);
    });
}

/// A setting by key and value (H1750, `xf::front::settings`): checked, kept, and applied — the game-side ones to the
/// session, `volume` (percent) to the mixer; `scale`, `fullscreen` and `pause_hidden` are the page's (kept only).
/// Returns 1 when taken, 0 for an unknown key / a bad value (logged).
#[no_mangle]
pub unsafe extern "C" fn xf_set(kp: *const u8, kn: usize, vp: *const u8, vn: usize) -> u32 {
    let (k, v) = (str_arg(kp, kn).to_string(), str_arg(vp, vn).to_string());
    with(|st| {
        if let Err(e) = st.set.set(&k, &v) {
            log(&format!("setting: {e}"), 0);
            return 0;
        }
        // H2305: a game language the data has not (a remembered one) becomes the session's
        if k == "game_lang" && !st.s.game_langs().contains(&v.as_str()) {
            let _ = st.set.set("game_lang", st.s.game_lang.code());
            return 1;
        }
        if k == "volume" {
            st.mix.volume = st.set.volume() as f32 / 100.0;
        } else if let Some(l) = st.set.apply_key(&k, &mut st.s) {
            log(&l, 0);
        }
        1
    })
    .unwrap_or(0)
}

/// The settings now as `key = value` lines (captions / hotspots as the session has them — F1 / F3 / the game's own
/// preference), then `choices.<key> = a,b,…` for the options whose values depend on the game data (H2305: the page
/// lists only these, and hides an option with one value). Pointer to UTF-8 text,
/// its length from [`xf_settings_len`].
#[no_mangle]
pub extern "C" fn xf_settings() -> *const u8 {
    with(|st| {
        let mut cur = st.set.clone();
        // the captions show the session's state unless the player forces them
        if cur.captions().is_none() {
            let on = st.s.r.w.closed_captioning();
            st.text = format!("captions_now = {}\n", if on { "on" } else { "off" });
        } else {
            st.text.clear();
        }
        let _ = cur.set("overlay", if st.s.overlay { "on" } else { "off" });
        st.text = cur.to_text() + &st.text;
        for (k, v) in xf::front::settings::choices(&st.s, &st.packs) {
            st.text += &format!("choices.{k} = {}\n", v.join(","));
        }
        st.text.as_ptr()
    })
    .unwrap_or(std::ptr::null())
}

#[no_mangle]
pub extern "C" fn xf_settings_len() -> usize {
    with(|st| st.text.len()).unwrap_or(0)
}

/// H2100: 1 once after a Save wrote the host's Preferences (the settings' `host_prefs`): the page asks for the
/// settings ([`xf_settings`]) and keeps them, as the window keeps them in its settings file.
#[no_mangle]
pub extern "C" fn xf_prefs_saved() -> u32 {
    with(|st| std::mem::take(&mut st.prefs_saved) as u32).unwrap_or(0)
}

/// 1 once after the menu screen's "Port settings" button was clicked (the page opens its settings dialog).
#[no_mangle]
pub extern "C" fn xf_settings_req() -> u32 {
    with(|st| std::mem::take(&mut st.settings_req) as u32).unwrap_or(0)
}

/// The pointer's last picture point (x << 16 | y; 0xffffffff: none) — the page's scale preview crops around it.
#[no_mangle]
pub extern "C" fn xf_mouse() -> u32 {
    with(|st| st.s.mouse.map_or(u32::MAX, |(x, y)| (x as u32) << 16 | y as u32)).unwrap_or(u32::MAX)
}

/// 1 once the menu's Quit was chosen (the page then goes back to its start screen).
#[no_mangle]
pub extern "C" fn xf_quit() -> u32 {
    with(|st| st.quit as u32).unwrap_or(0)
}

/// The View shown (0xffffffff: none).
#[no_mangle]
pub extern "C" fn xf_view() -> u32 {
    with(|st| st.s.r.w.view.unwrap_or(u32::MAX)).unwrap_or(u32::MAX)
}

/// Game time, ms.
#[no_mangle]
pub extern "C" fn xf_now() -> f64 {
    with(|st| st.s.now as f64).unwrap_or(0.0)
}
