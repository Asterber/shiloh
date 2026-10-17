//! xf-serve — local HTTP server for the web front (`port/web/`): the page + wasm bundle, and the user's own copy of
//! the game files with HTTP Range support. The browser runs the engine; this only serves files.
//!
//!   xf-serve [--data DIR|IMAGE ...] [--cache DIR] [--saves DIR] [--saves-in server|browser] [--web DIR] [--listen ADDR:PORT]
//!            [--trace T.jsonl] [--fonts DIR] [--en-data DIR|IMAGE ...] [--licenses]
//!
//! `--data` / `--en-data` may be given more than once and may name disc images (`.iso`, `.mdf`, `.bin`, `.img`) or a
//! folder of them (port feature, `xf::vfs`): the sources are merged into one virtual game folder, served from the
//! images by ranges (nothing is extracted). `XF_DATA` / `XF_EN_DATA` may hold a path list (`:`-separated).
//!
//! Binds to 127.0.0.1:8640 by default. `--listen 0.0.0.0:PORT` makes it reachable on the LAN — the game files must
//! never be exposed to the internet (they are the user's licensed copy; this project never distributes them).
//!
//! Routes (GET / HEAD):
//!   /                 index.html of the web dir; other paths = files of the web dir
//!   /data/<path>      a file of the data dir (the installed game), Range: bytes=… supported
//!   /cache/<path>     a file of the PCM cache dir (QDMC music made by xf-prep; `--cache`, default XF_CACHE, else
//!                     the user cache dir — `xf::media::pcmcache::dir`), Range supported
//!   /data-index       the data dir's files, one level of sub-directories deep (what `MediaLib` indexes):
//!                     lines `<size>\t<relative path>`
//!   /trace.jsonl      the trace the window takes system-value conditions from (`--trace`; default traces/fo-009.jsonl
//!                     when that file exists, else none: an empty body)
//!   /en-index         H1837: the English data set's files (`--en-data DIR`, env XF_EN_DATA: the EN DVD's disc root /
//!                     `English/`, or an installed English game; the game-language switch, H1836), up to three
//!                     sub-directory levels deep (`English/MININST/XV/…`): lines `<size>\t<relative path>`; 404 without one
//!   /en/<path>        a file of the English data set, Range supported
//!   /fonts/<name>     a .ttf / .otf font from the system font dirs (Courier New, Arial for the UI text)
//!   /LICENSES.txt     the license notices (`xf::notices::LICENSES`)
//! Paths are confined to their root (no `..`, no absolute paths; symlinks resolved and checked).
//!
//! The save slots (H402), only when the saves dir (`--saves`, default `xf::save::default_dir()` = `XF_SAVES` / the
//! window's) is writable — the same files as the window's saves dir (`slots.txt` + `<NAME>.x`), so a save moves
//! between window and web by copying it:
//!   GET  /saves/          the slot files present: lines `<size>\t<file>` (404: no writable saves dir — the page then
//!                         keeps its saves in the browser, IndexedDB)
//!   GET / HEAD /saves/<file>, PUT /saves/<file> (body = the file, replaced atomically: temp file + rename),
//!   DELETE /saves/<file>
//!   GET  /saves-mode      `server` or `browser`: where the page keeps the slots (H1733)
//! `--saves-in` (env `XF_SAVES_IN`, H1733): `server` = the saves dir must be writable (else xf-serve exits — a
//! private single-owner setup); `browser` = no saves dir is opened, /saves/ is 404 and the page keeps the slots in
//! its IndexedDB only (the public setup, `docs/web-hosting.md`); not given = `server` when the saves dir is writable,
//! else `browser` (the behaviour before the switch).
//! `<file>` is `slots.txt` or `<NAME>.x` with NAME as the game lets one type it (`xf::save::is_slot_file`: A–Z 0–9
//! space -(), at most 21) — nothing else is read or written. A `.x` must carry the HDB header (5 / 0x501) and be at
//! most 8 MiB, `slots.txt` at most 3 valid names; at most 64 `.x` files. Writes from another origin are refused.
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

struct Conf {
    data: PathBuf,
    /// PCM cache dir (None: not served)
    cache: Option<PathBuf>,
    /// the save slots' dir (None: no writable saves dir, /saves/ is 404)
    saves: Option<PathBuf>,
    web: PathBuf,
    /// None: no trace (/trace.jsonl answers an empty body)
    trace: Option<PathBuf>,
    fonts: Vec<PathBuf>,
    /// H1837: the English data set's root (None: not served)
    en: Option<PathBuf>,
}

/// `--saves-in` (H1733): where the save slots live.
#[derive(Clone, Copy)]
enum SavesIn {
    /// not given: the server when its saves dir is writable, else the browser
    Auto,
    Server,
    Browser,
}

