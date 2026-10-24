//! The game's menu screen (Esc / the top-right object, H169; also the main menu): a native object of the original
//! (FUN_0048f670), drawn by the front-end — the core only knows it is open (`Runner::menu_open`) and what was chosen
//! (`Runner::menu_choose`).
//!
//! HYPOTHESIS H404 (static, not traced — screenshots of the original's menu exist only as the
//! bot's E9 notes: "New" (570,101), "Load" (555,145), "Save" (555,195), "Return"):
//! - pictures: the object's activation builds players of four AssetRefs and one picture — 0xc1af
//!   LEFTmain.mov (XG/49583.xmv) in (0,0)–(472,480), 0xc1b1 MIDloop.mov (XG/49585) in (61,103)–(449,413), 0xc1b5
//!   TOPro.mov (XG/49589) in (57,0)–(409,60), 0xc1b3 RIGHT.mov (XG/49587, the item labels) in (472,0)–(640,480), the
//!   sound 0x17070 MIDloop.amv, and — while a game runs — 0x11cbf return.pic (x.pff 610) at (472,215)–(640,273).
//!   The rects are static constants of the original.
//! - items (hit rects all x 472..638, height 36): New y 78, Load 126, Save 175 (only while a game runs), Return 225,
//!   Options 275, Help 330, Quit 385. A click (FUN_004978a0) plays the click sound 0x107be, seeks RIGHT.mov to the
//!   item's "chosen" time, then New → a new game, Load → the load screen (H70), Save → the save screen (H140), Return →
//!   the menu removed and the game unpaused, Options → the preferences page + show 0xc24a, Help → show 0xc51c, Quit →
//!   the program's exit request.
//! - hover (FUN_0045eac0): an item plays RIGHT.mov from its "hover" time to its "chosen" time (the label turns into a
//!   description, e.g. the New item's description at 6700) and holds there; no item → the idle frame 5000; TOPro plays
//!   while the pointer is in its rect, else stops and rewinds. Times (ms, static): New 6200→6700, Load 7000→7500, Save
//!   7800→8300, Return 8600→9100, Options 9400→9900, Help 10200→10700, Quit 11000→11500. return.pic shows when no item
//!   is hovered.
//! - Opening (C290, fo-225 / fo-226 / fo-224 / fo-228 / fo-242): the **first** menu of the program (no menu shown
//!   before, PrefSkipIntro off) plays LEFTmain.mov 0→5400 alone (a blocking wait), then
//!   RIGHT.mov 0→5000 (the items fade in), MIDloop.mov and MIDloop.amv start together at LEFTmain's end (mv_start
//!   +5.42 s); every later opening (the Esc menu, the menu after a game end) starts only MIDloop.mov / MIDloop.amv,
//!   the others shown at their end frames. H2344: the first opening's two waits (LEFTmain, then RIGHT.mov 0→5000) take
//!   no pointer input (no message pump; fo-224 / fo-226); what came meanwhile is handled at their end. The game is paused while the menu shows (the front's clock stops; queued
//!   commands still run); Esc while the menu shows = Return (port choice).
//! - Sound (H1770, static + data, not traced — the bottle has no audio driver): the menu's players are QuickTime movies
//!   with their sound tracks (each gets its volume, mv_vol, at its creation in every trace): LEFTmain.mov, RIGHT.mov and
//!   TOPro.mov have ima4 tracks, MIDloop.mov none. RIGHT.mov's track has a short sound in each item's hover range, so
//!   the hover (a play range hover → chosen) **is** the hover sound; a click (stop, seek to the chosen time) and the
//!   idle seek (no item: a range 5000 → 5000) are silent; TOPro's sound loops with its picture while the pointer is in
//!   its rect; LEFTmain's / RIGHT.mov's opening ranges sound at the first opening.
use super::{Session, SCREEN_H, SCREEN_W};
use crate::world::devices::MenuChoice;

