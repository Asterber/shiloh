//! The game's own interface drawn over the View: the dialogue box (H136 / H262 layout from the core). Nothing here
//! decides anything — positions, rows and texts are the core's (`World::dbox_layout`, `World::dbox_line_texts`).
//!
//! HYPOTHESIS H300 (fo-055 k01 / k02 screenshots vs k00): the box picture (x.pff 247, AssetRef 0x87c9) is drawn with its
//! pure-blue pixels transparent (H63) and its other pixels opaque, except the pure-black pixels of its body (box rows
//! ≥ 12, below the tab strip), which darken what is under them to about a third (k00 → k01 in the View band: 25 → 8,
//! 41 → 14, 30 → 10); the tab strip's black rows (0..11) are opaque. Below the View band the screen is black, so the body
//! looks opaque there.
use super::{blit_fn, Session, SCREEN_H, SCREEN_W};
use crate::world::dbox;

/// H136: the box picture (folder-talk.pic) and the HISTORY variant (folder-history.pic, shown while the core's box has
/// its HISTORY panel selected, H502)
pub const BOX_TALK: u32 = 0x87c9;
pub const BOX_HISTORY: u32 = 0x87c8;
/// H300: box rows above this are the tab strip (height 12)
pub const BODY_TOP: i32 = 12;
/// H301: text colour of the rows (fo-055 k01)
pub const TEXT: [u8; 3] = [205, 205, 205];
/// H302 HYPOTHESIS (no screenshot of a hovered row): a row under the pointer is drawn white
pub const TEXT_HOVER: [u8; 3] = [255, 255, 255];
/// C76 / H2120: text colour of the HISTORY panel's rows (fo-097 k01-0 / k07: every row pixel (46,107,140))
pub const TEXT_HISTORY: [u8; 3] = [46, 107, 140];
/// H2120 (static only, no shot with a hovered history row): the HISTORY panel's hover colour
pub const TEXT_HISTORY_HOVER: [u8; 3] = [65, 140, 175];

/// H304: scrollbar pictures (0x87c7 up, 0x87ca bar, 0x87cb thumb, 0x87cc down) and their places in the
/// box (fo-056 k20)
pub const SCROLL_UP: (u32, i32, i32) = (0x87c7, 232, 31);
pub const SCROLL_BAR: (u32, i32, i32) = (0x87ca, 232, 41);
pub const SCROLL_THUMB: (u32, i32, i32) = (0x87cb, 232, 41);
pub const SCROLL_DOWN: (u32, i32, i32) = (0x87cc, 232, 109);
/// H304: opacity of the arrows and the thumb (fo-056 k20: down arrow 102 → 41, 76 → 31 on black; thumb 143 / 55 / 24 →
/// 118 / 82 / 69 over the bar's 102)
pub const SCROLL_ALPHA: f32 = 0.41;
/// H305: historyNONpatch (x.pff 711) and its place (over the HISTORY tab); covered pixels show the box picture × 0.37
pub const HISTORY_PATCH: (u32, i32, i32) = (0x14d87, 109, 0);
pub const HISTORY_PATCH_SHIFT: i32 = 0;
pub const HISTORY_PATCH_KEEP: f32 = 0.37;

fn mix(src: [u8; 3], dst: [u8; 3], a: f32) -> [u8; 3] {
    [0, 1, 2].map(|i| (src[i] as f32 * a + dst[i] as f32 * (1.0 - a)).round() as u8)
}

/// Draw picture `(asset, x, y)` (box-relative) with opacity `a`; blue = transparent
fn box_part(s: &mut Session, fb: &mut [u8], (asset, x, y): (u32, i32, i32), bx: i32, by: i32, a: f32) {
    let Some(p) = s.picture(asset).cloned() else { return };
    super::blit_fn(fb, bx + x, by + y, &p, |_, _, src, dst| {
        (src != super::KEY).then(|| if a >= 1.0 { src } else { mix(src, dst, a) })
    });
}

/// H300: darken to a third
fn darken(v: u8) -> u8 {
    v / 3
}

