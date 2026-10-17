//! Dialogue box interaction (H135–H139; all static decode + post-hoc, NOT traced: no trace up to fo-030 has a click on
//! a VCConversation). H126 (in `mod.rs`) builds the box; this module models what a click on one of its lines does, how
//! the list of lines changes, how the box closes, and where it is on screen (RE_NOTES H135–H139).
//!
//! - H135 — click on a Conversation line (the Conversation's click handler, FUN_0046d0d0, reached from the mouse
//!   command's hit test): for a Conversation built by the box: history bookkeeping if its history flag is 1 (0xff on
//!   every Conversation of 0x590d → skipped); the box gets a state-0 command (the box hides, its Conversations leave the
//!   talk list); a play sequence is built of the first enabled AssetRef of f30, then of f34 (f38 only on the replay
//!   path) and posted; then the Mouse Click fires on the Conversation (TL f3c event 0). Nothing to play → fire, removal
//!   of the Conversation and box state 1.
//! - H136 — box geometry / hit test: box picture = AssetRef 0x87c9 (x.pff 247 "folder-talk.pic", 246×120), placed at
//!   x = CharView region left + (region width − 246) / 2 (clamped to the screen), y = 0x11d; talk panel rect = (11,20)
//!   size 212×90; close box = (234,16)–(241,24); the TALK / HISTORY tabs. Lines: one wrapped text block per Conversation
//!   in the order the box's state-1 command reaches them (kind-8 list = build order; fo-027 580.86–581.06), the first at
//!   the panel's top-left, each next one below the previous. The box's hit category (only while the box is active)
//!   comes after the layouts' InterfaceItems and before nav-hotspots / CharViews / Explorables; a point in the box but on
//!   no line hits the box itself (nothing unless the close box).
//!   H161: the line height — the talk panel's style is face "Courier New" (not Dlg.ttr), font height −12, weight 400;
//!   the panel stores the line height = tmHeight + tmExternalLeading and half of it as the gap between two blocks.
//!   Courier New (the bottle maps it to macOS Courier New.ttf: upem 2048, win ascent 1705 / descent 615, line gap 0) at
//!   12 px → 10 + 4 = 14 px (GDI rounding; not traced) → [`LINE_H`] 14, [`LINE_GAP`] 7; every 0x590d line is one row
//!   (≤ 29 characters × 7 px advance ≤ 212 px wrap width).
//! - H137 — close: a click in the close box → box state 0 and removal → every Conversation (kind 8) of the box removed:
//!   each deactivated, then its asset (as H126's removal).
//! - H138 — list update: a type-3 Enable of a member of the open box's list builds its Conversation (TL f3c), posts it
//!   and, if the box is active, gives it state 1 (line appended); a Disable of a built one removes it at once (Deact,
//!   then its asset).
//! - H139 — end of the played sequence (after its last player): media cells lowered, the Conversation gets cmd 0xd:
//!   removed if it is disabled by then (C15); then the View / CharViews / Explorables / nav-hotspots get the re-show
//!   command. The core's "box stays hidden" was REFUTED by fo-033 D3 (box + Conversation state commands at
//!   184.00–184.52).
//! - H160 — the View's re-show command (the View's handler, FUN_0041c740): when it is not flagged and media cell (10,4)
//!   is not raised, the app's box, if inactive and not to be removed (it has lines not in the history set that are
//!   enabled), gets **state 1**: the box is shown again with its remaining lines. While the line plays cell (10,4) is
//!   raised (fo-033 166.52 → 183.13), so the re-show commands of 168.31 / 169.69 show nothing.
use super::{class, event, Out, RegCause, World};

/// H136: dialogue box picture size (x.pff entry 247, AssetRef 0x87c9)
pub const BOX_W: i32 = 246;
pub const BOX_H: i32 = 120;
/// H136: the box's top (the placement of FUN_004819b0)
pub const BOX_Y: i32 = 0x11d;
/// H136 (hypothesis): the clamp rectangle = the 640-pixel-wide screen
pub const SCREEN_W: i32 = 640;
/// H136: talk panel = point (0xb, 0x14), size (0xd4, 0x5a), relative to the box
pub const TALK: [i32; 4] = [11, 20, 11 + 212, 20 + 90];
/// H136: close box = (0xea, 0x10)–(0xf1, 0x18), relative to the box
pub const CLOSE: [i32; 4] = [234, 16, 241, 24];
/// H161 HYPOTHESIS (static + font metrics, not traced): height of a one-row text block = Courier New −12 tmHeight +
/// tmExternalLeading
pub const LINE_H: i32 = 14;
/// H161: gap between two blocks = line height / 2
pub const LINE_GAP: i32 = LINE_H / 2;

/// H262 (fo-055 k01 / k02 screenshots: text lines of one block 15 px apart, blocks 22 px = 15 + 7): DrawText line height
/// of the talk panel's Courier New −12 under the bottle
pub const LINE_H_262: i32 = 15;
/// H262: Courier New −12 advance (fo-055 k01: 28 characters span 196 px)
pub const CHAR_W: i32 = 7;
/// H262: the box is clamped to the View rect (fo-055: CharView 0x579c (45..165) → box x 20, text
/// at 31; 0x5684 (409..601) → box x 374 = 620 − 246, text at 385)
pub const VIEW_X: [i32; 2] = [20, 620];

