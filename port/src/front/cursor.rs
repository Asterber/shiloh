//! The game's cursor drawn into the frame (H307, H330–H333). Display only: which object is under the pointer is the
//! core's (`Runner::hit`).
//!
//! HYPOTHESIS H331 (static) — **the cursor over nothing is the Win32 crosshair of XFiles.exe**. The
//! original's cursor message handler (FUN_004468d0) asks the object hit at the pointer (the H130 chain) for its cursor;
//! with **no hit** the *null cursor* (an ownerless cursor object built after the 12 of the cursor table) is activated. A
//! VCCursor is drawn by the engine into the app's sprite layer with the system cursor hidden; the null cursor clears
//! that layer and sets the Win32 cursor: resource **0x81** by default, resource **0x84** while the app's gunsight flag
//! is set. XFiles.exe group 0x81 = a 32×32 monochrome
//! crosshair, hot spot (7,7): a 7×7 cross (white 1-px bars, black outline, black centre pixel); group 0x84 = a gunsight ring, hot
//! (16,15). A player's recollection of the original's default cursor is "a simple crosshair" — consistent. Icons
//! name their cursor by record field f34, which is 0 in every icon record (classes 0x47..0x4b) → the null cursor too.
//! The dialogue box rows / body (kind 0) have no VCCursor → crosshair (their cursor pick is not decoded).
//!
//! HYPOTHESIS H332 (static) — **the selected inventory item's cursor shows only over an object that reacts to it**.
//! An Explorable's cursor pick (FUN_0046e050): if the held value has a Mouse Click reaction (H155's test) → the
//! selected item's f38 VCCursor — or, for held type 9 (the gun), the gunsight flag set and the null cursor (= the 0x84
//! gunsight); else the Explorable's own region kind. A CharView's pick is the same
//! shape. C302 (`traces/fo-253.jsonl`): over an Explorable the port tests the reaction (`World::reacts_to_held`, Mouse
//! Click) — no reaction → the Explorable's own cursor; over a CharView (its refusal-class tests not modelled) the item's
//! cursor while an item is selected; over nothing → the crosshair (not the item).
//!
//! C301 (`traces/fo-253.jsonl`) — **the gunsight**: with the gun held (value 9) the held gun's cursor pick and a
//! movie-hotspot target's set the gunsight flag, so over the View band (the held gun hit, H450),
//! a goon's target, a playing video there and a reacting Explorable / CharView the null cursor is resource 0x84.
//!
//! HYPOTHESIS H2350 (static decode of the target's cursor pick; fo-264 / fo-265 / fo-220 hold) — **a movie-hotspot target picks the
//! cursor by its hotspot id**: the fist 0x4454 over 441 / 451 / 463 (the 'trap Mulder' red button is 451), with the
//! stiletto held 0x7cbb over 433 / 461 / 470 and the animated stiletto 0xe2ef over 462 (Cook at the blast door); else
//! the gunsight with the gun, else the cursor is left as it was ([`moviehot::hover_cursor`]).
//!
//! HYPOTHESIS H330 (static) — **animated cursors**: a VCCursor whose f38 picture is an XG/*.pff range `0..k` (kind 2,
//! the original's cursor builder FUN_00448230) gets one display frame per PFF entry with that entry's duration from the
//! PFF header (ms, rescaled to 1000 per second), and loops forever (loop count −1); each
//! activation rebuilds the frames, so the animation restarts at frame 0 when the cursor becomes current. Animated:
//! 0x1bfc eye (XG/07093.pff: 2000, 120, 1000, 1000 ms — open, blink, open, look aside; region kind of 411 hotspots /
//! items / Explorables), 0x4454 fist (XG/17477.pff, 5 × 100 ms; 181 regions), 0x63c5 blue X (XG/25538.pff, 11 × 100 + 1000;
//! the cursor table only — no region names it), 0xe2ef stiletto (XG/58092.pff, 4 × 100). Every other cursor (hands, lips,
//! inventory items, gunsights) is a single x.pff still.
use super::{Session, SCREEN_H, SCREEN_W};
use crate::media::pict::Picture;
use crate::pe::WinCursor;
use crate::world::class;
use crate::world::moviehot::{self, HoverCursor};
use std::collections::HashMap;

/// H331: XFiles.exe cursor groups — the default and the one while the gunsight flag is set
pub const WIN_CROSSHAIR: u32 = 0x81;
pub const WIN_GUNSIGHT: u32 = 0x84;