/// Draw the shown dialogue box with its rows (no-op when no box is shown).
pub fn draw_dbox(s: &mut Session, fb: &mut [u8]) {
    let Some(l) = s.r.w.dbox_layout() else { return };
    let (bx, by) = (l.rect[0] as i32, l.rect[1] as i32);
    // H502: the HISTORY panel selected → folder-history.pic (0x87c8), else folder-talk.pic
    let pic = if l.tab == 1 && s.picture(BOX_HISTORY).is_some() { BOX_HISTORY } else { BOX_TALK };
    if s.picture(pic).is_some() {
        let p = s.pictures[&pic].as_ref().expect("checked");
        blit_fn(fb, bx, by, p, |px, py, src, dst| {
            if src == super::KEY {
                return None;
            }
            if src == [0, 0, 0] && py >= BODY_TOP {
                let _ = px;
                return Some(dst.map(darken));
            }
            Some(src)
        });
    } else {
        // no picture file: a plain frame where the box is
        super::fill(fb, bx, by + BODY_TOP, bx + dbox::BOX_W, by + dbox::BOX_H, [0, 0, 0]);
        super::rect(fb, bx, by + BODY_TOP, bx + dbox::BOX_W - 1, by + dbox::BOX_H - 1, [30, 30, 30]);
        let c = l.close.map(|v| v as i32);
        super::rect(fb, c[0], c[1], c[2] - 1, c[3] - 1, [191, 191, 191]);
    }
    // H305: no line in the HISTORY panel → its tab is covered by the dimmed patch
    if !s.r.w.dbox_has_history() && l.tab == 0 {
        if let (Some(patch), Some(boxpic)) = (s.picture(HISTORY_PATCH.0).cloned(), s.picture(BOX_TALK).cloned()) {
            let (px0, py0) = (bx + HISTORY_PATCH.1, by + HISTORY_PATCH.2);
            super::blit_fn(fb, px0, py0, &patch, |x, y, src, _| {
                if src == super::KEY {
                    return None;
                }
                let (sx, sy) = ((HISTORY_PATCH.1 + x + HISTORY_PATCH_SHIFT) as usize, (HISTORY_PATCH.2 + y) as usize);
                let o = (sy * boxpic.width + sx.min(boxpic.width - 1)) * 3;
                let mut c = [boxpic.rgb[o], boxpic.rgb[o + 1], boxpic.rgb[o + 2]];
                if c == super::KEY {
                    c = [30, 30, 30];
                }
                Some(c.map(|v| (v as f32 * HISTORY_PATCH_KEEP) as u8))
            });
        }
    }
    let rows_all = s.r.w.dbox_line_texts();
    let panel_top = by + dbox::TALK[1];
    let panel_bottom = by + dbox::TALK[3];
    // H304: the scrollbar appears when the rows do not fit the talk panel; H1810: the thumb at the scroller's position
    if let Some(sc) = &l.scroll {
        box_part(s, fb, SCROLL_UP, bx, by, SCROLL_ALPHA);
        box_part(s, fb, SCROLL_BAR, bx, by, 1.0);
        box_part(s, fb, (SCROLL_THUMB.0, SCROLL_THUMB.1, sc.thumb[1] as i32 - by), bx, by, SCROLL_ALPHA);
        box_part(s, fb, SCROLL_DOWN, bx, by, SCROLL_ALPHA);
    }
    let hover = s.mouse.and_then(|(x, y)| s.r.hit(x, y)).map(|c| c.owner);
    for (k, (conv, r, text)) in rows_all.into_iter().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        let history = l.tab == 1 && s.r.w.rules.h2120_history_row_colour;
        let col = match (history, hover == Some(conv)) {
            (false, false) => TEXT,
            (false, true) => TEXT_HOVER,
            (true, false) => TEXT_HISTORY,
            (true, true) => TEXT_HISTORY_HOVER,
        };
        let rows = dbox::wrap_lines(&chars, dbox::TALK[2] - dbox::TALK[0], dbox::CHAR_W);
        let top0 = l.tops.get(k).copied().unwrap_or(r[1] as i32);
        for (i, row) in rows.iter().enumerate() {
            let top = top0 + i as i32 * dbox::LINE_H_262;
            if top >= panel_bottom {
                break; // H304: text below the panel is scrolled out of view (clipped at its bottom)
            }
            if top + dbox::LINE_H_262 <= panel_top {
                continue; // H1810: scrolled out above the panel
            }
            let row: String = row.iter().collect();
            super::text::draw_courier_clip_rows(fb, r[0] as i32, top, &row, col, panel_top, panel_bottom);
        }
    }
}

