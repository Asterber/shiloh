//! The player's settings (port feature, not the original's: H1750–H1754) — one model for both fronts.
//!
//! [`OPTIONS`] lists every player-facing option (key, label, choices, hotkey hint, which fronts show it); [`Settings`]
//! holds their values, reads / writes them by key ([`Settings::get`] / [`Settings::set`]) and as a small text file
//! (`key = value` lines, [`Settings::to_text`] / [`Settings::parse`]). The window (`bin/xf.rs`) keeps them in
//! [`config_path`] and draws [`SettingsUi`] (F10, or the "Port settings" button on the game's menu screen,
//! [`MENU_BUTTON`]); the web front (`port/web`) shows the same keys in an HTML dialog (`www/index.html`, one
//! `data-key` per web option — a test checks they match) and sends each change through `xf_set` → [`Settings::set`].
//! Options that act on the game ([`Settings::apply_key`]) go to the [`Session`]; the others (scale, volume, full
//! screen, window size, pause) are the host's.
//!
//! HYPOTHESIS H1750 (port choice): settings are host state — never part of a save, never read in headless runs (`xf
//! --frames N --dump DIR` stays deterministic); CLI flags override the file for this run without being written back.
use super::{fill, rect, Session, SCREEN_H, SCREEN_W};
use std::path::{Path, PathBuf};

/// The subtitle languages a pack can exist for (`xf-prep --en-subs` → `en`, `xf-prep --ru-subs` → `ru`; H955 / H956,
/// `media::subs::LANGS`).
pub const SUB_LANGS: &[(&str, &str)] = &[("en", "English"), ("ru", "Russian")];

/// How an option's value is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// one of (value, label)
    List(&'static [(&'static str, &'static str)]),
    /// 0..=100 percent
    Percent,
}

/// Which fronts show an option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fronts {
    Both,
    Window,
    Web,
    /// the window's settings file (and its command line) only — not a row of the menu (the panel is full)
    WindowFile,
}

/// One player-facing option.
#[derive(Clone, Copy, Debug)]
pub struct Opt {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// the hotkey that changes it too (shown as a hint)
    pub hint: &'static str,
    pub fronts: Fronts,
    /// the choices the web front offers when they differ (the browser scales only pixelated / smooth)
    pub web_kind: Option<Kind>,
    /// one line of help
    pub help: &'static str,
}

const ON_OFF: &[(&str, &str)] = &[("on", "on"), ("off", "off")];

/// Every option, in menu order.
pub const OPTIONS: &[Opt] = &[
    Opt {
        key: "scale",
        label: "Scaling",
        kind: Kind::List(&[("fsr", "smooth (FSR)"), ("linear", "smooth (linear)"), ("nearest", "pixelated")]),
        hint: "F2",
        fronts: Fronts::Both,
        web_kind: Some(Kind::List(&[("nearest", "pixelated"), ("linear", "smooth")])),
        help: "how the 640x480 picture is enlarged",
    },
    Opt {
        key: "window",
        label: "Window size",
        kind: Kind::List(&[("1", "1x (640x480)"), ("2", "2x (1280x960)"), ("3", "3x (1920x1440)")]),
        hint: "",
        fronts: Fronts::Window,
        web_kind: None,
        help: "the window's size at start (it stays resizable)",
    },
    Opt {
        key: "fullscreen",
        label: "Full screen",
        kind: Kind::List(ON_OFF),
        hint: "F11",
        fronts: Fronts::Both,
        web_kind: None,
        help: "the picture fills the screen",
    },
    Opt {
        key: "volume",
        label: "Volume",
        kind: Kind::Percent,
        hint: "Alt+0-9",
        fronts: Fronts::Both,
        web_kind: None,
        help: "the port's output volume (times the game's own Volume preference)",
    },
    Opt {
        key: "av_delay",
        label: "Picture delay",
        kind: Kind::List(&[
            ("auto", "auto"),
            ("0", "0 ms"),
            ("20", "20 ms"),
            ("50", "50 ms"),
            ("100", "100 ms"),
            ("150", "150 ms"),
            ("200", "200 ms"),
            ("250", "250 ms"),
        ]),
        hint: "",
        fronts: Fronts::WindowFile,
        web_kind: None,
        help: "movie pictures wait this long for their sound (auto: the audio buffer; more for Bluetooth, H1794)",
    },
    Opt {
        key: "captions",
        label: "Subtitles",
        kind: Kind::List(&[("game", "as the game"), ("on", "on"), ("off", "off")]),
        hint: "F3",
        fronts: Fronts::Both,
        web_kind: None,
        help: "the game's closed captions (its own preference when 'as the game')",
    },
    Opt {
        key: "subs",
        label: "Subtitle language",
        kind: Kind::List(&[("auto", "auto"), ("en", "English"), ("ru", "Russian"), ("off", "off (the game's own)")]),
        hint: "",
        fronts: Fronts::Both,
        web_kind: None,
        help: "a subtitle pack made by xf-prep (auto: Russian, else English); not shown without one or over English game data (its own captions)",
    },
    Opt {
        key: "game_lang",
        label: "Game language",
        kind: Kind::List(&[("ru", "Russian (RU 1.00)"), ("en", "English (EN DVD)")]),
        hint: "",
        fronts: Fronts::Both,
        web_kind: None,
        help: "voices and the game's own text (Session::set_game_lang; English needs --en-data DIR, H1836)",
    },
    Opt {
        key: "long_timers",
        label: "Long timers",
        kind: Kind::List(&[("wine", "Wine / 9x"), ("nt", "Windows NT")]),
        hint: "",
        fronts: Fronts::Both,
        web_kind: None,
        help: "NT: the Gordon's Hauling bomb goes off 120 s after the manifest",
    },
    Opt {
        key: "overlay",
        label: "Show hotspots",
        kind: Kind::List(ON_OFF),
        hint: "F1",
        fronts: Fronts::Both,
        web_kind: None,
        help: "outline everything clickable",
    },
    Opt {
        key: "pause_hidden",
        label: "Pause when hidden",
        kind: Kind::List(ON_OFF),
        hint: "",
        fronts: Fronts::Both,
        web_kind: None,
        help: "stop the game while the window is minimised / the tab is hidden",
    },
];