pub const LEFT: (u32, i32, i32) = (0xc1af, 0, 0);
pub const MID: (u32, i32, i32) = (0xc1b1, 61, 103);
pub const TOP: (u32, i32, i32) = (0xc1b5, 57, 0);
pub const RIGHT: (u32, i32, i32) = (0xc1b3, 472, 0);
pub const RETURN_PIC: (u32, i32, i32) = (0x11cbf, 472, 215);
pub const MID_SOUND: u32 = 0x17070;
pub const CLICK_SOUND: u32 = 0x107be;
pub const TOP_RECT: [i32; 4] = [57, 0, 409, 60];
/// RIGHT.mov time of the idle labels (H404)
pub const IDLE_MS: u64 = 5000;
/// LEFTmain.mov intro length (C290)
pub const LEFT_INTRO_MS: u64 = 5400;

/// (choice, hit rect x1 y1 x2 y2, RIGHT.mov hover start, chosen time)
pub const ITEMS: [(MenuChoice, [i32; 4], u64, u64); 7] = [
    (MenuChoice::New, [472, 78, 638, 114], 6200, 6700),
    (MenuChoice::Load, [472, 126, 638, 162], 7000, 7500),
    (MenuChoice::Save, [472, 175, 638, 211], 7800, 8300),
    (MenuChoice::Resume, [472, 225, 638, 261], 8600, 9100),
    (MenuChoice::Options, [472, 275, 638, 311], 9400, 9900),
    (MenuChoice::Help, [472, 330, 638, 366], 10200, 10700),
    (MenuChoice::Quit, [472, 385, 638, 421], 11000, 11500),
];