/// H262: DrawText(DT_WORDBREAK | DT_CALCRECT) of `text` at `wrap` px: lines broken at spaces (greedy), the rect as wide as
/// the longest line (trailing spaces not counted). Returns (width px, line count).
/// A blob byte read as a char (kind-4 texts) → its cp1251 letter (Cyrillic А..я, Ё, ё); other chars unchanged.
fn cp1251(c: char) -> char {
    match c as u32 {
        0xc0..=0xff => char::from_u32(0x410 + c as u32 - 0xc0).unwrap_or(c),
        0xa8 => 'Ё',
        0xb8 => 'ё',
        _ => c,
    }
}

pub fn wrap_block(text: &[char], wrap: i32, char_w: i32) -> (i32, i32) {
    let lines = wrap_lines(text, wrap, char_w);
    let max = (wrap / char_w).max(1) as usize;
    let widest = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    (widest.min(max) as i32 * char_w, lines.len() as i32)
}

/// H262: the lines of [`wrap_block`] themselves (greedy break at spaces, the space at a break dropped; a word longer than
/// a row stays whole) — the front-end draws them, the layout counts them.
pub fn wrap_lines(text: &[char], wrap: i32, char_w: i32) -> Vec<Vec<char>> {
    let max = (wrap / char_w).max(1) as usize;
    let mut lines: Vec<Vec<char>> = vec![Vec::new()];
    for (i, w) in text.split(|&c| c == ' ').enumerate() {
        let cur = lines.last_mut().expect("one line");
        if i == 0 {
            cur.extend_from_slice(w);
        } else if cur.len() + 1 + w.len() <= max {
            cur.push(' ');
            cur.extend_from_slice(w);
        } else {
            lines.push(w.to_vec());
        }
    }
    lines
}

/// Clickable kinds of the box (`Clickable::kind` with `cls` = VCConversationList)
pub const KIND_CLOSE: u32 = 1;
pub const KIND_BODY: u32 = 2;
/// H502: the TALK / HISTORY tabs (a click selects the panel whose tab rect holds the point)
pub const KIND_TAB_TALK: u32 = 3;
pub const KIND_TAB_HISTORY: u32 = 4;
/// H136 / H502: TALK tab = (10,0) 92×12, HISTORY tab = (110,0) 92×12, relative to the box
pub const TAB_TALK: [i32; 4] = [10, 0, 102, 12];
pub const TAB_HISTORY: [i32; 4] = [110, 0, 202, 12];

/// H1810: the scroller's parts, box-relative (bar = (0xe8, 0x29), up arrow = bar - (0, 10), down arrow = bar +
/// (0, 0x44)); sizes = their pictures (x.pff 245 / 248 / 249 / 250: arrows and thumb 12x10, bar 12x68). The thumb
/// starts at the bar's top.
pub const SCROLL_UP: [i32; 4] = [232, 31, 244, 41];
pub const SCROLL_BAR: [i32; 4] = [232, 41, 244, 109];
pub const SCROLL_DOWN: [i32; 4] = [232, 109, 244, 119];
pub const THUMB_H: i32 = 10;
/// Clickable kinds of the scroller (H1810)
pub const KIND_SCROLL_UP: u32 = 5;
pub const KIND_SCROLL_DOWN: u32 = 6;
pub const KIND_SCROLL_BAR: u32 = 7;

/// A scroller step (line up / down, page up / down).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
}

/// The selected panel's scroller (H1810-H1813), screen coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DboxScroller {
    /// the first row shown and the largest (the number of positions - 1, HYPOTHESIS H1811)
    pub first: usize,
    pub max: usize,
    /// pixels the text is scrolled up by
    pub offset: i32,
    pub up: [i16; 4],
    pub bar: [i16; 4],
    pub down: [i16; 4],
    /// the thumb (bar top + (bar height - thumb height) x first / (positions - 1))
    pub thumb: [i16; 4],
}

/// Where the open, shown dialogue box is on screen (H136).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DboxLayout {
    /// the VCConversationList (box object owner)
    pub list: u32,
    pub rect: [i16; 4],
    pub close: [i16; 4],
    /// (Conversation, rect of its line), top first — the selected panel's lines (H502: TALK or HISTORY)
    pub lines: Vec<(u32, [i16; 4])>,
    /// H502: the selected panel (0 TALK, 1 HISTORY)
    pub tab: u8,
    /// H502: the TALK and HISTORY tab rects (screen)
    pub tabs: [[i16; 4]; 2],
    /// H1810: the selected panel's scroller when its text does not fit (more than one position)
    pub scroll: Option<DboxScroller>,
    /// H1810: the talk panel's rect (screen) - the lines are clipped to it
    pub panel: [i16; 4],
    /// H1810: each line's text top (screen; above the panel when scrolled partly out), parallel to `lines`
    pub tops: Vec<i32>,
}

/// H791 / H1810: the scroller's rows - each entry's text lines (`line` px), the half-line gap between two
/// entries - as heights, top first.
pub fn scroll_rows(entries: &[i32], line: i32) -> Vec<i32> {
    let mut v = Vec::new();
    for (i, &n) in entries.iter().enumerate() {
        if i > 0 {
            v.push(line / 2);
        }
        v.extend(std::iter::repeat(line).take(n.max(0) as usize));
    }
    v
}