/// What the pointer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// a VCCursor (engine-drawn, H307)
    Game(u32),
    /// the null cursor: a Win32 cursor resource of XFiles.exe (H331)
    Win(u32),
}

/// One frame of a cursor's picture and how long it stays (ms).
#[derive(Clone, Debug)]
pub struct Frame {
    pub pic: Picture,
    pub ms: u32,
}

#[derive(Clone, Default)]
pub struct CursorState {
    /// the shape shown and since when (`Session::now`): an activation restarts the animation (H330)
    pub current: Option<(Shape, u64)>,
    frames: HashMap<u32, Vec<Frame>>,
    win: HashMap<u32, Option<WinCursor>>,
}

/// H307 / H331 / H332: the shape for the pointer now (None: no pointer).
pub fn shape(s: &Session) -> Option<Shape> {
    let h = s.r.w.h;
    let (x, y) = s.mouse?;
    let hit = s.r.hit(x, y);
    let rules = s.r.w.rules;
    // C301: the gun held = inventory value 9
    let gun = rules.c301_gunsight_cursor && s.r.w.inventory_held() && s.r.w.inventory_value() == 9;
    // H2350: a movie-hotspot target picks the cursor by its hotspot id and the held value
    if let Some(c) = hit.filter(|c| rules.h2350_movie_hot_cursor && moviehot::is_movie_hot(c.owner)) {
        let held = s.r.w.inventory_held().then(|| s.r.w.inventory_value());
        return Some(match moviehot::hover_cursor(c.owner & 0xffff, held) {
            HoverCursor::Cursor(id) if h.has(class::CURSOR, id) => Shape::Game(id),
            HoverCursor::Gunsight if rules.c301_gunsight_cursor => Shape::Win(WIN_GUNSIGHT),
            // nothing set: the cursor shown stays (the original changes it only on a cursor message)
            _ => s.cursor.current.map_or(Shape::Win(WIN_CROSSHAIR), |(sh, _)| sh),
        });
    }
    if gun {
        if let Some(c) = hit {
            use crate::world::devices::{GUN_OWNER, VIDEO_OWNER};
            if c.owner == GUN_OWNER || c.owner == VIDEO_OWNER || crate::world::moviehot::is_movie_hot(c.owner) {
                return Some(Shape::Win(WIN_GUNSIGHT));
            }
        }
    }
    if let (Some(item), Some(c)) = (s.r.inv_selected, hit) {
        // C302: an Explorable shows the item's cursor only when one of its Mouse Click triggers reacts to the held value
        let reacts = c.cls != class::EXPLORABLE || !rules.c302_item_cursor_reaction || {
            let tl = s.r.w.trigger_list_of(c.owner).unwrap_or(0);
            s.r.w.reacts_to_held(tl, crate::world::event::MOUSE_CLICK)
        };
        if (c.cls == class::EXPLORABLE || c.cls == class::CHAR_VIEW) && reacts {
            if gun {
                return Some(Shape::Win(WIN_GUNSIGHT)); // C301 / H332: held value 9 → the gunsight flag, the null cursor
            }
            let cur = h.read(class::INVENTORY, item).map_or(0, |r| r.u32("f38"));
            if h.has(class::CURSOR, cur) {
                return Some(Shape::Game(cur));
            }
        }
    }
    match hit {
        Some(c) if h.has(class::CURSOR, c.kind) => Some(Shape::Game(c.kind)),
        _ => Some(Shape::Win(WIN_CROSSHAIR)),
    }
}

/// H330: the frames of a VCCursor's picture (its f38 list's first AssetRef): one per PFF entry `start..=end` with the
/// header's durations; a still is one frame.
pub fn frames(s: &mut Session, id: u32) -> Vec<Frame> {
    if let Some(f) = s.cursor.frames.get(&id) {
        return f.clone();
    }
    let h = s.r.w.h;
    let list = h.read(class::CURSOR, id).map_or(0, |r| r.u32("f38"));
    let mut out = Vec::new();
    if let Some(a) = h.list_of(class::ASSET_REF_LIST, list).into_iter().next() {
        if let Some(c) = s.clip(a).filter(|c| c.ext() == "pff") {
            let pff = s.pffs.entry(c.file.clone()).or_insert_with(|| crate::media::pict::Pff::open(&c.file).ok());
            if let Some(p) = pff.as_ref() {
                let d = p.durations();
                for i in c.start as usize..=(c.end.max(c.start) as usize).min(p.len().saturating_sub(1)) {
                    if let Ok(pic) = p.picture(i) {
                        out.push(Frame { pic, ms: d.get(i).copied().unwrap_or(0) });
                    }
                }
            }
        }
    }
    s.cursor.frames.insert(id, out.clone());
    out
}

