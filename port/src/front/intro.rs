//! The application's cold start (port side): the intro movies before the main menu, then the menu with no game running.
//!
//! CONFIRMED C240 / C241 (fo-225 / fo-226, AG18: `traces/fo-225.jsonl`, `traces/fo-226.jsonl`, shots
//! `traces/fo-225.jsonl.burst/`); static side H1660 / H1661 (RE_NOTES). After the startup (C33) the app start calls the
//! intro routine (**FUN_004721a0**), which — unless PrefSkipIntro (var 0x3e2; static only) — plays four movies natively
//! (no REG / View / trigger in the trace; the port resolves their files through the AssetRefs the routine names), each
//! at the previous one's end:
//! **0xdac3** XV/56003.xmv Fox Interactive (9.04 s, 140×144 at (250,168)), **0xdac2** XV/56002.xmv HyperBole (10.01 s,
//! 560×400 at (40,40)), **0x4cd4** XV/19668.xmv the opening cinematic (72.375 s, 600×240 at (20,120)), **0xdac1**
//! XV/56001.xmv the title (45.02 s, 560×400 at (40,40)) — each centred on black; without input the menu
//! comes 136.6 s after the first logo (C240).
//! - Skip (C241): one left or right **button down** ends the current movie only, the next starts at once; after the 4th
//!   the main menu is built at once. Esc / Space / Enter and pointer moves skip nothing, and the keys are never delivered
//!   later. Static (not traced): Alt+X (WM_SYSKEYDOWN 'X' in the intro's message loop) ends the intro with 0 → the program exits.
//! - Static (not traced): the intro routine checks 0xdac3's media first — not found → no intro.
//! - Then (C240) the main menu (0xc1af / 0xc1b1 / 0x17070 / 0xc1b5 / 0xc1b3 within 0.3 s; H404's screen) with no game
//!   running; no attract loop.
//! Port assumption: each movie's sound track plays with it (the bottle had no audio driver, E7).
use super::{Cue, Session, SCREEN_H, SCREEN_W};

/// The intro routine's movies in play order (AssetRefs).
pub const MOVIES: [u32; 4] = [0xdac3, 0xdac2, 0x4cd4, 0xdac1];
/// PrefSkipIntro (H1660, static)
pub const VAR_SKIP_INTRO: u32 = 0x3e2;

/// How the program starts (the window's / the web page's choice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// as the original: the intro movies (unless PrefSkipIntro), then the main menu
    Intro,
    /// the main menu at once (`--skip-intro`)
    Menu,
    /// straight into New Game (the port's old start; `--new-game`)
    NewGame,
}

/// The intro's state: which movie plays and since when (the intro's own clock; the game's does not run).
#[derive(Clone, Debug, Default)]
pub struct Intro {
    /// index into [`MOVIES`]
    pub i: usize,
    /// intro clock, ms
    pub clock: u64,
    /// intro clock when the current movie started
    pub t0: u64,
}