/// H1810: the largest first row - a line down scrolls only while the text's bottom is below the view's bottom.
pub fn scroll_max(rows: &[i32], view_h: i32) -> usize {
    let total: i32 = rows.iter().sum();
    let mut off = 0;
    for (k, &h) in rows.iter().enumerate() {
        if total - off <= view_h {
            return k;
        }
        off += h;
    }
    rows.len()
}

/// H1810 / H892: the first row after a step from `first` (clamped to 0..=`max`). Page down = the view's height less a
/// line; page up = to the top when scrolled by at most the view's height less min(line, gap) (H892), else back by the
/// view's height less a line.
pub fn scroll_step(rows: &[i32], view_h: i32, line: i32, first: usize, max: usize, step: Scroll) -> usize {
    let first = first.min(max);
    let off = |k: usize| rows[..k.min(rows.len())].iter().sum::<i32>();
    let page = view_h - line;
    match step {
        Scroll::LineUp => first.saturating_sub(1),
        Scroll::LineDown => (first + 1).min(max),
        Scroll::PageDown => {
            let mut k = first;
            while k < max && off(k + 1) - off(first) <= page {
                k += 1;
            }
            k.max((first + 1).min(max))
        }
        Scroll::PageUp => {
            if off(first) <= view_h - line.min(line / 2) {
                return 0;
            }
            let mut k = first;
            while k > 0 && off(first) - off(k - 1) <= page {
                k -= 1;
            }
            k.min(first.saturating_sub(1))
        }
    }
}

fn r16(r: [i32; 4]) -> [i16; 4] {
    r.map(|v| v as i16)
}

impl<'a> World<'a> {
    /// H136: the shown box's geometry (None if no box, or it is hidden / not built yet).
    pub fn dbox_layout(&self) -> Option<DboxLayout> {
        if !self.rules.h136_dbox_hit {
            return None;
        }
        self.layout_any()
    }

    fn layout_any(&self) -> Option<DboxLayout> {
        if !self.dbox_shown {
            return None;
        }
        let (cv, list, convs) = self.dbox.as_ref()?;
        let region = match self.h.read(class::CHAR_VIEW, *cv).and_then(|r| r.field("f2c").cloned()) {
            Some(crate::hdb::Value::Region { rect, .. }) => rect.map(|v| v as i32),
            _ => return None,
        };
        // H136: centred under the CharView's region (the division truncates toward 0), clamped to the screen
        let h262 = self.rules.h262_dbox_rows;
        let (lo, hi) = if h262 { (VIEW_X[0], VIEW_X[1]) } else { (0, SCREEN_W) };
        let mut x = region[0] + (region[2] - region[0] - BOX_W) / 2;
        if x + BOX_W > hi {
            x = hi - BOX_W;
        }
        x = x.max(lo);
        let y = BOX_Y;
        let at = |r: [i32; 4]| [x + r[0], y + r[1], x + r[2], y + r[3]];
        let panel = at(TALK);
        let mut lines = Vec::new();
        let mut tops = Vec::new();
        let mut top = panel[1];
        // H502: the selected panel — the HISTORY panel holds the history-set lines (same rect)
        let tab = if self.rules.h502_dbox_history_tab { self.dbox_tab } else { 0 };
        // H1810: the panel's entries (Conversation, width, line count), then the scroller's offset
        let mut entries: Vec<(u32, i32, i32)> = Vec::new();
        // a line only for a Conversation whose f2c asset object exists (a text asset; FUN_0041c4d0)
        for &(c, a) in convs {
            if a.is_none() {
                continue;
            }
            if h262 {
                // a Conversation in the history set goes to the HISTORY panel, not the talk panel
                if self.conv_history.contains(&c) != (tab == 1) {
                    continue;
                }
                let text = a.and_then(|a| self.line_text(a)).unwrap_or_default();
                let (w, n) = wrap_block(&text, TALK[2] - TALK[0], CHAR_W);
                entries.push((c, w, n));
                continue;
            }
            if top + LINE_H > panel[3] {
                break; // below the panel (scrolling not modelled)
            }
            lines.push((c, r16([panel[0], top, panel[2], top + LINE_H])));
            tops.push(top);
            top += LINE_H + LINE_GAP;
        }
        let mut scroll = None;
        if h262 {
            let view_h = panel[3] - panel[1];
            let rows = scroll_rows(&entries.iter().map(|e| e.2).collect::<Vec<_>>(), LINE_H_262);
            let max = if self.rules.h1810_dbox_scroll { scroll_max(&rows, view_h) } else { 0 };
            let first = self.dbox_scroll[tab as usize & 1].min(max);
            let offset: i32 = rows[..first.min(rows.len())].iter().sum();
            top -= offset;
            for &(c, w, n) in &entries {
                let bottom = top + n * LINE_H_262;
                if top >= panel[3] {
                    break; // below the panel (clipped, H304)
                }
                // H1810: the lines are hit-tested only inside the view rect: a line scrolled out above the
                // panel is not hit, a partly shown one only on its shown part
                if bottom > panel[1] {
                    lines.push((c, r16([panel[0], top.max(panel[1]), panel[0] + w, bottom.min(panel[3])])));
                    tops.push(top);
                }
                top = bottom + LINE_H_262 / 2; // the gap = line height / 2 (H161)
            }
            if max > 0 {
                let travel = (SCROLL_BAR[3] - SCROLL_BAR[1]) - THUMB_H;
                // (bar - thumb) / (positions - 1) x index, truncated (HYPOTHESIS H1811 for the operand order)
                let ty = SCROLL_BAR[1] + (travel as f64 / max as f64 * first as f64) as i32;
                scroll = Some(DboxScroller {
                    first,
                    max,
                    offset,
                    up: r16(at(SCROLL_UP)),
                    bar: r16(at(SCROLL_BAR)),
                    down: r16(at(SCROLL_DOWN)),
                    thumb: r16(at([SCROLL_BAR[0], ty, SCROLL_BAR[2], ty + THUMB_H])),
                });
            }
        }
        let tabs = [r16(at(TAB_TALK)), r16(at(TAB_HISTORY))];
        Some(DboxLayout {
            list: *list,
            rect: r16([x, y, x + BOX_W, y + BOX_H]),
            close: r16(at(CLOSE)),
            lines,
            tab,
            tabs,
            scroll,
            panel: r16(panel),
            tops,
        })
    }

