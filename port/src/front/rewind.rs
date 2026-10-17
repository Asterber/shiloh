//! The "Try again?" box of a game over (H1700) and the frozen game of a suicide note (H1703), drawn and answered by
//! the front-end — the core only knows the box shows (`Runner::rewind_box`), holds the rewind snapshot
//! (`Runner::rewind_snap`) and closes the box (`Runner::rewind_answer`).
//!
//! Traced (C244 / C245 / C247–C250 / C253 / C254 / C255, `traces/fo-229.jsonl` … `traces/fo-242.jsonl`): EndGame with
//! var 0xa15 ≠ 0 shows AssetRef 0xd3b8 (`Graphics/Game/system dialogs/systemBOXmovie.mov`, XG/54200) with "Try again?",
//! YES at (273,278) and NO at (365,278) (the bot's points); YES → PrefClick 0x107be, the startup list 0x66d6,
//! transition_new to the snapshot's View (the last SaveGameState since the Load, else the loaded save's View); NO →
//! PrefClick, the main menu.
//!
//! HYPOTHESIS H1700 (port, display): the box movie (340×140, Motion-JPEG A) centred on a black screen (the original's box
//! routine FUN_004d72a0 centres it; the screen black as `traces/fo-229.jsonl.shots/end.png`), its frame at the time since the box opened
//! (held at the last); the caption and the two buttons are drawn by the port (Courier New bold, the buttons' rects
//! measured on that shot: YES (238,266)–(309,290), NO (330,266)–(401,290)) — the original's button pictures and the
//! caption's font are not decoded.
use super::{Session, SCREEN_H, SCREEN_W};
use crate::hdb::Hdb;

/// AssetRef of the box movie (the box routine opens it in category 0x35)
pub const BOX: u32 = 0xd3b8;
/// the box movie's size and its centred place
pub const BOX_W: i32 = 340;
pub const BOX_H: i32 = 140;
pub const BOX_X: i32 = (SCREEN_W as i32 - BOX_W) / 2;
pub const BOX_Y: i32 = (SCREEN_H as i32 - BOX_H) / 2;
/// the buttons' hit rects (x1, y1, x2, y2), measured on `traces/fo-229.jsonl.shots/end.png`
pub const YES_RECT: [i32; 4] = [238, 266, 309, 290];
pub const NO_RECT: [i32; 4] = [330, 266, 401, 290];
pub const CAPTION: &str = "Try again?";

