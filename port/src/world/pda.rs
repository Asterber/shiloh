//! HYPOTHESIS H624 (static, NOT traced) — the PDA note pad (layout 0x8035).
//!
//! The notes are AssetRefs in the game state's VCPDANotes list (EnablePDANote, H178: `Procs::notes`; H1602: an ordered
//! insert by objId — ascending unless a loaded list was not). They are grouped by day: the exe lists 23 InterfaceItems
//! of layout 0x8035 (never shown) whose AssetRefLists hold the notes of day 1..23, a key note per day and two extra
//! notes for day 3 (`exetables`: `note_days`, `note_keys`, `note_day3_extra`). Procedures:
//! - NotePadInit (the panel item 0xc8f4's Activation): the text panel (Courier New 12) in item 0xc8f4 (213,90)–(411,344);
//!   days := the number of leading days with at least one note in the list (at least 1); the current day is kept
//!   (initially 0); the day number (current + 1) is shown in item 0x1079e; the panel lists every note of the list, in
//!   list order, that belongs to the current day; the address link 0xf7bf is enabled (engine type 3) iff the day's key
//!   note is in the list — on day 3 also both extra notes.
//! - NoteFlipDay(0) next day (only while current < days − 1), NoteFlipDay(1) previous day (only while current > 0): on a
//!   change the panel is rebuilt; then the day number and 0xf7bf as above.
//! - EnableAllNotes: every day item's notes added to the list.
//! - MainScroll(0) / MainScroll(1) (items 0xace6 / 0xace7, repeated while held by their Timer triggers): the panel one
//!   line up / down (the step is not decoded — one text line assumed).
use super::{class, Msg, PostSrc, World};

/// the note pad's text panel (213,90)–(411,344)
pub const NOTE_PANEL: u32 = 0xc8f4;
/// the day number's item (365,365)–(380,380)
pub const NOTE_DAY_TEXT: u32 = 0x1079e;
/// the address link (303,365)–(335,381)
const NOTE_ADDR_LINK: u32 = 0xf7bf;

/// Native state of the note pad.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotePad {
    /// the day shown (0-based)
    pub day: usize,
    /// days with notes (≥ 1 after NotePadInit)
    pub days: usize,
    /// the main scroller's first line shown (MainScroll)
    pub scroll: usize,
    /// NotePadInit ran and ScrollTextKill did not
    pub open: bool,
}

impl World<'_> {
    /// H624: NotePadInit / NoteFlipDay / EnableAllNotes / MainScroll / ScrollTextKill; true = handled.
    pub(super) fn note_proc(&mut self, name: &str, args: &[u32]) -> bool {
        match (name, args) {
            ("NotePadInit", _) => {
                self.notes_ui.days = self.note_days().max(1);
                self.notes_ui.scroll = 0;
                self.notes_ui.open = true;
                self.note_link();
            }
            ("NoteFlipDay", &[dir, ..]) => {
                let n = &mut self.notes_ui;
                let changed = if dir == 0 && n.day + 1 < n.days {
                    n.day += 1;
                    true
                } else if dir == 1 && n.day > 0 {
                    n.day -= 1;
                    true
                } else {
                    false
                };
                if changed {
                    n.scroll = 0;
                }
                self.note_link();
            }
            ("EnableAllNotes", _) => {
                for &d in &self.h.exe.note_days {
                    let list = self.h.read(class::INTERFACE_ITEM, d).map_or(0, |r| r.u32("f44"));
                    for a in self.h.list_of(class::ASSET_REF_LIST, list) {
                        if !self.px.notes.contains(&a) {
                            super::list_add(&mut self.px.notes, a, self.rules.h1602_sorted_lists);
                        }
                    }
                }
                self.notes_ui.days = self.h.exe.note_days.len();
            }
            ("MainScroll", &[dir, ..]) if self.notes_ui.open => {
                // the front clamps the line to the laid-out text
                if dir == 0 {
                    self.notes_ui.scroll = self.notes_ui.scroll.saturating_sub(1);
                } else {
                    self.notes_ui.scroll += 1;
                }
            }
            ("ScrollTextKill", _) if self.notes_ui.open => self.notes_ui.open = false,
            _ => return false,
        }
        true
    }

    /// The day item's notes.
    fn day_notes(&self, day: usize) -> Vec<u32> {
        let Some(&item) = self.h.exe.note_days.get(day) else { return Vec::new() };
        let list = self.h.read(class::INTERFACE_ITEM, item).map_or(0, |r| r.u32("f44"));
        self.h.list_of(class::ASSET_REF_LIST, list)
    }

    /// Leading days with at least one note in the list.
    fn note_days(&self) -> usize {
        (0..self.h.exe.note_days.len()).take_while(|&d| self.day_notes(d).iter().any(|a| self.px.notes.contains(a))).count()
    }

    /// The address link 0xf7bf shown iff the day's key note (on day 3 also both extra notes) is in the list.
    fn note_link(&mut self) {
        let day = self.notes_ui.day;
        let exe = &self.h.exe;
        let mut on = exe.note_keys.get(day).is_some_and(|k| self.px.notes.contains(k));
        if day == 2 {
            on &= exe.note_day3_extra.iter().all(|k| self.px.notes.contains(k));
        }
        // an engine-built type-3 message on the InterfaceItem (as procs.rs engine_enable, H97)
        self.engine_enables.push_back((class::INTERFACE_ITEM, NOTE_ADDR_LINK, on as u8));
        self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
    }

    /// H624: the notes the panel shows — the list's notes of the current day, in list order.
    pub fn note_assets(&self) -> Vec<u32> {
        let day = self.day_notes(self.notes_ui.day);
        self.px.notes.iter().copied().filter(|a| day.contains(a)).collect()
    }
}