    /// The selected panel's rows (H1810).
    fn dbox_rows(&self) -> Vec<i32> {
        let Some((_, _, convs)) = self.dbox.as_ref() else { return Vec::new() };
        let tab = if self.rules.h502_dbox_history_tab { self.dbox_tab } else { 0 };
        let n: Vec<i32> = convs
            .iter()
            .filter(|(c, a)| a.is_some() && self.conv_history.contains(c) == (tab == 1))
            .map(|&(_, a)| wrap_block(&a.and_then(|a| self.line_text(a)).unwrap_or_default(), TALK[2] - TALK[0], CHAR_W).1)
            .collect();
        scroll_rows(&n, LINE_H_262)
    }

    fn dbox_scroll_tab(&self) -> usize {
        (if self.rules.h502_dbox_history_tab { self.dbox_tab } else { 0 }) as usize & 1
    }

    /// HYPOTHESIS H1810 — a scroller step on the selected panel (the arrows: a line up / down; the bar above / below the
    /// thumb: a page up / down). Display only: no event, no state command.
    pub fn dbox_scroll_step(&mut self, step: Scroll) {
        let Some(sc) = self.dbox_layout().and_then(|l| l.scroll) else { return };
        let rows = self.dbox_rows();
        let tab = self.dbox_scroll_tab();
        self.dbox_scroll[tab] = scroll_step(&rows, TALK[3] - TALK[1], LINE_H_262, sc.first, sc.max, step);
    }

    /// HYPOTHESIS H1812 — a button-down on the bar: on the thumb it starts a drag (grab offset = y - thumb top), above it
    /// a page up, below it a page down.
    pub fn dbox_bar_press(&mut self, y: i32) {
        let Some(sc) = self.dbox_layout().and_then(|l| l.scroll) else { return };
        if y >= sc.thumb[1] as i32 && y < sc.thumb[3] as i32 {
            self.dbox_drag = Some(y - sc.thumb[1] as i32);
        } else if y < sc.thumb[1] as i32 {
            self.dbox_scroll_step(Scroll::PageUp);
        } else {
            self.dbox_scroll_step(Scroll::PageDown);
        }
    }

    /// HYPOTHESIS H1812 — the thumb dragged to pointer `(x, y)` (the drag state of FUN_004823a0): with x within 30 px of
    /// the thumb the thumb's top follows y - the grab offset (clamped to the bar) and the text scrolls to the matching
    /// position (the thumb's travel split evenly over the positions); farther away nothing moves.
    pub fn dbox_drag_to(&mut self, x: i32, y: i32) {
        let Some(grab) = self.dbox_drag else { return };
        let Some(sc) = self.dbox_layout().and_then(|l| l.scroll) else { return };
        if x < sc.thumb[0] as i32 - 30 || x > sc.thumb[2] as i32 + 30 {
            return;
        }
        let (top, bottom) = (sc.bar[1] as i32, sc.bar[3] as i32 - THUMB_H);
        let want = (y - grab).clamp(top, bottom);
        let step = (bottom - top) as f64 / sc.max as f64;
        let k = (((want - top) as f64 / step).round() as usize).min(sc.max);
        let tab = self.dbox_scroll_tab();
        self.dbox_scroll[tab] = k;
    }

    /// H262: the text of a line's asset (the first enabled AssetRef of the Conversation's f2c list) as characters (kind
    /// 0: XfilesT.dll string, H260 — e.g. 0x44a1 = string 13; kind 4: the blob's cp1251 bytes, one
    /// character each — only the count and the spaces matter for the layout)
    pub fn line_text(&self, a: u32) -> Option<Vec<char>> {
        let r = self.h.read(class::ASSET_REF, a)?;
        if r.u32("f40") & 7 == 4 {
            let b = self.h.blob_bytes(r.blob("asset")?)?;
            let n = *b.first()? as usize;
            let len: usize = std::str::from_utf8(b.get(1..1 + n)?).ok()?.parse().ok()?;
            return Some(b.get(1 + n..1 + n + len)?.iter().map(|&x| x as char).collect());
        }
        super::devices::asset_text(self, a).map(|t| t.chars().collect())
    }

