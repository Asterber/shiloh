//! xf — the port with an SDL2 window (640×480): the cold start as the original (H1660 / H1661: the intro movies —
//! a left / right click skips the current one, Alt+X quits — then the main menu; `--skip-intro` = the menu at once,
//! `--new-game` = straight into New Game, `--load SAVE.x` = straight into the save), the View picture (background / playing video), hotspot
//! overlay, clicks → core. Game logic is all in the core (`world`, `world::run`); this file only shows and forwards.
//!
//!   xf [--data DIR] [--cache DIR] [--load SAVE.x] [--trace T.jsonl] [--overlay] [--quit-after MS] [--volume PCT]   interactive window
//!                                                              (sound volume in percent, default 100)
//!   xf --frames N --dump DIR [--trace T.jsonl | --route fo-009] [--every MS]   (headless starts with New Game; `--intro`
//!                                                              / `--skip-intro`: the cold start's intro / main menu, then `--steps`)
//!   xf --frames N --dump DIR --hyp --load SAVE.x --steps "p:X,Y;m:X,Y;o:OWNER;k:VK;t:TEXT"
//!                                                              headless from a save, user input first (front::script);
//!                                                              --game-cursor draws the game's cursor (H307) at the pointer
//!                                                              --live-rand SEED draws RandGen values as the window (H550)
//!                                                              headless: no window, PPM frames every MS of game time
//!   --hyp   (headless; the window has it on by default, --no-hyp turns it off) every hypothesis rule on (H55 Explorables, H58 type 5 / IFaceLayout, H59 timers, H60 PreloadEmIcon besides
//!           H48/H53): the core creates the Explorables, the drawer items, the dialogue icons and Timer Expiration itself
//!           (route fo-009 and `--trace` routes without registrations taken from the trace)
//!
//! Captions (H340–H344): `--subtitles on|off|auto` sets Pref_bClosedCaptioning (var 0xa0d) after New Game / Load (auto =
//! the game's own value); `--subtitles-dir DIR` (or `XF_SUBTITLES`) adds user captions: `DIR/<movie path>.srt` (SubRip,
//! UTF-8, movie-file time; e.g. `DIR/XV/23434.srt`) or a movie of the same path with text tracks.
//! `--play ASSET` (headless) shows one asset in the movie area (viewer).
//! `--subs en|ru|off|auto` (or `XF_SUBS`, H955 / H956): a subtitle pack of the cache dir (`subs-v1/<lang>.json`): the
//! English one made by `xf-prep --en-subs`, the Russian one by `xf-prep --ru-subs TABLE`. `en` / `ru` = use that pack
//! and switch captions on (unless `--subtitles` says otherwise; error if the pack is missing), `auto` (default) = use
//! `ru` when present, else `en`, else none; captions stay as the game has them (F3 / the game's preference), `off` = never.
//!
//! Game language (H1836, port feature): `--game-lang ru|en` (or `XF_GAME_LANG`; default: the game data's own, H2305) —
//! the voice-over, the movies and the game's own texts (dialogue lines, notes, documents, names) of the installed RU data
//! or of an English data set laid over it, `--en-data DIR` (or `XF_EN_DATA`): the EN DVD Edition's disc root /
//! `English/` dir, or an installed English game. The game logic stays the `--data` set's (saves, routes and the e2e are
//! unchanged). F4 in the window switches the language live (from the next movie / line). With the EN DVD Edition as
//! the game data itself (`--data`), English is the only language: no Russian is offered, `--en-data` is not needed and
//! the subtitles are the movies' own English captions (no pack; F3 / `--subtitles on` shows them, H2305 / H2310).
//!
//! Long timers (C191 / H1203): `--long-timers wine|nt` — `wine` (default) = as the original under the Wine / CrossOver
//! bottle (and probably Windows 9x): a timer armed with a delay above 65535 ms never fires, so the Gordon's Hauling bomb
//! does not go off; `nt` = as on Windows NT: the bomb ends the game 120 s after the log manifest. Kept by New / Load.
//!
//! Sound (window only): the sound tracks of the playing media and of the View background (`Session::audio_cues`), mixed
//! into one SDL audio device.
//!
//! Saves (H400–H405): `--saves DIR` (default `xf::save::default_dir()`, never inside the game data) holds the save
//! slots (`slots.txt`) and `<NAME>.x` files the game's save screen writes; a game started from XFiles.gam is saved by
//! building the save from XFiles.gam (H470, `save::build`; with `Rules::h470_save_builder` off: over a save of the
//! original found in DIR or the data directory, H401 template). The load screen switches the window to
//! the chosen save; the menu's New starts over, Quit (or the window's close button) ends the program.
//!
//! Settings (H1750–H1755, window only): F10 (or the "Port settings" button under Quit on the game's menu screen) opens the
//! port's settings screen over the paused game — scaling, window size, full screen, volume, subtitles, subtitle language,
//! game language, long timers, hotspots, pause when minimised — with each option's hotkey and a pixelated / smooth preview
//! of the current frame; mouse-driven, F10 / Esc / Close / a click outside closes it. The settings live in
//! `front::settings::config_path()` (`XF_CONFIG`, or `--config FILE`; `--no-config` = neither read nor written); the CLI
//! flags (`--scale`, `--volume`, `--subtitles`, `--subs`, `--long-timers`, `--overlay`) override the file for this run and
//! are not written back unless changed in the screen. F1 / F2 / F3 / F11 change the same settings. Headless runs never
//! read the file; `--port-ui` draws the menu screen's button into the frames, `--settings-screen` the screen itself.
//!
//! Keys: Space / Enter = end the playing media now (fallback when a file cannot be decoded), F1 = overlay on/off,
//! letters / digits / Enter / Backspace / Tab = key presses to the game (H174), right button = right click (H172); right click while a video plays skips it, a left click is ignored (H322),
//! F2 = scaling mode (fsr / linear / nearest), F3 = closed captions on / off (var 0xa0d, H343), F4 = game language RU / EN (H1836), F11 = full screen, Esc = the game's menu (H169 / H404).
//! The original's Alt hotkeys (H1868 / H2060–H2068, front::altkeys): Alt+0…9 volume (0 = mute, 9 = full; the settings'
//! volume), Alt+A / M / R / T / V Artificial Intuition / Subliminal Messages / Rollover Text / Transitions / High Quality
//! Video on / off, Alt+C captions, Alt+P pause (any key resumes), Alt+S save screen, Alt+X exit, Shift+Alt+F FPS counter,
//! Shift+Alt+H hotspots (= F1), Shift+Alt+I the state box (CurrNode / Asta / Loss / Paranoia / XTrack). The window is resizable (2× at start);
//! the picture keeps its 4:3 aspect (black bars; clicks on the bars go nowhere). Without OpenGL 3.3 the window falls
//! back to the SDL renderer (linear only). `--trace` takes from a trace of the original the inputs the core cannot produce yet: values of
//! conditions on system values; in headless mode also the user's clicks, timers and registrations (front::trace_route).
use std::path::{Path, PathBuf};