/// The option of a key.
pub fn opt(key: &str) -> Option<&'static Opt> {
    OPTIONS.iter().find(|o| o.key == key)
}

/// The options a front shows (`web`: the web front's, else the window's).
pub fn options(web: bool) -> impl Iterator<Item = &'static Opt> {
    OPTIONS.iter().filter(move |o| match o.fronts {
        Fronts::Both => true,
        Fronts::Window => !web,
        Fronts::Web => web,
        Fronts::WindowFile => false,
    })
}

/// H2100: the settings key that keeps the host's Preferences (not an option of the screen; written after a Save).
pub const HOST_PREFS: &str = "host_prefs";

/// H2100: `var:value` pairs, the variable in hex, the value decimal (`369:1 3d5:72 …`).
pub fn host_prefs_text(prefs: &[(u32, i32)]) -> String {
    prefs.iter().map(|(v, x)| format!("{v:x}:{x}")).collect::<Vec<_>>().join(" ")
}

/// H2100: [`host_prefs_text`] read back; only RegPreferences variables ([`crate::world::PREF_VARS`]).
pub fn parse_host_prefs(text: &str) -> Result<Vec<(u32, i32)>, String> {
    text.split_whitespace()
        .map(|p| {
            let bad = || format!("{HOST_PREFS}: {p}? (var:value, var in hex)");
            let (v, x) = p.split_once(':').ok_or_else(bad)?;
            let v = u32::from_str_radix(v.trim_start_matches("0x"), 16).map_err(|_| bad())?;
            let x: i32 = x.parse().map_err(|_| bad())?;
            crate::world::PREF_VARS.contains(&v).then_some((v, x)).ok_or_else(bad)
        })
        .collect()
}

/// The settings' values (strings for lists, 0..=100 for the volume), defaults = the port's behaviour without a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    vals: Vec<(&'static str, String)>,
}

impl Default for Settings {
    fn default() -> Settings {
        let d = |k: &'static str, v: &str| (k, v.to_string());
        Settings {
            vals: vec![
                d("scale", "fsr"),
                d("window", "2"),
                d("fullscreen", "off"),
                d("volume", "100"),
                d("av_delay", "auto"),
                d("captions", "game"),
                d("subs", "auto"),
                d("game_lang", "ru"),
                d("long_timers", "wine"),
                d("overlay", "off"),
                d("pause_hidden", "on"),
            ],
        }
    }
}

impl Settings {
    /// The value of `key` ("" for an unknown key).
    pub fn get(&self, key: &str) -> &str {
        self.vals.iter().find(|(k, _)| *k == key).map_or("", |(_, v)| v.as_str())
    }

    /// An on / off option is on.
    pub fn on(&self, key: &str) -> bool {
        self.get(key) == "on"
    }

    pub fn volume(&self) -> u32 {
        self.get("volume").parse().unwrap_or(100)
    }

    /// H1794: the window's picture delay in ms; None = auto (from the audio buffer, front::avsync).
    pub fn av_delay(&self) -> Option<u64> {
        self.get("av_delay").parse().ok()
    }