    /// Window aid: the shown box's lines as (Conversation, rect, text) — the text of each line's asset (H260 / H262).
    pub fn dbox_line_texts(&self) -> Vec<(u32, [i16; 4], String)> {
        let Some(l) = self.dbox_layout() else { return Vec::new() };
        let Some((_, _, convs)) = self.dbox.as_ref() else { return Vec::new() };
        l.lines
            .iter()
            .map(|&(c, r)| {
                let a = convs.iter().find(|x| x.0 == c).and_then(|x| x.1);
                let t: String = a.and_then(|a| self.line_text(a)).map(|v| v.into_iter().map(cp1251).collect()).unwrap_or_default();
                (c, r, t)
            })
            .collect()
    }

    /// Window aid (H262): the shown box has lines in its HISTORY panel (Conversations of the box in the history set).
    pub fn dbox_has_history(&self) -> bool {
        self.dbox.as_ref().is_some_and(|d| d.2.iter().any(|x| self.conv_history.contains(&x.0)))
    }

    /// The box's built Conversations not in the history set (H262) whose HDB object is enabled (C15) — the
    /// lines of the TALK panel.
    pub fn dbox_talk_lines(&self) -> usize {
        self.dbox.as_ref().map_or(0, |d| {
            d.2.iter()
                .filter(|x| self.is_enabled(class::CONVERSATION, x.0))
                .filter(|x| !(self.rules.h262_dbox_rows && self.conv_history.contains(&x.0)))
                .count()
        })
    }

    /// HYPOTHESIS H502 — a click on a tab of the shown box (FUN_0044c8f0): that panel is selected. Display only — no
    /// event, no state command.
    pub fn dbox_select_tab(&mut self, tab: u8) {
        if self.rules.h502_dbox_history_tab && self.dbox.is_some() && self.dbox_shown {
            self.dbox_tab = tab.min(1);
        }
    }

    /// H135: `conv` is a line of the shown box (a Mouse Click on it is the dialogue click).
    pub fn dbox_line(&self, conv: u32) -> bool {
        self.layout_any().is_some_and(|l| l.lines.iter().any(|x| x.0 == conv))
    }

    /// HYPOTHESIS H135 — the line click before its fire: the box hides (state-0 command), the sequence of the first
    /// enabled AssetRef of f30 and of f34 is played (each registered + activated as a media player). Returns false when
    /// nothing is played (then [`World::conversation_unplayed`] runs after the fire).
    pub(super) fn conversation_clicked(&mut self, conv: u32) -> bool {
        let r = self.h.read(class::CONVERSATION, conv);
        let f = |n: &str| r.as_ref().map_or(0, |r| r.u32(n));
        let (f30, f34, f38) = (f("f30"), f("f34"), f("f38"));
        // H503: a line of the HISTORY panel (not in the talk list) is a replay
        let replay = self.rules.h503_history_replay && self.rules.h262_dbox_rows && self.conv_history.contains(&conv);
        // history flag == 1 → talk/history bookkeeping, no registration or event; H262: a talk line moves to the history
        // with count 1 — every clicked line of the traced saves is there; a history line only counts up (the count is
        // not modelled)
        // C231: only a line whose HDB f40 is set (0xff; 0 → the line stays on the talk panel, e.g. the bum's hint row)
        let kept = self.rules.c231_conv_history_flag && f("f40") == 0;
        if self.rules.h262_dbox_rows && !kept && !self.conv_history.contains(&conv) {
            super::list_add(&mut self.conv_history, conv, self.rules.h1602_sorted_lists);
        }
        if !self.idea_answer_playing {
            // C107: an idea's answer comes after the box's state 0 at the icon's button down (no second state command)
            self.dbox_state(false); // the box's state 0 (lines leave the talk list)
        }
        // H503 / H863 (R31: fo-096 replays 0x590e as f30 0x4e75, f38 0x529a, no f34): the replay's count path
        // (FUN_004e9440) — the first ask set the count to 1, the replay made it 2; nothing extra below 4; then a
        // time-seeded draw r1 in 1..10: r1 < 9 → f38, then r2 < 4 → f34 too. The core takes the likeliest outcome
        // (r1 < 9, r2 ≥ 4: p = 0.56, the traced one): f30, f38. r1 = 9 / 10 (refusal slots) and the count ≥ 4 path are
        // not modelled. Without an f38 asset no draw is made: the refusal-clip chooser (no clip without CharView slots /
        // with its draw ≥ 4) gives nothing and f34 is played
        let has_f38 = self.first_enabled_asset(f38).is_some();
        let tail_path = !(replay && has_f38);
        let lists: Vec<u32> = if replay && has_f38 { vec![f30, f38] } else { vec![f30, f34] };
        let mut assets: Vec<u32> = lists.into_iter().filter_map(|l| self.first_enabled_asset(l)).collect();
        if assets.is_empty() {
            return false;
        }
        // H1409 (`traces/fo-119.jsonl` 84.509 REG 0x4ff7 (XV/20471.xmv 0..2199) → Deact 106.243 = f34 0x10cd8's end 21598,
        // no REG of 0x10cd8; fo-066 / fo-102 0xdcdc, fo-059, fo-164, fo-187, fo-190 alike): an f34 asset in the f30 asset's
        // movie file is played by the f30 player, which runs on to the f34 asset's end
        if let Some(a30) = self.first_enabled_asset(f30) {
            self.media_tail.remove(&a30);
            if self.rules.h1409_conv_tail_merge && tail_path && assets.len() == 2 && assets[0] == a30 {
                let a34 = assets[1];
                if same_movie(self.h, a30, a34) {
                    self.media_tail.insert(a30, a34);
                    assets.truncate(1);
                }
            }
        }
        for &a in &assets {
            let tl = self.trigger_list_of(a);
            self.played.insert(a); // H145: its player's media end deactivates it (fo-033 173.17 / 182.00)
            self.out.push(Out::Reg { owner: a, tl, cause: RegCause::ConversationPlay { conv } });
            self.out.push(Out::Fire { owner: a, event: event::ACTIVATION });
        }
        self.dbox_seq = Some((conv, assets));
        true
    }