fn inside(r: [i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

/// The menu's display state (its own clock: the game's is stopped while it shows).
#[derive(Clone, Debug, Default)]
pub struct MenuUi {
    /// menu clock, ms (advances while the menu shows)
    pub clock: u64,
    /// the menu was open at the last tick
    pub shown: bool,
    pub opened: u64,
    /// this opening plays the opening movies (C290: only the program's first menu, no game running)
    pub first: bool,
    /// a menu was shown before in this program (C290; kept across a new game / a load)
    pub seen: bool,
    /// hovered item (index into [`ITEMS`]) and since when
    pub hover: Option<(usize, u64)>,
    /// TOPro hovered since
    pub top: Option<u64>,
    /// the item clicked last (RIGHT.mov holds its chosen time) and when
    pub chosen: Option<(usize, u64)>,
    /// one-shot sounds (asset, start clock)
    pub sounds: Vec<(u32, u64)>,
    /// New / Quit chosen: the host's to act on (a new session / the end of the program)
    pub request: Option<MenuChoice>,
    /// H2344: pointer input that came during the first opening (the original's blocking waits pump no messages), in
    /// order, consecutive moves merged; handled at the opening's end
    pub pending: Vec<MenuInput>,
}

/// A pointer event held back during the first opening (H2344).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuInput {
    Move(i32, i32),
    Click(i32, i32),
}

impl MenuUi {
    /// The item at a screen point (Save only while a game runs).
    pub fn item_at(x: i32, y: i32, game_running: bool) -> Option<usize> {
        ITEMS.iter().position(|(c, r, ..)| inside(*r, x, y) && (*c != MenuChoice::Save || game_running))
    }

    /// When RIGHT.mov / MIDloop start (C290): LEFTmain's end at the first opening, else at once.
    pub fn rest_start(&self) -> u64 {
        self.opened + if self.first { LEFT_INTRO_MS } else { 0 }
    }

    /// H2344: the first opening's two blocking waits run (C290: LEFTmain.mov 0→5400, then RIGHT.mov 0→5000 inside the
    /// menu opening routine FUN_0044e060, no message pump) — the menu takes no pointer input yet.
    pub fn opening(&self) -> bool {
        self.first && self.clock < self.rest_start() + IDLE_MS
    }

    /// LEFTmain.mov time now (its last frame after the opening).
    pub fn left_time(&self) -> u64 {
        let end = LEFT_INTRO_MS - 1;
        if self.first {
            (self.clock - self.opened).min(end)
        } else {
            end
        }
    }

    /// RIGHT.mov time now.
    pub fn right_time(&self) -> u64 {
        // a click seeks to the item's chosen time
        if let Some((i, _)) = self.chosen {
            return ITEMS[i].3;
        }
        if let Some((i, t)) = self.hover {
            return (ITEMS[i].2 + (self.clock - t)).min(ITEMS[i].3);
        }
        if self.first {
            return self.clock.saturating_sub(self.rest_start()).min(IDLE_MS);
        }
        IDLE_MS
    }

    /// RIGHT.mov's playing range now (H1770: its sound): (range start, range end, time), ms. None while it stands still.
    pub fn right_playing(&self) -> Option<(u64, u64, u64)> {
        if self.chosen.is_some() {
            return None;
        }
        if let Some((i, t)) = self.hover {
            let at = ITEMS[i].2 + (self.clock - t);
            return (at < ITEMS[i].3).then_some((ITEMS[i].2, ITEMS[i].3, at));
        }
        let r0 = self.rest_start();
        (self.first && self.clock >= r0 && self.clock - r0 < IDLE_MS).then(|| (0, IDLE_MS, self.clock - r0))
    }
}

impl Session<'_> {
    /// The pointer over the menu (H404 hover).
    pub fn menu_hover(&mut self, x: i32, y: i32) -> Option<MenuChoice> {
        // H2344: no hover (no item / TOPro reaction, no hover sound) during the first opening; the last position counts
        // at its end (the queued WM_MOUSEMOVE)
        if self.menu.opening() {
            match self.menu.pending.last_mut() {
                Some(MenuInput::Move(px, py)) => (*px, *py) = (x, y),
                _ => self.menu.pending.push(MenuInput::Move(x, y)),
            }
            return None;
        }
        let now = self.menu.clock;
        let it = MenuUi::item_at(x, y, self.r.game_running);
        if it != self.menu.hover.map(|h| h.0) {
            self.menu.hover = it.map(|i| (i, now));
        }
        let top = inside(TOP_RECT, x, y);
        if top != self.menu.top.is_some() {
            self.menu.top = top.then_some(now);
        }
        it.map(|i| ITEMS[i].0)
    }

    /// A click on the menu (H404): the item's choice goes to the core, New / Quit become the host's request.
    pub fn menu_click(&mut self, x: i32, y: i32) -> Option<MenuChoice> {
        // H2344: a click during the first opening is held and handled at its end (fo-226: the bot's click at 59.8 reached
        // the window proc at 62.367, RIGHT.mov's wait end, and started the New Game)
        if self.menu.opening() {
            self.menu.pending.push(MenuInput::Click(x, y));
            return None;
        }
        let i = MenuUi::item_at(x, y, self.r.game_running)?;
        let c = ITEMS[i].0;
        self.menu.chosen = Some((i, self.menu.clock));
        self.menu.sounds.push((CLICK_SOUND, self.menu.clock));
        self.menu_choose(c);
        Some(c)
    }

    /// Choose a menu item (also the keyboard's Esc = Return).
    pub fn menu_choose(&mut self, c: MenuChoice) {
        // C243 (H1625 REFUTED, H1662 REFUTED for a game end): with no game running "Return" starts a new game — PrefClick,
        // the startup list 0x66d6, transition_new 0x2b3, 0x6298 (C240's New Game); H1701: the cold start's menu (no
        // game ever ran) the same (not traced)
        let c = if c == MenuChoice::Resume && !self.r.game_running {
            self.log.push("menu: Return with no game running → New Game (C243)".into());
            MenuChoice::New
        } else {
            c
        };
        if matches!(c, MenuChoice::New | MenuChoice::Quit) {
            self.menu.request = Some(c);
        }
        self.r.menu_choose(c);
        self.log.push(format!("menu: {c:?}"));
    }

    /// Clock hook: the menu's clock runs while it shows; an opening restarts its intro.
    pub(super) fn menu_tick(&mut self, dt: u64) {
        let open = self.r.menu_open;
        if open && !self.menu.shown {
            let clock = self.menu.clock;
            // C290: the opening movies only at the program's first menu (the flag is set after them; no game ran: the
            // cold start's menu) and not with PrefSkipIntro
            let skip = self.r.w.vars.get(super::intro::VAR_SKIP_INTRO).is_some_and(|v| v != 0);
            let first = !self.menu.seen && !self.r.game_running && !skip;
            self.menu = MenuUi {
                clock,
                opened: clock,
                shown: true,
                first,
                seen: true,
                request: self.menu.request,
                ..Default::default()
            };
        }
        self.menu.shown = open;
        if open {
            self.menu.clock += dt;
        }
        // H2344: the opening's end delivers the input held during it, in order (the rest dropped once the menu closes)
        if open && !self.menu.pending.is_empty() && !self.menu.opening() {
            for ev in std::mem::take(&mut self.menu.pending) {
                if !self.r.menu_open {
                    break;
                }
                match ev {
                    MenuInput::Move(x, y) => {
                        self.menu_hover(x, y);
                    }
                    MenuInput::Click(x, y) => {
                        self.menu_click(x, y);
                    }
                }
            }
        }
        let c = self.menu.clock;
        self.menu.sounds.retain(|&(_, t)| c - t < 3000);
    }

    /// Frame of a menu movie at `t` ms (whole movie; `looped` wraps), with its size.
    fn menu_movie(&mut self, asset: u32, t: u64, looped: bool) -> Option<(Vec<u8>, usize, usize)> {
        let c = self.clip(asset)?;
        let f = self.lang_file(&c, self.game_lang);
        let p = self.player(&f)?;
        if !p.has_video() {
            return None;
        }
        let mt = super::movie_time(&c, t, looped, p.duration_ms());
        match p.seek(mt) {
            Ok(true) => Some((p.rgb.clone(), p.width, p.height)),
            _ => None,
        }
    }

    /// Compose the menu screen (H404).
    pub fn render_menu(&mut self) -> Vec<u8> {
        let mut fb = vec![0u8; SCREEN_W * SCREEN_H * 3];
        let since = self.menu.clock - self.menu.opened;
        let top_t = self.menu.top.map_or(0, |t| self.menu.clock - t);
        let right_t = self.menu.right_time();
        let mid_t = self.menu.clock.saturating_sub(self.menu.rest_start());
        // H2341: during the first opening's LEFTmain.mov intro only LEFTmain (and RIGHT.mov at its start, black) shows —
        // MIDloop.mov and TOPro.mov come with the rest at LEFTmain's end (C290; the original's shot fo-226 c049.9)
        let intro = self.menu.first && self.menu.clock < self.menu.rest_start();
        for (asset, x, y, t, looped) in [
            (LEFT.0, LEFT.1, LEFT.2, self.menu.left_time(), false),
            (MID.0, MID.1, MID.2, mid_t, true),
            (TOP.0, TOP.1, TOP.2, top_t, true),
            (RIGHT.0, RIGHT.1, RIGHT.2, right_t, false),
        ] {
            if intro && (asset == MID.0 || asset == TOP.0) {
                continue;
            }
            if let Some((px, w, h)) = self.menu_movie(asset, t, looped) {
                put(&mut fb, x, y, &px, w, h);
            }
        }
        // H823: return.pic ("Return", English in this build) only while a game runs (fo-086 k01: "Return" in
        // the Esc menu); after a game end RIGHT.mov's own label "Return" shows (fo-091 k14)
        let idle = !self.menu.first || since >= LEFT_INTRO_MS + IDLE_MS;
        if self.r.game_running && self.menu.hover.is_none() && self.menu.chosen.is_none() && idle {
            if let Some(p) = self.picture(RETURN_PIC.0).cloned() {
                super::blit(&mut fb, RETURN_PIC.1, RETURN_PIC.2, &p);
            }
        }
        fb
    }

    /// The menu's sounds: MIDloop.amv looped from RIGHT.mov's start (C290), the click sounds once, and the movies' own
    /// sound tracks while they play (H1770): LEFTmain.mov's / RIGHT.mov's opening ranges, RIGHT.mov's hover range (the
    /// hover sound), TOPro.mov looped while hovered.
    pub fn menu_audio(&mut self) -> Vec<super::Cue> {
        let mut out = Vec::new();
        let m = &self.menu;
        let clock = m.clock;
        let r0 = m.rest_start();
        // (asset, range start ms, range end ms or None = the whole track, time in the range ms, looped)
        let mut v: Vec<(u32, u64, Option<u64>, u64, bool)> = Vec::new();
        if clock >= r0 {
            v.push((MID_SOUND, 0, None, clock - r0, true));
        }
        v.extend(m.sounds.iter().map(|&(a, t)| (a, 0, None, clock - t, false)));
        if m.first && clock - m.opened < LEFT_INTRO_MS {
            v.push((LEFT.0, 0, Some(LEFT_INTRO_MS), clock - m.opened, false));
        }
        if let Some((s0, s1, at)) = m.right_playing() {
            v.push((RIGHT.0, s0, Some(s1), at - s0, false));
        }
        if let Some(t) = m.top {
            v.push((TOP.0, 0, None, clock - t, true));
        }
        for (a, s0, s1, t, looped) in v {
            let Some(pcm) = self.sound(a) else { continue };
            let total = pcm.frames();
            let ms = |t: u64| ((t * pcm.rate as u64 / 1000) as usize).min(total);
            let (start, end) = (ms(s0), s1.map_or(total, ms));
            if end <= start {
                continue;
            }
            let t = (t * pcm.rate as u64 / 1000) as usize;
            let pos = start + if looped { t % (end - start) } else { t };
            if pos >= end {
                continue;
            }
            out.push(super::Cue { asset: a, pcm, pos, start, end, looped });
        }
        out
    }
}