// The statically linked SDL2 (feature `sdl-bundled`, the release builds) reads the registry on Windows; the sdl2-sys
// static build does not add advapi32 itself.
#[cfg(all(windows, feature = "sdl-bundled"))]
#[link(name = "advapi32")]
extern "system" {}
use std::time::{Duration, Instant};
use xf::front::{self, Session, Step};
use xf::hdb::Hdb;
use xf::replay::{parse_trace, Ev, TraceEv};
use xf::world::run::Runner;

struct Args {
    data: PathBuf,
    /// `--data` as given (repeatable: installed folder, disc images, a folder of images; xf::vfs)
    data_src: Vec<PathBuf>,
    trace: Option<PathBuf>,
    /// sound volume, percent (`--volume PCT`, default 100)
    volume: f32,
    /// window only: `--av-delay MS` — the picture's delay behind the clock (H1794; default: from the audio buffer)
    av_delay: Option<u64>,
    route: Option<String>,
    frames: Option<usize>,
    dump: Option<PathBuf>,
    every: u64,
    overlay: bool,
    /// window mode: quit after this many ms of wall time (smoke test of the SDL path, e.g. SDL_VIDEODRIVER=dummy)
    quit_after: Option<u64>,
    hyp: bool,
    /// headless: draw the game's cursor (H307) at the last pointer position of `--steps` (the window always does)
    game_cursor: bool,
    /// `--load SAVE.x`: start from a saved game (H65) instead of New Game
    load: Option<PathBuf>,
    /// `--steps "p:X,Y;m:X,Y;o:OWNER;k:VK;t:TEXT"` (headless): user input after the start, settled after each
    /// (front::script, xfplan's syntax)
    steps: Option<String>,
    /// window only: the old minimal rule set (H48 + H53) instead of every hypothesis rule
    no_hyp: bool,
    /// headless: `--live-rand SEED` draws RandGen values as the window does (H550), seed base SEED
    live_rand: Option<u32>,
    /// window only: picture scaling (`--scale fsr|linear|nearest`, default fsr; F2 cycles)
    scale: xf::gpu::gl::Scale,
    /// window only: FSR RCAS sharpness in stops, 0 = strongest (`--sharpness`, default 0.2)
    sharpness: f32,
    /// `--subtitles on|off|auto` (H343): Some(on) = var 0xa0d after New Game / Load; None = the game's value
    subtitles: Option<bool>,
    /// `--subtitles-dir DIR` (H344)
    subtitles_dir: Option<PathBuf>,
    /// `--subs en|ru|off|auto` (H955 / H956): "en" / "ru" = that pack, required; "off" = none; "auto" = ru, else en, if present
    subs: String,
    /// `--play ASSET` (headless viewer)
    play: Option<u32>,
    /// `--layout LAYOUT` (headless viewer): show an InterfaceLayout as the engine's show after the start, before `--steps`
    /// (close-ups the saves do not reach: journal 0xb130, Bible 0xec66 — front::doc)
    layout: Option<u32>,
    /// `--saves DIR`: the save slots' directory (H402)
    saves: PathBuf,
    /// `--long-timers wine|nt` (C191 / H1203): a timer above 65535 ms (the Gordon's Hauling bomb) never fires (wine,
    /// default) or runs (nt)
    long_timers: xf::world::LongTimers,
    /// H1660 / H1661: how the program starts without `--load` — `--intro` (the window's default: the intro movies, then
    /// the main menu), `--skip-intro` (the main menu at once), `--new-game` (headless default: New Game at once)
    start: Option<xf::front::intro::Start>,
    /// H1836: `--game-lang ru|en` (env XF_GAME_LANG; default: the game data's own language, H2305)
    game_lang: Option<xf::media::lang::GameLang>,
    /// H1836: `--en-data DIR` (env XF_EN_DATA): the English data set
    en_data: Vec<PathBuf>,
    /// `--config FILE` (window): the settings file instead of `front::settings::config_path()`
    config: Option<PathBuf>,
    /// `--no-config` (window): no settings file (defaults + CLI flags)
    no_config: bool,
    /// the settings the CLI flags give (key, value): they override the file for this run (H1750)
    cli_settings: Vec<(&'static str, String)>,
    /// headless: draw the port's own UI as the window would (the menu screen's settings button)
    port_ui: bool,
    /// headless: the frames show the settings screen (H1751) over the game
    settings_screen: bool,
}

fn args() -> Args {
    let mut a = Args {
        data: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame"),
        data_src: Vec::new(),
        trace: None,
        volume: 100.0,
        av_delay: None,
        route: None,
        frames: None,
        dump: None,
        every: 500,
        overlay: false,
        quit_after: None,
        hyp: false,
        no_hyp: false,
        live_rand: None,
        load: None,
        steps: None,
        game_cursor: false,
        scale: xf::gpu::gl::Scale::Fsr,
        sharpness: 0.2,
        subtitles: None,
        subtitles_dir: std::env::var_os("XF_SUBTITLES").map(PathBuf::from),
        subs: std::env::var("XF_SUBS").ok().map(|v| subs_mode(&v)).unwrap_or_else(|| "auto".into()),
        play: None,
        layout: None,
        saves: xf::save::default_dir(),
        long_timers: Default::default(),
        start: None,
        game_lang: std::env::var("XF_GAME_LANG")
            .ok()
            .map(|v| xf::media::lang::GameLang::parse(&v).unwrap_or_else(|| die("XF_GAME_LANG ru|en"))),
        en_data: xf::vfs::env_sources("XF_EN_DATA"),
        config: None,
        no_config: false,
        cli_settings: Vec::new(),
        port_ui: false,
        settings_screen: false,
    };
    let mut en_cli = false;
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| die(&format!("{x}: value missing")));
        match x.as_str() {
            "--data" => a.data_src.push(val().into()),
            // PCM cache of the QDMC music (xf-prep); default XF_CACHE, else the user cache dir
            "--cache" => xf::media::pcmcache::set_dir(PathBuf::from(val())),
            "--trace" => a.trace = Some(val().into()),
            "--route" => a.route = Some(val()),
            "--frames" => a.frames = Some(val().parse().unwrap_or_else(|_| die("--frames N"))),
            "--dump" => a.dump = Some(val().into()),
            "--volume" => {
                a.volume = val().parse().unwrap_or_else(|_| die("--volume PCT"));
                a.cli_settings.push(("volume", a.volume.to_string()));
            }
            // H1794: exact ms, over the settings' "Picture delay"
            "--av-delay" => a.av_delay = Some(val().parse().unwrap_or_else(|_| die("--av-delay MS"))),
            "--every" => a.every = val().parse().unwrap_or_else(|_| die("--every MS")),
            "--no-overlay" => a.overlay = false,
            "--overlay" => {
                a.overlay = true;
                a.cli_settings.push(("overlay", "on".into()));
            }
            "--game-cursor" => a.game_cursor = true,
            "--load" => a.load = Some(val().into()),
            "--steps" => a.steps = Some(val()),
            "--hyp" => a.hyp = true,
            "--no-hyp" => a.no_hyp = true,
            "--live-rand" => a.live_rand = Some(val().parse().unwrap_or_else(|_| die("--live-rand SEED"))),
            "--scale" => {
                a.scale = xf::gpu::gl::Scale::parse(&val()).unwrap_or_else(|| die("--scale fsr|linear|nearest"));
                a.cli_settings.push(("scale", a.scale.name().into()));
            }
            "--sharpness" => a.sharpness = val().parse().unwrap_or_else(|_| die("--sharpness STOPS")),
            "--subtitles" => {
                a.subtitles = match val().as_str() {
                    "on" => Some(true),
                    "off" => Some(false),
                    "auto" => None,
                    _ => die("--subtitles on|off|auto"),
                };
                let v = a.subtitles.map_or("game", |on| if on { "on" } else { "off" });
                a.cli_settings.push(("captions", v.into()));
            }
            "--subtitles-dir" => a.subtitles_dir = Some(val().into()),
            "--subs" => {
                a.subs = subs_mode(&val());
                a.cli_settings.push(("subs", a.subs.clone()));
            }
            "--saves" => a.saves = val().into(),
            "--game-lang" => {
                let l = xf::media::lang::GameLang::parse(&val()).unwrap_or_else(|| die("--game-lang ru|en"));
                a.game_lang = Some(l);
                a.cli_settings.push(("game_lang", l.code().into()));
            }
            "--en-data" => {
                if !en_cli {
                    a.en_data.clear();
                    en_cli = true;
                }
                a.en_data.push(val().into())
            }
            "--long-timers" => {
                a.long_timers = xf::world::LongTimers::parse(&val()).unwrap_or_else(|| die("--long-timers wine|nt"));
                a.cli_settings.push(("long_timers", a.long_timers.name().into()));
            }
            "--config" => a.config = Some(val().into()),
            "--no-config" => a.no_config = true,
            "--port-ui" => a.port_ui = true,
            "--settings-screen" => (a.port_ui, a.settings_screen) = (true, true),
            "--play" => {
                let v = val();
                a.play = Some(
                    u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap_or_else(|_| die("--play ASSET (hex objId)")),
                )
            }
            "--layout" => {
                let v = val();
                a.layout = Some(
                    u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap_or_else(|_| die("--layout LAYOUT (hex objId)")),
                )
            }
            "--intro" => a.start = Some(xf::front::intro::Start::Intro),
            "--skip-intro" => a.start = Some(xf::front::intro::Start::Menu),
            "--new-game" => a.start = Some(xf::front::intro::Start::NewGame),
            "--pause-when-minimised" => {
                a.cli_settings.push(("pause_hidden", "on".into()));
            }
            "--quit-after" => a.quit_after = Some(val().parse().unwrap_or_else(|_| die("--quit-after MS"))),
            "--licenses" => {
                print!("{}", xf::notices::LICENSES);
                std::process::exit(0);
            }
            "-h" | "--help" => {
                eprintln!("usage: xf [--data DIR|IMAGE ...] [--cache DIR] [--trace T.jsonl] [--load SAVE.x] [--overlay] [--hyp] [--scale fsr|linear|nearest] [--sharpness STOPS] [--subtitles on|off|auto] [--subtitles-dir DIR] [--subs en|ru|off|auto] [--saves DIR] [--game-lang ru|en] [--en-data DIR|IMAGE ...] [--long-timers wine|nt] [--pause-when-minimised] [--intro | --skip-intro | --new-game] [--config FILE | --no-config] [--licenses]\n       xf --frames N --dump DIR [--load SAVE.x --steps S] [--intro | --skip-intro | --new-game] [--trace T.jsonl | --route fo-009] [--every MS] [--play ASSET] [--layout LAYOUT] [--port-ui] [--settings-screen]\n\nkeys: Esc menu, F1 hotspots, F2 scaling, F3 captions, F4 game language, F10 settings, F11 full screen, Space / Enter end the media\nAlt keys (the original's): Alt+0..9 volume (0 mute, 9 full), Alt+A / M / R / T / V Artificial Intuition / Subliminal Messages / Rollover Text / Transitions / High Quality Video,\n  Alt+C captions, Alt+P pause (any key resumes), Alt+S save screen, Alt+X exit, Shift+Alt+F FPS, Shift+Alt+H hotspots, Shift+Alt+I state box");
                std::process::exit(0);
            }
            _ => die(&format!("unknown argument {x}")),
        }
    }
    // the game data: `--data` (repeatable), else XF_DATA (a path list), else ../data/InstalledGame; disc images and
    // several sources are merged into one virtual folder (xf::vfs)
    if a.data_src.is_empty() {
        a.data_src = xf::vfs::env_sources("XF_DATA");
    }
    if a.data_src.is_empty() {
        a.data_src.push(a.data.clone());
    }
    a.data = xf::vfs::data_root_or_exit("xf", &a.data_src);
    // `--subs en|ru`: captions on unless --subtitles says otherwise
    if xf::media::subs::is_lang(&a.subs) && a.subtitles.is_none() {
        a.subtitles = Some(true);
        a.cli_settings.push(("captions", "on".into()));
    }
    a
}