    /// HYPOTHESIS H135 — nothing to play: after the fire the Conversation is removed and the box gets state 1 again.
    pub(super) fn conversation_unplayed(&mut self, conv: u32) {
        self.remove_conversation(conv);
        if self.dbox.is_some() {
            self.dbox_state(true);
        }
    }

    /// HYPOTHESIS H139 — a Deactivation of a played asset; after the sequence's last one the Conversation's cmd 0xd
    /// removes it if it is disabled now. HYPOTHESIS H160 — then the View's re-show command shows the box again if it
    /// still has an enabled line.
    pub(super) fn dbox_seq_deact(&mut self, owner: u32) {
        let Some((conv, assets)) = &mut self.dbox_seq else { return };
        let conv = *conv;
        assets.retain(|&a| a != owner);
        if !assets.is_empty() {
            return;
        }
        self.dbox_seq = None;
        if self.rules.h651_dbox_reshow_queued {
            // H651: the sequence's end posts the Conversation's cmd 0xd and the View's re-show as queue commands;
            // the last asset's Deactivation posts its trigger actions in front of them (C24), so a line's follow-up
            // questions (type-3 Enables, H138) are built before the box decides to show or remove itself
            self.out.push(Out::DboxSeqEndPost { conv });
            return;
        }
        self.dbox_seq_end(conv);
    }

    /// The end of a played line's sequence (H139 cmd 0xd, then H160 / H501 re-show); with H651 a queued command.
    pub fn dbox_seq_end(&mut self, conv: u32) {
        if self.dbox.is_none() {
            return;
        }
        // H701 (provisional): an idea's answer (not a member of the box's list) is removed after its sequence too
        let idea_answer = (self.rules.h701_idea_drop || self.rules.c107_idea_click)
            && self.dbox.as_ref().is_some_and(|d| !self.h.list_of(class::CONVERSATION_LIST, d.1).contains(&conv));
        if !self.is_enabled(class::CONVERSATION, conv) || idea_answer {
            self.remove_conversation(conv);
        }
        // H1123: the re-show does nothing while cell (10,4) is raised (a player started by the line's last Deactivation)
        if self.rules.h1123_dbox_reshow_after_media && self.media_busy {
            self.dbox_reshow_waiting = true;
            return;
        }
        self.dbox_view_reshow();
    }

    /// H1123: the View's re-show after the last player of the FIFO ended, when a line's
    /// end had found media playing
    pub fn dbox_reshow_after_media(&mut self) {
        if std::mem::take(&mut self.dbox_reshow_waiting) && self.dbox.is_some() {
            self.dbox_view_reshow();
        }
    }

    /// H160 / H501: the re-show's box part (FUN_0041c740) — state 1 if a talk line is left, else removal
    fn dbox_view_reshow(&mut self) {
        if self.rules.h160_dbox_reshow && !self.dbox_shown {
            // the box's built Conversations not in the history set whose HDB object is enabled (C15)
            let lines = self.dbox_talk_lines();
            if lines > 0 {
                self.dbox_state(true);
            } else if self.rules.h501_dbox_removed_when_empty {
                // H501 — the first branch: on the TALK tab with no talk line and no enabled IdeaIcon of the box's
                // CharView → the box is removed; otherwise the inactive box gets state 1 (H502: on the HISTORY tab)
                let cv = self.dbox.as_ref().map(|d| d.0);
                let tab = if self.rules.h502_dbox_history_tab { self.dbox_tab } else { 0 };
                if tab == 0 && !cv.is_some_and(|cv| self.char_view_has_ideas(cv)) {
                    self.remove_dbox();
                } else {
                    self.dbox_state(true);
                }
            }
            // without H501: the box stays hidden (the removal branch not modelled)
        }
    }