/// Display state the UI keeps between frames.
#[derive(Clone, Debug, Default)]
pub struct UiState {
    /// H309: the icon lit by the last Roll Into (cleared when the bars are re-placed)
    pub lit: Option<u32>,
    prev_hover: Option<u32>,
    placed: Option<Vec<u32>>,
    /// H792: caret item → (blink start in ms of the session clock, the last render that drew it)
    pub carets: std::collections::HashMap<u32, (u64, u64)>,
    /// renders so far (`Session::render`)
    pub frame: u64,
    /// H1923: a movie-kind icon picture's clock — owner → (movie position in ms when it last stopped, playing since)
    pub icon_movies: std::collections::HashMap<u32, (u64, Option<u64>)>,
}

/// HYPOTHESIS H303 — the icon bars (H150) and the inventory (H170): every icon the core has placed in a bar and that is
/// active is drawn with its shown picture (H150: first enabled AssetRef of f30) at its rect's top-left, blue = transparent
/// (H63). With prefs 'H' the core keeps only the selected inventory item in the bar while the panel is hidden (H170), as
/// the screenshots show (fo-060 k11: the selected badge alone at (301,420), k12: the gun at (336,420)).
pub fn draw_bars(s: &mut Session, fb: &mut [u8]) {
    let placed: Vec<(u32, [i16; 4])> =
        s.r.icon_bars
            .placed
            .iter()
            .filter(|o| s.r.active.contains(o))
            .filter_map(|o| s.r.icon_bars.rects.get(o).map(|r| (*o, r.1)))
            .collect();
    let hovered = hovered_icon(s);
    // H309: a Roll Into lights the icon (FUN_004631b0); a re-display of the bar (a state change rebuilds the icon's
    // display with frame 0) puts it out until the next Roll Into
    if hovered != s.ui.prev_hover {
        s.ui.prev_hover = hovered;
        s.ui.lit = hovered;
    }
    let now_placed = s.r.icon_bars.placed.clone();
    if s.ui.placed.as_ref().is_some_and(|p| *p != now_placed) {
        s.ui.lit = None;
    }
    s.ui.placed = Some(now_placed);
    if hovered.is_none() {
        s.ui.lit = None;
    }
    for &(o, r) in &placed {
        let (ro, _) = s.r.w.icon_rollover(o);
        // H309: the roll-over picture while the pointer is on the icon. The original also
        // keeps an inventory item lit after Roll Out Of when it is the held object's item — not modelled: fo-060 k11 shows
        // the selected badge with its normal picture
        let lit = s.ui.lit == Some(o);
        if super::ai::draw(s, fb, o, r[0] as i32, r[1] as i32) {
            continue; // H621 / H622: the AI icon (front::ai)
        }
        let Some(a) = ro.filter(|_| lit).or_else(|| s.r.w.icon_picture(o)) else { continue };
        let p = if ro.is_none() && s.r.w.rules.h1923_icon_movie_on_hover {
            icon_movie_frame(s, o, a, hovered == Some(o))
        } else {
            asset_frame(s, a)
        };
        if let Some(p) = p {
            super::blit(fb, r[0] as i32, r[1] as i32, &p);
        }
    }
    s.ui.icon_movies.retain(|o, _| placed.iter().any(|p| p.0 == *o));
    if let Some(&(o, r)) = s.ui.lit.and_then(|h| placed.iter().find(|x| x.0 == h)) {
        draw_rollover_text(s, fb, o, r);
    }
}

/// The icon under the pointer (a placed icon the hit test returns there), if any.
pub fn hovered_icon(s: &Session) -> Option<u32> {
    let (x, y) = s.mouse?;
    let c = s.r.hit(x, y)?;
    s.r.icon_bars.rects.contains_key(&c.owner).then_some(c.owner)
}

/// HYPOTHESIS H1923 (refines H311; C292 = `traces/fo-220.jsonl` 56.145 / 57.754): an icon without a roll-over picture
/// whose picture is a movie (kind 0, e.g. the ActionIcons' XG/24485.xmv) plays it only while the pointer is on it —
/// Roll Into sets the display's loop flag and starts it (FUN_0043bcb0); Roll Out Of stops it, no rewind for a movie.
/// The movie is built at the icon's activation and stands at its first frame until the first Roll Into. A PFF picture stays as [`asset_frame`] draws it.
fn icon_movie_frame(s: &mut Session, owner: u32, a: u32, hovered: bool) -> Option<crate::media::pict::Picture> {
    if let Some(p) = s.picture(a) {
        return Some(p.clone());
    }
    let c = s.clip(a)?;
    let now = s.now;
    let e = s.ui.icon_movies.entry(owner).or_insert((0, None));
    match (hovered, e.1) {
        (true, None) => e.1 = Some(now),
        (false, Some(t0)) => *e = (e.0 + now.saturating_sub(t0), None),
        _ => {}
    }
    let t = e.0 + e.1.map_or(0, |t0| now.saturating_sub(t0));
    s.frame(&c, t, true, s.game_lang)
}