fn die(m: &str) -> ! {
    eprintln!("xf-serve: {m}");
    std::process::exit(2)
}

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // the file backend that also reads disc images (xf::vfs); other paths go to std::fs
    xf::vfs::install();
    let mut data: Vec<PathBuf> = Vec::new();
    let mut web = default_web(std::env::current_exe().ok().as_deref().and_then(Path::parent), &manifest);
    let mut cache = xf::media::pcmcache::dir();
    let mut saves = xf::save::default_dir();
    // the research trace is optional (not part of every checkout); without it the page runs with no trace
    let mut trace = Some(manifest.join("../traces/fo-009.jsonl")).filter(|t| t.is_file());
    let mut listen: SocketAddr = "127.0.0.1:8640".parse().unwrap();
    let mut saves_in = std::env::var("XF_SAVES_IN").ok().filter(|v| !v.is_empty());
    let mut fonts: Vec<PathBuf> = std::env::var_os("XF_FONT_DIR").map(PathBuf::from).into_iter().collect();
    let mut en: Vec<PathBuf> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| die(&format!("{x}: value missing")));
        match x.as_str() {
            "--data" => data.push(val().into()),
            "--cache" => cache = Some(val().into()),
            "--saves" => saves = val().into(),
            "--saves-in" => saves_in = Some(val()),
            "--web" => web = val().into(),
            "--trace" => trace = Some(val().into()),
            "--fonts" => fonts.push(val().into()),
            "--en-data" => en.push(val().into()),
            "--listen" => listen = val().parse().unwrap_or_else(|_| die("--listen ADDR:PORT, e.g. 127.0.0.1:8640")),
            "--licenses" => {
                print!("{}", xf::notices::LICENSES);
                return;
            }
            "-h" | "--help" => {
                eprintln!("usage: xf-serve [--data DIR|IMAGE ...] [--cache DIR] [--saves DIR] [--saves-in server|browser] [--web DIR] [--listen ADDR:PORT] [--trace T.jsonl] [--fonts DIR] [--en-data DIR|IMAGE ...] [--licenses]");
                return;
            }
            _ => die(&format!("unknown argument {x}")),
        }
    }
    fonts.extend(
        ["/System/Library/Fonts/Supplemental", "/Library/Fonts", "/usr/share/fonts/truetype/msttcorefonts", "C:\\Windows\\Fonts"]
            .iter()
            .map(PathBuf::from),
    );
    if data.is_empty() {
        data = xf::vfs::env_sources("XF_DATA");
    }
    if data.is_empty() {
        data.push(manifest.join("../data/InstalledGame"));
    }
    let data = game_root("--data", &data);
    if !xf::io::is_file(&data.join("XFiles.hdb")) {
        eprintln!("xf-serve: warning: no XFiles.hdb in {}", data.display());
    }
    let web = web.canonicalize().unwrap_or_else(|e| die(&format!("--web {}: {e}", web.display())));
    if !web.join("pkg/xf_web.wasm").is_file() {
        eprintln!("xf-serve: warning: {} missing — build it with port/web/build.sh", web.join("pkg/xf_web.wasm").display());
    }
    if !listen.ip().is_loopback() {
        eprintln!("xf-serve: WARNING: listening on {listen} — reachable from other machines. The game files are your own");
        eprintln!("          licensed copy: use this on a trusted LAN only and NEVER expose this port to the internet.");
    }
    let cache = cache.and_then(|c| c.canonicalize().ok());
    match &cache {
        Some(c) if c.join(xf::media::pcmcache::SUBDIR).is_dir() => eprintln!("xf-serve: PCM cache {}", c.display()),
        _ => eprintln!("xf-serve: no PCM cache — QDMC music stays silent (run xf-prep once, see port/README.md)"),
    }
    // the system fonts the core draws its own texts with (front::text / doc / computer: UI, phone, Bible, computer)
    for f in ["Courier New.ttf", "Courier New Bold.ttf", "Arial.ttf", "Arial Bold.ttf"] {
        match fonts.iter().find_map(|d| confined(d, f)) {
            Some(p) => eprintln!("xf-serve: font {}", p.display()),
            None => eprintln!("xf-serve: warning: font {f} not found (--fonts DIR) — those texts fall back or stay empty"),
        }
    }
    let saves_in = match saves_in.as_deref() {
        None => SavesIn::Auto,
        Some("server") => SavesIn::Server,
        Some("browser") => SavesIn::Browser,
        Some(v) => die(&format!("--saves-in {v}: server or browser")),
    };
    let saves = match (saves_in, saves_dir(&saves, &data)) {
        (SavesIn::Browser, _) => {
            eprintln!("xf-serve: saves in the browser (--saves-in browser): each browser keeps its own slots (IndexedDB)");
            None
        }
        (_, Ok(d)) => {
            eprintln!("xf-serve: saves on the server: {}", d.display());
            Some(d)
        }
        (SavesIn::Server, Err(e)) => die(&format!("--saves-in server: no writable saves dir ({}: {e})", saves.display())),
        (SavesIn::Auto, Err(e)) => {
            eprintln!("xf-serve: no writable saves dir ({}: {e}) — the page keeps its saves in the browser", saves.display());
            None
        }
    };
    if en.is_empty() {
        en = xf::vfs::env_sources("XF_EN_DATA");
    }
    let en = (!en.is_empty()).then(|| game_root("--en-data", &en));
    if let Some(d) = &en {
        eprintln!("xf-serve: English data set {} ({} files)", d.display(), deep_index(d, 3).lines().count());
    }
    let conf: &'static Conf = Box::leak(Box::new(Conf { data, cache, saves, web, trace, fonts, en }));
    let l = TcpListener::bind(listen).unwrap_or_else(|e| die(&format!("{listen}: {e}")));
    eprintln!("xf-serve: data {}  web {}", conf.data.display(), conf.web.display());
    eprintln!("xf-serve: open http://{listen}/");
    for s in l.incoming().flatten() {
        std::thread::spawn(move || {
            let _ = serve(s, conf);
        });
    }
}