    /// A state command of the box (the box's state handler, FUN_00435d90): 1 = lines in the talk list (shown).
    pub(super) fn dbox_state(&mut self, shown: bool) {
        self.dbox_shown = shown;
        // H1810: the panels are built anew at state 1 (and emptied at state 0): the text at the top
        self.dbox_scroll = [0; 2];
        self.dbox_drag = None;
        // H502 — state 1: the HISTORY panel (key 1) is selected iff there is no talk line, else TALK (key 0)
        if shown && self.rules.h502_dbox_history_tab {
            self.dbox_tab = u8::from(self.dbox_talk_lines() == 0);
        }
        if let Some(list) = self.dbox.as_ref().map(|d| d.1) {
            self.out.push(Out::DboxState { list, state: shown as u8 });
        }
        if self.rules.h700_dbox_idea_icons {
            self.dbox_idea_icons(shown);
        }
    }

    /// HYPOTHESIS H700 — the box's state handler after the Conversations' states: state 1 → every IdeaMap of the
    /// CharView's f4c whose IdeaIcon (f28) is enabled gets a new IdeaIcon object, posted, state 1; state 0 →
    /// every IdeaIcon object gets state 0 (if 1) and is removed. The port has no drag object (H151), so none is spared.
    /// Port guard (not in the decode): an IdeaIcon that already has an object is not built twice.
    fn dbox_idea_icons(&mut self, shown: bool) {
        let ideas: Vec<u32> = self.icon_objs.iter().filter(|o| o.cls == super::icons::IDEA_ICON_CLASS).map(|o| o.id).collect();
        if !shown {
            // the dragged icon (C107: the one being clicked) is spared
            let dragged = if self.rules.c107_idea_click { self.idea_drag } else { None };
            for id in ideas.into_iter().filter(|&i| Some(i) != dragged) {
                self.icon_remove(id);
            }
            return;
        }
        let Some(cv) = self.dbox.as_ref().map(|d| d.0) else { return };
        for icon in self.char_view_enabled_ideas(cv) {
            if ideas.contains(&icon) {
                continue;
            }
            self.icon_build(super::icons::IDEA_ICON_CLASS, icon, false, RegCause::BoxIdea { char_view: cv });
            self.icon_set_state(icon, true);
        }
    }

    /// H701 (provisional): `icon` dropped on CharView `cv` — the IdeaMap's Conversation is built into the box (REG TL f3c,
    /// Activation) and played as a clicked line (H135; nothing to play → H135's removal + re-show). False = no mapping.
    pub(super) fn idea_drop(&mut self, cv: u32, icon: u32) -> bool {
        let Some(list) = self.dbox.as_ref().filter(|d| d.0 == cv).map(|d| d.1) else { return false };
        let conv = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f4c"));
        let conv = self
            .h
            .list_of(super::icons::IDEA_MAP_LIST_CLASS, conv)
            .into_iter()
            .filter_map(|m| self.h.read(super::icons::IDEA_MAP_CLASS, m))
            .find(|r| r.u32("f28") == icon)
            .map_or(0, |r| r.u32("f2c"));
        if conv == 0 || !self.h.has(class::CONVERSATION, conv) {
            return false;
        }
        let tl = self.trigger_list_of(conv);
        self.out.push(Out::Reg { owner: conv, tl, cause: RegCause::Conversation { list } });
        if let Some(d) = &mut self.dbox {
            d.2.push((conv, None));
        }
        self.out.push(Out::Fire { owner: conv, event: event::ACTIVATION });
        if !self.conversation_clicked(conv) {
            self.conversation_unplayed(conv);
        }
        true
    }

    /// C107 (traced, `traces/fo-124.jsonl` 63.96–64.37): the button-down on an IdeaIcon of the shown box — the icon's
    /// Mouse Click, the drag object, the box's state 0 with every other IdeaIcon removed (Deactivations; the dragged
    /// one spared, H700). The answer waits
    /// for the button-up ([`World::idea_icon_release`]). False = not an IdeaIcon of a box (the caller runs the plain click).
    pub fn idea_icon_press(&mut self, icon: u32) -> bool {
        if !self.rules.c107_idea_click
            || self.dbox.is_none()
            || !self.icon_objs.iter().any(|o| o.cls == super::icons::IDEA_ICON_CLASS && o.id == icon)
        {
            return false;
        }
        self.fire(icon, event::MOUSE_CLICK);
        self.idea_drag = Some(icon);
        if self.dbox_shown {
            self.dbox_state(false);
        }
        true
    }

