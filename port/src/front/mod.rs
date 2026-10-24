//! Front-end layer without a window: a [`Session`] = core runner + media clock + frame composition (640×480 RGB24)
//! + hit-testing. The SDL binary (`bin/xf.rs`) and the headless smoke use the same code. Nothing here decides game
//! logic: what is shown is the runner's background / playing asset / clickables, input goes to `Runner::click`.
//!
//! Display assumptions (not traced, see RE_NOTES §8 "Media"): the 600×240 movie is drawn at (20, 90); region rects are
//! (x1, y1, x2, y2) screen pixels.

pub mod ai;
pub mod altkeys;
pub mod avsync;
pub mod computer;
pub mod cursor;
pub mod doc;
pub mod font;
pub mod host;
pub mod intro;
pub mod rewind;
pub mod light;
pub mod menu;
pub mod pda;
pub mod script;
pub mod settings;
pub mod text;
pub mod ui;

use crate::hdb::Hdb;
use crate::media::audio::Pcm;
use crate::media::pict::{Pff, Picture};
use crate::media::captions::{Caption, Cue as SubCue};
use crate::media::{Clip, MediaLib, MoviePlayer, FRAME_H, FRAME_W};
use crate::world::run::{Clickable, Runner};
use crate::world::Rules;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SCREEN_W: usize = 640;
pub const SCREEN_H: usize = 480;
/// top-left of the movie area (hypothesis: centred horizontally in the 640-wide view band y 90..330)
pub const VIEW_X: usize = 20;
pub const VIEW_Y: usize = 90;
/// media whose length cannot be read (no file, not a QuickTime movie): assumed duration
pub const DEFAULT_MEDIA_MS: u64 = 3000;
/// H1561: how long a played still (a navN.nmv frame) shows — the player's 0xbb8 timer
pub const STILL_MS: u64 = 3000;
/// H1813 (port choice: the original's repeat rate of a held box arrow is not decoded — FUN_004823a0 runs from a
/// vtable slot whose caller is not found): the first repeat after 250 ms, then every 100 ms — the HDB's own held
/// computer arrows' timers (H1814: 0xfa then 0x64 ms, e.g. trigger 0xd99e / 0x103a1)
pub const SCROLL_REPEAT_FIRST_MS: u64 = 250;
pub const SCROLL_REPEAT_MS: u64 = 100;
/// byte budget of the decoded-sound cache ([`Session::sound`])
pub const SOUND_CACHE_BYTES: usize = 192 << 20;

pub struct Session<'a> {
    pub r: Runner<'a>,
    /// C107: what the left button went down on ([`Session::press_at`]), for its button-up
    pressed: Option<Clickable>,
    /// H1813: a dialogue-box scroll arrow held down (its clickable kind) and the session time of its next repeat
    scroll_hold: Option<(u32, u64)>,
    pub lib: MediaLib,
    /// virtual clock, ms
    pub now: u64,
    /// (asset, start time) of the media playing: A3 the head of the player; H280 also the sounds started alongside it
    heads: Vec<(u32, u64)>,
    /// H1601: a head's media position at its start time (a restored player resumes at its saved position), ms
    head_off: HashMap<u32, u64>,
    /// (asset, start time) of the background shown
    bg: Option<(u32, u64)>,
    /// H100: (asset, restart time) of looping players past their first media end ([`Runner::looping`]), played again
    loops: Vec<(u32, u64)>,
    clips: HashMap<u32, Option<Clip>>,
    players: HashMap<PathBuf, Option<MoviePlayer>>,
    pffs: HashMap<PathBuf, Option<Pff>>,
    pictures: HashMap<u32, Option<Picture>>,
    /// C306: camera shots' pictures by (video AssetRef, f68, rect)
    shot_pictures: HashMap<(u32, u32, [i16; 4]), Option<Picture>>,
    sounds: HashMap<PathBuf, Option<Arc<Pcm>>>,
    /// decode order of `sounds` (oldest first), for the cache's byte budget ([`SOUND_CACHE_BYTES`])
    sound_order: std::collections::VecDeque<PathBuf>,
    /// byte budget of the decoded-sound cache (default [`SOUND_CACHE_BYTES`])
    pub sound_budget: usize,
    pub log: Vec<String>,
    /// mouse position for hover highlight
    pub mouse: Option<(i32, i32)>,
    pub overlay: bool,
    /// draw the game's cursor (H307) into the frame at `mouse` (the window turns it on and hides the system cursor)
    pub game_cursor: bool,
    /// display state of the UI layers between frames (front::ui)
    pub ui: ui::UiState,
    /// the cursor shown and its frames (front::cursor, H330)
    pub cursor: cursor::CursorState,
    /// H344: user subtitles (`--subtitles-dir`): `<dir>/<movie path>.srt` or a movie of the same path with text tracks
    pub subs: Option<MediaLib>,
    srts: HashMap<PathBuf, Option<Vec<SubCue>>>,
    /// C86 (`Rules::c86_new_game_host_prefs`): the host's Preferences (the original's registry) — written into the
    /// RegPreferences variables at the cold start ([`Session::boot`]) and at every New Game ([`Session::reload`]); taken
    /// back from the running core's variables at every New / Load (the game's Options write them, H189; a Load's
    /// RegPreferences writes the save's values to the Preferences, H81). Starts as C86's registry values.
    pub host_prefs: Vec<(u32, i32)>,
    /// H2100: a Save wrote `host_prefs` (the front persists them — the original's registry — and clears this)
    pub prefs_saved: bool,
    /// H955: a subtitle pack (`xf-prep --en-subs`, `xf --subs en`): cues per movie file, over the movie's own text tracks
    pub sub_pack: Option<std::sync::Arc<crate::media::subs::Pack>>,
    /// port viewer (`--play ASSET`): an asset shown in the movie area from (asset, start time), whatever the core plays
    pub preview: Option<(u32, u64)>,
    /// H404: the menu screen's display state (front::menu)
    pub menu: menu::MenuUi,
    /// H402: the save slots (directory + names); None = saving not possible
    pub saves: Option<crate::save::Slots>,
    /// H401: a save of the original used as the base when the session's gam is not a save (XFiles.gam)
    pub template: Option<PathBuf>,
    /// H403: a Load chosen on the load screen — the host switches the session to this file ([`Session::reload`])
    pub load_req: Option<PathBuf>,
    /// H1660: the intro movies of the cold start play (front::intro); None = over / not started
    pub intro: Option<intro::Intro>,
    /// H1700: when the "Try again?" box opened (session clock; its movie's time), None = not shown
    pub rewind_t0: Option<u64>,
    /// H1836 (port feature): the game language — whose voice-over / movies / texts play ([`Session::set_game_lang`])
    pub game_lang: crate::media::lang::GameLang,
    /// H1836: the English data set (`--en-data DIR`), None = only RU
    pub en_data: Option<Arc<crate::media::lang::EnData>>,
    /// H2305: the language of the game data itself (`--data`; [`crate::media::lang::data_lang`]): RU 1.00 → Ru (an
    /// English data set can be laid over it), the EN DVD Edition → En (the only language then)
    pub data_lang: crate::media::lang::GameLang,
    /// H1836: the language each started media slot (asset, start time) plays in — a switch takes effect on the next
    /// movie / line, a playing one finishes in its language
    slot_lang: HashMap<(u32, u64), crate::media::lang::GameLang>,
    /// H1790–H1795: the picture's delay behind the session clock, ms — the front's audio latency minus its display
    /// latency (front::avsync), so a movie's frame is shown when its sound is heard. 0 = headless (no outputs).
    pub av_delay: u64,
    /// H2060–H2068: the Alt hotkeys' display state (pause, state box, FPS counter; front::altkeys)
    pub alt: altkeys::AltState,
}

/// A copy of the session for planners that branch from one runtime state (the private research planner's `search`):
/// the core ([`Runner`]), the clock and every playback position are copied; the decoded-media caches (movie players,
/// PFF files, pictures) start empty in the copy and are filled again on demand — they are pure functions of the media
/// files.
impl<'a> Clone for Session<'a> {
    fn clone(&self) -> Self {
        Session {
            r: self.r.clone(),
            lib: self.lib.clone(),
            now: self.now,
            heads: self.heads.clone(),
            head_off: self.head_off.clone(),
            pressed: self.pressed,
            scroll_hold: self.scroll_hold,
            bg: self.bg,
            loops: self.loops.clone(),
            clips: self.clips.clone(),
            players: HashMap::new(),
            pffs: HashMap::new(),
            pictures: HashMap::new(),
            shot_pictures: HashMap::new(),
            sounds: self.sounds.clone(),
            sound_order: self.sound_order.clone(),
            sound_budget: self.sound_budget,
            log: self.log.clone(),
            mouse: self.mouse,
            overlay: self.overlay,
            game_cursor: self.game_cursor,
            ui: self.ui.clone(),
            cursor: self.cursor.clone(),
            subs: self.subs.clone(),
            srts: self.srts.clone(),
            host_prefs: self.host_prefs.clone(),
            prefs_saved: self.prefs_saved,
            sub_pack: self.sub_pack.clone(),
            preview: self.preview,
            menu: self.menu.clone(),
            saves: self.saves.clone(),
            template: self.template.clone(),
            load_req: self.load_req.clone(),
            av_delay: self.av_delay,
            intro: self.intro.clone(),
            rewind_t0: self.rewind_t0,
            game_lang: self.game_lang,
            en_data: self.en_data.clone(),
            data_lang: self.data_lang,
            slot_lang: self.slot_lang.clone(),
            alt: self.alt.clone(),
        }
    }
}