impl Session<'_> {
    /// The cold start up to the first interactive screen: the startup (H69) and then the intro movies (H1660) or the
    /// main menu at once. `Start::NewGame` runs New Game as before.
    pub fn boot(&mut self, start: Start) {
        // C86: the application reads the host's Preferences before anything runs (the intro's PrefSkipIntro, the menu)
        self.apply_host_prefs();
        if start == Start::NewGame {
            self.r.new_game();
            return;
        }
        self.r.boot();
        let skip = self.r.w.vars.get(VAR_SKIP_INTRO).is_some_and(|v| v != 0);
        // the first movie's media missing → no intro (H1660)
        if start == Start::Intro && !skip && self.clip(MOVIES[0]).is_some() {
            self.intro = Some(Intro::default());
            self.log.push(format!("H1660: intro movie {:#x}", MOVIES[0]));
            self.intro_settle();
        } else {
            self.intro_done();
        }
    }

    /// The intro is playing.
    pub fn intro_playing(&self) -> bool {
        self.intro.is_some()
    }

    /// The intro movie on screen now.
    pub fn intro_movie(&self) -> Option<u32> {
        self.intro.as_ref().map(|i| MOVIES[i.i])
    }

    /// A button down (C241): the current movie ends, the next one starts.
    pub fn intro_skip(&mut self) -> Option<String> {
        let a = self.intro_movie()?;
        self.intro_next();
        Some(format!("H1660: intro movie {a:#x} skipped"))
    }

    /// The intro's clock (the game's is not running).
    pub(super) fn intro_tick(&mut self, dt: u64) {
        let Some(it) = self.intro.as_mut() else { return };
        it.clock += dt;
        self.intro_settle();
    }

    /// Movies whose time is up (or that do not open) end; past the last, the main menu.
    fn intro_settle(&mut self) {
        while let Some(it) = self.intro.as_ref() {
            let (a, since, t0) = (MOVIES[it.i], it.clock - it.t0, it.t0);
            let len = if self.clip(a).is_some() && self.intro_has_video(a) { self.duration(a) } else { 0 };
            if since < len {
                return;
            }
            // C240: the next movie starts at the previous one's end (not at the tick that noticed it)
            let end = t0 + len;
            self.intro_next();
            if let Some(it) = self.intro.as_mut() {
                it.t0 = end;
            }
        }
    }

    fn intro_has_video(&mut self, a: u32) -> bool {
        let Some(c) = self.clip(a) else { return false };
        self.player(&c.file).is_some_and(|p| p.has_video())
    }

    fn intro_next(&mut self) {
        let Some(it) = self.intro.as_mut() else { return };
        it.i += 1;
        it.t0 = it.clock;
        if it.i >= MOVIES.len() {
            self.intro = None;
            self.intro_done();
        } else {
            let a = MOVIES[it.i];
            self.log.push(format!("H1660: intro movie {a:#x}"));
        }
    }

    /// H1661: the intro is over — the main-menu object is built.
    fn intro_done(&mut self) {
        self.r.open_main_menu();
        self.log.push("H1661: main menu (no game running)".into());
    }

    /// The intro frame: the current movie centred on black.
    pub fn render_intro(&mut self) -> Vec<u8> {
        let mut fb = vec![0u8; SCREEN_W * SCREEN_H * 3];
        let Some(it) = self.intro.clone() else { return fb };
        let a = MOVIES[it.i];
        let Some(c) = self.clip(a) else { return fb };
        if let Some(p) = self.frame(&c, self.intro_picture_ms(&it), false, self.game_lang) {
            let (w, h) = (p.width, p.rgb.len().checked_div(p.width * 3).unwrap_or(0).min(p.height));
            let x0 = (SCREEN_W as i32 - w as i32) / 2;
            let y0 = (SCREEN_H as i32 - h as i32) / 2;
            for y in 0..h {
                let sy = y0 + y as i32;
                if !(0..SCREEN_H as i32).contains(&sy) {
                    continue;
                }
                let xa = x0.max(0);
                let xb = (x0 + w as i32).min(SCREEN_W as i32);
                if xb <= xa {
                    continue;
                }
                let s = (y * w + (xa - x0) as usize) * 3;
                let d = (sy as usize * SCREEN_W + xa as usize) * 3;
                let n = (xb - xa) as usize * 3;
                fb[d..d + n].copy_from_slice(&p.rgb[s..s + n]);
            }
        }
        fb
    }

    /// H1795: the intro movie's time the picture shows — its clock less the picture delay ([`Session::av_delay`]).
    pub fn intro_picture_ms(&self, it: &Intro) -> u64 {
        (it.clock - it.t0).saturating_sub(self.av_delay)
    }

    /// The intro movie's sound track (port assumption: it plays with the picture).
    pub fn intro_audio(&mut self) -> Vec<Cue> {
        let Some(it) = self.intro.clone() else { return Vec::new() };
        let a = MOVIES[it.i];
        let Some(pcm) = self.sound(a) else { return Vec::new() };
        let total = pcm.frames();
        let pos = ((it.clock - it.t0) * pcm.rate as u64 / 1000) as usize;
        if pos >= total {
            return Vec::new();
        }
        vec![Cue { asset: a, pcm, pos, start: 0, end: total, looped: false }]
    }
}