/// `en` / `ru` / `off` / `auto` (anything else is an error)
fn subs_mode(v: &str) -> String {
    if v == "off" || v == "auto" || xf::media::subs::is_lang(v) {
        v.to_string()
    } else {
        die("--subs en|ru|off|auto")
    }
}

fn die(m: &str) -> ! {
    eprintln!("xf: {m}");
    std::process::exit(2)
}

fn main() {
    // the file backend that also reads disc images (before the first read of a game file)
    xf::vfs::install();
    let a = args();
    let open = |n: &str| Hdb::open(a.data.join(n)).unwrap_or_else(|e| die(&e.to_string()));
    let h = open("XFiles.hdb");
    let g = match &a.load {
        Some(p) => Hdb::open(p).unwrap_or_else(|e| die(&format!("{}: {e}", p.display()))),
        None => open("XFiles.gam"),
    };
    let trace: Option<Vec<TraceEv>> = a.trace.as_ref().map(|p| {
        let t = std::fs::read_to_string(p).unwrap_or_else(|e| die(&format!("{}: {e}", p.display())));
        parse_trace(&t).unwrap_or_else(|e| die(&e))
    });
    // the window runs the rule set the bot verifies against the original (every hypothesis on) unless --no-hyp
    let window_mode = a.frames.is_none() && a.dump.is_none();
    let mut rules = if a.hyp || (window_mode && !a.no_hyp) { front::rules_hyp() } else { front::rules() };
    rules.set_long_timers(a.long_timers);
    let mut s = Session::with_rules(&h, &g, &a.data, rules);
    if a.saves.starts_with(&a.data) {
        die("--saves: not inside the game data");
    }
    s.set_saves(xf::save::Slots::open(&a.saves));
    s.template = xf::save::find_template(&[&a.saves, &a.data]);
    s.overlay = a.overlay;
    s.game_cursor = a.game_cursor;
    if let Some(d) = &a.subtitles_dir {
        if !d.is_dir() {
            die(&format!("--subtitles-dir {}: not a directory", d.display()));
        }
        s.subs = Some(xf::media::MediaLib::new(d));
    }
    // H1836: the English data set and the game language
    if !a.en_data.is_empty() {
        let d = &xf::vfs::data_root_or_exit("xf: --en-data", &a.en_data);
        match xf::media::lang::EnData::open(d) {
            Ok(en) => {
                eprintln!("xf: English data set {} ({} files, {} strings)", d.display(), en.index.len(), en.strings.len());
                s.set_en_data(Some(std::sync::Arc::new(en)));
            }
            Err(e) => die(&format!("--en-data: {e}")),
        }
    }
    if let Some(l) = a.game_lang {
        if let Err(e) = s.set_lang(l) {
            die(&format!("--game-lang {}: {e}", l.code()));
        }
    }
    // H955 / H956: the subtitle pack (xf-prep --en-subs / --ru-subs) from the cache dir
    match xf::media::pcmcache::dir() {
        Some(c) => match xf::media::subs::Pack::pick_for(&c, &a.subs, s.data_lang) {
            Ok(Some(p)) => {
                eprintln!("xf: {} subtitles for {} movies from {}", p.lang, p.movies.len(), c.join(xf::media::subs::SUBDIR).display());
                s.sub_pack = Some(std::sync::Arc::new(p));
            }
            Ok(None) => {}
            Err(e) => die(&format!(
                "--subs {}: {e} (make it once: xf-prep --en-subs <your English game dir>, then xf-prep --ru-subs <translation table>; see port/README.md)",
                a.subs
            )),
        },
        None if xf::media::subs::is_lang(&a.subs) => die("--subs: no cache dir (pass --cache DIR or set XF_CACHE)"),
        None => {}
    }
    // conditions on system values (operand kind 1) are not evaluable by the core: take the values the original had
    // (an environment input) from the given trace, else from traces/fo-009.jsonl when present, else none. Under the
    // window's rules (H73 / H90) New Game and fo-009's route have no unevaluable condition, so frames are the same
    // either way (headless --hyp, 1200 frames of `--route fo-009`, compared with and without the file)
    let default_trace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../traces/fo-009.jsonl");
    match &trace {
        Some(t) => s.r.uneval = Runner::uneval_from_trace(t, &h),
        None => {
            if let Ok(t) = std::fs::read_to_string(&default_trace) {
                if let Ok(t) = parse_trace(&t) {
                    s.r.uneval = Runner::uneval_from_trace(&t, &h);
                    eprintln!("xf: system-value conditions from {}", default_trace.display());
                }
            }
        }
    }
    match (a.frames, &a.dump) {
        (Some(n), Some(dir)) => headless(&mut s, n, dir, a.every, &a, trace.as_deref()),
        (None, None) => window(&mut s, &a),
        _ => die("--frames and --dump go together"),
    }
}