/// Rules the front-end runs with: confirmed rules + H48/H53 (confirmed by fo-009, C27).
/// H150: size (w, h) of every AssetRef of an icon's f30 list (classes 0x47..0x4b) — PFF picture or movie frame size; the
/// runner places an icon by the size of the picture it shows.
pub fn icon_asset_sizes(h: &Hdb, lib: &MediaLib) -> HashMap<u32, (i16, i16)> {
    let mut out = HashMap::new();
    let mut pffs: HashMap<std::path::PathBuf, Option<crate::media::pict::Pff>> = HashMap::new();
    for cls in 0x47u32..=0x4b {
        let Some(c) = h.class(cls) else { continue };
        for &(id, _) in &c.objects {
            let Some(r) = h.read(cls, id) else { continue };
            for a in h.list_of(crate::world::class::ASSET_REF_LIST, r.u32("f30")) {
                if out.contains_key(&a) {
                    continue;
                }
                let Some(clip) = lib.resolve(h, a) else { continue };
                let size = if clip.ext() == "pff" {
                    let p = pffs.entry(clip.file.clone()).or_insert_with(|| crate::media::pict::Pff::open(&clip.file).ok());
                    p.as_ref().and_then(|p| p.picture(clip.start as usize).ok()).map(|p| (p.width, p.height))
                } else {
                    MoviePlayer::open(&clip.file).ok().map(|m| (m.width, m.height))
                };
                if let Some((w, hh)) = size {
                    out.insert(a, (w as i16, hh as i16));
                }
            }
        }
    }
    out
}

pub fn rules() -> Rules {
    Rules { h48_play_first_enabled: true, h53_background: true, ..Rules::default() }
}

/// Every hypothesis switch on: also H55 (Explorables), H58 (type 5 / IFaceLayout), H59 (type-2 timer), H60 (PreloadEmIcon)
/// — the core then creates the Explorables, the drawer's InterfaceItems, the EmotionIcons and Timer Expiration itself.
pub fn rules_hyp() -> Rules {
    Rules::all()
}