/// A picture asset as a still: a PFF entry, or the current frame of a movie (looped on the session clock — H311: icon
/// movies such as the EmotionIcons' XG/*.xmv loop while shown).
pub fn asset_frame(s: &mut Session, a: u32) -> Option<crate::media::pict::Picture> {
    if let Some(p) = s.picture(a) {
        return Some(p.clone());
    }
    let c = s.clip(a)?;
    let now = s.now;
    s.frame(&c, now, true, s.game_lang)
}

/// HYPOTHESIS H310 — the roll-over text (FUN_00437600, only while Pref_bRolloverText (var 0x3d6) ≠ 0): the icon's f3c
/// text in Arial 12 (text style 3 at size 12), centred under the
/// icon's rect, its top 2 px below the rect's bottom, x kept ≥ 3 and inside the screen's right edge. Colour: white (not
/// decoded).
pub const ROLLOVER_TEXT: [u8; 3] = [255, 255, 255];

fn draw_rollover_text(s: &mut Session, fb: &mut [u8], o: u32, r: [i16; 4]) {
    if !s.r.w.bar_modes().1 {
        return;
    }
    let Some(t) = s.r.w.icon_rollover(o).1.and_then(|a| s.r.w.line_text(a)) else { return };
    let t: String = t.into_iter().map(dbox_cp1251).collect();
    let Some(f) = super::text::arial12() else {
        let w = t.chars().count() as i32 * 8;
        let x = ((r[0] as i32 + r[2] as i32) / 2 - w / 2).clamp(3, (SCREEN_W as i32 - w).max(3));
        super::font::text(fb, x, r[3] as i32 + 2, &t, ROLLOVER_TEXT);
        return;
    };
    let w = f.width(&t);
    let x = ((r[0] as i32 + r[2] as i32) / 2 - w / 2).clamp(3, (SCREEN_W as i32 - w).max(3));
    f.draw(fb, x, r[3] as i32 + 2, &t, ROLLOVER_TEXT);
}

/// HYPOTHESIS H306 (GUESS for the looks — no screenshot of these screens): texts of InterfaceItems — the typed login
/// name / password (H191 / H260) and the phone number (H166) from `World::typed_texts`, and text AssetRefs an item shows
/// (H176 SetIItemText, the phone-book cards) — drawn in Courier New 12, black, from the item region's left + 1, centred
/// in its height when one line fits, else from its top; one line per "\r\n", clipped to the region; the password as
/// '*'s; a 1-px caret after the text of the focused field.
pub const ITEM_TEXT: [u8; 3] = [0, 0, 0];

/// H1960 (fo-086 k05.png, the original's save screen: the names are exactly (255,255,255) on every row — black rows 2
/// / 3 and the blue selected row 1): the slot names of the save screen 0x14279 / the load screen 0x142ca are white.
/// Black (ITEM_TEXT) on the black rows 2 and 3 was invisible — only the highlighted row 1 showed its name (H1735).
pub const SLOT_TEXT: [u8; 3] = [255, 255, 255];
const SLOT_ITEMS: [u32; 6] = [0x1427c, 0x1427d, 0x1427e, 0x1438a, 0x1438b, 0x1438c];

fn item_rect(s: &Session, item: u32) -> Option<[i32; 4]> {
    match s.r.w.h.read(crate::world::class::INTERFACE_ITEM, item)?.field("region")? {
        crate::hdb::Value::Region { rect, .. } => Some(rect.map(|v| v as i32)),
        _ => None,
    }
}

/// HYPOTHESIS H865 (fo-100 k08..k12 of the original; C79 / R32): the phone display 0x9a59 — Courier New Bold 15 px
/// (PhoneInit's style), ink (27,48,15), from the item's top (not centred) and left + 1, rows 17 px apart
pub const PHONE_TEXT: [u8; 3] = [27, 48, 15];
pub const PHONE_LINE_H: i32 = 17;