/// Headless: New Game, then the route; a frame every `every` ms of game time; steps run whenever the game is idle.
fn headless(s: &mut Session, n: usize, dir: &Path, every: u64, a: &Args, trace: Option<&[TraceEv]>) {
    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(&e.to_string()));
    let steps = match (a.route.as_deref(), trace) {
        (Some("fo-009"), _) if a.hyp => front::route_fo009_core(),
        (Some("fo-009"), _) => front::route_fo009(),
        (Some(r), _) => die(&format!("unknown route {r}")),
        (None, Some(t)) => front::trace_route_with(t, a.hyp),
        (None, None) => Vec::new(),
    };
    s.r.w.live_rand = a.live_rand;
    if a.load.is_some() {
        s.load_game();
    } else {
        s.boot(a.start.unwrap_or(xf::front::intro::Start::NewGame));
    }
    if let Some(on) = a.subtitles {
        s.r.set_closed_captioning(on);
    }
    if let Some(asset) = a.play {
        s.play_preview(asset);
    }
    if let Some(l) = a.layout {
        front::settle_timers(s, 100, 900_000);
        s.r.viewer_show_layout(l);
    }
    if let Some(st) = &a.steps {
        // user input first (each settled like the bot waits on the original), then the frames
        let inputs = front::script::parse(st).unwrap_or_else(|e| die(&e));
        front::settle_timers(s, 100, 900_000);
        for i in &inputs {
            front::script::apply(s, i);
            front::settle_timers(s, 100, 900_000);
            if host_request(s, &a.data, a.subtitles) {
                println!("quit requested by the menu");
                break;
            }
        }
    }
    let mut next = 0usize;
    let mut written = 0usize;
    let mut idle_since: Option<u64> = None;
    while written < n {
        s.tick(every);
        if s.idle() {
            let t0 = *idle_since.get_or_insert(s.now);
            // one step per idle period, after a short pause so the frames show the settled View
            let waiting = matches!(steps.get(next), Some(Step::WaitTimers)) && s.r.one_shot_timers() > 0;
            if next < steps.len() && s.now - t0 >= every && !waiting {
                run_step(s, &steps[next]);
                next += 1;
                idle_since = None;
            }
        } else {
            idle_since = None;
        }
        let mut fb = s.render();
        if a.settings_screen {
            let ui = front::settings::SettingsUi::opened(s);
            fb = ui.draw(&fb, &cli_overrides(front::settings::Settings::window_default(), a));
        } else if a.port_ui && s.r.menu_open {
            front::settings::draw_menu_button(&mut fb, false);
        }
        let p = dir.join(format!("{written:05}.ppm"));
        front::write_ppm(&p, &fb).unwrap_or_else(|e| die(&e.to_string()));
        written += 1;
    }
    println!("frames {written} in {}, game time {} ms, steps run {next}/{}", dir.display(), s.now, steps.len());
    println!("views: {}", s.r.transitions.iter().map(|(v, k)| format!("{v:#x}/{k}")).collect::<Vec<_>>().join(" -> "));
    if let Some(t) = trace {
        let want: Vec<(u32, u32)> =
            t.iter().filter_map(|e| if let Ev::Transition { view, kind } = e.ev { Some((view, kind)) } else { None }).collect();
        let ok = s.r.transitions == want;
        println!("trace transitions {} — {}", want.len(), if ok { "same sequence" } else { "DIFFERENT" });
        // the closed loop's posts and executed actions vs the original's, in order (fo-010 W8)
        let tp: Vec<_> = t.iter().filter_map(|e| if let Ev::Post { msg, .. } = e.ev { Some(msg) } else { None }).collect();
        let te: Vec<_> = t.iter().filter_map(|e| if let Ev::Exec { action, .. } = e.ev { Some(action) } else { None }).collect();
        let same = |a: bool| if a { "same sequence" } else { "DIFFERENT" };
        println!("trace posts {} — {} (core {}); executed actions {} — {} (core {})", tp.len(), same(s.r.posts == tp),
            s.r.posts.len(), te.len(), same(s.r.execs == te), s.r.execs.len());
        println!("registrations taken from the trace: {} {:x?}", s.r.externals.len(), s.r.externals);
    }
    for l in s.r.log.iter().chain(&s.log) {
        eprintln!("  {l}");
    }
}