impl<'a> Session<'a> {
    pub fn new(h: &'a Hdb, gam: &'a Hdb, data: &Path) -> Session<'a> {
        Session::with_rules(h, gam, data, rules())
    }

    pub fn with_rules(h: &'a Hdb, gam: &'a Hdb, data: &Path, rules: Rules) -> Session<'a> {
        let mut r = Runner::new(h, gam, rules);
        if rules.h150_icon_bars {
            r.asset_sizes = icon_asset_sizes(h, &MediaLib::new(data));
        }
        let data_lang = crate::media::lang::data_lang(h, data);
        Session {
            r,
            lib: MediaLib::new(data),
            now: 0,
            heads: Vec::new(),
            head_off: HashMap::new(),
            pressed: None,
            scroll_hold: None,
            bg: None,
            loops: Vec::new(),
            clips: HashMap::new(),
            players: HashMap::new(),
            pffs: HashMap::new(),
            pictures: HashMap::new(),
            shot_pictures: HashMap::new(),
            sounds: HashMap::new(),
            sound_order: Default::default(),
            sound_budget: SOUND_CACHE_BYTES,
            log: Vec::new(),
            mouse: None,
            overlay: false,
            game_cursor: false,
            ui: ui::UiState::default(),
            cursor: cursor::CursorState::default(),
            subs: None,
            srts: HashMap::new(),
            host_prefs: crate::world::C86_HOST_PREFS.to_vec(),
            prefs_saved: false,
            sub_pack: None,
            preview: None,
            menu: Default::default(),
            saves: None,
            template: None,
            load_req: None,
            intro: None,
            av_delay: 0,
            rewind_t0: None,
            game_lang: data_lang,
            en_data: None,
            data_lang,
            slot_lang: HashMap::new(),
            alt: Default::default(),
        }
    }

    /// H1836: hand the session an English data set ([`crate::media::lang::EnData::open`]); None also switches back to
    /// the game data's language. Ignored when the game data is English itself (H2305).
    pub fn set_en_data(&mut self, en: Option<Arc<crate::media::lang::EnData>>) {
        if self.data_lang == crate::media::lang::GameLang::En {
            if en.is_some() {
                self.log.push("H2305: the game data is English already: the English data set is not used".into());
            }
            return;
        }
        self.en_data = en;
        let l = if self.en_data.is_some() { self.game_lang } else { self.data_lang };
        let _ = self.set_lang(l);
    }

    /// H1836 (port feature, the settings' "game language"): switch the voice-over and the game's own texts, live. The
    /// core's logic runs on as before (the RU HDB, the RU media lengths); string-resource texts (dialogue lines, notes,
    /// names) change at once, movies / sounds from their next start. Err (language unchanged) for English without an
    /// English data set. The game data's own language (H2305) swaps nothing.
    pub fn set_lang(&mut self, lang: crate::media::lang::GameLang) -> Result<(), String> {
        use crate::media::lang::GameLang;
        let strings = match (lang, &self.en_data) {
            (l, _) if l == self.data_lang => None,
            (GameLang::En, Some(en)) => Some(en.strings.clone()),
            (GameLang::En, None) => return Err("no English data set (xf --en-data DIR)".into()),
            (l, _) => {
                return Err(format!("there is no {} data set (the game data is {})", l.code(), self.data_lang.code()))
            }
        };
        if self.game_lang != lang {
            self.log.push(format!("H1836: game language {}", lang.code()));
        }
        self.game_lang = lang;
        self.r.w.lang_strings = strings;
        Ok(())
    }

    /// H1836: the file a clip plays from in `lang` — the English data set's file of the same name for a language
    /// resource ([`crate::media::lang::is_lang_key`]), else the RU file.
    pub fn lang_file(&self, c: &Clip, lang: crate::media::lang::GameLang) -> PathBuf {
        if lang == crate::media::lang::GameLang::En && self.data_lang != lang {
            if let Some(p) = self.en_data.as_ref().and_then(|en| en.index.lang_path(&c.name)) {
                return p.clone();
            }
        }
        c.file.clone()
    }

    /// H2306: the language (code page) of the files [`Session::lang_file`] gives for `lang`: the English data set's
    /// when it is laid over and `lang` is English, else the game data's own
    pub fn file_lang(&self, lang: crate::media::lang::GameLang) -> crate::media::lang::GameLang {
        if lang == crate::media::lang::GameLang::En && self.en_data.is_some() {
            lang
        } else {
            self.data_lang
        }
    }

    /// H1836: the language of the media slot (asset, start time): the game language when the slot was first seen.
    fn slot_lang(&mut self, a: u32, t0: u64) -> crate::media::lang::GameLang {
        if self.slot_lang.len() > 256 {
            let live: Vec<(u32, u64)> = self.media_slots().iter().map(|&(a, t, _)| (a, t)).collect();
            self.slot_lang.retain(|k, _| live.contains(k));
        }
        let l = self.game_lang;
        *self.slot_lang.entry((a, t0)).or_insert(l)
    }

    /// Start over with `gam` (a save: `load`; else a new game from it): a new core with the same rules and inputs; the
    /// media state is dropped, the clock and the display settings stay (H403 Load, menu New).
    pub fn reload(&mut self, gam: &'a Hdb, load: bool) {
        let prefs = self.r.w.rules.c86_new_game_host_prefs;
        if prefs {
            self.host_prefs = self.r.w.pref_values();
        }
        self.reload_core(gam);
        if load {
            self.load_game();
        } else {
            if prefs {
                self.apply_host_prefs();
            }
            self.r.new_game();
        }
    }

    /// A Load of the session's gam (a save): H2100 (`Rules::h2100_load_host_prefs`) — the host's Preferences into the
    /// RegPreferences variables first (over the save's values), then the core's Load.
    pub fn load_game(&mut self) {
        if self.r.w.rules.h2100_load_host_prefs && self.r.w.rules.c86_new_game_host_prefs {
            let p = self.host_prefs.clone();
            self.r.w.set_prefs(&p);
        }
        self.r.load_game();
    }

    /// H2100 / H2084: a Save writes the running game's Preferences to the host's (the original's registry after the
    /// save sequence); `prefs_saved` tells the front to persist them.
    fn save_host_prefs(&mut self) {
        if self.r.w.rules.h2100_load_host_prefs && self.r.w.rules.c86_new_game_host_prefs {
            self.host_prefs = self.r.w.pref_values();
            self.prefs_saved = true;
        }
    }

    /// C86: the host's Preferences into the core's RegPreferences variables (cold start, New Game)
    pub fn apply_host_prefs(&mut self) {
        if self.r.w.rules.c86_new_game_host_prefs {
            let p = self.host_prefs.clone();
            self.r.w.set_prefs(&p);
        }
    }

    /// The game languages (voices + the game's own texts) this session can switch to now (H1755): the game data's own
    /// (H2305), plus "en" once an English data set is laid over RU data (H1836, [`Session::set_en_data`]). Only these
    /// are offered (the settings, F4); a language without data is not listed.
    pub fn game_langs(&self) -> Vec<&'static str> {
        let mut v = vec![self.data_lang.code()];
        if self.en_data.is_some() && self.data_lang != crate::media::lang::GameLang::En {
            v.push("en");
        }
        v
    }

    /// The game language after the current one in [`Session::game_langs`] (F4); None when there is only one.
    pub fn next_game_lang(&self) -> Option<&'static str> {
        let v = self.game_langs();
        let i = v.iter().position(|&l| l == self.game_lang.code()).unwrap_or(0);
        (v.len() > 1).then(|| v[(i + 1) % v.len()])
    }

    /// Switch the game language live by its code (settings' "game language", H1755 / H1836): [`Session::set_lang`].
    pub fn set_game_lang(&mut self, lang: &str) -> Result<(), String> {
        match crate::media::lang::GameLang::parse(lang) {
            Some(l) if self.game_langs().contains(&l.code()) => self.set_lang(l),
            _ => Err(format!("{lang}: no data for it (languages here: {})", self.game_langs().join(", "))),
        }
    }

    /// H2305: the subtitle choices that fit this session's data, in the settings' order: a pack of `packs` (the
    /// languages whose pack the cache holds, [`crate::media::subs::packs_in`]) only over Russian game data — the
    /// English movies carry their own English captions (drawn while the captions are on, F3), and a Russian pack is not
    /// offered over them; `auto` always, `off` only beside a pack (without one both mean the movies' own captions, so
    /// the option has the single value `auto` and is not shown; the captions on / off stay the "Subtitles" option).
    /// H2323: over German game data only the English pack (its movies carry no captions of their own but 37 English
    /// sound-effect captions; the English cues follow the English speech, as over the RU dub).
    pub fn sub_langs(&self, packs: &[&str]) -> Vec<&'static str> {
        use crate::media::lang::GameLang;
        let langs: &[&'static str] = match self.data_lang {
            GameLang::Ru => &["en", "ru"],
            GameLang::De => &["en"],
            GameLang::En => &[],
        };
        let offered: Vec<&'static str> = langs.iter().copied().filter(|l| packs.contains(l)).collect();
        if offered.is_empty() {
            return vec!["auto"];
        }
        std::iter::once("auto").chain(offered).chain(std::iter::once("off")).collect()
    }

    /// H2323: whether this session has no subtitles to show — German game data (its movies carry no captions of their
    /// own but a few English sound-effect lines), no English data set laid over it and no pack of `packs` offered
    /// ([`Session::sub_langs`]). The settings then offer no "Subtitles" option, and switching the captions on says why
    /// nothing shows.
    pub fn no_subtitles(&self, packs: &[&str]) -> bool {
        self.data_lang == crate::media::lang::GameLang::De
            && self.en_data.is_none()
            && self.sub_pack.is_none()
            && self.sub_langs(packs) == ["auto"]
    }

    /// [`Session::reload`] up to the game's start: the new core on `gam`, not started yet (the caller runs
    /// `load_game` / `new_game`; e2e `--chained` sets its harness inputs in between, H1580).
    pub fn reload_core(&mut self, gam: &'a Hdb) {
        let (h, rules) = (self.r.w.h, self.r.w.rules);
        let mut r = Runner::new(h, gam, rules);
        r.uneval = std::mem::take(&mut self.r.uneval);
        r.asset_sizes = std::mem::take(&mut self.r.asset_sizes);
        r.w.live_rand = self.r.w.live_rand; // H550
        r.w.lang_strings = self.r.w.lang_strings.clone(); // H1836
        self.r = r;
        self.r.advance(self.now);
        self.heads.clear();
        self.head_off.clear();
        self.bg = None;
        self.loops.clear();
        self.preview = None;
        self.ui = ui::UiState::default();
        self.menu = menu::MenuUi { clock: self.menu.clock, seen: self.menu.seen, ..Default::default() };
        self.load_req = None;
        self.intro = None;
        self.rewind_t0 = None;
        if let Some(sl) = &self.saves {
            self.r.w.save_names = sl.names.clone();
        }
    }

    /// H402: use `slots` for the save / load screens (their names go to the core).
    pub fn set_saves(&mut self, slots: crate::save::Slots) {
        self.r.w.save_names = slots.names.clone();
        self.saves = Some(slots);
    }

    /// H405: the running game as a save over `base` (the core's state plus the looping players' positions, ms).
    pub fn save_state(&mut self, base: &crate::save::SaveState) -> crate::save::SaveState {
        let mut st = self.r.save_state(base);
        // C130 / H1101: a photo without a thumbnail (taken in the port, or an older port save) gets its picture's
        // 80×60 thumbnail (black when the picture cannot be read)
        for ph in &mut st.photos {
            if ph.thumb.is_empty() {
                let frame = Some((ph.f68(), ph.rect()));
                ph.thumb = crate::save::Photo::thumbnail(self.photo_picture(ph.picture(), frame).as_ref());
            }
        }
        for p in &mut st.players {
            let a = p[0];
            if let Some(&(_, t0)) = self.loops.iter().find(|l| l.0 == a) {
                let d = self.duration(a).max(1);
                p[2] = ((self.now - t0) % d) as u32;
            } else if let Some(&(_, t0)) = self.heads.iter().find(|l| l.0 == a) {
                p[2] = self.head_t(a, t0) as u32;
            }
        }
        st
    }

    /// H400 / H401 / H470: the bytes of a save of the running game — over the session's gam when it is a save (AU's
    /// in-place writer), else built from the session's gam (XFiles.gam, `Rules::h470_save_builder`), else over the
    /// template.
    pub fn save_bytes(&mut self) -> Result<(Vec<u8>, crate::save::WriteReport), String> {
        let gam = self.r.w.gam;
        if crate::save::game_state_id(gam).is_none() && self.r.w.rules.h470_save_builder {
            let st = self.save_state(&crate::save::SaveState::from_gam(gam));
            return crate::save::build(gam, &st);
        }
        let tpl;
        let base: &Hdb = if crate::save::game_state_id(gam).is_some() {
            gam
        } else {
            let p = self.template.clone().ok_or("no save of the original to use as a template (H401)")?;
            tpl = Hdb::open(&p).map_err(|e| e.to_string())?;
            &tpl
        };
        let st = self.save_state(&crate::save::SaveState::from_gam(base));
        crate::save::write(base, &st)
    }

    /// H403: the save screens' requests — a Save writes `<saves>/<name>.x`, a Load is left for the host (`load_req`).
    fn save_requests(&mut self) {
        for req in std::mem::take(&mut self.r.save_req) {
            match req {
                crate::world::Out::SaveGame { slot, name } => {
                    let res = self.save_bytes().and_then(|(bytes, rep)| {
                        let sl = self.saves.as_mut().ok_or("no saves directory")?;
                        let p = sl.store(slot, &name, &bytes).map_err(|e| e.to_string())?;
                        Ok((p, rep))
                    });
                    match res {
                        Ok((p, rep)) => {
                            self.log.push(format!("saved {} ({rep:?})", p.display()));
                            self.save_host_prefs();
                            if let Some(sl) = &self.saves {
                                self.r.w.save_names = sl.names.clone();
                            }
                        }
                        Err(e) => self.log.push(format!("save {name:?} failed: {e}")),
                    }
                }
                crate::world::Out::LoadGame { slot } => {
                    self.load_req = self.saves.as_ref().and_then(|s| s.path(slot));
                    if self.load_req.is_none() {
                        self.log.push(format!("load: slot {slot} has no file"));
                    }
                }
                _ => {}
            }
        }
    }

    /// The decoded-media caches as (name, entries, bytes) — robustness monitoring (a monkey-test probe).
    pub fn cache_stats(&self) -> Vec<(&'static str, usize, usize)> {
        vec![
            ("players", self.players.len(), self.players.values().flatten().map(|p| p.mem_bytes()).sum()),
            ("pffs", self.pffs.len(), self.pffs.values().flatten().map(|p| p.mem_bytes()).sum()),
            ("pictures", self.pictures.len(), self.pictures.values().flatten().map(|p| p.rgb.len()).sum()),
            ("sounds", self.sounds.len(), self.sounds.values().flatten().map(|p| p.samples.len() * 2).sum()),
            ("clips", self.clips.len(), 0),
        ]
    }

    pub fn clip(&mut self, asset: u32) -> Option<Clip> {
        let mut c = self.base_clip(asset)?;
        // H1409: a line's f30 player runs on into the f34 range of the same movie file
        if let Some(&tail) = self.r.w.media_tail.get(&asset) {
            if let Some(t) = self.base_clip(tail) {
                if t.file == c.file && c.end > c.start && t.end > c.end {
                    c.end = t.end;
                }
            }
        }
        Some(c)
    }

    fn base_clip(&mut self, asset: u32) -> Option<Clip> {
        if !self.clips.contains_key(&asset) {
            let c = self.lib.resolve(self.r.w.h, asset);
            if c.is_none() {
                self.log.push(format!("asset {asset:#x}: no file found"));
            }
            self.clips.insert(asset, c);
        }
        self.clips[&asset].clone()
    }

    fn player(&mut self, file: &Path) -> Option<&mut MoviePlayer> {
        if !self.players.contains_key(file) {
            if self.players.len() > 12 {
                self.players.clear();
            }
            let p = MoviePlayer::open(file);
            if let Err(e) = &p {
                self.log.push(e.clone());
            }
            self.players.insert(file.to_path_buf(), p.ok());
        }
        self.players.get_mut(file).and_then(|p| p.as_mut())
    }

    /// H1280 / H1281: hand the core the `.HOT` hotspots of `asset`'s movie file once (times relative to the clip's start;
    /// none for a missing file).
    fn movie_hotspots_of(&mut self, asset: u32) {
        if !self.r.w.rules.h1281_movie_hotspots || self.r.movie_hotspots.contains_key(&asset) {
            return;
        }
        let hs = match self.clip(asset) {
            Some(c) => crate::world::moviehot::relative_to_start(&self.lib.movie_hotspots(&c.name), c.start),
            None => Vec::new(),
        };
        self.r.movie_hotspots.insert(asset, hs);
    }

    /// Length of a played asset: its range, or the whole movie; [`DEFAULT_MEDIA_MS`] if unreadable.
    pub fn duration(&mut self, asset: u32) -> u64 {
        let Some(c) = self.clip(asset) else { return DEFAULT_MEDIA_MS };
        // H1561 (C270): a played still ends 3 s after its start — the player arms a 0xbb8 timer
        // and its media end follows +3.0 s (fo-220 0x7109 / 0x711b / 0x712b /
        // 0x7158, fo-111 0xe2d)
        if c.is_still() {
            return STILL_MS;
        }
        if c.end > c.start {
            return (c.end - c.start) as u64;
        }
        match self.player(&c.file) {
            Some(p) => p.duration_ms().saturating_sub(c.start as u64),
            None => {
                self.log.push(format!("asset {asset:#x}: length unknown, {DEFAULT_MEDIA_MS} ms assumed"));
                DEFAULT_MEDIA_MS
            }
        }
    }

    /// H1601: how far (ms) into its media a head started at `t0` is — its start offset (a restored player's saved
    /// position) plus the time since
    fn head_t(&self, a: u32, t0: u64) -> u64 {
        self.now.saturating_sub(t0) + self.head_off.get(&a).copied().unwrap_or(0)
    }

    /// H1795: the clock the picture shows — the session clock less the picture delay ([`Session::av_delay`]).
    pub fn picture_now(&self) -> u64 {
        self.now.saturating_sub(self.av_delay)
    }

    /// The playing media shown in the movie area and its time on the picture's clock (ms since its start, H1795): the
    /// viewer's asset, else the player's head.
    pub fn picture_head(&self) -> Option<(u32, u64)> {
        self.picture_head_start().map(|(a, t0)| (a, self.head_t_shown(a, t0)))
    }

    /// The (asset, start time) slot of [`Session::picture_head`] (H1836: its language is pinned per slot).
    fn picture_head_start(&self) -> Option<(u32, u64)> {
        self.preview.or_else(|| self.r.playing().and_then(|p| self.heads.iter().copied().find(|h| h.0 == p)))
    }

    /// [`Session::head_t`] on the picture's clock (a head started less than the delay ago shows its first frame).
    fn head_t_shown(&self, a: u32, t0: u64) -> u64 {
        self.picture_now().saturating_sub(t0) + self.head_off.get(&a).copied().unwrap_or(0)
    }

    /// Advance the clock by `dt` ms: run the core's queue, end media whose time is up.
    pub fn tick(&mut self, dt: u64) {
        // H2067: the FPS counter's frame times; H2066: the state box (a MessageBox) holds the whole program
        if let Some(f) = &mut self.alt.fps {
            f.push(dt);
        }
        if self.alt.msgbox.is_some() {
            return;
        }
        // H1660: the intro movies play in a loop of their own; the game's queue does not run
        // (the virtual clock runs on, so settling waits for the intro's end)
        if self.intro.is_some() {
            self.now += dt;
            self.intro_tick(dt);
            return;
        }
        // H404: while the menu shows, the game's clock stops (queued commands still run)
        self.menu_tick(dt);
        // H2340: also under a screen the menu opened (Save / Load / Options / Help): the game stays paused
        if self.r.game_paused() {
            self.r.run(100_000);
            self.save_requests();
            return;
        }
        // H1700 / H1703: the "Try again?" box's modal loop / the frozen game
        if self.rewind_tick(dt) {
            self.save_requests();
            return;
        }
        self.tick_game(dt);
        self.save_requests();
    }

    fn tick_game(&mut self, dt: u64) {
        self.now += dt;
        self.scroll_hold_tick();
        self.r.advance(self.now); // H59 timers
        for _ in 0..64 {
            self.r.run(100_000);
            let started = self.r.started();
            self.heads.retain(|h| started.contains(&h.0));
            self.head_off.retain(|a, _| started.contains(a));
            for a in started {
                if !self.heads.iter().any(|h| h.0 == a) {
                    self.heads.push((a, self.now));
                    let off = self.r.take_start_offset(a); // H1601
                    if off > 0 {
                        self.head_off.insert(a, off);
                    }
                }
                self.movie_hotspots_of(a); // H1281
            }
            // the first started media whose time is up ends; then the queue runs again
            let mut due = None;
            for (a, t0) in self.heads.clone() {
                if self.head_t(a, t0) >= self.duration(a) {
                    due = Some(a);
                    break;
                }
            }
            let Some(a) = due else { break };
            self.r.media_ended(a);
            self.heads.retain(|h| h.0 != a);
            self.head_off.remove(&a);
        }
        let bg = self.r.background;
        if let Some(b) = bg {
            self.movie_hotspots_of(b); // H1281: a View movie's hotspots (the goon Views)
        }
        if bg != self.bg.map(|x| x.0) {
            self.bg = bg.map(|a| (a, self.now));
        }
        // H100: a looping player restarts at its media end (the clip loops from here on); stopped ones are dropped
        let looping = self.r.looping();
        self.loops.retain(|(a, _)| looping.contains(a));
        for a in looping {
            if !self.loops.iter().any(|x| x.0 == a) {
                self.loops.push((a, self.now));
            }
        }
    }

    /// Nothing queued, nothing playing.
    pub fn idle(&self) -> bool {
        self.intro.is_none()
            && (self.r.game_paused()
                || self.r.rewind_box
                || (self.r.pending() == 0 && (self.r.frozen || self.r.playing().is_none())))
    }

    /// Pointer at a screen point: hover changes go to the core (Roll Into / Roll Out Of).
    pub fn hover_at(&mut self, x: i32, y: i32) -> Option<Clickable> {
        self.mouse = Some((x, y));
        if self.r.menu_open {
            self.menu_hover(x, y);
            return None;
        }
        if self.r.rewind_box || self.r.frozen {
            return None; // H1700: the modal box takes the pointer; H1703: the frozen objects get no Roll Into
        }
        // H1812: a dragged thumb of the dialogue box follows the pointer while the button is down
        if self.r.button_down && self.r.w.dbox_drag.is_some() {
            self.r.w.dbox_drag_to(x, y);
        }
        let c = self.r.hover_at(x, y); // H170: the inventory panel follows the point
        // H822: the original's main loop dispatches its queue between window messages — the Roll Into's commands (e.g.
        // H820's hover-video player, fo-081: REG 0x4e15 61.623 before the button-down 61.748) run before a click that
        // follows the move
        self.r.run(100_000);
        c
    }

    /// H1813: while a dialogue-box arrow stays held under the pointer, its step repeats (first after
    /// [`SCROLL_REPEAT_FIRST_MS`], then every [`SCROLL_REPEAT_MS`]).
    fn scroll_hold_tick(&mut self) {
        let Some((kind, mut next)) = self.scroll_hold else { return };
        if !self.r.button_down {
            self.scroll_hold = None;
            return;
        }
        // H1813: the arrow steps only while the pointer is still on it
        let on = self.mouse.and_then(|(x, y)| self.r.hit(x, y)).is_some_and(|c| c.kind == kind && c.cls == crate::world::class::CONVERSATION_LIST);
        while next <= self.now {
            if on {
                self.r.w.dbox_scroll_step(if kind == crate::world::dbox::KIND_SCROLL_UP {
                    crate::world::dbox::Scroll::LineUp
                } else {
                    crate::world::dbox::Scroll::LineDown
                });
            }
            next += SCROLL_REPEAT_MS;
        }
        self.scroll_hold = Some((kind, next));
    }

    /// HYPOTHESIS H1815 (port choice — the original's window procedure FUN_0043f1f0 has no WM_MOUSEWHEEL case): the
    /// mouse wheel at a screen point, `lines` < 0 = up. Over the shown dialogue box it steps the box's scroller; else it
    /// clicks the nearest shown line-up / line-down item of the FBI computer (an InterfaceItem whose Mouse Click calls
    /// MainScroll 0 / 1, ScrollHeaders 0 / 1, ScrollAddrs (k, 0 / 1) or PhotoScroll 0 / 1) — as if that arrow was clicked.
    /// Returns whether something scrolled.
    pub fn wheel(&mut self, x: i32, y: i32, lines: i32) -> bool {
        if lines == 0 || self.r.menu_open || self.r.rewind_box || self.r.frozen || self.intro.is_some() {
            return false;
        }
        if let Some(l) = self.r.w.dbox_layout() {
            let r = l.rect.map(|v| v as i32);
            if x >= r[0] && x < r[2] && y >= r[1] && y < r[3] {
                if l.scroll.is_none() {
                    return false;
                }
                for _ in 0..lines.unsigned_abs() {
                    self.r.w.dbox_scroll_step(if lines < 0 {
                        crate::world::dbox::Scroll::LineUp
                    } else {
                        crate::world::dbox::Scroll::LineDown
                    });
                }
                return true;
            }
        }
        let Some(item) = self.r.wheel_item(x, y, lines > 0) else { return false };
        for _ in 0..lines.unsigned_abs() {
            self.r.click(item);
            self.r.run(100_000);
        }
        true
    }

    /// Click at a screen point: the topmost clickable there goes to the core.
    pub fn click_at(&mut self, x: i32, y: i32) -> Option<Clickable> {
        self.r.button_down = false; // a whole click (down + up at once): a stale press from a front ends here
        self.r.w.held_item = None;
        self.scroll_hold = None;
        self.r.w.dbox_drag = None;
        self.click_hit(x, y)
    }

    fn click_hit(&mut self, x: i32, y: i32) -> Option<Clickable> {
        if self.r.menu_open {
            self.menu_click(x, y);
            return None;
        }
        if self.r.rewind_box {
            self.rewind_click(x, y);
            return None;
        }
        if self.r.frozen {
            self.log.push(format!("H1703: click ({x},{y}) on the frozen game (ignored)"));
            return None;
        }
        let c = self.r.hit(x, y)?;
        if c.cls == crate::world::class::CONVERSATION_LIST && c.kind == crate::world::dbox::KIND_SCROLL_BAR {
            self.r.w.dbox_bar_press(y); // H1812: page up / down or the thumb's drag
        } else {
            self.r.click_clickable(&c);
        }
        Some(c)
    }

    /// The left button went down at a screen point (a front with separate button events; [`Session::release_at`]
    /// follows): the click as [`Session::click_at`], except that a dragged IdeaIcon's answer waits for the button-up (C107).
    pub fn press_at(&mut self, x: i32, y: i32) -> Option<Clickable> {
        if self.r.menu_open || self.r.rewind_box || self.r.frozen {
            return self.click_at(x, y);
        }
        self.r.button_press();
        let c = self.click_hit(x, y);
        if c.is_none() {
            self.r.button_down = false;
        }
        self.pressed = c;
        // H1814: the app keeps the object the button-down hit, held until the button-up (IsIItemHeld, H184)
        self.r.w.held_item = c.filter(|_| self.r.button_down).map(|c| c.owner);
        // H1813: a held arrow of the dialogue box repeats its step
        self.scroll_hold = c
            .filter(|c| c.cls == crate::world::class::CONVERSATION_LIST)
            .filter(|c| matches!(c.kind, crate::world::dbox::KIND_SCROLL_UP | crate::world::dbox::KIND_SCROLL_DOWN))
            .map(|c| (c.kind, self.now + SCROLL_REPEAT_FIRST_MS));
        c
    }

    /// The left button went up at a screen point (None = off the picture): a dragged IdeaIcon is dropped on the owner
    /// under it (C107: on the icon itself → its idea's answer; H921 elsewhere).
    pub fn release_at(&mut self, at: Option<(i32, i32)>) {
        // the pressed owner keeps its place under the button (the original drags the icon along with the pointer; the
        // port's bar may re-place it once the other icons left)
        let pressed = self.pressed.take();
        self.r.w.held_item = None; // H1814: the button-up clears the held object
        self.scroll_hold = None;
        self.r.w.dbox_drag = None; // H1812: the button-up ends the thumb's drag
        let target = at.and_then(|(x, y)| {
            let inside = |c: &Clickable| {
                let [x1, y1, x2, y2] = c.rect.map(|v| v as i32);
                x >= x1 && x < x2 && y >= y1 && y < y2
            };
            match pressed {
                Some(p) if inside(&p) => Some(p.owner),
                _ => self.r.hit(x, y).map(|c| c.owner),
            }
        });
        self.r.button_release(target);
    }

    /// Frame of `clip` at `t` ms into it (movie time = start + t; ranges loop, stills stay).
    fn frame(&mut self, clip: &Clip, t: u64, looped: bool, lang: crate::media::lang::GameLang) -> Option<Picture> {
        let file = self.lang_file(clip, lang);
        let p = self.player(&file)?;
        if !p.has_video() {
            return None;
        }
        let mt = movie_time(clip, t, looped, p.duration_ms());
        match p.seek(mt) {
            Ok(true) => Some(Picture { width: p.width, height: p.height, rgb: p.rgb.clone(), alpha: None }),
            Ok(false) => None,
            Err(e) => {
                self.log.push(format!("{}: {e}", clip.name));
                None
            }
        }
    }

    /// Show `asset` in the movie area from now on (port viewer `--play`; the core is not told).
    pub fn play_preview(&mut self, asset: u32) {
        self.preview = Some((asset, self.now));
    }

    /// The media slots whose pictures / captions can show: the preview, the started media (A3 head + H280 sounds), the
    /// looping players (H100), the View background — (asset, start time, looped), deduplicated.
    fn media_slots(&self) -> Vec<(u32, u64, bool)> {
        let mut v: Vec<(u32, u64, bool)> = Vec::new();
        let all = self
            .preview
            .iter()
            .map(|&(a, t)| (a, t, false))
            .chain(self.heads.iter().map(|&(a, t)| (a, t, false)))
            .chain(self.loops.iter().map(|&(a, t)| (a, t, true)))
            .chain(self.bg.iter().map(|&(a, t)| (a, t, true)));
        for x in all {
            if !v.iter().any(|y| y.0 == x.0) {
                v.push(x);
            }
        }
        v
    }

    /// H344: the user's SubRip cues for a movie file (`<subs>/<movie path>.srt`), cached
    fn srt_cues(&mut self, clip: &Clip) -> Option<Vec<SubCue>> {
        let subs = self.subs.as_ref()?;
        let rel = match clip.name.rsplit_once('.') {
            Some((stem, _)) => format!("{stem}.srt"),
            None => format!("{}.srt", clip.name),
        };
        let path = subs.find(&rel)?.clone();
        if !self.srts.contains_key(&path) {
            let cues = match std::fs::read(&path) {
                Ok(b) => Some(crate::media::captions::parse_srt(&String::from_utf8_lossy(&b))),
                Err(e) => {
                    self.log.push(format!("{}: {e}", path.display()));
                    None
                }
            };
            self.srts.insert(path.clone(), cues);
        }
        self.srts[&path].clone()
    }

    /// HYPOTHESIS H340–H344 — the closed captions on screen now, in movie coordinates (origin = the View's top-left,
    /// [`VIEW_X`], [`VIEW_Y`]), back to front. Empty unless Pref_bClosedCaptioning (var 0xa0d) is on. Per playing movie:
    /// the user's `.srt` for its file, else the subtitle pack's cues for it (H955), else the text tracks of a movie of
    /// the same path in the subtitles directory, else its own text tracks (the original: QuickTime draws the enabled text
    /// tracks, FUN_0046b560). Display only.
    pub fn captions(&mut self) -> Vec<Caption> {
        if !self.r.w.closed_captioning() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (a, t0, looped) in self.media_slots() {
            let Some(c) = self.clip(a) else { continue };
            if c.ext() == "pff" {
                continue;
            }
            let t = if looped { self.picture_now().saturating_sub(t0) } else { self.head_t_shown(a, t0) };
            let Some(dur) = self.player(&c.file).map(|p| p.duration_ms()) else { continue };
            let mt = movie_time(&c, t, looped, dur);
            if let Some(cues) = self.srt_cues(&c) {
                if let Some(text) = crate::media::captions::cue_at(&cues, mt) {
                    out.push(user_caption(&text));
                }
                continue;
            }
            // H955: the subtitle pack's cues replace the movie's own text tracks
            if let Some(cues) = self.sub_pack.as_ref().and_then(|p| p.cues(&c.file)) {
                if let Some(text) = crate::media::captions::cue_at(cues, mt) {
                    out.push(user_caption(&text));
                }
                continue;
            }
            let alt = self.subs.as_ref().and_then(|s| s.find(&c.name)).cloned();
            let file = alt.unwrap_or(c.file.clone());
            if let Some(p) = self.player(&file) {
                out.extend(p.captions_at(mt));
            }
        }
        out
    }

    /// H793 / C306: the picture of a photo — a PFF asset's still (TakePhoto's), else (`frame` = its f68 / rect) a
    /// camera shot's: the video clip's frame at f68 / 600 s (modulo the clip's length), cropped to the rect (320×240 of
    /// the 600×240 frame). None when neither can be read.
    pub fn photo_picture(&mut self, asset: u32, frame: Option<(u32, [i16; 4])>) -> Option<Picture> {
        if let Some(p) = self.picture(asset) {
            return Some(p.clone());
        }
        let (f68, rect) = frame?;
        let key = (asset, f68, rect);
        if !self.shot_pictures.contains_key(&key) {
            let pic = self.clip(asset).filter(|c| c.ext() != "pff").and_then(|c| {
                let mut m = MoviePlayer::open(&c.file).ok()?;
                let len = if c.end > c.start { (c.end - c.start) as u64 } else { m.duration_ms() };
                let t = c.start as u64 + (f68 as u64 * 1000 / 600) % len.max(1);
                m.seek(t).ok()?;
                let [x0, y0, x1, y1] = rect.map(|v| v.max(0) as usize);
                let (w, h) = (x1.min(m.width).saturating_sub(x0), y1.min(m.height).saturating_sub(y0));
                if w == 0 || h == 0 {
                    return None;
                }
                let mut rgb = Vec::with_capacity(w * h * 3);
                for y in y0..y0 + h {
                    let o = (y * m.width + x0) * 3;
                    rgb.extend_from_slice(&m.rgb[o..o + w * 3]);
                }
                Some(Picture { width: w, height: h, rgb, alpha: None })
            });
            self.shot_pictures.insert(key, pic);
        }
        self.shot_pictures[&key].clone()
    }

    /// The still picture of a PFF asset (entry `start` of its .pff; an XG/*.pff animation shows its first frame here;
    /// the cursor's animation is front::cursor, H330). None for other media.
    pub fn picture(&mut self, asset: u32) -> Option<&Picture> {
        if !self.pictures.contains_key(&asset) {
            let pic = match self.clip(asset) {
                Some(c) if c.ext() == "pff" => {
                    let pff = self.pffs.entry(c.file.clone()).or_insert_with(|| Pff::open(&c.file).ok());
                    match pff.as_ref().map(|p| p.picture(c.start as usize)) {
                        Some(Ok(pic)) => Some(pic),
                        Some(Err(e)) => {
                            self.log.push(format!("asset {asset:#x} ({} #{}): {e}", c.name, c.start));
                            None
                        }
                        None => {
                            self.log.push(format!("asset {asset:#x}: {} unreadable", c.name));
                            None
                        }
                    }
                }
                _ => None,
            };
            self.pictures.insert(asset, pic);
        }
        self.pictures[&asset].as_ref()
    }

    /// Pictures of active owners with a region (VCInterfaceItem, VCExplorable): every *active* AssetRef of the owner's
    /// AssetRefList (f44) that is a PFF picture, drawn with its top-left at the region's top-left, bottom-most owner
    /// first (A4 stacking). Which assets are active is the core's (H58: the item's first enabled AssetRef). Placement
    /// is a hypothesis from data: 967 of 1134 InterfaceItem pictures have exactly the region's size, the rest differ by a
    /// few pixels or reuse one button picture in several rects (a probe over the data); not traced.
    pub fn region_pictures(&mut self) -> Vec<(i32, i32, u32)> {
        self.region_pictures_of().into_iter().map(|(x, y, a, _)| (x, y, a)).collect()
    }

    /// [`Session::region_pictures`] with each picture's owner class (VCInterfaceItem or VCExplorable).
    pub fn region_pictures_of(&mut self) -> Vec<(i32, i32, u32, u32)> {
        let h = self.r.w.h;
        let mut out = Vec::new();
        for c in self.r.clickables().into_iter().rev() {
            if c.cls != crate::world::class::INTERFACE_ITEM && c.cls != crate::world::class::EXPLORABLE {
                continue;
            }
            let list = h.read(c.cls, c.owner).map(|r| r.u32("f44")).unwrap_or(0);
            for a in h.list_of(crate::world::class::ASSET_REF_LIST, list) {
                if self.r.active.contains(&a) && self.picture(a).is_some() {
                    // H578: a caret item is drawn where its field's text ends; H792: blinking
                    let (x, y) = self.r.w.pc_item_pos(c.owner).unwrap_or((c.rect[0] as i32, c.rect[1] as i32));
                    let a = computer::caret_asset(self, c.owner, a);
                    out.push((x, y, a, c.cls));
                }
            }
        }
        out
    }

    /// The sound track of an asset's file as PCM (None: no file, no sound track, codec not decoded — logged once).
    pub fn sound(&mut self, asset: u32) -> Option<Arc<Pcm>> {
        let l = self.game_lang;
        self.sound_in(asset, l)
    }

    /// [`Session::sound`] in `lang` (H1836: the English file of a language resource).
    pub fn sound_in(&mut self, asset: u32, lang: crate::media::lang::GameLang) -> Option<Arc<Pcm>> {
        let mut c = self.clip(asset)?;
        if c.ext() == "pff" {
            return None;
        }
        c.file = self.lang_file(&c, lang);
        if !self.sounds.contains_key(&c.file) {
            let p = match crate::media::sound(&c.file) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => {
                    if !e.ends_with("no sound track") {
                        self.log.push(e);
                    }
                    None
                }
            };
            if p.is_some() {
                self.sound_order.push_back(c.file.clone());
            }
            self.sounds.insert(c.file.clone(), p);
            self.trim_sounds(&c.file);
        }
        self.sounds[&c.file].clone()
    }

    /// Robustness: the decoded sounds are kept for reuse, but within `sound_budget` ([`SOUND_CACHE_BYTES`]) — the game's sound tracks
    /// decode to ~1.2 GB in all, so a long session kept growing. The oldest decoded ones go first; `keep` (just decoded)
    /// and sounds a mixer still holds (another `Arc` reference) stay. A dropped sound is decoded again when asked for.
    fn trim_sounds(&mut self, keep: &Path) {
        let bytes = |p: &Option<Arc<Pcm>>| p.as_ref().map_or(0, |p| p.samples.len() * 2);
        let mut total: usize = self.sounds.values().map(bytes).sum();
        for _ in 0..self.sound_order.len() {
            if total <= self.sound_budget {
                break;
            }
            let Some(f) = self.sound_order.pop_front() else { break };
            let held = self.sounds.get(&f).and_then(|p| p.as_ref()).is_some_and(|p| Arc::strong_count(p) > 1);
            if f == keep || held {
                self.sound_order.push_back(f);
                continue;
            }
            if let Some(p) = self.sounds.remove(&f) {
                total -= bytes(&p);
            }
        }
    }

    /// What should sound now (display only — the core decides what is active): the started media players (A3 / H280; each
    /// its range from `start`, once), the View background (looped like its picture; none with H320) and looping players
    /// (H100, looped). Positions are in sample frames of each PCM. Whether the original loops an .amv's sound is not
    /// known (Q12).
    pub fn audio_cues(&mut self) -> Vec<Cue> {
        if self.intro.is_some() {
            return self.intro_audio();
        }
        if self.r.menu_open {
            return self.menu_audio();
        }
        // H2340: under a screen the menu opened the game's players stand still too (the menu's own sound there: not modelled)
        if self.r.rewind_box || self.r.frozen || self.alt.msgbox.is_some() || self.r.menu_screen_up() {
            return Vec::new(); // H1700 / H1703: the players stand still (not traced: what the original's sound does)
        }
        let mut out = Vec::new();
        // H320: a View background is not a media player (no player state switch in any trace) — no sound;
        // H520: except a whole movie file (empty range, not a still), which runs as a movie with its sound track
        let rules = self.r.w.rules;
        let whole_movie = |c: &Clip| !c.is_still() && c.end <= c.start && c.ext() != "pff";
        let bg = match self.bg {
            Some((a, _)) if rules.h320_background_silent => {
                let keep = rules.h520_whole_movie_background_sound && self.clip(a).is_some_and(|c| whole_movie(&c));
                self.bg.filter(|_| keep)
            }
            bg => bg,
        };
        let slots = self
            .heads
            .iter()
            .map(|&h| (Some(h), false))
            .chain([(bg, true)])
            .chain(self.loops.iter().map(|&l| (Some(l), true)));
        for (slot, looped) in slots.collect::<Vec<_>>() {
            let Some((a, t0)) = slot else { continue };
            let Some(c) = self.clip(a).filter(|c| !c.is_still()) else { continue };
            let l = self.slot_lang(a, t0);
            let Some(pcm) = self.sound_in(a, l) else { continue };
            let ms = |t: u64| (t * pcm.rate as u64 / 1000) as usize;
            let total = pcm.frames();
            let start = ms(c.start as u64).min(total);
            let end = if c.end > c.start { ms(c.end as u64).min(total) } else { total };
            if end <= start {
                continue;
            }
            let t = ms(if looped { self.now - t0 } else { self.head_t(a, t0) });
            let pos = if looped { start + t % (end - start) } else { start + t };
            if pos >= end {
                continue;
            }
            out.push(Cue { asset: a, pcm, pos, start, end, looped });
        }
        out
    }

    /// Compose the screen (640×480 RGB24).
    pub fn render(&mut self) -> Vec<u8> {
        let mut fb = self.render_game();
        altkeys::draw(self, &mut fb);
        fb
    }

    /// [`Session::render`] without the Alt keys' state box / FPS counter.
    fn render_game(&mut self) -> Vec<u8> {
        self.ui.frame += 1;
        if self.intro.is_some() {
            return self.render_intro();
        }
        if self.r.menu_open {
            return self.render_menu();
        }
        if self.r.rewind_box {
            return self.render_rewind();
        }
        let mut fb = vec![0u8; SCREEN_W * SCREEN_H * 3];
        // movie area: playing video, else background, else a plain fill
        let mut shown = None;
        if let Some((a, t)) = self.picture_head() {
            if let Some(c) = self.clip(a) {
                let t0 = self.picture_head_start().map_or(0, |x| x.1);
                let l = self.slot_lang(a, t0);
                shown = self.frame(&c, t, false, l);
            }
        }
        // H100: a looping player's video (no media at the head), looped
        for (a, t0) in self.loops.clone() {
            if shown.is_some() {
                break;
            }
            if let Some(c) = self.clip(a) {
                let l = self.slot_lang(a, t0);
                shown = self.frame(&c, self.picture_now().saturating_sub(t0), true, l);
            }
        }
        if shown.is_none() {
            if let Some((a, t0)) = self.bg {
                if let Some(c) = self.clip(a) {
                    let t = self.picture_now().saturating_sub(t0);
                    let l = self.slot_lang(a, t0);
                    shown = self.frame(&c, t, true, l);
                }
            }
        }
        match shown {
            Some(px) => {
                // a movie of another size than 600×240 (XV/56003.xmv 140×144, XG/24465.xmv 80×60, XV/19666.xmv
                // 600×400): its top-left part inside the movie area, the rest of the area the plain fill (robustness:
                // the copy indexed every frame as 600×240 and panicked on smaller ones; where the original places
                // such a movie is not decoded)
                let (w, h) = (px.width, px.rgb.len().checked_div(px.width * 3).unwrap_or(0).min(px.height));
                if (w, h) != (FRAME_W, FRAME_H) {
                    fill(&mut fb, VIEW_X as i32, VIEW_Y as i32, (VIEW_X + FRAME_W) as i32, (VIEW_Y + FRAME_H) as i32, [40, 40, 48]);
                }
                let (cw, ch) = (w.min(FRAME_W), h.min(FRAME_H));
                for y in 0..ch {
                    let s = y * w * 3;
                    let d = ((VIEW_Y + y) * SCREEN_W + VIEW_X) * 3;
                    fb[d..d + cw * 3].copy_from_slice(&px.rgb[s..s + cw * 3]);
                }
            }
            None => fill(&mut fb, VIEW_X as i32, VIEW_Y as i32, (VIEW_X + FRAME_W) as i32, (VIEW_Y + FRAME_H) as i32, [40, 40, 48]),
        }
        // H620: under a dimming layout the scene is darkened (front::pda, which keeps H500's box layer order)
        if !pda::compose_dimmed(self, &mut fb) {
            // HYPOTHESIS H500 (display): the dialogue box lies over the View's pictures (Explorables) and under the shown
            // layouts' items — a document layout shown at the end of an answer covers the box (fo-054 429.42, fo-067 65.47)
            let under = self.r.w.rules.h500_layouts_over_dbox;
            let pics = self.region_pictures_of();
            // H1040–H1043: with a tint / goggles / beam the View's own pictures go first, then the band is lit, then the
            // layouts' items (InterfaceItems) lie over it
            let lit = self.lighting();
            let lighting = lit.tint.is_some() || lit.night_vision || lit.beam.is_some() || lit.binoc.is_some();
            for (x, y, a, cls) in &pics {
                if (under || lighting) && *cls == crate::world::class::INTERFACE_ITEM {
                    continue;
                }
                if let Some(p) = self.pictures.get(a).and_then(|p| p.as_ref()) {
                    blit(&mut fb, *x, *y, p);
                }
            }
            let mut untinted = None;
            if lighting {
                if let Some(c) = lit.tint {
                    if lit.beam.is_some_and(|b| b.dark) {
                        untinted = Some(light::band_copy(&fb));
                    }
                    light::tint_band(&mut fb, c);
                }
                if lit.night_vision {
                    if let Some(p) = self.picture(light::NIGHT_VISION_HUD) {
                        blit(&mut fb, VIEW_X as i32, VIEW_Y as i32, p);
                    }
                }
                if !under {
                    for (x, y, a, cls) in &pics {
                        if *cls == crate::world::class::INTERFACE_ITEM {
                            if let Some(p) = self.pictures.get(a).and_then(|p| p.as_ref()) {
                                blit(&mut fb, *x, *y, p);
                            }
                        }
                    }
                }
            }
            if under {
                ui::draw_dbox_layer(self, &mut fb);
                for (x, y, a, cls) in &pics {
                    if *cls != crate::world::class::INTERFACE_ITEM {
                        continue;
                    }
                    if let Some(p) = self.pictures.get(a).and_then(|p| p.as_ref()) {
                        blit(&mut fb, *x, *y, p);
                    }
                }
            }
            // C-BX3: the beam lies over the View band's whole content (the dialogue box too)
            if let Some(b) = lit.beam {
                if let Some(p) = self.picture(light::FLASHLIGHT_MASK).cloned() {
                    light::draw_beam(&mut fb, &b, &p, untinted.as_deref());
                }
            }
            // H1772 / H1773: the binoculars' mask and the camera's viewfinder, over a playing video too (fo-127 k14)
            if let Some(at) = lit.binoc {
                if let Some(p) = self.picture(light::BINOCULAR_MASK).cloned() {
                    light::draw_binoc(&mut fb, at, &p);
                }
            }
            if let Some((at, left)) = lit.viewfinder {
                if let Some(p) = self.picture(light::VIEWFINDER).cloned() {
                    light::draw_viewfinder(&mut fb, at, left, &p);
                }
            }
            ui::draw(self, &mut fb);
        }
        if self.overlay {
            let hover = self.mouse.and_then(|(x, y)| self.r.hit(x, y)).map(|c| c.owner);
            for c in self.r.clickables() {
                let col = match (c.cls, c.kind) {
                    (0x33, 0x11c) => [80, 160, 255],
                    (0x33, 0x11d) => [255, 160, 60],
                    (0x33, 0x11e) => [80, 230, 120],
                    (0x33, _) => [230, 230, 80],
                    _ => [230, 90, 230],
                };
                let [x1, y1, x2, y2] = c.rect.map(|v| v as i32);
                rect(&mut fb, x1, y1, x2 - 1, y2 - 1, col);
                if hover == Some(c.owner) {
                    rect(&mut fb, x1 + 1, y1 + 1, x2 - 2, y2 - 2, col);
                }
                font::text(&mut fb, x1 + 3, y1 + 3, &format!("{:x}", c.owner), col);
            }
            let view = self.r.w.view.map_or("-".into(), |v| format!("{v:x}"));
            let play = self.r.playing().map_or("-".into(), |v| format!("{v:x}"));
            let bg = self.r.background.map_or("-".into(), |v| format!("{v:x}"));
            font::text(&mut fb, 4, 4, &format!("VIEW {view}  BG {bg}  MEDIA {play}  Q {}  T {}", self.r.pending(), self.now / 1000), [200, 200, 200]);
        }
        fb
    }
}