fn draw_text_in(fb: &mut [u8], r: [i32; 4], lines: &[String], caret: bool, phone: bool, ink: [u8; 3]) {
    if phone {
        if let Some(f) = super::text::courier15_bold() {
            let fit = ((r[2] - r[0] - 1) / f.advance.max(1)).max(0) as usize;
            for (i, l) in lines.iter().enumerate() {
                let y = r[1] + i as i32 * PHONE_LINE_H;
                if y >= r[3] {
                    break;
                }
                let l: String = l.chars().take(fit).collect();
                f.draw_clip(fb, r[0], y, &l, PHONE_TEXT, r[3]);
            }
            return;
        }
    }
    let n = lines.len().max(1) as i32;
    let top =
        if n * dbox::LINE_H_262 <= r[3] - r[1] && n == 1 { r[1] + (r[3] - r[1] - dbox::LINE_H_262) / 2 } else { r[1] };
    let mut end = r[0] + 1;
    for (i, l) in lines.iter().enumerate() {
        let y = top + i as i32 * dbox::LINE_H_262;
        if y >= r[3] {
            break;
        }
        // clip to the region's right: whole characters
        let fit = ((r[2] - r[0] - 1) / dbox::CHAR_W).max(0) as usize;
        let l: String = l.chars().take(fit).collect();
        end = super::text::draw_courier_clip(fb, r[0] + 1, y, &l, ink, r[3]);
    }
    if caret {
        let y = top + 1;
        super::fill(fb, end, y, end + 1, (y + dbox::LINE_H_262 - 2).min(r[3]), ink);
    }
}

pub fn draw_item_texts(s: &mut Session, fb: &mut [u8]) {
    for (item, text, focused) in s.r.w.typed_texts() {
        if super::computer::handles(s, item) {
            continue;
        }
        let Some(r) = item_rect(s, item) else { continue };
        let shown = if matches!(item, 0xc68c | 0xc898) { "*".repeat(text.chars().count()) } else { text };
        let ink = if SLOT_ITEMS.contains(&item) { SLOT_TEXT } else { ITEM_TEXT };
        draw_text_in(fb, r, &[shown], focused, item == crate::world::devices::PHONE_DISPLAY, ink);
    }
    // text AssetRefs shown by items (H176, phone cards): every shown layout item whose asset has a text
    let items: Vec<(u32, u32)> =
        s.r.w
            .layouts()
            .iter()
            .flat_map(|l| l.1.iter())
            .filter_map(|&(i, a)| a.filter(|&a| a != 0).map(|a| (i, a)))
            .collect();
    for (item, a) in items {
        if !s.r.active.contains(&a) {
            continue;
        }
        if super::computer::handles(s, item) {
            continue;
        }
        let Some(t) = s.r.w.line_text(a) else { continue };
        let Some(r) = item_rect(s, item) else { continue };
        let t: String = t.into_iter().map(dbox_cp1251).collect();
        let lines: Vec<String> = t.split("\r\n").flat_map(|x| x.split('\n')).map(String::from).collect();
        draw_text_in(fb, r, &lines, false, item == crate::world::devices::PHONE_DISPLAY, ITEM_TEXT);
    }
}

/// kind-4 text blobs are cp1251 bytes read as Latin-1 chars → Cyrillic
fn dbox_cp1251(c: char) -> char {
    match c as u32 {
        0xc0..=0xff => char::from_u32(0x410 + c as u32 - 0xc0).unwrap_or(c),
        0xa8 => 'Ё',
        0xb8 => 'ё',
        _ => c,
    }
}

/// HYPOTHESIS H307 — the game's cursor: the VCCursor whose id is the region kind of the clickable under the pointer (nav
/// hotspots 0x11c / 0x11d / 0x11e = left / right / forward hands, CharViews 0x122 = lips; the selected inventory item's
/// f38 cursor over Explorables / CharViews, H332); None where the null cursor (the Win32 crosshair, H331) shows. Drawn by
/// [`super::cursor`] (H330 animation).
pub fn cursor_id(s: &Session) -> Option<u32> {
    match super::cursor::shape(s)? {
        super::cursor::Shape::Game(id) => Some(id),
        super::cursor::Shape::Win(_) => None,
    }
}

pub fn draw_cursor(s: &mut Session, fb: &mut [u8]) {
    super::cursor::draw(s, fb);
}