/// The menu's New / Quit and the load screen's Load (H403 / H404): a new session from XFiles.gam or the save (the gam
/// is leaked: it lives as long as the program). True = quit.
fn host_request(s: &mut Session, data: &Path, captions: Option<bool>) -> bool {
    let Some(req) = front::host::request(s) else { return false };
    let (path, load) = match req {
        front::host::HostReq::Quit => return true,
        front::host::HostReq::New => (data.join("XFiles.gam"), false),
        front::host::HostReq::Load(p) => (p, true),
    };
    match Hdb::open(&path) {
        Ok(g) => {
            let g: &'static Hdb = Box::leak(Box::new(g));
            eprintln!("xf: {} {}", if load { "load" } else { "new game from" }, path.display());
            s.reload(g, load);
            if let Some(on) = captions {
                s.r.set_closed_captioning(on);
            }
        }
        Err(e) => eprintln!("xf: {}: {e}", path.display()),
    }
    false
}

/// `set` with the CLI flags' settings applied over it (H1750).
fn cli_overrides(mut set: front::settings::Settings, a: &Args) -> front::settings::Settings {
    for (k, v) in &a.cli_settings {
        if let Err(e) = set.set(k, v) {
            eprintln!("xf: {e}");
        }
    }
    set
}

fn run_step(s: &mut Session, st: &Step) {
    match st {
        Step::Register(v) => v.iter().for_each(|&(o, tl)| s.r.external_register(o, tl)),
        Step::Fire(o, ev) => s.r.external_fire(*o, *ev),
        Step::WaitTimers => {}
        Step::ClickOwner(o) => {
            s.r.click(*o);
        }
        Step::User(o) => {
            front::user_click(s, *o);
        }
        Step::Click(o) | Step::Traced(o, _) => {
            if let Step::Traced(o, tl) = st {
                if !s.r.active.contains(o) && !s.r.w.hotspots.contains(o) {
                    s.r.external_register(*o, *tl);
                    s.tick(0);
                }
            }
            match front::click_point(s, *o) {
                Some((x, y)) => {
                    s.hover_at(x, y);
                    s.click_at(x, y);
                }
                None => {
                    s.r.click(*o);
                }
            }
        }
    }
}

/// The SDL audio callback over the shared software mixer (front::host::Mix).
mod mixer {
    use std::sync::{Arc, Mutex};
    use xf::front::host::Mix;

    pub struct Out(pub Arc<Mutex<Mix>>);

    impl sdl2::audio::AudioCallback for Out {
        type Channel = i16;
        fn callback(&mut self, out: &mut [i16]) {
            if let Ok(mut m) = self.0.lock() {
                m.fill(out);
            }
        }
    }
}

/// The window's picture output: OpenGL upscaler (FSR / linear / nearest) or, when GL is not available, the SDL renderer.
trait Present {
    fn window(&mut self) -> &mut sdl2::video::Window;
    /// Show one 640×480 RGB frame.
    fn show(&mut self, fb: &[u8]);
    /// A mouse position from an SDL event → a 640×480 picture pixel; `None` on the black bars.
    fn to_picture(&self, x: i32, y: i32) -> Option<(i32, i32)>;
    /// The scaling mode (settings `scale`: fsr / linear / nearest); returns the mode in use.
    fn set_scale(&mut self, name: &str) -> &'static str;
}

/// OpenGL 3.3 core context + [`xf::gpu::gl::Upscaler`]: the picture scaled to the drawable size (high-DPI pixels).
struct GlPresent {
    up: xf::gpu::gl::Upscaler,
    mode: xf::gpu::gl::Scale,
    _ctx: sdl2::video::GLContext,
    win: sdl2::video::Window,
}

impl Present for GlPresent {
    fn window(&mut self) -> &mut sdl2::video::Window {
        &mut self.win
    }
    fn show(&mut self, fb: &[u8]) {
        self.up.upload(fb);
        self.up.draw(0, self.win.drawable_size(), self.mode);
        self.win.gl_swap_window();
    }
    fn to_picture(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let (w, h) = (front::SCREEN_W as u32, front::SCREEN_H as u32);
        xf::gpu::window_to_picture(x, y, self.win.size(), self.win.drawable_size(), w, h)
    }
    fn set_scale(&mut self, name: &str) -> &'static str {
        self.mode = xf::gpu::gl::Scale::parse(name).unwrap_or(self.mode);
        self.mode.name()
    }
}

/// Fallback: the SDL renderer with a logical size of 640×480 (SDL scales linearly and maps the mouse).
struct SdlPresent<'a> {
    canvas: sdl2::render::Canvas<sdl2::video::Window>,
    tex: sdl2::render::Texture<'a>,
}

impl Present for SdlPresent<'_> {
    fn window(&mut self) -> &mut sdl2::video::Window {
        self.canvas.window_mut()
    }
    fn show(&mut self, fb: &[u8]) {
        if self.tex.update(None, fb, front::SCREEN_W * 3).is_ok() {
            self.canvas.clear();
            let _ = self.canvas.copy(&self.tex, None, None);
        }
        self.canvas.present();
    }
    fn to_picture(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        ((0..front::SCREEN_W as i32).contains(&x) && (0..front::SCREEN_H as i32).contains(&y)).then_some((x, y))
    }
    fn set_scale(&mut self, _: &str) -> &'static str {
        "linear (SDL renderer fallback: no other modes)"
    }
}