/// Movie-file time (ms) of `clip` at `t` ms into it: ranges loop (`looped`) or stop at their end, stills stay, whole
/// movies loop when `looped` (`dur` = the movie's length).
pub fn movie_time(clip: &Clip, t: u64, looped: bool, dur: u64) -> u64 {
    let mt = if clip.is_still() {
        clip.start as u64
    } else if clip.end > clip.start {
        let len = (clip.end - clip.start) as u64;
        clip.start as u64 + if looped { t % len.max(1) } else { t.min(len) }
    } else {
        t
    };
    let dur = dur.max(1);
    if looped && clip.end <= clip.start && !clip.is_still() {
        mt % dur
    } else {
        mt.min(dur - 1)
    }
}

/// H344: a user cue as a caption in the text tracks' place and style (the 600×20 strip under the picture, box inset 2,
/// centred, white on black, line 16, ascent 12); more lines than one grow the strip upwards over the picture.
pub fn user_caption(text: &str) -> Caption {
    let n = text.lines().count().max(1) as i32;
    let top = FRAME_H as i32 - 16 * (n - 1);
    Caption {
        text: text.to_string(),
        track: [0, top, FRAME_W as i32, FRAME_H as i32 + 20],
        text_box: [2, top + 2, FRAME_W as i32 - 2, FRAME_H as i32 + 18],
        justify: 1,
        fg: [255, 255, 255],
        bg: [0, 0, 0],
        keyed: false,
        ascent: 12,
        line_h: 16,
        size: 12,
        layer: 0,
    }
}