/// The game folder of `--data` / `--en-data`: a single folder canonical (as before), else the merged sources' virtual
/// root (`xf::vfs`, disc images).
/// `--web` not given: a `web/` dir next to the executable (the release archives' layout) when it holds the page,
/// else the source tree's `port/web/www`.
fn default_web(exe_dir: Option<&Path>, manifest: &Path) -> PathBuf {
    exe_dir.map(|d| d.join("web")).filter(|w| w.join("index.html").is_file()).unwrap_or_else(|| manifest.join("web/www"))
}

fn game_root(flag: &str, src: &[PathBuf]) -> PathBuf {
    if !xf::vfs::needs_vfs(src) {
        let d = &src[0];
        return d.canonicalize().unwrap_or_else(|e| die(&format!("{flag} {}: {e}", d.display())));
    }
    xf::vfs::data_root(src, &mut |l| eprintln!("xf-serve: {flag}: {l}")).unwrap_or_else(|e| die(&format!("{flag}: {e}")))
}

/// One connection: HTTP/1.1 requests until the client closes (keep-alive: the worker's many sync range reads).
fn serve(s: TcpStream, conf: &Conf) -> std::io::Result<()> {
    let _ = s.set_nodelay(true);
    let mut w = s.try_clone()?;
    let mut r = BufReader::new(s);
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
        let mut range = None;
        let mut close = parts.next() == Some("HTTP/1.0");
        let (mut len, mut chunked, mut origin, mut host) = (None::<u64>, false, None::<String>, None::<String>);
        loop {
            let mut h = String::new();
            if r.read_line(&mut h)? == 0 {
                return Ok(());
            }
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                let v = v.trim();
                match k.trim().to_ascii_lowercase().as_str() {
                    "range" => range = Some(v.to_string()),
                    "connection" if v.eq_ignore_ascii_case("close") => close = true,
                    "content-length" => len = Some(v.parse().unwrap_or(u64::MAX)),
                    "transfer-encoding" => chunked = true,
                    "origin" => origin = Some(v.to_string()),
                    "host" => host = Some(v.to_string()),
                    _ => {}
                }
            }
        }
        let head = method == "HEAD";
        let saves_target = target.starts_with("/saves/");
        if chunked {
            // bodies only with a Content-Length
            respond(&mut w, "411 Length Required", "text/plain", b"Content-Length required\n", false, &[])?;
            return Ok(());
        }
        if saves_target && (method == "PUT" || method == "DELETE") {
            let body = match len {
                Some(n) if method == "PUT" && n <= MAX_SAVE => {
                    let mut b = vec![0; n as usize];
                    r.read_exact(&mut b)?;
                    b
                }
                Some(n) if method == "PUT" || n > 0 => {
                    respond(&mut w, "413 Content Too Large", "text/plain", b"too large\n", false, &[])?;
                    return Ok(());
                }
                None if method == "PUT" => {
                    respond(&mut w, "411 Length Required", "text/plain", b"Content-Length required\n", false, &[])?;
                    return Ok(());
                }
                _ => Vec::new(),
            };
            let (status, msg) = if !same_origin(origin.as_deref(), host.as_deref()) {
                ("403 Forbidden", "another origin\n".to_string())
            } else {
                saves_write(conf, &target, &method, &body)
            };
            respond(&mut w, status, "text/plain", msg.as_bytes(), false, &[("Cache-Control", "no-store")])?;
        } else if len.unwrap_or(0) > 0 {
            // GET / HEAD carry no body; anything else is refused (and the connection closed: the body is not read)
            respond(&mut w, "405 Method Not Allowed", "text/plain", b"no request bodies here\n", false, &[])?;
            return Ok(());
        } else if method != "GET" && !head {
            let allow = if saves_target { "GET, HEAD, PUT, DELETE" } else { "GET, HEAD" };
            respond(&mut w, "405 Method Not Allowed", "text/plain", b"method not allowed\n", head, &[("Allow", allow)])?;
        } else {
            route(&mut w, conf, &target, range.as_deref(), head)?;
        }
        w.flush()?;
        if close {
            return Ok(());
        }
    }
}