    /// Set `key` to `val` (checked against the option's choices; the volume is clamped to 0..=100).
    pub fn set(&mut self, key: &str, val: &str) -> Result<(), String> {
        if key == HOST_PREFS {
            parse_host_prefs(val)?;
            self.vals.retain(|(k, _)| *k != HOST_PREFS);
            if !val.trim().is_empty() {
                self.vals.push((HOST_PREFS, val.trim().to_string()));
            }
            return Ok(());
        }
        let o = opt(key).ok_or_else(|| format!("unknown setting {key}"))?;
        let v = match o.kind {
            Kind::List(ch) => {
                // the old web names (pixelated / smooth) are kept readable
                let val = match (key, val) {
                    ("scale", "pixelated") => "nearest",
                    ("scale", "smooth") => "linear",
                    ("long_timers", "9x" | "bottle") => "wine",
                    _ => val,
                };
                ch.iter().find(|c| c.0 == val).ok_or_else(|| {
                    format!("{key}: {val}? ({})", ch.iter().map(|c| c.0).collect::<Vec<_>>().join(" | "))
                })?;
                val.to_string()
            }
            Kind::Percent => {
                let n: f32 = val.trim().trim_end_matches('%').parse().map_err(|_| format!("{key}: {val}? (0..100)"))?;
                (n.round().clamp(0.0, 100.0) as u32).to_string()
            }
        };
        if let Some(slot) = self.vals.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = v;
        }
        Ok(())
    }

    /// The next (`dir` = 1) or previous (−1) choice of `key`; the volume moves by 5. Returns the new value.
    pub fn step(&mut self, key: &str, dir: i32) -> String {
        self.step_in(key, dir, None)
    }

    /// [`Settings::step`] among the values `only` (H2305: the ones the session's data has; None = every value).
    pub fn step_in(&mut self, key: &str, dir: i32, only: Option<&[&str]>) -> String {
        let Some(o) = opt(key) else { return String::new() };
        let v = match o.kind {
            Kind::List(ch) => {
                let ch: Vec<_> = ch.iter().filter(|c| only.is_none_or(|v| v.contains(&c.0))).collect();
                if ch.is_empty() {
                    return self.get(key).to_string();
                }
                let i = ch.iter().position(|c| c.0 == self.get(key)).unwrap_or(0) as i32;
                ch[(i + dir).rem_euclid(ch.len() as i32) as usize].0.to_string()
            }
            Kind::Percent => {
                let n = self.volume() as i32;
                // steps of 5; an odd value (e.g. 1 %) goes to the next multiple of 5
                let n = if dir > 0 { (n / 5 + 1) * 5 } else { ((n + 4) / 5 - 1) * 5 };
                n.clamp(0, 100).to_string()
            }
        };
        let _ = self.set(key, &v);
        v
    }

    /// The label of the current value.
    pub fn label(&self, key: &str) -> String {
        let v = self.get(key);
        match opt(key).map(|o| o.kind) {
            Some(Kind::List(ch)) => ch.iter().find(|c| c.0 == v).map_or(v.to_string(), |c| c.1.to_string()),
            _ => format!("{v} %"),
        }
    }

    /// Copy one key's value from `other`.
    pub fn copy_key(&mut self, other: &Settings, key: &str) {
        let v = other.get(key).to_string();
        let _ = self.set(key, &v);
    }

    /// `key = value` lines (# comments, unknown keys and bad values are skipped — returned as warnings).
    pub fn parse(text: &str) -> (Settings, Vec<String>) {
        Settings::parse_over(Settings::default(), text)
    }

    /// [`Settings::parse`] over `base` (the defaults of a front, e.g. [`Settings::window_default`]).
    pub fn parse_over(base: Settings, text: &str) -> (Settings, Vec<String>) {
        let mut s = base;
        let mut warn = Vec::new();
        for l in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            match l.split_once('=') {
                Some((k, v)) => {
                    if let Err(e) = s.set(k.trim(), v.trim()) {
                        warn.push(e);
                    }
                }
                None => warn.push(format!("not key = value: {l}")),
            }
        }
        (s, warn)
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("# The X-Files port: player settings (the in-game settings screen writes this file)\n");
        for (k, v) in &self.vals {
            out += &format!("{k} = {v}\n");
        }
        out
    }

    /// The window's defaults: as [`Settings::default`] but `pause_hidden` off (H1732: a minimised window runs on unless
    /// asked; the web pauses a hidden tab by default, H1681).
    pub fn window_default() -> Settings {
        let mut s = Settings::default();
        let _ = s.set("pause_hidden", "off");
        s
    }

    /// Read a settings file over `base` (missing = `base`).
    pub fn load(path: &Path, base: Settings) -> (Settings, Vec<String>) {
        match std::fs::read_to_string(path) {
            Ok(t) => Settings::parse_over(base, &t),
            Err(_) => (base, Vec::new()),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, self.to_text())
    }

    /// H2100: the host's Preferences kept in the file (the original's registry; written after a Save), None = not yet.
    pub fn host_prefs(&self) -> Option<Vec<(u32, i32)>> {
        parse_host_prefs(self.get(HOST_PREFS)).ok().filter(|p| !p.is_empty())
    }

    /// H2100: keep the session's host Preferences (after a Save wrote them, `Session::prefs_saved`).
    pub fn set_host_prefs(&mut self, prefs: &[(u32, i32)]) {
        let _ = self.set(HOST_PREFS, &host_prefs_text(prefs));
    }

    /// The captions value to force after New Game / Load: None = the game's own preference.
    pub fn captions(&self) -> Option<bool> {
        match self.get("captions") {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        }
    }

    pub fn long_timers(&self) -> crate::world::LongTimers {
        crate::world::LongTimers::parse(self.get("long_timers")).unwrap_or_default()
    }

    /// Apply a game-side option to the session (captions, subtitle pack, long timers, overlay); other keys are the
    /// host's (None). Returns a log line.
    pub fn apply_key(&self, key: &str, s: &mut Session) -> Option<String> {
        match key {
            "captions" => {
                let on = self.captions().unwrap_or_else(|| s.r.w.closed_captioning());
                s.r.set_closed_captioning(on);
                Some(format!("captions {}", if on { "on" } else { "off" }))
            }
            "subs" => Some(match load_sub_pack(self.get("subs"), s) {
                Ok(m) => m,
                Err(e) => format!("subtitles: {e}"),
            }),
            // H1755: the game's data set (voices + texts) — the engine hook; Err while only the installed set exists
            "game_lang" => Some(match s.set_game_lang(self.get("game_lang")) {
                Ok(()) => format!("game language {}", self.get("game_lang")),
                Err(e) => format!("game language: {e}"),
            }),
            "long_timers" => {
                s.r.w.rules.set_long_timers(self.long_timers());
                Some(format!("long timers {}", self.get("long_timers")))
            }
            // H2100: the kept host Preferences (the registry) — the session's, and the running core's variables
            HOST_PREFS => {
                let p = self.host_prefs()?;
                s.host_prefs = p;
                s.apply_host_prefs();
                Some(format!("host preferences ({} values)", s.host_prefs.len()))
            }
            "overlay" => {
                s.overlay = self.on("overlay");
                Some(format!("overlay {}", self.get("overlay")))
            }
            _ => None,
        }
    }

    /// H2305: a value the session's data cannot take is replaced (not kept in the file): the game language by the
    /// session's, the subtitle language by `auto` (unless `keep_subs`: `--subs` asked for it by name).
    pub fn fit(&mut self, s: &Session, packs: &[&str], keep_subs: bool) {
        for (k, ok) in choices(s, packs) {
            if (k == "subs" && keep_subs) || ok.contains(&self.get(k)) {
                continue;
            }
            let v = if k == "game_lang" { s.game_lang.code() } else { "auto" };
            let _ = self.set(k, v);
        }
    }

    /// Apply every game-side option (at start; the captions only when forced).
    pub fn apply_game(&self, s: &mut Session) -> Vec<String> {
        let mut log = Vec::new();
        for k in ["game_lang", "subs", "long_timers", "overlay"] {
            log.extend(self.apply_key(k, s));
        }
        if self.captions().is_some() {
            log.extend(self.apply_key("captions", s));
        }
        log
    }

    /// Take the session's current state of the hotkey-driven game options (F1 overlay, F3 captions) after a hotkey.
    pub fn follow(&mut self, s: &Session) {
        let _ = self.set("overlay", if s.overlay { "on" } else { "off" });
        let _ = self.set("captions", if s.r.w.closed_captioning() { "on" } else { "off" });
    }
}