pub fn fill(fb: &mut [u8], x1: i32, y1: i32, x2: i32, y2: i32, c: [u8; 3]) {
    for y in y1.max(0)..y2.min(SCREEN_H as i32) {
        for x in x1.max(0)..x2.min(SCREEN_W as i32) {
            let o = (y as usize * SCREEN_W + x as usize) * 3;
            fb[o..o + 3].copy_from_slice(&c);
        }
    }
}

/// Colour treated as transparent when drawing PFF pictures (H63, hypothesis from data: 97 of the 727 x.pff pictures have a
/// pure-blue top-left pixel, e.g. the drawer items 469–473 are objects on pure blue; the engine draws pictures with
/// MaskBlt (FUN_005656e0) but where the mask comes from is not decoded).
pub const KEY: [u8; 3] = [0, 0, 255];

/// HYPOTHESIS H1023 (the source term confirmed pixel-exact over black by `traces/fo-123.jsonl.burst/k15-0.png`: the
/// three IdeaIcons x.pff 174 / 188 / 172, 4156 of 4156 pixels): a picture with an alpha plane (32-bit DirectBitsRect,
/// cmpCount 4) is drawn through it — α 255 copies, α 0 leaves the screen, else (src·α + dst·(256 − α)) >> 8 per
/// channel (the dst term is a guess: every checked pixel lies on black). The white backgrounds of the IdeaIcons,
/// inventory pictures and the PDA map's place-selector compass (x.pff 616) have α 0.
pub fn alpha_px(src: [u8; 3], dst: [u8; 3], a: u8) -> [u8; 3] {
    let a = a as u32;
    [0, 1, 2].map(|i| ((src[i] as u32 * a + dst[i] as u32 * (256 - a)) >> 8) as u8)
}