/// Window mode: the SDL window with an OpenGL 3.3 core context and the upscaler; if GL fails, the SDL renderer.
fn window(s: &mut Session, a: &Args) {
    use sdl2::pixels::PixelFormatEnum;
    use xf::front::settings::{config_path, Settings};

    // H1750: the settings file, then the CLI flags over it
    let path = if a.no_config { None } else { a.config.clone().or_else(config_path) };
    let file = match &path {
        Some(p) => {
            let (f, warn) = Settings::load(p, Settings::window_default());
            for w in warn {
                eprintln!("xf: {}: {w}", p.display());
            }
            eprintln!("xf: settings {} (F10)", p.display());
            f
        }
        None => Settings::window_default(),
    };
    let set = cli_overrides(file.clone(), a);
    let mut st = WinSettings { file, cur: set, path, cli_subs: a.cli_settings.iter().any(|c| c.0 == "subs") };

    // H550: live play draws RandGen values (the phone's ring, H202) like the original, seeded from the wall clock
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u32);
    s.r.w.live_rand = Some(ms);

    let sdl = sdl2::init().unwrap_or_else(|e| die(&e));
    let video = sdl.video().unwrap_or_else(|e| die(&e));
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 3);
    gl_attr.set_context_flags().forward_compatible().set();
    gl_attr.set_double_buffer(true);
    // resizable window at 2× by default; the 640×480 picture keeps 4:3 with black bars; F11 = full screen
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "1");
    let zoom: u32 = st.cur.get("window").parse().unwrap_or(2);
    let build = |opengl: bool| {
        let mut b = video.window("Shiloh", front::SCREEN_W as u32 * zoom, front::SCREEN_H as u32 * zoom);
        b.position_centered().resizable().allow_highdpi();
        if opengl {
            b.opengl();
        }
        b.build().map_err(|e| e.to_string())
    };
    let gl = build(true).and_then(|win| {
        let ctx = win.gl_create_context()?;
        win.gl_make_current(&ctx)?;
        xf::gpu::gl::load(|n| video.gl_get_proc_address(n) as *const _);
        let _ = video.gl_set_swap_interval(1);
        let mut up = xf::gpu::gl::Upscaler::new(front::SCREEN_W as u32, front::SCREEN_H as u32)?;
        up.sharpness = a.sharpness;
        let mode = xf::gpu::gl::Scale::parse(st.cur.get("scale")).unwrap_or(a.scale);
        Ok(GlPresent { up, mode, _ctx: ctx, win })
    });
    match gl {
        Ok(mut p) => {
            eprintln!("xf: OpenGL upscaler, scale {} (F2 cycles fsr / linear / nearest), RCAS sharpness {} stops",
                p.mode.name(), a.sharpness);
            run_window(s, &sdl, &mut p, a, &mut st);
        }
        Err(e) => {
            eprintln!("xf: OpenGL upscaler unavailable ({e}); falling back to the SDL renderer (linear scaling)");
            let win = build(false).unwrap_or_else(|e| die(&e));
            let mut canvas = win.into_canvas().present_vsync().build().unwrap_or_else(|e| die(&e.to_string()));
            let _ = canvas.set_logical_size(front::SCREEN_W as u32, front::SCREEN_H as u32);
            let tc = canvas.texture_creator();
            let tex = tc
                .create_texture_streaming(PixelFormatEnum::RGB24, front::SCREEN_W as u32, front::SCREEN_H as u32)
                .unwrap_or_else(|e| die(&e.to_string()));
            run_window(s, &sdl, &mut SdlPresent { canvas, tex }, a, &mut st);
        }
    }
}

/// The window's settings (H1750): `file` = what the settings file holds, `cur` = in effect (the file + the CLI flags).
struct WinSettings {
    file: xf::front::settings::Settings,
    cur: xf::front::settings::Settings,
    path: Option<PathBuf>,
    /// `--subs` was given: its pack (loaded at start, fatal when missing) stands until the screen changes it
    cli_subs: bool,
}

impl WinSettings {
    /// The option `key` changed in `cur` (the screen or a hotkey): keep it in the file.
    fn keep(&mut self, key: &str) {
        self.file.copy_key(&self.cur, key);
        if let Some(p) = &self.path {
            if let Err(e) = self.file.save(p) {
                eprintln!("xf: settings {}: {e}", p.display());
            }
        }
    }
}

/// Apply a changed setting (H1750–H1755) to the window or the session, then keep it.
fn setting_changed(key: &str, st: &mut WinSettings, s: &mut Session, out: &mut dyn Present) {
    use sdl2::video::FullscreenType;
    match key {
        "scale" => eprintln!("scale: {}", out.set_scale(st.cur.get("scale"))),
        "fullscreen" => {
            let next = if st.cur.on("fullscreen") { FullscreenType::Desktop } else { FullscreenType::Off };
            let _ = out.window().set_fullscreen(next);
        }
        "window" => {
            let z: u32 = st.cur.get("window").parse().unwrap_or(2);
            let w = out.window();
            if w.fullscreen_state() == FullscreenType::Off {
                let _ = w.set_size(front::SCREEN_W as u32 * z, front::SCREEN_H as u32 * z);
            }
        }
        "volume" | "pause_hidden" => {}
        // H956: picking a language also switches the captions on (as the web's select)
        "subs" => {
            if let Some(l) = st.cur.apply_key("subs", s) {
                eprintln!("{l}");
            }
            if xf::media::subs::is_lang(st.cur.get("subs")) && s.sub_pack.is_some() {
                s.r.set_closed_captioning(true);
            }
        }
        k => {
            if let Some(l) = st.cur.apply_key(k, s) {
                eprintln!("{l}");
            }
        }
    }
    st.keep(key);
}

/// The host's part of an Alt hotkey (H2060): the settings follow (volume, captions, hotspots — kept like the settings
/// screen's changes); true = exit (Alt+X).
fn alt_out(r: front::altkeys::AltOut, st: &mut WinSettings, s: &mut Session, out: &mut dyn Present) -> bool {
    use front::altkeys::AltOut;
    let onoff = |b: bool| if b { "on" } else { "off" };
    let (key, val) = match r {
        AltOut::Exit => return true,
        AltOut::Volume(v) => ("volume", v.to_string()),
        AltOut::Captions(on) => ("captions", onoff(on).to_string()),
        AltOut::Hotspots(on) => ("overlay", onoff(on).to_string()),
        _ => return false,
    };
    let _ = st.cur.set(key, &val);
    setting_changed(key, st, s, out);
    false
}