/// Load the subtitle pack of `lang` (`off` = none) from the PCM cache dir into the session (H1754: the captions read
/// the pack at every frame, so a switch shows at the next cue, mid-movie too). Err when the pack is missing (then
/// none is used).
pub fn load_sub_pack(lang: &str, s: &mut Session) -> Result<String, String> {
    use crate::media::{pcmcache, subs};
    if lang == "off" {
        s.sub_pack = None;
        return Ok("subtitle pack off".into());
    }
    let cache = pcmcache::dir().ok_or("no cache dir (pass --cache DIR or set XF_CACHE)")?;
    // H956: auto = ru, else en, else none
    match subs::Pack::pick_for(&cache, lang, s.data_lang) {
        Ok(Some(pack)) => {
            let m = format!("{} subtitles for {} movies", pack.lang, pack.movies.len());
            s.sub_pack = Some(std::sync::Arc::new(pack));
            Ok(m)
        }
        Ok(None) => {
            s.sub_pack = None;
            Ok("no subtitle pack".into())
        }
        Err(e) => {
            s.sub_pack = None;
            Err(format!("{e} (make it once with xf-prep, see port/README.md)"))
        }
    }
}

/// The settings file: `XF_CONFIG`, else the user's config dir + `xf-port/settings.txt`.
pub fn config_path() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(p) = env("XF_CONFIG") {
        return Some(p);
    }
    let dir = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    };
    dir.map(|d| d.join("xf-port").join("settings.txt"))
}

// ---- the window's settings screen (drawn by the port over the paused game)

/// H1751 (port choice): the "Port settings" button on the game's menu screen, under Quit (the original's menu has
/// nothing there: hit rects end at y 421, menu.rs ITEMS) — x1 y1 x2 y2.
pub const MENU_BUTTON: [i32; 4] = [488, 440, 634, 466];