/// Copy a picture with its top-left at (x, y), clipped to the screen; [`KEY`] pixels are skipped, an alpha plane is
/// applied (H1023).
pub fn blit(fb: &mut [u8], x: i32, y: i32, p: &Picture) {
    if let Some(al) = &p.alpha {
        return blit_fn(fb, x, y, p, |px, py, src, dst| match al[py as usize * p.width + px as usize] {
            _ if src == KEY => None,
            255 => Some(src),
            0 => None,
            a => Some(alpha_px(src, dst, a)),
        });
    }
    blit_key(fb, x, y, p)
}

/// [`blit`] without the alpha plane: [`KEY`] pixels skipped, every other pixel copied.
pub fn blit_key(fb: &mut [u8], x: i32, y: i32, p: &Picture) {
    for py in 0..p.height {
        let sy = y + py as i32;
        if sy < 0 || sy >= SCREEN_H as i32 {
            continue;
        }
        for px in 0..p.width {
            let sx = x + px as i32;
            if sx < 0 || sx >= SCREEN_W as i32 {
                continue;
            }
            let (s, d) = ((py * p.width + px) * 3, (sy as usize * SCREEN_W + sx as usize) * 3);
            if p.rgb[s..s + 3] != KEY {
                fb[d..d + 3].copy_from_slice(&p.rgb[s..s + 3]);
            }
        }
    }
}