/// HYPOTHESIS H308 — the StartVideoText caption: the text AssetRef's string, the game's
/// date in upper case and, when the flag variable is 1, the time on a 12-hour clock. Courier New 12, white, at the View
/// rect's left + 15, its bottom − 15 − the text height. Month names: English (the bottle's locale is assumed en-US).
pub const VIDEO_TEXT_COLOR: [u8; 3] = [255, 255, 255];

/// H308: the caption's lines for (text, day value, time value, flag value)
pub fn video_text_lines(text: &str, day: i32, time: i32, flag: i32) -> Vec<String> {
    let mut lines = vec![text.to_string(), game_date(day).to_uppercase()];
    if flag == 1 {
        lines.push(clock_time(time));
    }
    lines
}

/// The calendar date of a day variable's value: values above 10 count from 10, the index is clamped to 0..=7; day
/// index 0 is March 29, 1996, index k is April k + 1 (the first day of the case is April 2).
fn game_date(day: i32) -> String {
    let index = if day > 10 { day - 10 } else { day }.clamp(0, 7);
    if index == 0 {
        "March 29, 1996".to_string()
    } else {
        format!("April {}, 1996", index + 1)
    }
}

/// A time variable's value (hours · 100 + minutes) on a 12-hour clock, wrapping past midnight.
fn clock_time(time: i32) -> String {
    const DAY_MINUTES: i32 = 24 * 60;
    let minutes = (time / 100 * 60 + time % 100).rem_euclid(DAY_MINUTES);
    let (hour, minute) = (minutes / 60, minutes % 60);
    let shown_hour = match hour % 12 {
        0 => 12,
        h => h,
    };
    format!("{shown_hour}:{minute:02} {}", if hour < 12 { "AM" } else { "PM" })
}

/// HYPOTHESIS H312 — the caption types itself out (static decode of the StartVideoText handler, FUN_00484340, and
/// a player's memory of a white typewriter caption): the text object starts empty; a 200 ms one-shot timer posts it
/// and starts a 50 ms ticker (run from the main loop); every tick shows 3 more characters (a prefix of the text; the
/// "\n"s count) and re-arms; the first tick that finds the whole text shown arms a 4000 ms one-shot that hides and
/// removes it. No caret, no sound. Returns the characters shown `elapsed` ms after the call, None = not on screen.
pub const VT_POST_MS: u64 = 200;
pub const VT_TICK_MS: u64 = 50;
pub const VT_CHARS_PER_TICK: usize = 3;
pub const VT_HOLD_MS: u64 = 4000;

pub fn video_text_shown(len: usize, elapsed: u64) -> Option<usize> {
    if elapsed < VT_POST_MS {
        return None;
    }
    let ticks = ((elapsed - VT_POST_MS) / VT_TICK_MS) as usize;
    let full_ticks = len.div_ceil(VT_CHARS_PER_TICK) as u64;
    // the tick after the text is complete arms the removal
    let end = VT_POST_MS + VT_TICK_MS * (full_ticks + 1) + VT_HOLD_MS;
    if elapsed >= end {
        return None;
    }
    Some((ticks * VT_CHARS_PER_TICK).min(len))
}

pub fn draw_video_text(s: &mut Session, fb: &mut [u8]) {
    let Some(vt) = s.r.w.video_text.clone() else { return };
    let Some(t) = crate::world::devices::asset_text(&s.r.w, vt.asset) else { return };
    let full = video_text_lines(&t, vt.day, vt.time, vt.flag).join("\n");
    let Some(n) = video_text_shown(full.chars().count(), s.now.saturating_sub(vt.at)) else { return };
    let shown: String = full.chars().take(n).collect();
    let lines: Vec<&str> = full.split('\n').collect();
    // the object's rect is measured on the whole text (before the empty text is set): the position is fixed
    let h = lines.len() as i32 * dbox::LINE_H_262;
    let (x, y) = (super::VIEW_X as i32 + 15, (super::VIEW_Y + crate::media::FRAME_H) as i32 - 15 - h);
    for (i, l) in shown.split('\n').enumerate() {
        super::text::draw_courier(fb, x, y + i as i32 * dbox::LINE_H_262, l, VIDEO_TEXT_COLOR);
    }
}

/// H407: layouts the menu opens over the whole screen
pub const FULL_SCREEN_LAYOUTS: [u32; 4] = [0x14269, 0x142ca, 0xc24a, 0xc51c];