pub fn menu_button_at(x: i32, y: i32) -> bool {
    let r = MENU_BUTTON;
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

/// Draw the menu screen's "Port settings" button (hovered: brighter).
pub fn draw_menu_button(fb: &mut [u8], hover: bool) {
    let r = MENU_BUTTON;
    let c = if hover { [150, 200, 230] } else { [70, 110, 140] };
    rect(fb, r[0], r[1], r[2] - 1, r[3] - 1, c);
    label(fb, r[0] + 8, r[1] + 6, "Port settings  F10", c);
}

/// H2060: one line of the Alt hotkeys under the options (front::altkeys)
pub const ALT_HELP: &str = "Alt: 0-9 volume, A/M/R/T/V prefs, C captions, P pause, S save, X exit; Shift+Alt: F fps, H spots, I state";

const PANEL: [i32; 4] = [36, 16, 604, 464];
const ROW0: i32 = 52;
const ROW_H: i32 = 20;
const VAL_X: i32 = 250;
const VAL_W: i32 = 210;
const HINT_X: i32 = 540;
const CLOSE: [i32; 4] = [520, 24, 596, 44];
/// the scale preview: a crop of the frame, 3× nearest | 3× linear
const PREVIEW_Y: i32 = 306;
const CROP: (usize, usize) = (80, 50);
const ZOOM: usize = 3;

const BG: [u8; 3] = [10, 16, 20];
const LINE: [u8; 3] = [40, 70, 90];
const FG: [u8; 3] = [200, 215, 220];
const DIM: [u8; 3] = [110, 130, 140];
const ACCENT: [u8; 3] = [120, 200, 240];

/// What a click on the settings screen did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiClick {
    /// the option `key` changed
    Changed(&'static str),
    Close,
}

/// A row's hit areas: (key, row rect, "<" rect, ">" rect, value rect).
type Row = (&'static Opt, [i32; 4], [i32; 4], [i32; 4], [i32; 4]);

/// The screen's rows: every window option but the `hidden` ones.
fn rows_without(hidden: &[&str]) -> Vec<Row> {
    options(false)
        .filter(|o| !hidden.contains(&o.key))
        .enumerate()
        .map(|(i, o)| {
            let y = ROW0 + i as i32 * ROW_H;
            (
                o,
                [PANEL[0] + 8, y, PANEL[2] - 8, y + ROW_H],
                [VAL_X - 22, y + 1, VAL_X - 4, y + ROW_H - 2],
                [VAL_X + VAL_W + 4, y + 1, VAL_X + VAL_W + 22, y + ROW_H - 2],
                [VAL_X, y + 1, VAL_X + VAL_W, y + ROW_H - 2],
            )
        })
        .collect()
}

fn inside(r: [i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

#[cfg(test)]
fn rows() -> Vec<Row> {
    rows_without(&[])
}

/// H2305: the values the session's data offers for the options that depend on it — the game languages
/// ([`Session::game_langs`]) and the subtitle choices ([`Session::sub_langs`] of the cache's `packs`). A language without
/// data is not offered; an option with a single value is not shown (the window's screen, the web dialog).
pub fn choices(s: &Session, packs: &[&str]) -> Vec<(&'static str, Vec<&'static str>)> {
    vec![("game_lang", s.game_langs()), ("subs", s.sub_langs(packs))]
}

/// The window's settings screen: open / hovered row; the host owns the [`Settings`].
#[derive(Clone, Debug, Default)]
pub struct SettingsUi {
    pub open: bool,
    /// the pointer when the screen opened: the scale preview crops around it
    pub focus: Option<(i32, i32)>,
    /// H2305: the values offered per data-dependent option ([`choices`]); one value = the row is not shown
    pub choices: Vec<(&'static str, Vec<&'static str>)>,
    pub hover: Option<(i32, i32)>,
    /// the volume bar is being dragged
    drag: bool,
}

impl SettingsUi {
    /// The screen opened over the session `s` (only the choices its data has, [`choices`]).
    pub fn opened(s: &Session) -> SettingsUi {
        SettingsUi { open: true, choices: choices(s, &crate::media::subs::packs_in(crate::media::pcmcache::dir().as_deref())), focus: s.mouse, ..Default::default() }
    }

    /// The values offered for `key` (None = every value of the option).
    fn only(&self, key: &str) -> Option<&[&'static str]> {
        self.choices.iter().find(|c| c.0 == key).map(|c| c.1.as_slice())
    }

    /// The rows shown: an option with a single offered value is left out.
    fn rows(&self) -> Vec<Row> {
        let hidden: Vec<&str> = self.choices.iter().filter(|c| c.1.len() < 2).map(|c| c.0).collect();
        rows_without(&hidden)
    }

    /// Something clickable is under (x, y) (the host shows a hand pointer).
    pub fn clickable(&self, x: i32, y: i32) -> bool {
        inside(CLOSE, x, y)
            || self.rows().iter().any(|r| inside(r.2, x, y) || inside(r.3, x, y) || inside(r.4, x, y))
    }

    /// The pointer moved (a held volume bar follows it).
    pub fn motion(&mut self, x: i32, y: i32, set: &mut Settings) -> Option<UiClick> {
        self.hover = Some((x, y));
        if self.drag {
            let r = self.rows().into_iter().find(|r| r.0.kind == Kind::Percent)?;
            let _ = set.set(r.0.key, &bar_value(r.4, x).to_string());
            return Some(UiClick::Changed(r.0.key));
        }
        None
    }

    pub fn release(&mut self) {
        self.drag = false;
    }

    /// A left click at (x, y): "<" / ">" step the option, a click on the value steps forward (the volume bar takes the
    /// point), Close / outside the panel closes.
    pub fn click(&mut self, x: i32, y: i32, set: &mut Settings) -> Option<UiClick> {
        if inside(CLOSE, x, y) || !inside(PANEL, x, y) {
            self.open = false;
            return Some(UiClick::Close);
        }
        for (o, _, prev, next, val) in self.rows() {
            let dir = if inside(prev, x, y) {
                -1
            } else if inside(next, x, y) {
                1
            } else if inside(val, x, y) {
                if o.kind == Kind::Percent {
                    self.drag = true;
                    let _ = set.set(o.key, &bar_value(val, x).to_string());
                    return Some(UiClick::Changed(o.key));
                }
                1
            } else {
                continue;
            };
            set.step_in(o.key, dir, self.only(o.key));
            return Some(UiClick::Changed(o.key));
        }
        None
    }

    /// The screen: `frame` (the paused game) dimmed, the panel with every window option, its hotkey hint, and the scale
    /// preview (H1752: a crop of `frame` around the pointer's last point, 3× pixelated next to 3× smooth).
    pub fn draw(&self, frame: &[u8], set: &Settings) -> Vec<u8> {
        let mut fb: Vec<u8> = frame.iter().map(|&c| c / 4).collect();
        fill(&mut fb, PANEL[0], PANEL[1], PANEL[2], PANEL[3], BG);
        rect(&mut fb, PANEL[0], PANEL[1], PANEL[2] - 1, PANEL[3] - 1, LINE);
        label(&mut fb, PANEL[0] + 16, PANEL[1] + 10, "PORT SETTINGS", ACCENT);
        label(&mut fb, PANEL[0] + 150, PANEL[1] + 10, "F10 / Esc: close, changes are saved", DIM);
        let hc = self.hover.is_some_and(|(x, y)| inside(CLOSE, x, y));
        rect(&mut fb, CLOSE[0], CLOSE[1], CLOSE[2] - 1, CLOSE[3] - 1, if hc { ACCENT } else { LINE });
        label(&mut fb, CLOSE[0] + 20, CLOSE[1] + 4, "Close", if hc { ACCENT } else { FG });
        let mut help: Option<String> = None;
        let rows = self.rows();
        for &(o, row, prev, next, val) in &rows {
            let hot = self.hover.is_some_and(|(x, y)| inside(row, x, y));
            if hot {
                fill(&mut fb, row[0], row[1], row[2], row[3], [18, 30, 38]);
                help = Some(o.help.to_string());
            }
            label(&mut fb, row[0] + 8, row[1] + 3, o.label, if hot { FG } else { DIM });
            for (r, t) in [(prev, "<"), (next, ">")] {
                let h = self.hover.is_some_and(|(x, y)| inside(r, x, y));
                rect(&mut fb, r[0], r[1], r[2] - 1, r[3] - 1, if h { ACCENT } else { LINE });
                label(&mut fb, r[0] + 5, r[1] + 2, t, if h { ACCENT } else { FG });
            }
            if o.kind == Kind::Percent {
                let v = set.volume() as i32;
                let w = (val[2] - val[0] - 50) * v / 100;
                rect(&mut fb, val[0], val[1] + 4, val[2] - 51, val[3] - 5, LINE);
                fill(&mut fb, val[0] + 1, val[1] + 5, val[0] + 1 + w.max(0), val[3] - 5, ACCENT);
                label(&mut fb, val[2] - 40, row[1] + 3, &set.label(o.key), FG);
            } else {
                label(&mut fb, val[0] + 6, row[1] + 3, &set.label(o.key), FG);
            }
            if !o.hint.is_empty() {
                label(&mut fb, HINT_X, row[1] + 3, o.hint, DIM);
            }
        }
        let ly = ROW0 + rows.len() as i32 * ROW_H + 6;
        label(&mut fb, PANEL[0] + 16, ly, help.as_deref().unwrap_or("Hotkeys at the right also work in the game."), DIM);
        // H2060: the original's Alt hotkeys (front::altkeys)
        label(&mut fb, PANEL[0] + 16, ly + 14, ALT_HELP, DIM);
        // the scale preview
        let (cw, ch) = CROP;
        let (zw, zh) = (cw * ZOOM, ch * ZOOM);
        let gap = 24;
        let x0 = (SCREEN_W as i32 - (2 * zw as i32 + gap)) / 2;
        label(&mut fb, x0, PREVIEW_Y - 18, "pixelated", if set.get("scale") == "nearest" { ACCENT } else { DIM });
        label(&mut fb, x0 + zw as i32 + gap, PREVIEW_Y - 18, "smooth", if set.get("scale") != "nearest" { ACCENT } else { DIM });
        let (cx, cy) = crop_origin(self.focus);
        let crop = crop_rgb(frame, cx, cy, cw, ch);
        put(&mut fb, x0, PREVIEW_Y, &zoom_nearest(&crop, cw, ch, ZOOM), zw, zh);
        put(&mut fb, x0 + zw as i32 + gap, PREVIEW_Y, &zoom_linear(&crop, cw, ch, ZOOM), zw, zh);
        rect(&mut fb, x0 - 1, PREVIEW_Y - 1, x0 + zw as i32, PREVIEW_Y + zh as i32, LINE);
        rect(&mut fb, x0 + zw as i32 + gap - 1, PREVIEW_Y - 1, x0 + 2 * zw as i32 + gap, PREVIEW_Y + zh as i32, LINE);
        fb
    }
}

/// The volume bar's value at x.
fn bar_value(val: [i32; 4], x: i32) -> u32 {
    let w = (val[2] - val[0] - 50).max(1);
    ((x - val[0]) * 100 / w).clamp(0, 100) as u32
}

/// The crop's top-left: around `at` (a picture point), else the picture's centre.
fn crop_origin(at: Option<(i32, i32)>) -> (usize, usize) {
    let (x, y) = at.unwrap_or((SCREEN_W as i32 / 2, SCREEN_H as i32 / 2));
    let cx = (x - CROP.0 as i32 / 2).clamp(0, (SCREEN_W - CROP.0) as i32) as usize;
    let cy = (y - CROP.1 as i32 / 2).clamp(0, (SCREEN_H - CROP.1) as i32) as usize;
    (cx, cy)
}

fn crop_rgb(frame: &[u8], x: usize, y: usize, w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 3);
    for row in y..y + h {
        let o = (row * SCREEN_W + x) * 3;
        out.extend_from_slice(&frame[o..o + w * 3]);
    }
    out
}

/// Enlarge `px` (w×h RGB) `z` times, each pixel a z×z block.
pub fn zoom_nearest(px: &[u8], w: usize, h: usize, z: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * z * h * z * 3];
    for y in 0..h * z {
        for x in 0..w * z {
            let s = ((y / z) * w + x / z) * 3;
            let d = (y * w * z + x) * 3;
            out[d..d + 3].copy_from_slice(&px[s..s + 3]);
        }
    }
    out
}

/// Enlarge `px` (w×h RGB) `z` times with bilinear filtering (pixel centres, edges clamped — as GL_LINEAR).
pub fn zoom_linear(px: &[u8], w: usize, h: usize, z: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * z * h * z * 3];
    let at = |x: usize, y: usize, c: usize| px[(y * w + x) * 3 + c] as f32;
    for y in 0..h * z {
        let fy = ((y as f32 + 0.5) / z as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(h - 1);
        for x in 0..w * z {
            let fx = ((x as f32 + 0.5) / z as f32 - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(w - 1);
            for c in 0..3 {
                let top = at(x0, y0, c) * (1.0 - tx) + at(x1, y0, c) * tx;
                let bot = at(x0, y1, c) * (1.0 - tx) + at(x1, y1, c) * tx;
                out[(y * w * z + x) * 3 + c] = (top * (1.0 - ty) + bot * ty).round() as u8;
            }
        }
    }
    out
}

fn put(fb: &mut [u8], x: i32, y: i32, px: &[u8], w: usize, h: usize) {
    super::menu::put(fb, x, y, px, w, h)
}

/// Text in Arial 12 (the tiny overlay font when the system has no Arial).
pub(super) fn label(fb: &mut [u8], x: i32, y: i32, s: &str, c: [u8; 3]) {
    match super::text::arial12() {
        Some(f) => {
            f.draw(fb, x, y, s, c);
        }
        None => {
            super::font::text(fb, x, y + 1, s, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        let mut s = Settings::default();
        s.set("scale", "nearest").unwrap();
        s.set("volume", "37.4").unwrap();
        s.set("subs", "ru").unwrap();
        s.set("game_lang", "en").unwrap();
        let (t, w) = Settings::parse(&s.to_text());
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(t, s);
        assert_eq!(t.volume(), 37);
        // unknown keys and bad values are warnings, the rest is kept
        let (t, w) = Settings::parse("scale = pixelated\nfoo = 1\nvolume = loud\nlong_timers=nt\n");
        assert_eq!((t.get("scale"), t.get("long_timers"), t.volume(), w.len()), ("nearest", "nt", 100, 2));
    }

    #[test]
    fn steps() {
        let mut s = Settings::default();
        assert_eq!(s.step("volume", 1), "100");
        assert_eq!(s.step("volume", -1), "95");
        s.set("volume", "1").unwrap();
        assert_eq!(s.step("volume", 1), "5");
        assert_eq!(s.step("volume", -1), "0");
        assert_eq!(s.step("volume", -1), "0");
        assert_eq!(s.step("scale", 1), "linear");
        assert_eq!(s.step("scale", -1), "fsr");
        assert_eq!(s.step("scale", -1), "nearest");
        assert_eq!(s.step("overlay", 1), "on");
        assert!(s.set("subs", "xx").is_err());
    }

    #[test]
    fn defaults_cover_every_option() {
        let d = Settings::default();
        for o in OPTIONS {
            let v = d.get(o.key);
            match o.kind {
                Kind::List(ch) => assert!(ch.iter().any(|c| c.0 == v), "{} = {v}", o.key),
                Kind::Percent => assert!(v.parse::<u32>().is_ok()),
            }
            // the web choices are a subset
            if let Some(Kind::List(w)) = o.web_kind {
                let Kind::List(ch) = o.kind else { panic!() };
                assert!(w.iter().all(|c| ch.iter().any(|d| d.0 == c.0)));
            }
        }
        assert_eq!(d.vals.len(), OPTIONS.len());
        // every subtitle language is a choice
        let Kind::List(ch) = opt("subs").unwrap().kind else { panic!() };
        assert!(SUB_LANGS.iter().all(|l| ch.iter().any(|c| c.0 == l.0)));
        assert!(crate::media::subs::LANGS.iter().all(|l| SUB_LANGS.iter().any(|s| s.0 == *l)));
        assert_eq!(Settings::window_default().get("pause_hidden"), "off");
    }

    #[test]
    fn web_dialog_has_every_web_option() {
        // the web dialog (www/index.html) shows each web option with data-key="<key>" (one model, two views)
        let html = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/web/www/index.html")).unwrap();
        for o in options(true) {
            assert!(html.contains(&format!("data-key=\"{}\"", o.key)), "index.html lacks data-key=\"{}\"", o.key);
        }
    }

    #[test]
    fn ui_clicks() {
        let mut ui = SettingsUi { open: true, ..Default::default() };
        let mut s = Settings::default();
        let r = rows();
        let (o, _, prev, next, _) = r.iter().find(|r| r.0.key == "scale").unwrap();
        assert_eq!(o.key, "scale");
        assert_eq!(ui.click(next[0] + 2, next[1] + 2, &mut s), Some(UiClick::Changed("scale")));
        assert_eq!(s.get("scale"), "linear");
        ui.click(prev[0] + 2, prev[1] + 2, &mut s);
        assert_eq!(s.get("scale"), "fsr");
        let (_, _, _, _, val) = r.iter().find(|r| r.0.key == "volume").unwrap();
        ui.click(val[0], val[1] + 4, &mut s);
        assert_eq!(s.volume(), 0);
        ui.motion(val[2] - 50, val[1], &mut s);
        assert_eq!(s.volume(), 100);
        ui.release();
        ui.motion(val[0], val[1], &mut s);
        assert_eq!(s.volume(), 100);
        // H2305: an option with one offered value is not shown; the others step among the offered values only
        ui.choices = vec![("game_lang", vec!["en"]), ("subs", vec!["auto", "off"])];
        assert!(ui.rows().iter().all(|r| r.0.key != "game_lang"));
        let (_, _, _, next, _) = ui.rows().into_iter().find(|r| r.0.key == "subs").unwrap();
        assert_eq!(ui.click(next[0] + 2, next[1] + 2, &mut s), Some(UiClick::Changed("subs")));
        assert_eq!(s.get("subs"), "off");
        ui.click(next[0] + 2, next[1] + 2, &mut s);
        assert_eq!(s.get("subs"), "auto");
        ui.choices.clear();
        assert_eq!(ui.click(5, 5, &mut s), Some(UiClick::Close));
        assert!(!ui.open);
        // the rows fit above the preview
        assert!(ROW0 + r.len() as i32 * ROW_H + 26 < PREVIEW_Y - 18);
    }

    #[test]
    fn zooms() {
        let px = [0u8, 0, 0, 255, 255, 255];
        let n = zoom_nearest(&px, 2, 1, 2);
        assert_eq!(n.len(), 4 * 2 * 3);
        assert_eq!((n[0], n[3], n[6], n[9]), (0, 0, 255, 255));
        let l = zoom_linear(&px, 2, 1, 4);
        // left edge clamped black, right edge white, a ramp between
        assert_eq!((l[0], l[7 * 3]), (0, 255));
        assert!(l[3 * 3] > 0 && l[3 * 3] < 255);
    }
}