pub(super) fn put(fb: &mut [u8], x: i32, y: i32, px: &[u8], w: usize, h: usize) {
    for row in 0..h {
        let sy = y + row as i32;
        if sy < 0 || sy >= SCREEN_H as i32 {
            continue;
        }
        let x0 = x.max(0);
        let x1 = (x + w as i32).min(SCREEN_W as i32);
        if x1 <= x0 {
            continue;
        }
        let s = (row * w + (x0 - x) as usize) * 3;
        let d = (sy as usize * SCREEN_W + x0 as usize) * 3;
        let n = (x1 - x0) as usize * 3;
        fb[d..d + n].copy_from_slice(&px[s..s + n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_by_point() {
        // the bot's points on the original (E9): "New" (570,101), "Load" (555,145), "Save" (555,195)
        assert_eq!(MenuUi::item_at(570, 101, true).map(|i| ITEMS[i].0), Some(MenuChoice::New));
        assert_eq!(MenuUi::item_at(555, 145, true).map(|i| ITEMS[i].0), Some(MenuChoice::Load));
        assert_eq!(MenuUi::item_at(555, 195, true).map(|i| ITEMS[i].0), Some(MenuChoice::Save));
        assert_eq!(MenuUi::item_at(555, 195, false), None);
        assert_eq!(MenuUi::item_at(300, 195, true), None);
    }

    #[test]
    fn right_movie_times() {
        // C290: the first menu: LEFTmain alone 0..5400, then RIGHT.mov 0→5000
        let mut m = MenuUi { first: true, ..Default::default() };
        m.clock = 2000;
        assert_eq!((m.left_time(), m.right_time(), m.right_playing()), (2000, 0, None));
        m.clock = 7400;
        assert_eq!((m.left_time(), m.right_time()), (LEFT_INTRO_MS - 1, 2000));
        assert_eq!(m.right_playing(), Some((0, IDLE_MS, 2000)));
        m.clock = 11_000;
        assert_eq!((m.right_time(), m.right_playing()), (IDLE_MS, None));
        // H1770: a hover plays its range (the hover sound), then holds at the chosen time, silent
        m.hover = Some((0, 10_900));
        assert_eq!((m.right_time(), m.right_playing()), (6300, Some((6200, 6700, 6300))));
        m.clock = 13_000;
        assert_eq!((m.right_time(), m.right_playing()), (6700, None));
        // a click seeks to the chosen time, silent
        m.hover = Some((1, 12_900));
        m.chosen = Some((1, 12_950));
        assert_eq!((m.right_time(), m.right_playing()), (7500, None));
        // a later opening: no opening movies
        let m = MenuUi { clock: 100, opened: 100, ..Default::default() };
        assert_eq!(
            (m.left_time(), m.right_time(), m.right_playing(), m.rest_start()),
            (LEFT_INTRO_MS - 1, IDLE_MS, None, 100)
        );
    }
}