fn route(w: &mut TcpStream, conf: &Conf, target: &str, range: Option<&str>, head: bool) -> std::io::Result<()> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let Some(path) = percent_decode(path) else { return not_found(w, head) };
    let nocache = &[("Cache-Control", "no-cache")];
    if path == "/data-index" {
        return respond(w, "200 OK", "text/plain; charset=utf-8", data_index(&conf.data).as_bytes(), head, nocache);
    }
    if path == "/en-index" {
        return match &conf.en {
            Some(d) => respond(w, "200 OK", "text/plain; charset=utf-8", deep_index(d, 3).as_bytes(), head, nocache),
            None => not_found(w, head),
        };
    }
    if let Some(rel) = path.strip_prefix("/en/") {
        return match conf.en.as_ref().and_then(|d| confined_game(d, rel)) {
            Some(p) => file(w, &p, range, head, &[("Cache-Control", "private, max-age=3600")]),
            None => not_found(w, head),
        };
    }
    if path == "/trace.jsonl" {
        return match conf.trace.as_ref().map(std::fs::read) {
            Some(Ok(b)) => respond(w, "200 OK", "application/x-ndjson", &b, head, nocache),
            Some(Err(_)) => not_found(w, head),
            None => respond(w, "200 OK", "application/x-ndjson", b"", head, nocache),
        };
    }
    if path == "/LICENSES.txt" {
        return respond(w, "200 OK", "text/plain; charset=utf-8", xf::notices::LICENSES.as_bytes(), head, nocache);
    }
    if path == "/saves-mode" {
        let m: &[u8] = if conf.saves.is_some() { b"server\n" } else { b"browser\n" };
        return respond(w, "200 OK", "text/plain; charset=utf-8", m, head, &[("Cache-Control", "no-store")]);
    }
    if let Some(name) = path.strip_prefix("/saves/") {
        let Some(dir) = &conf.saves else { return not_found(w, head) };
        let ns = &[("Cache-Control", "no-store")];
        if name.is_empty() {
            return respond(w, "200 OK", "text/plain; charset=utf-8", saves_index(dir).as_bytes(), head, ns);
        }
        if !xf::save::is_slot_file(name) {
            return not_found(w, head);
        }
        return match confined(dir, name) {
            Some(p) => file(w, &p, range, head, ns),
            None => not_found(w, head),
        };
    }
    if let Some(rel) = path.strip_prefix("/data/") {
        return match confined_game(&conf.data, rel) {
            Some(p) => file(w, &p, range, head, &[("Cache-Control", "private, max-age=3600")]),
            None => not_found(w, head),
        };
    }
    if let Some(rel) = path.strip_prefix("/cache/") {
        return match conf.cache.as_ref().and_then(|c| confined(c, rel)) {
            Some(p) => file(w, &p, range, head, nocache),
            None => not_found(w, head),
        };
    }
    if let Some(name) = path.strip_prefix("/fonts/") {
        let ext = name.rsplit_once('.').map(|x| x.1.to_ascii_lowercase()).unwrap_or_default();
        if name.contains(['/', '\\']) || !(ext == "ttf" || ext == "otf") {
            return not_found(w, head);
        }
        return match conf.fonts.iter().find_map(|d| confined(d, name)) {
            Some(p) => file(w, &p, range, head, &[("Cache-Control", "private, max-age=3600")]),
            None => not_found(w, head),
        };
    }
    let rel = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
    match confined(&conf.web, rel) {
        Some(p) => file(w, &p, range, head, nocache),
        None => not_found(w, head),
    }
}

/// The largest save file accepted (the original's saves are ~160 KB).
const MAX_SAVE: u64 = 8 << 20;
/// At most this many `.x` files in the saves dir (a new name beyond that is refused).
const MAX_SAVE_FILES: usize = 64;

/// The saves dir, created if missing, canonical, not inside the game data, and writable (a probe file is written with
/// the same atomic write the saves use, then removed).
fn saves_dir(dir: &Path, data: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let d = dir.canonicalize().map_err(|e| e.to_string())?;
    if d.starts_with(data) {
        return Err("inside the game data".into());
    }
    let probe = d.join(".xf-serve-probe");
    xf::io::write_atomic(&probe, b"").map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&probe);
    Ok(d)
}

/// Lines `<size>\t<file>`: the slot files of the saves dir.
fn saves_index(dir: &Path) -> String {
    let mut v: Vec<(String, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let m = e.metadata().ok()?;
            (xf::save::is_slot_file(&name) && m.is_file()).then_some((name, m.len()))
        })
        .collect();
    v.sort();
    v.iter().map(|(n, l)| format!("{l}\t{n}\n")).collect()
}

/// A browser page of another site may send a write (a form / fetch to this LAN address): refused when the request
/// names an Origin that is not this server (`http://<Host>`). Requests without Origin (curl, same-origin GET) pass.
fn same_origin(origin: Option<&str>, host: Option<&str>) -> bool {
    match (origin, host) {
        (None, _) => true,
        (Some(o), Some(h)) => o.strip_prefix("http://").or_else(|| o.strip_prefix("https://")) == Some(h),
        (Some(_), None) => false,
    }
}