fn inside(r: [i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

/// The box's button at a screen point: Some(true) = YES, Some(false) = NO.
pub fn button_at(x: i32, y: i32) -> Option<bool> {
    if inside(YES_RECT, x, y) {
        Some(true)
    } else if inside(NO_RECT, x, y) {
        Some(false)
    } else {
        None
    }
}

impl<'a> Session<'a> {
    /// Clock hook (from [`Session::tick`]): true when the game does not run this tick. H1700: the box's modal loop
    /// — the game's queue and clock stand still, the session clock runs on (the box's movie). H1703: the
    /// frozen game — its timers are paused (the core), the players hold their frames (their start times move with the
    /// clock), the queue still runs (the note's layout, posted after the freeze).
    pub(super) fn rewind_tick(&mut self, dt: u64) -> bool {
        if self.r.rewind_box {
            self.rewind_t0.get_or_insert(self.now);
            self.now += dt;
            return true;
        }
        self.rewind_t0 = None;
        if self.r.frozen {
            self.now += dt;
            for h in &mut self.heads {
                h.1 += dt;
            }
            for l in &mut self.loops {
                l.1 += dt;
            }
            if let Some(b) = &mut self.bg {
                b.1 += dt;
            }
            self.r.advance(self.now);
            self.r.run(100_000);
            return true;
        }
        false
    }

    /// A click while the box shows: YES / NO, elsewhere nothing.
    pub fn rewind_click(&mut self, x: i32, y: i32) {
        match button_at(x, y) {
            Some(true) => self.rewind_yes(),
            Some(false) => self.rewind_no(),
            None => {}
        }
    }

    /// NO (C255): the core's game end (no game running, the main menu), PrefClick 0x107be on the menu.
    pub fn rewind_no(&mut self) {
        if !self.r.rewind_answer(false) {
            return;
        }
        self.menu_tick(0);
        self.menu.sounds.push((super::menu::CLICK_SOUND, self.menu.clock));
        self.log.push("\"Try again?\": NO → the main menu (C255)".into());
    }

    /// YES (C244 / C245): a Load of the rewind snapshot — the last SaveGameState's state written as a save over the
    /// session's gam (H1700), else the loaded save itself; a new game from its gam when the session neither loaded a
    /// save nor took a snapshot (H1700, not traced). The new core continues the old one's histories and harness inputs.
    pub fn rewind_yes(&mut self) {
        if !self.r.rewind_answer(true) {
            return;
        }
        let gam = self.r.w.gam;
        let saved = crate::save::game_state_id(gam).is_some();
        let snap = self.r.rewind_snap.take();
        let mut what = if saved { "a Load of the loaded save (no SaveGameState since)" } else { "a new game (no save, no snapshot)" };
        let (g, load): (&'a Hdb, bool) = match snap.map(|st| self.snapshot_bytes(st)) {
            Some(Ok(b)) => {
                what = "a Load of the last SaveGameState's snapshot";
                (Box::leak(Box::new(Hdb::from_bytes(b))), true)
            }
            Some(Err(e)) => {
                self.log.push(format!("\"Try again?\": the snapshot could not be written ({e})"));
                (gam, saved)
            }
            None => (gam, saved),
        };
        let hist = self.r.take_history();
        let seq = std::mem::take(&mut self.r.uneval_seq);
        let picks = std::mem::take(&mut self.r.w.smol_trace_picks);
        let will = std::mem::take(&mut self.r.w.smol_trace_will);
        self.reload(g, load);
        self.r.continue_history(hist);
        self.r.uneval_seq = seq;
        self.r.w.smol_trace_picks = picks;
        self.r.w.smol_trace_will = will;
        self.log.push(format!("\"Try again?\": YES → {what}"));
    }

    /// The bytes of the rewind snapshot as a save (as [`Session::save_bytes`] over the session's gam).
    fn snapshot_bytes(&mut self, mut st: crate::save::SaveState) -> Result<Vec<u8>, String> {
        for ph in &mut st.photos {
            if ph.thumb.is_empty() {
                let frame = Some((ph.f68(), ph.rect()));
                ph.thumb = crate::save::Photo::thumbnail(self.photo_picture(ph.picture(), frame).as_ref());
            }
        }
        let gam = self.r.w.gam;
        if crate::save::game_state_id(gam).is_some() {
            return crate::save::write(gam, &st).map(|x| x.0);
        }
        if self.r.w.rules.h470_save_builder {
            return crate::save::build(gam, &st).map(|x| x.0);
        }
        let p = self.template.clone().ok_or("no save of the original to use as a template (H401)")?;
        let tpl = Hdb::open(&p).map_err(|e| e.to_string())?;
        crate::save::write(&tpl, &st).map(|x| x.0)
    }

    /// Compose the box screen (H1700).
    pub fn render_rewind(&mut self) -> Vec<u8> {
        let mut fb = vec![0u8; SCREEN_W * SCREEN_H * 3];
        let t = self.now.saturating_sub(self.rewind_t0.unwrap_or(self.now));
        if let Some((px, w, h)) = self.box_frame(t) {
            super::menu::put(&mut fb, BOX_X, BOX_Y, &px, w, h);
        }
        let face = super::text::courier15_bold();
        let tw = |s: &str| face.map_or(s.len() as i32 * 9, |f| f.width(s));
        let draw = |fb: &mut [u8], x: i32, y: i32, s: &str| match face {
            Some(f) => {
                f.draw(fb, x, y, s, [0, 0, 0]);
            }
            None => {
                super::font::text(fb, x, y + 3, s, [0, 0, 0]);
            }
        };
        draw(&mut fb, (SCREEN_W as i32 - tw(CAPTION)) / 2, 213, CAPTION);
        let hover = self.mouse.and_then(|(x, y)| button_at(x, y));
        for (r, label, yes) in [(YES_RECT, "YES", true), (NO_RECT, "NO", false)] {
            let fill = if hover == Some(yes) { [70, 160, 210] } else { [40, 120, 175] };
            rect(&mut fb, r, [120, 200, 235], fill);
            draw(&mut fb, (r[0] + r[2] - tw(label)) / 2, r[1] + 5, label);
        }
        fb
    }

    /// The box movie's frame at `t` ms (held at its last frame).
    fn box_frame(&mut self, t: u64) -> Option<(Vec<u8>, usize, usize)> {
        let c = self.clip(BOX)?;
        let f = self.lang_file(&c, self.game_lang);
        let p = self.player(&f)?;
        if !p.has_video() {
            return None;
        }
        let d = p.duration_ms().max(1);
        match p.seek(t.min(d - 1)) {
            Ok(true) => Some((p.rgb.clone(), p.width, p.height)),
            _ => None,
        }
    }
}

/// A button: a 1-px border, filled inside.
fn rect(fb: &mut [u8], r: [i32; 4], border: [u8; 3], fill: [u8; 3]) {
    for y in r[1].max(0)..r[3].min(SCREEN_H as i32) {
        for x in r[0].max(0)..r[2].min(SCREEN_W as i32) {
            let edge = x == r[0] || y == r[1] || x == r[2] - 1 || y == r[3] - 1;
            let o = (y as usize * SCREEN_W + x as usize) * 3;
            fb[o..o + 3].copy_from_slice(if edge { &border } else { &fill });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_by_point() {
        // the bot's points on the original (C244 / C255): YES (273,278), NO (365,278)
        assert_eq!(button_at(273, 278), Some(true));
        assert_eq!(button_at(365, 278), Some(false));
        assert_eq!(button_at(320, 278), None);
        assert_eq!((BOX_X, BOX_Y), (150, 170));
    }
}