/// Draw a picture with its top-left at (x, y), clipped to the screen; `f(px, py, src, dst)` gives the pixel to write
/// (None = leave the screen pixel).
pub fn blit_fn(fb: &mut [u8], x: i32, y: i32, p: &Picture, f: impl Fn(i32, i32, [u8; 3], [u8; 3]) -> Option<[u8; 3]>) {
    for py in 0..p.height {
        let sy = y + py as i32;
        if sy < 0 || sy >= SCREEN_H as i32 {
            continue;
        }
        for px in 0..p.width {
            let sx = x + px as i32;
            if sx < 0 || sx >= SCREEN_W as i32 {
                continue;
            }
            let (s, d) = ((py * p.width + px) * 3, (sy as usize * SCREEN_W + sx as usize) * 3);
            let src = [p.rgb[s], p.rgb[s + 1], p.rgb[s + 2]];
            let dst = [fb[d], fb[d + 1], fb[d + 2]];
            if let Some(c) = f(px as i32, py as i32, src, dst) {
                fb[d..d + 3].copy_from_slice(&c);
            }
        }
    }
}

pub fn rect(fb: &mut [u8], x1: i32, y1: i32, x2: i32, y2: i32, c: [u8; 3]) {
    fill(fb, x1, y1, x2 + 1, y1 + 1, c);
    fill(fb, x1, y2, x2 + 1, y2 + 1, c);
    fill(fb, x1, y1, x1 + 1, y2 + 1, c);
    fill(fb, x2, y1, x2 + 1, y2 + 1, c);
}

pub fn write_ppm(path: &Path, fb: &[u8]) -> std::io::Result<()> {
    let mut out = format!("P6\n{SCREEN_W} {SCREEN_H}\n255\n").into_bytes();
    out.extend_from_slice(fb);
    std::fs::write(path, out)
}

/// A sound that should be playing ([`Session::audio_cues`]): `pcm` frames `start..end`, now at `pos`.
#[derive(Clone, Debug)]
pub struct Cue {
    pub asset: u32,
    pub pcm: Arc<Pcm>,
    pub pos: usize,
    pub start: usize,
    pub end: usize,
    pub looped: bool,
}