/// PUT / DELETE of a slot file → (status, message).
fn saves_write(conf: &Conf, target: &str, method: &str, body: &[u8]) -> (&'static str, String) {
    let Some(dir) = &conf.saves else { return ("404 Not Found", "no saves dir\n".into()) };
    let Some(file) = target.split(['?', '#']).next().and_then(percent_decode) else {
        return ("404 Not Found", "not found\n".into());
    };
    let file = file.strip_prefix("/saves/").unwrap_or("");
    if !xf::save::is_slot_file(file) {
        return ("404 Not Found", "not a save slot file\n".into());
    }
    let p = dir.join(file);
    if method == "DELETE" {
        return match std::fs::remove_file(&p) {
            Ok(()) => ("204 No Content", String::new()),
            Err(_) => ("404 Not Found", "not found\n".into()),
        };
    }
    if let Err(e) = check_slot_file(file, body) {
        return ("422 Unprocessable Content", format!("{e}\n"));
    }
    if file.ends_with(".x") && !p.is_file() && saves_index(dir).lines().filter(|l| l.ends_with(".x")).count() >= MAX_SAVE_FILES {
        return ("507 Insufficient Storage", format!("at most {MAX_SAVE_FILES} saves\n"));
    }
    match xf::io::write_atomic(&p, body) {
        Ok(()) => ("204 No Content", String::new()),
        Err(e) => ("500 Internal Server Error", format!("{e}\n")),
    }
}

/// The content of a slot file: `slots.txt` = at most 3 lines, each empty or a save name; `<NAME>.x` = an HDB (header
/// 5 / 0x501, the saves' container header — neodb `validate`) of at most [`MAX_SAVE`] bytes.
fn check_slot_file(file: &str, body: &[u8]) -> Result<(), &'static str> {
    if file == "slots.txt" {
        let t = std::str::from_utf8(body).map_err(|_| "slots.txt: not UTF-8")?;
        if body.len() > 256 || t.lines().count() > 3 || !t.lines().all(|l| l.is_empty() || xf::save::sanitize(l) == l) {
            return Err("slots.txt: at most 3 lines of save names");
        }
        return Ok(());
    }
    let be32 = |o: usize| body.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    if body.len() as u64 > MAX_SAVE || be32(0) != Some(5) || be32(0x14) != Some(0x501) {
        return Err("not a save (HDB header 5 / 0x501)");
    }
    Ok(())
}

/// `root/rel` if `rel` is a plain relative path (no `..`, `.`, root or prefix components) naming a file that, with
/// symlinks resolved, lies inside `root` (canonical); else None.
fn confined(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains(['\\', '\0']) {
        return None;
    }
    let rp = Path::new(rel);
    if !rp.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let p = root.join(rp).canonicalize().ok()?;
    (p.starts_with(&root) && p.is_file()).then_some(p)
}

/// [`confined`] for a game data root: a mounted data set (`xf::vfs`, disc images) is checked through its layers (plain
/// relative components only; a folder layer resolves symlinks inside itself), a real folder as any other root.
fn confined_game(root: &Path, rel: &str) -> Option<PathBuf> {
    if xf::vfs::find(root).is_none() {
        return confined(root, rel);
    }
    if rel.is_empty() || rel.contains(['\\', '\0']) || !Path::new(rel).components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let p = root.join(rel);
    xf::io::is_file(&p).then_some(p)
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Lines `<size>\t<path relative to the data dir>`: the files of the dir and of its direct sub-directories.
fn data_index(dir: &Path) -> String {
    let mut out = String::new();
    let mut add = |rel: &str, p: &Path| {
        if let Ok(n) = xf::io::fs().size(p) {
            if !rel.contains(['\n', '\t']) {
                out.push_str(&format!("{n}\t{rel}\n"));
            }
        }
    };
    let Ok(rd) = xf::io::read_dir(dir) else { return out };
    for e in rd {
        if e.is_dir {
            if let Ok(sub) = xf::io::read_dir(&e.path) {
                for f in sub.into_iter().filter(|f| !f.is_dir) {
                    add(&format!("{}/{}", e.name, f.name), &f.path);
                }
            }
        } else {
            add(&e.name, &e.path);
        }
    }
    out
}

/// H1837: the files under `dir` down to `depth` sub-directory levels: lines `<size>\t<relative path>` (sorted).
fn deep_index(dir: &Path, depth: usize) -> String {
    fn walk(base: &Path, rel: &str, p: &Path, depth: usize, out: &mut Vec<String>) {
        let Ok(rd) = xf::io::read_dir(p) else { return };
        for e in rd {
            let r = if rel.is_empty() { e.name.clone() } else { format!("{rel}/{}", e.name) };
            if e.is_dir {
                if depth > 0 {
                    walk(base, &r, &e.path, depth - 1, out);
                }
            } else if let Ok(n) = xf::io::fs().size(&e.path) {
                if !r.contains(['\n', '\t']) {
                    out.push(format!("{n}\t{r}\n"));
                }
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, "", dir, depth, &mut v);
    v.sort();
    v.concat()
}

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    }
}

/// `bytes=a-b` / `bytes=a-` / `bytes=-n` (a single range) → (start, end inclusive) within `len`.
fn parse_range(r: &str, len: u64) -> Option<(u64, u64)> {
    let (a, b) = r.trim().strip_prefix("bytes=")?.split_once('-')?;
    if b.contains(',') || len == 0 {
        return None;
    }
    let (a, b) = (a.trim(), b.trim());
    let (s, e) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.saturating_sub(n), len - 1)
    } else {
        let s: u64 = a.parse().ok()?;
        let e = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
        (s, e)
    };
    (s <= e && s < len).then_some((s, e))
}