    /// The button-up while an IdeaIcon is dragged, over `target` (the topmost clickable owner under the point).
    ///
    /// C107 (traced): released on the icon itself → the IdeaMap's Conversation (f2c of the map whose f28 is the icon, in
    /// the box CharView's f4c list) registered and posted, event 6 on the icon (TL f40 = 0: nothing), the icon removed
    /// (Deactivation), the Conversation's
    /// Activation (fo-124: trigger 0x8881 → 0x8884 / 0x8885), its media played as a clicked line (H135); its sequence's
    /// end removes it and re-shows the box, which rebuilds the icons (69.317; R38).
    ///
    /// HYPOTHESIS H921 (port choice, not traced — no trace releases elsewhere): released on the box's CharView → the same
    /// answer (the walkthrough's "drag the idea onto the character"); released anywhere else → the drag is dropped and
    /// the box shown again (state 1: the other icons rebuilt, the dragged one kept), nothing played.
    pub fn idea_icon_release(&mut self, target: Option<u32>) {
        let Some(icon) = self.idea_drag.take() else { return };
        let cv = self.dbox.as_ref().map(|d| d.0);
        let answer = target == Some(icon) || (target.is_some() && target == cv);
        let conv = match cv.filter(|_| answer) {
            Some(cv) => {
                let f4c = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f4c"));
                self.h
                    .list_of(super::icons::IDEA_MAP_LIST_CLASS, f4c)
                    .into_iter()
                    .filter_map(|m| self.h.read(super::icons::IDEA_MAP_CLASS, m))
                    .find(|r| r.u32("f28") == icon)
                    .map_or(0, |r| r.u32("f2c"))
            }
            None => 0,
        };
        let list = self.dbox.as_ref().map(|d| d.1);
        let (Some(list), true) = (list, conv != 0 && self.h.has(class::CONVERSATION, conv)) else {
            // H921: no answer — the box comes back (the spared icon keeps its object; the others are rebuilt)
            if self.dbox.is_some() && !self.dbox_shown {
                self.dbox_state(true);
            } else {
                self.icon_remove(icon);
            }
            return;
        };
        let tl = self.trigger_list_of(conv);
        self.out.push(Out::Reg { owner: conv, tl, cause: RegCause::Conversation { list } });
        if let Some(d) = &mut self.dbox {
            d.2.push((conv, None));
        }
        self.icon_remove(icon);
        self.out.push(Out::Fire { owner: conv, event: event::ACTIVATION });
        // the box is already in state 0 (button-down): H135's own state-0 command is not repeated
        self.idea_answer_playing = true;
        if !self.conversation_clicked(conv) {
            self.conversation_unplayed(conv);
        }
        self.idea_answer_playing = false;
    }

    /// Removal of a built Conversation: Deactivation, then its asset (its cmd 12).
    fn remove_conversation(&mut self, conv: u32) {
        let Some(d) = &mut self.dbox else { return };
        let Some(i) = d.2.iter().position(|x| x.0 == conv) else { return };
        let (c, a) = d.2.remove(i);
        self.pending_children.retain(|&o| o != c);
        self.out.push(Out::Fire { owner: c, event: event::DEACTIVATION });
        if let Some(a) = a {
            self.out.push(Out::Fire { owner: a, event: event::DEACTIVATION });
        }
    }

    /// HYPOTHESIS H137 — a click in the close box (FUN_004bdb70): the box is removed with its
    /// Conversations. False if no box is shown.
    pub fn dbox_close(&mut self) -> bool {
        if !self.rules.h137_dbox_close || self.dbox.is_none() || !self.dbox_shown {
            return false;
        }
        self.remove_dbox();
        true
    }

    /// HYPOTHESIS H138 — type-3 Enable / Disable (C15 flag change) of a Conversation while a box is open.
    pub(super) fn conversation_enable(&mut self, target: u32, on: bool) {
        let Some(list) = self.dbox.as_ref().map(|d| d.1) else { return };
        if on {
            // a member of the box's list → built (no check for an existing object), posted, state 1 if
            // the box is active
            let built_by_box = !self.dbox_pending.contains(&list);
            if !built_by_box || !self.h.list_of(class::CONVERSATION_LIST, list).contains(&target) {
                return;
            }
            let tl = self.trigger_list_of(target);
            self.out.push(Out::Reg { owner: target, tl, cause: RegCause::Conversation { list } });
            self.pending_children.push(target);
            self.out.push(Out::Fire { owner: target, event: event::ACTIVATION });
            if let Some(d) = &mut self.dbox {
                d.2.push((target, None));
            }
        } else {
            // a Disable: the built Conversation gets a state-0 command and is removed
            self.remove_conversation(target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wrap_block;

    fn wb(s: &str) -> (i32, i32) {
        wrap_block(&s.chars().collect::<Vec<_>>(), 212, 7)
    }

    /// Placeholders with the word lengths of the dialogue lines on fo-055 k01 / k02 (`traces/fo-055.jsonl.shots`: one
    /// line, two lines wrapped before their last word) and fo-036 (a line 98 px wide: the click (345,311)
    /// right of it hit the box body)
    #[test]
    fn h262_wrap() {
        assert_eq!(wb("x xxx xxxxx?"), (84, 1));
        assert_eq!(wb("x xxx xxxx xxxxxx xx xxxxxxx xxxxxxxxx?"), (196, 2));
        assert_eq!(wb("x xxx xxx xxxxx x xxxxxxx?"), (182, 1));
        assert_eq!(wb("xxxxx xx xxxxxx xxxx x xxxx xxxx?"), (189, 2));
        assert_eq!(wb("xxx xxxx xxxx?"), (98, 1));
        assert_eq!(wb("xxxxx xx xxx xx xxxxxx xxxxxx?"), (210, 1));
    }
}

/// H1409: the two AssetRefs' first blob entries name the same movie file
fn same_movie(h: &crate::hdb::Hdb, a: u32, b: u32) -> bool {
    let file = |x: u32| {
        let r = h.read(class::ASSET_REF, x)?;
        let blob = crate::media::asset::AssetBlob::parse(h.blob_bytes(r.blob("asset")?)?, r.u32("f40"))?;
        blob.entries.first().map(|e| e.file.to_ascii_lowercase())
    };
    matches!((file(a), file(b)), (Some(x), Some(y)) if x == y)
}