/// Draw every UI layer (called by `Session::render` after the View and its region pictures).
pub fn draw(s: &mut Session, fb: &mut [u8]) {
    debug_assert_eq!(fb.len(), SCREEN_W * SCREEN_H * 3);
    // H2340: the captions / video texts of the paused game's movie stay under a full-screen menu layout (H407) — its
    // picture covers the movie area
    let screen = s.r.w.layouts().iter().any(|l| FULL_SCREEN_LAYOUTS.contains(&l.0));
    if !(screen && s.r.w.rules.h2340_menu_screen_pauses) {
        draw_captions(s, fb);
        draw_video_text(s, fb);
    }
    draw_item_texts(s, fb);
    super::computer::draw(s, fb); // H570–H578: the FBI computer's texts
    super::doc::draw_documents(s, fb); // H530: XT/*.xtx document texts (journal, Bible, notes)
    super::pda::draw_note_pad(s, fb); // H624
    // HYPOTHESIS H407 (display, not traced): the menu's full-screen layouts (save 0x14269, load 0x142ca, options 0xc24a,
    // help 0xc51c) cover the icon bars and the dialogue box
    if !screen {
        draw_bars(s, fb);
        if !s.r.w.rules.h500_layouts_over_dbox {
            draw_dbox(s, fb);
        }
    }
    if s.game_cursor {
        draw_cursor(s, fb);
    }
}

/// H500: the dialogue box as its own layer, under the shown layouts' items (called by `Session::render` between the
/// View's pictures and the layouts' pictures); H407: not while a full-screen menu layout is up.
pub fn draw_dbox_layer(s: &mut Session, fb: &mut [u8]) {
    if !s.r.w.layouts().iter().any(|l| FULL_SCREEN_LAYOUTS.contains(&l.0)) {
        draw_dbox(s, fb);
    }
}

/// Word-wrap `line` to `width` pixels with `measure` (a word wider than the box stays whole on its own row).
pub fn wrap_words(line: &str, width: i32, measure: impl Fn(&str) -> i32) -> Vec<String> {
    let mut rows = Vec::new();
    let mut cur = String::new();
    for w in line.split(' ') {
        let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if !cur.is_empty() && measure(&cand) > width {
            rows.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else {
            cur = cand;
        }
    }
    rows.push(cur);
    rows
}

/// HYPOTHESIS H342 — draw the closed captions ([`Session::captions`], movie coordinates + the View origin): each
/// caption's track rect filled with its background (unless keyed), its text word-wrapped in the text box, every row
/// justified (1 = centred), baseline = row top + ascent, rows `line_h` apart, clipped to the track rect. The movies'
/// font is Geneva 12 (font id 3); QuickTime for Windows draws it with a Windows face — Arial 12 assumed, monochrome as
/// H301 (no anti-aliasing flag in the samples).
pub fn draw_captions(s: &mut Session, fb: &mut [u8]) {
    let caps = s.captions();
    if caps.is_empty() {
        return;
    }
    let (ox, oy) = (super::VIEW_X as i32, super::VIEW_Y as i32);
    let face = super::text::arial12();
    let measure = |t: &str| face.map_or(t.chars().count() as i32 * 6, |f| f.width(t));
    for c in caps {
        let [tl, tt, tr, tb] = [c.track[0] + ox, c.track[1] + oy, c.track[2] + ox, c.track[3] + oy];
        let (tt, tb) = (tt.max(0), tb.min(SCREEN_H as i32));
        if tt >= tb {
            continue;
        }
        if !c.keyed {
            super::fill(fb, tl.max(0), tt, tr.min(SCREEN_W as i32), tb, c.bg);
        }
        let [bl, bt, br, _] = [c.text_box[0] + ox, c.text_box[1] + oy, c.text_box[2] + ox, c.text_box[3] + oy];
        let rows: Vec<String> = c
            .text
            .split(['\r', '\n'])
            .flat_map(|l| wrap_words(l.trim_end(), br - bl, measure))
            .collect();
        for (i, row) in rows.iter().enumerate() {
            let base = bt + c.ascent + i as i32 * c.line_h;
            if base > tb {
                break; // below the track: clipped
            }
            let w = measure(row);
            let x = match c.justify {
                1 => bl + (br - bl - w) / 2,
                -1 => br - w,
                _ => bl,
            };
            match face {
                Some(f) => {
                    f.draw_clip(fb, x, base - f.ascent, row, c.fg, tb);
                }
                None => {
                    super::font::text(fb, x, base - 6, row, c.fg);
                }
            }
        }
    }
}