fn run_window(s: &mut Session, sdl: &sdl2::Sdl, out: &mut dyn Present, a: &Args, st: &mut WinSettings) {
    let (quit_after, load) = (a.quit_after, a.load.is_some());
    use sdl2::event::Event;
    use sdl2::keyboard::Keycode;
    use sdl2::mouse::{Cursor, MouseButton, SystemCursor};

    let arrow = Cursor::from_system(SystemCursor::Arrow).ok();
    let hand = Cursor::from_system(SystemCursor::Hand).ok();
    let mut events = sdl.event_pump().unwrap_or_else(|e| die(&e));
    // sound: the session's audio cues mixed in software; no audio device (e.g. SDL_AUDIODRIVER=dummy fails) → silent
    let mix = std::sync::Arc::new(std::sync::Mutex::new(front::host::Mix { volume: st.cur.volume() as f32 / 100.0, ..Default::default() }));
    let _audio = sdl.audio().ok().and_then(|a| {
        let want = sdl2::audio::AudioSpecDesired { freq: Some(44100), channels: Some(2), samples: Some(1024) };
        let dev = a
            .open_playback(None, &want, |spec| {
                if let Ok(mut m) = mix.lock() {
                    m.rate = spec.freq as u32;
                }
                mixer::Out(mix.clone())
            })
            .map_err(|e| eprintln!("xf: no sound: {e}"))
            .ok()?;
        dev.resume();
        Some(dev)
    });
    // H1794: the picture waits for the sound's output latency (the obtained buffer; the settings' "Picture delay" or
    // `--av-delay MS` override it, applied in the loop)
    let lat = _audio.as_ref().map_or(0.0, |d| xf::front::avsync::sdl_latency_ms(d.spec().samples as u32, d.spec().freq as u32));
    let auto_delay = xf::front::avsync::picture_delay_ms(lat);
    s.av_delay = a.av_delay.unwrap_or_else(|| st.cur.av_delay().unwrap_or(auto_delay));
    if _audio.is_some() {
        eprintln!("xf: sound latency ~{lat:.0} ms, picture delay {} ms (settings / --av-delay MS)", s.av_delay);
    }
    s.game_cursor = true; // H307
    // H2100: the host's Preferences the settings file keeps (written after a Save; the original's registry)
    if let Some(p) = st.cur.host_prefs() {
        s.host_prefs = p;
    }
    if load {
        s.load_game();
    } else {
        // H1660 / H1661: as the original, the intro movies, then the main menu (`--skip-intro`: the menu at once,
        // `--new-game`: straight into New Game)
        s.boot(a.start.unwrap_or(xf::front::intro::Start::Intro));
    }
    // H1750: the game-side settings (the subtitle pack only when no --subs gave it; captions only when forced); H2305:
    // a language the data has not is not kept from the file (the data's own / auto instead)
    st.cur.fit(s, &xf::media::subs::packs_in(xf::media::pcmcache::dir().as_deref()), st.cli_subs);
    for k in ["game_lang", "long_timers", "overlay"].into_iter().chain((!st.cli_subs).then_some("subs")) {
        if let Some(l) = st.cur.apply_key(k, s) {
            eprintln!("xf: {l}");
        }
    }
    let captions = st.cur.captions();
    if let Some(on) = captions {
        s.r.set_closed_captioning(on);
    }
    if st.cur.on("fullscreen") {
        let _ = out.window().set_fullscreen(sdl2::video::FullscreenType::Desktop);
    }
    let mut ui = front::settings::SettingsUi::default();
    let mut frame = vec![0u8; front::SCREEN_W * front::SCREEN_H * 3];
    let mut menu_hover = false;
    let mut last = Instant::now();
    let mut clock = xf::front::avsync::WallClock::default();
    let mut logged = (0usize, 0usize);
    let mut title_view = None;
    let start = Instant::now();
    let mut shown_lines: Vec<(u32, [i16; 4], String)> = Vec::new();
    let mut minimised = false;
    'main: loop {
        if quit_after.is_some_and(|q| start.elapsed().as_millis() as u64 >= q) {
            eprintln!("quit after {} ms: view {:x?}, transitions {}", start.elapsed().as_millis(), s.r.w.view, s.r.transitions.len());
            break;
        }
        for e in events.poll_iter() {
            use sdl2::event::WindowEvent;
            // the settings screen (H1751) takes every input while it shows
            if ui.open {
                match e {
                    Event::Quit { .. } => break 'main,
                    Event::KeyDown { keycode: Some(Keycode::Escape | Keycode::F10), .. } => ui.open = false,
                    Event::MouseButtonDown { mouse_btn: MouseButton::Left, x, y, .. } => {
                        let p = out.to_picture(x, y).unwrap_or((-1, -1));
                        match ui.click(p.0, p.1, &mut st.cur) {
                            Some(front::settings::UiClick::Changed(k)) => setting_changed(k, st, s, out),
                            _ => {}
                        }
                    }
                    Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } => ui.release(),
                    Event::MouseMotion { x, y, .. } => {
                        let p = out.to_picture(x, y).unwrap_or((-1, -1));
                        if let Some(front::settings::UiClick::Changed(k)) = ui.motion(p.0, p.1, &mut st.cur) {
                            setting_changed(k, st, s, out);
                        }
                        sdl.mouse().show_cursor(true);
                        if let Some(c) = if ui.clickable(p.0, p.1) { &hand } else { &arrow } {
                            c.set();
                        }
                    }
                    _ => {}
                }
                continue;
            }
            match e {
                Event::Quit { .. } => break 'main,
                // H1732 / H1753: pause when minimised (setting pause_hidden, `--pause-when-minimised`) — minimised = no ticks
                // (the game clock and its timers stop), no drawing, the sound device paused; restored = the clock goes on
                // from then (no catch-up)
                Event::Window { win_event: WindowEvent::Minimized, .. } if st.cur.on("pause_hidden") => {
                    minimised = true;
                    if let Some(d) = &_audio {
                        d.pause();
                    }
                    eprintln!("minimised: paused");
                }
                Event::Window { win_event: WindowEvent::Restored | WindowEvent::Maximized | WindowEvent::Shown, .. }
                    if minimised =>
                {
                    minimised = false;
                    last = Instant::now();
                    if let Some(d) = &_audio {
                        d.resume();
                    }
                    eprintln!("restored: running");
                }
                // F10 = the port's settings screen (H1751)
                Event::KeyDown { keycode: Some(Keycode::F10), .. } => {
                    ui = front::settings::SettingsUi::opened(s);
                    sdl.mouse().show_cursor(true);
                }
                // H1751: the "Port settings" button on the game's menu screen (under Quit)
                Event::MouseButtonDown { mouse_btn: MouseButton::Left, x, y, .. }
                    if s.r.menu_open && out.to_picture(x, y).is_some_and(|(x, y)| front::settings::menu_button_at(x, y)) =>
                {
                    ui = front::settings::SettingsUi::opened(s);
                }
                // Esc = the game's menu (H169 / H404); on the save screens the slot item's key (H403)
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => eprintln!("{}", front::host::key(s, 0x1b)),
                // H2060 (H1868): Alt + a letter / digit = the original's Alt hotkeys (WM_SYSKEYDOWN, FUN_0046c390) — never a
                // key press to the game; H1660: Alt+X during the intro ends the program (FUN_00408cf0)
                Event::KeyDown { keycode: Some(k), keymod, repeat: false, .. }
                    if keymod.intersects(sdl2::keyboard::Mod::LALTMOD | sdl2::keyboard::Mod::RALTMOD)
                        && front::host::vk(&k.name()).is_some_and(|v| (0x30..=0x5a).contains(&v)) =>
                {
                    let vk = front::host::vk(&k.name()).unwrap_or(0);
                    let shift = keymod.intersects(sdl2::keyboard::Mod::LSHIFTMOD | sdl2::keyboard::Mod::RSHIFTMOD);
                    let r = front::host::alt(s, vk, shift);
                    if r != front::altkeys::AltOut::None {
                        eprintln!("{}", r.describe());
                    }
                    if alt_out(r, st, s, out) {
                        break 'main;
                    }
                }
                // Alt held (the Alt hotkeys' auto-repeat, Alt + other keys): nothing reaches the game
                Event::KeyDown { keycode: Some(k), keymod, .. }
                    if keymod.intersects(sdl2::keyboard::Mod::LALTMOD | sdl2::keyboard::Mod::RALTMOD)
                        && front::host::vk(&k.name()).is_some() => {}
                Event::KeyDown { keycode: Some(Keycode::Space | Keycode::Return), .. } if s.r.playing().is_some() => {
                    front::host::end_playing(s);
                }
                Event::KeyDown { keycode: Some(Keycode::F11), .. } => {
                    let full = out.window().fullscreen_state() != sdl2::video::FullscreenType::Off;
                    let _ = st.cur.set("fullscreen", if full { "off" } else { "on" });
                    setting_changed("fullscreen", st, s, out);
                }
                // F4 = the next game language (H1836: RU ↔ EN with --en-data; nothing with one language, H2305) — the
                // settings follow (H1750)
                Event::KeyDown { keycode: Some(Keycode::F4), .. } => {
                    if let Some(next) = s.next_game_lang() {
                        let _ = st.cur.set("game_lang", next);
                        setting_changed("game_lang", st, s, out);
                    }
                }
                // F2 = scaling mode (fsr → linear → nearest)
                Event::KeyDown { keycode: Some(Keycode::F2), .. } => {
                    st.cur.step("scale", 1);
                    setting_changed("scale", st, s, out);
                }
                // F3 = closed captions on / off (H343: the original's 'C' key, FUN_0046c390 case 0x43)
                Event::KeyDown { keycode: Some(Keycode::F3), .. } => {
                    let on = !s.r.w.closed_captioning();
                    let _ = st.cur.set("captions", if on { "on" } else { "off" });
                    setting_changed("captions", st, s, out);
                }
                // F1 = overlay (O is a letter the game may need)
                Event::KeyDown { keycode: Some(Keycode::F1), .. } => {
                    st.cur.step("overlay", 1);
                    setting_changed("overlay", st, s, out);
                }
                // H174: other keys go to the focused InterfaceItem as Windows virtual-key codes
                Event::KeyDown { keycode: Some(k), .. } => {
                    if let Some(code) = front::host::vk(&k.name()) {
                        eprintln!("{}", front::host::key(s, code));
                    }
                }
                // right button while a video plays skips it, a left click is ignored (H322): front::host::button
                Event::MouseButtonDown { mouse_btn: b @ (MouseButton::Left | MouseButton::Right), x, y, .. } => {
                    let b = if b == MouseButton::Left { front::host::Button::Left } else { front::host::Button::Right };
                    if let Some(l) = front::host::button(s, b, out.to_picture(x, y)) {
                        eprintln!("{l}");
                    }
                }
                // C107: the left button's up — a dragged IdeaIcon is dropped there (its answer plays on the icon)
                Event::MouseButtonUp { mouse_btn: b @ (MouseButton::Left | MouseButton::Right), x, y, .. } => {
                    let b = if b == MouseButton::Left { front::host::Button::Left } else { front::host::Button::Right };
                    if let Some(l) = front::host::release(s, b, out.to_picture(x, y)) {
                        eprintln!("{l}");
                    }
                }
                // H1815 (port choice): the wheel scrolls the dialogue box / the computer's text under the pointer
                Event::MouseWheel { y, direction, mouse_x, mouse_y, .. } => {
                    let y = if direction == sdl2::mouse::MouseWheelDirection::Flipped { -y } else { y };
                    if let Some(l) = front::host::wheel(s, out.to_picture(mouse_x, mouse_y), -y) {
                        eprintln!("{l}");
                    }
                }
                Event::MouseMotion { x, y, .. } => {
                    // H307 / H331: the game's own cursor (or its Win32 crosshair) is drawn into the frame wherever the
                    // pointer is over the picture; the system one only outside it
                    let at = out.to_picture(x, y);
                    menu_hover = s.r.menu_open && at.is_some_and(|(x, y)| front::settings::menu_button_at(x, y));
                    let p = if menu_hover { front::host::Pointer::Hand } else { front::host::motion(s, at) };
                    sdl.mouse().show_cursor(p != front::host::Pointer::Hidden);
                    if let Some(c) = if p == front::host::Pointer::Hand { &hand } else { &arrow } {
                        c.set();
                    }
                }
                _ => {}
            }
        }
        if minimised {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        // H1790: whole ms with the remainder carried (as_millis() per iteration lost ~4 % of the time)
        let now = Instant::now();
        let dt = clock.step(now.duration_since(last), 250);
        last = now;
        // H1753: the game stops while the settings screen shows (minimised: H1732 above)
        let paused = ui.open;
        if !paused {
            s.tick(dt);
        }
        // H1794: the picture delay — `--av-delay MS`, else the settings' choice ("auto": from the audio buffer)
        s.av_delay = a.av_delay.unwrap_or_else(|| st.cur.av_delay().unwrap_or(auto_delay));
        // H2100 / H2084: a Save wrote the running game's Preferences to the host's — kept in the settings file
        if std::mem::take(&mut s.prefs_saved) {
            st.cur.set_host_prefs(&s.host_prefs);
            st.keep(xf::front::settings::HOST_PREFS);
        }
        if host_request(s, &a.data, st.cur.captions()) {
            break 'main;
        }
        let cues = if paused { Vec::new() } else { s.audio_cues() };
        if let Ok(mut m) = mix.lock() {
            m.sync(&cues);
            // H406: the game's Volume preference scales the settings' volume
            m.volume = st.cur.volume() as f32 / 100.0 * front::host::volume(s);
        }
        if ui.open {
            out.show(&ui.draw(&frame, &st.cur));
        } else {
            frame = s.render();
            let mut fb = frame.clone();
            if s.r.menu_open {
                front::settings::draw_menu_button(&mut fb, menu_hover);
            }
            out.show(&fb);
        }
        if title_view != s.r.w.view {
            title_view = s.r.w.view;
            let _ = out.window().set_title(&format!("Shiloh — View {:#x}", title_view.unwrap_or(0)));
        }
        // the dialogue box is not drawn yet: list its lines (text + click point) in the terminal when they change
        let lines = s.r.w.dbox_line_texts();
        if lines != shown_lines {
            for (i, (c, r, t)) in lines.iter().enumerate() {
                eprintln!("dialogue line {}: {:#x} click at ({}, {}): {t}", i + 1, c, (r[0] + r[2]) / 2, (r[1] + r[3]) / 2);
            }
            shown_lines = lines;
        }
        // the core's log restarts after a Load / New (H403 / H404)
        if s.r.log.len() < logged.0 {
            logged.0 = 0;
        }
        for l in s.r.log.iter().skip(logged.0).chain(s.log.iter().skip(logged.1)) {
            eprintln!("  {l}");
        }
        // robustness: printed lines are dropped and the core's histories bounded (a long session grew without end)
        s.r.log.clear();
        s.log.clear();
        logged = (0, 0);
        s.r.trim_history(100_000);
        std::thread::sleep(Duration::from_millis(10));
    }
}