/// One step of a scripted run.
#[derive(Clone, Debug)]
pub enum Step {
    /// click the clickable whose owner is this id, at a point inside its rect found by hit-testing
    Click(u32),
    /// click an active owner that has no known screen position (e.g. a VCEmotionIcon of the dialogue bar)
    ClickOwner(u32),
    /// a user click taken from a trace: if the owner is not active (the core has no rule creating it) it is
    /// registered with the traced TriggerList first; clicked by position when it has a region, else by owner
    Traced(u32, u32),
    /// a user click taken from a trace with every hypothesis on: by position when the owner is clickable on screen, else
    /// by owner; never registers anything (the owner must have been created by the core)
    User(u32),
    /// fire an event on a registered owner from outside (e.g. Timer Expiration: type-2 actions are not implemented)
    Fire(u32, u8),
    /// register + activate owners the core cannot create (input taken from a trace): (owner, TriggerList)
    Register(Vec<(u32, u32)>),
    /// the user does nothing until every one-shot timer has expired (H59) and the world is idle again
    WaitTimers,
}

/// Result of [`smoke`].
#[derive(Debug, Default)]
pub struct SmokeResult {
    /// transitions (view, kind) in order
    pub transitions: Vec<(u32, u32)>,
    /// steps whose target was not clickable when its turn came
    pub failed: Vec<String>,
    pub frames: Vec<PathBuf>,
}

/// Advance the clock in `dt` steps until nothing is queued or playing, or `max_ms` passed.
pub fn settle(s: &mut Session, dt: u64, max_ms: u64) {
    let t0 = s.now;
    s.tick(0);
    while !s.idle() && s.now - t0 < max_ms {
        s.tick(dt);
    }
}

/// Like [`settle`], but also until no one-shot timer (H59) is running.
pub fn settle_timers(s: &mut Session, dt: u64, max_ms: u64) {
    let t0 = s.now;
    s.tick(0);
    // the menu stops the game's clock (H404): its timers wait
    while (!s.idle() || (!s.r.game_paused() && s.r.one_shot_timers() > 0)) && s.now - t0 < max_ms {
        s.tick(dt);
    }
}

/// Headless run: New Game, then `steps`, settling after each; frames dumped to `dump` (PPM) after every step.
pub fn smoke(s: &mut Session, steps: &[Step], dump: Option<&Path>) -> SmokeResult {
    let mut res = SmokeResult::default();
    let shot = |s: &mut Session, res: &mut SmokeResult, name: String| {
        if let Some(d) = dump {
            let fb = s.render();
            let p = d.join(name);
            if write_ppm(&p, &fb).is_ok() {
                res.frames.push(p);
            }
        }
    };
    s.r.new_game();
    settle(s, 100, 600_000);
    shot(s, &mut res, "00-start.ppm".into());
    for (i, st) in steps.iter().enumerate() {
        match st {
            Step::Register(v) => {
                for &(o, tl) in v {
                    s.r.external_register(o, tl);
                }
            }
            Step::Traced(o, tl) => {
                if !s.r.active.contains(o) && !s.r.w.hotspots.contains(o) {
                    s.r.external_register(*o, *tl);
                    settle(s, 100, 600_000);
                }
                match click_point(s, *o) {
                    Some((x, y)) => {
                        s.hover_at(x, y);
                        s.click_at(x, y);
                    }
                    None => {
                        if !s.r.click(*o) {
                            res.failed.push(format!("step {i}: {o:#x} not active"));
                        }
                    }
                }
            }
            Step::Fire(o, ev) => s.r.external_fire(*o, *ev),
            Step::User(o) => {
                if !user_click(s, *o) {
                    res.failed.push(format!("step {i}: {o:#x} not active"));
                }
            }
            Step::WaitTimers => settle_timers(s, 100, 600_000),
            Step::ClickOwner(o) => {
                if !s.r.click(*o) {
                    res.failed.push(format!("step {i}: {o:#x} not active"));
                }
            }
            Step::Click(o) => {
                let Some(c) = s.r.clickables().into_iter().find(|c| c.owner == *o) else {
                    res.failed.push(format!("step {i}: {o:#x} not clickable in view {:x?}", s.r.w.view));
                    continue;
                };
                let _ = c;
                match click_point(s, *o) {
                    Some((x, y)) => {
                        s.hover_at(x, y);
                        s.click_at(x, y);
                    }
                    None => res.failed.push(format!("step {i}: {o:#x} covered by other clickables")),
                }
            }
        }
        settle(s, 100, 600_000);
        shot(s, &mut res, format!("{:02}-{}.ppm", i + 1, match st {
            Step::Click(o) | Step::ClickOwner(o) | Step::Traced(o, _) | Step::User(o) => format!("click-{o:x}"),
            Step::Register(_) => "register".into(),
            Step::Fire(o, ev) => format!("event{ev}-{o:x}"),
            Step::WaitTimers => "wait-timers".into(),
        }));
    }
    res.transitions = s.r.transitions.clone();
    res
}

/// [`Step::User`]: click `owner` by position if it is clickable on screen, else by owner. False if it is not active.
pub fn user_click(s: &mut Session, owner: u32) -> bool {
    match click_point(s, owner) {
        Some((x, y)) => {
            s.hover_at(x, y);
            s.click_at(x, y).is_some()
        }
        None => s.r.click(owner),
    }
}

/// A point of `owner`'s rect whose topmost clickable is `owner`.
pub fn click_point(s: &Session, owner: u32) -> Option<(i32, i32)> {
    let c = s.r.clickables().into_iter().find(|c| c.owner == owner)?;
    let [x1, y1, x2, y2] = c.rect.map(|v| v as i32);
    (y1..y2)
        .step_by(4)
        .flat_map(|y| (x1..x2).step_by(4).map(move |x| (x + 2, y + 2)))
        .find(|&(x, y)| s.r.hit(x, y).map(|h| h.owner) == Some(owner))
}

/// The user's clicks of a trace as steps: every Mouse Click (event 0) fired on a registered owner, with the
/// TriggerList the trace registered it with; plus Timer Expiration events (7), which the core cannot generate.
pub fn trace_route(trace: &[crate::replay::TraceEv]) -> Vec<Step> {
    trace_route_with(trace, false)
}

/// [`trace_route`]; with `hyp` (every hypothesis on) a traced Timer Expiration becomes [`Step::WaitTimers`] — the user's
/// input is only that they waited; the event itself comes from the core's timer (H59) — and a click becomes
/// [`Step::User`]: the clicked owner (EmotionIcon H60, InterfaceItem H58, Explorable H55) must come from the core.
pub fn trace_route_with(trace: &[crate::replay::TraceEv], hyp: bool) -> Vec<Step> {
    let core_timers = hyp;
    use crate::replay::Ev;
    let mut owner_of = HashMap::new();
    let mut tl_of = HashMap::new();
    let mut steps = Vec::new();
    for e in trace {
        match &e.ev {
            Ev::Reg { scope, owner, tl } => {
                owner_of.insert(*scope, *owner);
                tl_of.insert(*owner, *tl);
            }
            Ev::Fire { scope, event: 0, .. } => {
                if let Some(&o) = owner_of.get(scope) {
                    steps.push(if hyp { Step::User(o) } else { Step::Traced(o, tl_of[&o]) });
                }
            }
            // Timer Expiration (fo-007: on View 0x2b3 30.3 s after type-2 action 0x9d59 `00007530 0000`)
            Ev::Fire { scope, event: 7, .. } => {
                if core_timers {
                    steps.push(Step::WaitTimers);
                } else if let Some(&o) = owner_of.get(scope) {
                    steps.push(Step::Fire(o, 7));
                }
            }
            _ => {}
        }
    }
    steps
}

/// The user route of fo-009 (traces/fo-009.md) as screen clicks, plus the inputs the core cannot create, as traced:
/// the dialogue icon 0x4700 (registered by PreloadEmIcon, not implemented; no region → clicked by owner) and the
/// drawer's IFaceLayout 0xe4b9 + InterfaceItems of View 0x4cc (shown / hidden by type-5 actions 0xe4e5 / 0xe4e9, not
/// implemented): registered on each entry into 0x4cc and deactivated on leaving it (fo-009 330.3, 345.8, 383.7, 397.5).
/// The user route of fo-009 with every hypothesis on ([`rules_hyp`]): screen clicks and the click on the dialogue icon
/// 0x4700 only — the icon (H60), the drawer's layout, InterfaceItems and their assets (H58) come from the core.
pub fn route_fo009_core() -> Vec<Step> {
    vec![
        Step::ClickOwner(0x4700), // dialogue choice (fo-009 188.15); the icon has no region → clicked by owner
        Step::Click(0x2b9),       // right
        Step::Click(0x552),       // forward
        Step::Click(0x404),       // left
        Step::Click(0x521),       // forward
        Step::Click(0x4cd),       // left → 0x4cc
        Step::Click(0xe4bf),      // open the drawer (InterfaceItem) → 0x14983
        Step::Click(0x14987),     // close the drawer (VCNav) → 0x4cc
        Step::Click(0x4d1),       // left → 0x4c6
    ]
}

pub fn route_fo009() -> Vec<Step> {
    let items = [(0xe4b9, 0), (0xe4bf, 0xe4d8), (0xe4c1, 0xe4dd), (0xe4c3, 0xe4d3), (0xe4c5, 0xe4ce), (0xe4c7, 0xe4c9)];
    let hide = || items.iter().map(|&(o, _)| Step::Fire(o, crate::world::event::DEACTIVATION)).collect::<Vec<_>>();
    let mut r = vec![
        Step::Register(vec![(0x4700, 0x6fd9)]),
        Step::ClickOwner(0x4700), // dialogue choice (fo-009 188.15)
        Step::Click(0x2b9),       // right
        Step::Click(0x552),       // forward
        Step::Click(0x404),       // left
        Step::Click(0x521),       // forward
        Step::Click(0x4cd),       // left → 0x4cc
        Step::Register(items.to_vec()),
        Step::Click(0xe4bf), // open the drawer (InterfaceItem) → 0x14983
    ];
    r.extend(hide());
    r.push(Step::Click(0x14987)); // close the drawer (VCNav) → 0x4cc
    r.push(Step::Register(items.to_vec()));
    r.push(Step::Click(0x4d1)); // left → 0x4c6
    r.extend(hide());
    r
}