fn file(w: &mut TcpStream, p: &Path, range: Option<&str>, head: bool, extra: &[(&str, &str)]) -> std::io::Result<()> {
    // a file of a mounted data set (disc images, xf::vfs): read by ranges through xf::io, never whole
    let mounted = xf::vfs::find(p).is_some();
    let mut f = None;
    let len = if mounted {
        match xf::io::fs().size(p) {
            Ok(n) => n,
            Err(_) => return not_found(w, head),
        }
    } else {
        let Ok(x) = std::fs::File::open(p) else { return not_found(w, head) };
        let n = x.metadata()?.len();
        f = Some(x);
        n
    };
    let (status, s, e) = match range {
        None => ("200 OK", 0, len.saturating_sub(1)),
        Some(r) => match parse_range(r, len) {
            Some((s, e)) => ("206 Partial Content", s, e),
            None => {
                let cr = format!("bytes */{len}");
                return respond(w, "416 Range Not Satisfiable", "text/plain", b"", head, &[("Content-Range", &cr)]);
            }
        },
    };
    let n = if len == 0 { 0 } else { e - s + 1 };
    let mut hdr = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {}\r\nContent-Length: {n}\r\nAccept-Ranges: bytes\r\nX-Content-Type-Options: nosniff\r\n",
        mime(p)
    );
    if status.starts_with("206") {
        hdr.push_str(&format!("Content-Range: bytes {s}-{e}/{len}\r\n"));
    }
    for (k, v) in extra {
        hdr.push_str(&format!("{k}: {v}\r\n"));
    }
    hdr.push_str("\r\n");
    w.write_all(hdr.as_bytes())?;
    if head || n == 0 {
        return Ok(());
    }
    match f {
        Some(mut f) => {
            f.seek(SeekFrom::Start(s))?;
            std::io::copy(&mut f.take(n), w)?;
        }
        None => {
            let mut buf = vec![0u8; 256 << 10];
            let (mut at, end) = (s, s + n);
            while at < end {
                let want = ((end - at) as usize).min(buf.len());
                let k = xf::io::fs().read_at(p, at, &mut buf[..want])?;
                if k == 0 {
                    break;
                }
                w.write_all(&buf[..k])?;
                at += k as u64;
            }
        }
    }
    Ok(())
}

fn not_found(w: &mut TcpStream, head: bool) -> std::io::Result<()> {
    respond(w, "404 Not Found", "text/plain", b"not found\n", head, &[])
}