/// H330: the frame index shown `t` ms after the activation — the frames in order, each for its duration, looped forever
/// (a frame of duration 0 is skipped; all 0 → frame 0).
pub fn frame_at(durations: &[u32], t: u64) -> usize {
    let total: u64 = durations.iter().map(|&d| d as u64).sum();
    if total == 0 {
        return 0;
    }
    let mut t = t % total;
    for (i, &d) in durations.iter().enumerate() {
        if t < d as u64 {
            return i;
        }
        t -= d as u64;
    }
    0
}

/// H331: a Win32 cursor resource of XFiles.exe (32×32 entry, as a 32×32 system metric picks)
fn win_cursor(s: &mut Session, group: u32) -> Option<WinCursor> {
    if !s.cursor.win.contains_key(&group) {
        let c =
            s.lib.find("XFiles.exe").and_then(|p| crate::io::read(p).ok()).and_then(|d| crate::pe::cursor(&d, group, 32));
        s.cursor.win.insert(group, c.or_else(|| (group == WIN_CROSSHAIR).then(builtin_crosshair)));
    }
    s.cursor.win[&group].clone()
}

/// Port feature: the crosshair drawn when XFiles.exe (and so its cursor resource) is not beside the data — the port's
/// own design, not the exe's bitmap: four white arms one pixel thick with a black outline, around a
/// hollow centre; hot spot at the centre.
fn builtin_crosshair() -> WinCursor {
    const SIZE: usize = 11;
    const MID: i32 = SIZE as i32 / 2;
    const GAP: i32 = 1;
    let arm = |x: i32, y: i32| (x == MID || y == MID) && (x - MID).abs().max((y - MID).abs()) > GAP;
    let mut c = WinCursor { width: SIZE, height: SIZE, hot: (MID, MID), and: Vec::new(), xor: Vec::new() };
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let white = arm(x, y);
            let outline = !white && [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| arm(x + dx, y + dy));
            // AND 0 paints (XOR picks white / black); AND 1 with XOR 0 leaves the screen
            c.and.push(!(white || outline));
            c.xor.push(white);
        }
    }
    c
}

/// Draw a monochrome Win32 cursor: AND 1 / XOR 0 transparent, AND 1 / XOR 1 inverts, AND 0 → black (XOR 0) / white (1).
fn draw_win(fb: &mut [u8], x: i32, y: i32, c: &WinCursor) {
    for py in 0..c.height {
        for px in 0..c.width {
            let (sx, sy) = (x - c.hot.0 + px as i32, y - c.hot.1 + py as i32);
            if sx < 0 || sy < 0 || sx >= SCREEN_W as i32 || sy >= SCREEN_H as i32 {
                continue;
            }
            let i = py * c.width + px;
            let o = (sy as usize * SCREEN_W + sx as usize) * 3;
            match (c.and[i], c.xor[i]) {
                (true, false) => {}
                (true, true) => fb[o..o + 3].iter_mut().for_each(|v| *v = 255 - *v),
                (false, w) => fb[o..o + 3].fill(if w { 255 } else { 0 }),
            }
        }
    }
}

/// Draw the cursor for the pointer (H307 / H330 / H331); the animation clock is `Session::now`.
pub fn draw(s: &mut Session, fb: &mut [u8]) {
    let (Some(sh), Some((x, y))) = (shape(s), s.mouse) else {
        s.cursor.current = None;
        return;
    };
    let since = match s.cursor.current {
        Some((c, t)) if c == sh => t,
        _ => {
            s.cursor.current = Some((sh, s.now));
            s.now
        }
    };
    match sh {
        Shape::Win(g) => {
            if let Some(c) = win_cursor(s, g) {
                draw_win(fb, x, y, &c);
            }
        }
        Shape::Game(id) => {
            let Some(r) = s.r.w.h.read(class::CURSOR, id) else { return };
            let hot = match r.field("hotPoint") {
                Some(crate::hdb::Value::Point(p)) => [p[0] as i32, p[1] as i32],
                _ => [0, 0],
            };
            let f = frames(s, id);
            let d: Vec<u32> = f.iter().map(|f| f.ms).collect();
            if let Some(fr) = f.get(frame_at(&d, s.now - since)) {
                super::blit(fb, x - hot[0], y - hot[1], &fr.pic);
            }
        }
    }
}