fn respond(w: &mut TcpStream, status: &str, ctype: &str, body: &[u8], head: bool, extra: &[(&str, &str)]) -> std::io::Result<()> {
    let mut hdr = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nX-Content-Type-Options: nosniff\r\n", body.len());
    for (k, v) in extra {
        hdr.push_str(&format!("{k}: {v}\r\n"));
    }
    hdr.push_str("\r\n");
    w.write_all(hdr.as_bytes())?;
    if !head {
        w.write_all(body)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_range("bytes=90-", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=50-500", 100), Some((50, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=5-1", 100), None);
        assert_eq!(parse_range("bytes=0-1,5-6", 100), None);
    }

    /// A server on a free local port with `saves` as its saves dir (data / web = the port's source dir).
    fn server(saves: &Path) -> SocketAddr {
        server_en(saves, None)
    }

    fn server_en(saves: &Path, en: Option<PathBuf>) -> SocketAddr {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap();
        server_with(Some(saves_dir(saves, &root.join("../data/InstalledGame")).unwrap()), en)
    }

    /// A server with this saves dir (None = `--saves-in browser`).
    fn server_with(saves: Option<PathBuf>, en: Option<PathBuf>) -> SocketAddr {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap();
        let conf: &'static Conf = Box::leak(Box::new(Conf {
            data: root.join("src"),
            cache: None,
            saves,
            web: root.clone(),
            trace: Some(root.join("Cargo.toml")),
            fonts: vec![],
            en,
        }));
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || serve(s, conf));
            }
        });
        a
    }

    /// One request (`Connection: close`) → (status code, body).
    fn req(a: SocketAddr, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> (u32, Vec<u8>) {
        let mut s = TcpStream::connect(a).unwrap();
        let mut h = format!("{method} {path} HTTP/1.1\r\nHost: {a}\r\nConnection: close\r\n");
        for (k, v) in headers {
            h.push_str(&format!("{k}: {v}\r\n"));
        }
        if method == "PUT" && !headers.iter().any(|h| h.0 == "Content-Length") {
            h.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        h.push_str("\r\n");
        s.write_all(h.as_bytes()).unwrap();
        let _ = s.write_all(body);
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        let end = out.windows(4).position(|w| w == b"\r\n\r\n").map_or(out.len(), |p| p + 4);
        let code = std::str::from_utf8(&out[9..12]).unwrap().parse().unwrap();
        (code, out[end..].to_vec())
    }

    /// A minimal body with the saves' HDB header (5 / 0x501).
    fn fake_save(n: usize) -> Vec<u8> {
        let mut b = vec![0u8; n.max(0x18)];
        b[0..4].copy_from_slice(&5u32.to_be_bytes());
        b[0x14..0x18].copy_from_slice(&0x501u32.to_be_bytes());
        b
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("xf-serve-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// The web dir defaults to `web/` next to the executable when it holds index.html, else `port/web/www`.
    #[test]
    fn web_dir_default() {
        let m = Path::new("/src/port");
        let d = tmp("webdefault");
        assert_eq!(default_web(None, m), m.join("web/www"));
        assert_eq!(default_web(Some(&d), m), m.join("web/www"));
        std::fs::create_dir_all(d.join("web")).unwrap();
        assert_eq!(default_web(Some(&d), m), m.join("web/www"));
        std::fs::write(d.join("web/index.html"), b"<!doctype html>").unwrap();
        assert_eq!(default_web(Some(&d), m), d.join("web"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// The license notices ship with the web front: `/LICENSES.txt` serves `xf::notices::LICENSES`.
    #[test]
    fn licenses_route() {
        let a = server_with(None, None);
        let (code, body) = req(a, "GET", "/LICENSES.txt", &[], b"");
        assert_eq!(code, 200);
        assert_eq!(body, xf::notices::LICENSES.as_bytes());
    }

    /// H1837: the English data set's index (three sub-directory levels) and files; 404 without `--en-data`.
    #[test]
    fn h1837_en_data_routes() {
        let d = tmp("en");
        for (p, b) in [("English/MININST/XV/1.XMV", "a"), ("English/XV/2.XMV", "bb"), ("English/a/b/c/d/deep.x", "x")] {
            std::fs::create_dir_all(d.join(p).parent().unwrap()).unwrap();
            std::fs::write(d.join(p), b).unwrap();
        }
        let en = d.canonicalize().unwrap();
        let a = server_en(&d.join("saves"), Some(en));
        let (code, body) = req(a, "GET", "/en-index", &[], b"");
        assert_eq!(code, 200);
        assert_eq!(String::from_utf8(body).unwrap(), "1\tEnglish/MININST/XV/1.XMV\n2\tEnglish/XV/2.XMV\n");
        assert_eq!(req(a, "GET", "/en/English/XV/2.XMV", &[], b""), (200, b"bb".to_vec()));
        assert_eq!(req(a, "GET", "/en/English/XV/2.XMV", &[("Range", "bytes=1-")], b"").1, b"b".to_vec());
        assert_eq!(req(a, "GET", "/en/../x", &[], b"").0, 404);
        let b = server(&tmp("en-none"));
        assert_eq!(req(b, "GET", "/en-index", &[], b"").0, 404);
        assert_eq!(req(b, "GET", "/en/English/XV/2.XMV", &[], b"").0, 404);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn saves_names_are_checked() {
        let d = tmp("names");
        let a = server(&d);
        let ok = fake_save(64);
        // only slot file names: no traversal, no other file, no lower case
        for bad in ["/saves/..%2Fx.x", "/saves/../x.x", "/saves/%2E%2E/A.x", "/saves/a.x", "/saves/A.X", "/saves/A.txt",
            "/saves/sub/A.x", "/saves/.xf-serve-probe", "/saves/A.x%00", "/saves/A.x.tmp"]
        {
            assert_eq!(req(a, "PUT", bad, &[], &ok).0, 404, "{bad}");
            assert_eq!(req(a, "GET", bad, &[], b"").0, 404, "{bad}");
        }
        // content: an HDB header for .x, save names for slots.txt
        assert_eq!(req(a, "PUT", "/saves/A.x", &[], b"not a save").0, 422);
        assert_eq!(req(a, "PUT", "/saves/slots.txt", &[], b"../etc\n").0, 422);
        assert_eq!(req(a, "PUT", "/saves/slots.txt", &[], b"A\nB\nC\nD\n").0, 422);
        let big = (MAX_SAVE + 1).to_string();
        assert_eq!(req(a, "PUT", "/saves/A.x", &[("Content-Length", &big)], b"").0, 413);
        // writes from a page of another origin
        assert_eq!(req(a, "PUT", "/saves/A.x", &[("Origin", "http://evil.example")], &ok).0, 403);
        assert_eq!(req(a, "DELETE", "/saves/A.x", &[("Origin", "http://evil.example")], b"").0, 403);
        assert_eq!(req(a, "POST", "/saves/A.x", &[], b"").0, 405);
        // nothing was written
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// H1733: `--saves-in browser` — the page is told `browser`, /saves/ is 404 for reads and writes (nothing on disk).
    #[test]
    fn saves_in_browser_serves_no_saves() {
        let a = server_with(None, None);
        assert_eq!(req(a, "GET", "/saves-mode", &[], b""), (200, b"browser\n".to_vec()));
        assert_eq!(req(a, "GET", "/saves/", &[], b"").0, 404);
        assert_eq!(req(a, "GET", "/saves/slots.txt", &[], b"").0, 404);
        assert_eq!(req(a, "PUT", "/saves/GAME%201.x", &[], &fake_save(1000)).0, 404);
        let d = tmp("mode");
        assert_eq!(req(server(&d), "GET", "/saves-mode", &[], b""), (200, b"server\n".to_vec()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn saves_round_trip() {
        let d = tmp("rt");
        let a = server(&d);
        assert_eq!(req(a, "GET", "/saves/", &[], b""), (200, vec![]));
        assert_eq!(req(a, "GET", "/saves/GAME%201.x", &[], b"").0, 404);
        let one = fake_save(1000);
        let origin = format!("http://{a}");
        assert_eq!(req(a, "PUT", "/saves/GAME%201.x", &[("Origin", &origin)], &one).0, 204);
        assert_eq!(req(a, "PUT", "/saves/slots.txt", &[], b"GAME 1\n\n\n").0, 204);
        assert_eq!(req(a, "GET", "/saves/GAME%201.x", &[], b""), (200, one.clone()));
        assert_eq!(req(a, "GET", "/saves/", &[], b"").1, b"1000\tGAME 1.x\n9\tslots.txt\n");
        // replaced atomically: the new content, no temporary file left
        let two = fake_save(2000);
        assert_eq!(req(a, "PUT", "/saves/GAME%201.x", &[], &two).0, 204);
        assert_eq!(req(a, "GET", "/saves/GAME%201.x", &[], b"").1, two);
        let mut names: Vec<String> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, ["GAME 1.x", "slots.txt"]);
        // the window's slot list sees it
        assert_eq!(xf::save::Slots::open(&d).names[0], "GAME 1");
        assert_eq!(req(a, "DELETE", "/saves/GAME%201.x", &[], b"").0, 204);
        assert_eq!(req(a, "DELETE", "/saves/GAME%201.x", &[], b"").0, 404);
        assert_eq!(xf::save::Slots::open(&d).names[0], ""); // file gone → slot empty (FUN_00515dd0)
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A save made by the web path (the running game's bytes PUT to /saves/ as the worker does: `<NAME>.x`, then
    /// slots.txt) is a save the window loads (its `Slots` + `Session::reload`). Needs the game data.
    #[test]
    fn web_save_loads_in_the_window() {
        use xf::front::Session;
        use xf::hdb::Hdb;
        let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
        if !data.join("XFiles.gam").is_file() {
            eprintln!("skip: no game data");
            return;
        }
        let leak = |n: &str| -> &'static Hdb { Box::leak(Box::new(Hdb::open(data.join(n)).unwrap())) };
        let (h, g) = (leak("XFiles.hdb"), leak("XFiles.gam"));
        let mut s = Session::with_rules(h, g, &data, xf::front::rules_hyp());
        s.r.new_game();
        s.tick(5000);
        let (bytes, _) = s.save_bytes().unwrap();
        let d = tmp("web");
        let a = server(&d);
        assert_eq!(req(a, "PUT", "/saves/WEB%201.x", &[], &bytes).0, 204);
        assert_eq!(req(a, "PUT", "/saves/slots.txt", &[], b"\nWEB 1\n\n").0, 204);
        // the window's path: its saves dir, slot 1, Hdb::open, reload as a Load
        let sl = xf::save::Slots::open(&d);
        let p = sl.path(1).expect("slot 1");
        assert!(xf::save::is_save(&p));
        let gl: &'static Hdb = Box::leak(Box::new(Hdb::open(&p).unwrap()));
        let want = (s.r.w.node, s.r.w.view);
        let mut w = Session::with_rules(h, g, &data, xf::front::rules_hyp());
        w.set_saves(sl);
        w.reload(gl, true);
        w.tick(1);
        assert_eq!((w.r.w.node, w.r.w.view), want);
        assert_eq!(w.r.w.save_names[1], "WEB 1");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn confinement() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        assert!(confined(&root, "Cargo.toml").is_some());
        assert!(confined(&root, "src/lib.rs").is_some());
        assert!(confined(&root, "../README.md").is_none());
        assert!(confined(&root, "src/../../README.md").is_none());
        assert!(confined(&root, "/etc/passwd").is_none());
        assert!(confined(&root, "./Cargo.toml").is_none());
        assert!(confined(&root, "src").is_none()); // directories are not served
        assert!(confined(&root, "").is_none());
        assert_eq!(percent_decode("/data/a%20b").as_deref(), Some("/data/a b"));
        assert_eq!(percent_decode("/x%2"), None);
    }
}
