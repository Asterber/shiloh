//! Icons, inventory and CharViews as clickables (H150–H157; static decode of XFiles.exe, NOT traced except the
//! EmotionIcon click points checked post-hoc — RE_NOTES H150–H157).
//!
//! - H150 — icon bars (the original's display manager, FUN_004373e0). Every icon runtime object has a hit rect, set by
//!   the display manager when the icon's state becomes 1: position = the bar's cursor, or (cursor.x, cursor.y − height)
//!   for a bottom-aligned bar, size = the icon's display bounds (its shown picture = first enabled AssetRef of f30);
//!   then the cursor advances by width + spacing right (dir 2) or left (3), or by height + spacing down (0) or up (1);
//!   the bar counts its icons and its cursor goes back to the start when the count drops to 0 (state 0). Bars (start
//!   points fixed in the exe; mode = Pref_bClosedCaptioning (var 0xa0d, RegPreferences arg 4) for bars 1–5,
//!   Pref_bRolloverText (var 0x3d6, arg 5) for bar 0):
//!   0 kind 0xc IdeaIcon start (20,80) / rollover (20,74), bottom-aligned, right, spacing 5;
//!   1 kind 0xe ActionIcon (520,340) / cc (520,351), fixed, **left**, 20;
//!   2 kind 0x10 Inventory (10,420) / cc (10,425), fixed, right, 5;
//!   3 kind 0xf EmotionIcon (180,340) / cc (180,351), fixed, right, 20;
//!   4 kind 0x12 EvidenceIcon, not kit (type-3 Enable path) (20,390), bottom-aligned, right, 5;
//!   5 kind 0x12 EvidenceIcon, kit (10,418) / cc (10,422), bottom-aligned, up, 5 (not built by any modelled path);
//!   6 kind 0x22 = the game's EvidenceIcon 0xdb79 (530,48), fixed, right, 5 — hit only while the current View's picture
//!   is 400 high → never offered here (View pictures are 240 high).
//!   Hit chain (H130 (9)): y ≥ 330 → ActionIcons, EvidenceIcons (not kit), EmotionIcons, then Inventory
//!   objects, then the inventory panel (kind 0x11, rect (0,420)–(640,480), click = no-op; not offered: no owner); y < 90 →
//!   0xdb79, kind 0x1a ((501,0)–(640,34), not offered: no owner), IdeaIcons, kind 0xd ((0,12)–(500,80), no-op).
//! - H151 — icon click: every class starts with a Mouse Click (event 0) on the icon's TL. EmotionIcon: H68. ActionIcon
//!   (the original's FUN_004c0660): then every ActionIcon gets state 0 and is removed (not modelled: logged).
//!   EvidenceIcon: then the dialogue box is removed and the inventory panel gets state 0. IdeaIcon (draggable): a drag
//!   object is posted (not modelled).
//! - H152 — Inventory click (the original's FUN_0049c180): nothing at all if the item is the selected one; else the
//!   dialogue box is removed, the held object dropped, the selection set (the old item's cursor object (f38 VCCursor)
//!   gets state 0 and is removed, a cursor object of the new item's f38 is built (TL 0) and posted unless it exists),
//!   then the Mouse Click on the item (its TL f40 event 0 = OnInvClick(var), H153).
//! - H155 — CharView click (the original's FUN_00436080): Mouse Click (TL f54); then, with no inventory value held
//!   (RegCurrInvVar's variable = its "none" value), DisplayDBox(cv) (H126) iff an enabled VCConversation of its f48
//!   list or an enabled IdeaIcon (f28 of a VCIdeaMap) of its f4c list exists. With a value held: if a Mouse Click
//!   trigger action of its TL is conditioned on that variable's value nothing more, else a refusal video by value class
//!   (the CharView's refusal players) — not modelled (logged).
use super::{class, event, Out, RegCause, World};
use std::collections::HashMap;

/// H130 (9): the View band's top / bottom
pub const BAND_TOP: i32 = 90;
pub const BAND_BOTTOM: i32 = 330;

/// One bar of the display manager (H150): start (normal, alternative mode), fixed (else bottom-aligned), direction
/// (0 down, 1 up, 2 right, 3 left), spacing.
#[derive(Clone, Copy, Debug)]
pub struct BarCfg {
    pub start: [(i32, i32); 2],
    pub fixed: bool,
    pub dir: u8,
    pub spacing: i32,
}

pub const BARS: [BarCfg; 7] = [
    BarCfg { start: [(20, 80), (20, 74)], fixed: false, dir: 2, spacing: 5 }, // 0 IdeaIcon (mode: rollover text)
    BarCfg { start: [(520, 340), (520, 351)], fixed: true, dir: 3, spacing: 20 }, // 1 ActionIcon
    BarCfg { start: [(10, 420), (10, 425)], fixed: true, dir: 2, spacing: 5 }, // 2 Inventory
    BarCfg { start: [(180, 340), (180, 351)], fixed: true, dir: 2, spacing: 20 }, // 3 EmotionIcon
    BarCfg { start: [(20, 390), (20, 390)], fixed: false, dir: 2, spacing: 5 }, // 4 EvidenceIcon, not kit
    BarCfg { start: [(10, 418), (10, 422)], fixed: false, dir: 1, spacing: 5 }, // 5 EvidenceIcon, kit
    BarCfg { start: [(530, 48), (530, 48)], fixed: true, dir: 2, spacing: 5 }, // 6 kind 0x22 (0xdb79)
];

/// What a held value does to a CharView that does not react to it (H360).
#[derive(Clone, Copy, Debug)]
enum Refusal {
    /// play the video in this refusal slot
    Slot(usize),
    /// the first slot when it holds a video, else the second
    PreferSlot(usize, usize),
}

/// H360: inventory values → refusal, by ascending value.
const REFUSALS: [(std::ops::RangeInclusive<i32>, Refusal); 12] = [
    (1..=1, Refusal::Slot(4)),
    (4..=4, Refusal::Slot(4)),
    (5..=5, Refusal::PreferSlot(3, 5)),
    (6..=7, Refusal::Slot(4)),
    (9..=9, Refusal::Slot(7)),
    (10..=10, Refusal::Slot(8)),
    (11..=12, Refusal::Slot(4)),
    (16..=20, Refusal::Slot(6)),
    (21..=22, Refusal::Slot(4)),
    (23..=23, Refusal::Slot(7)),
    (25..=26, Refusal::Slot(4)),
    (30..=31, Refusal::Slot(4)),
];

/// H450: a group of Locations and the game-over video a gun shot into their Views plays.
pub struct GunScene {
    pub locations: &'static [u32],
    pub video: u32,
    /// only at the story points `gun_view_video` names (the warehouse)
    pub story_gated: bool,
}

/// H450: the gun-shot scenes.
pub const GUN_SCENES: [GunScene; 3] = [
    // Field Office, Comity Inn, Wong's Boat, Coroner's Office, Crime Lab, Hospital: Willmore shot
    GunScene { locations: &[0x12f, 0x197b, 0x11b7, 0x2973, 0x2a21, 0x5cff], video: 0x4cc6, story_gated: false },
    // Willmore's apartment: the arrest
    GunScene { locations: &[0xef1], video: 0x4cc8, story_gated: false },
    // Warehouse interior, link 1, link 2, exterior
    GunScene { locations: &[0x6b1, 0xa0c, 0xaaf, 0xbf3], video: 0xa83e, story_gated: true },
];

/// The game's own EvidenceIcon (H71), kind 0x22 (bar 6)
pub const GAME_ICON: u32 = 0xdb79;
/// Pref_bClosedCaptioning (RegPreferences arg 4) and Pref_bRolloverText (arg 5)
pub const VAR_CLOSED_CAPTIONING: u32 = 0xa0d;
pub const VAR_ROLLOVER_TEXT: u32 = 0x3d6;
const IDEA_ICON: u32 = 0x4a;
/// VCIdeaIcon class id
pub const IDEA_ICON_CLASS: u32 = IDEA_ICON;
pub const IDEA_MAP_CLASS: u32 = IDEA_MAP;
pub const IDEA_MAP_LIST_CLASS: u32 = IDEA_MAP_LIST;
const IDEA_MAP: u32 = 0x4c;
const IDEA_MAP_LIST: u32 = 0x4d;

/// Bar of an icon owner (by its HDB class), None for non-icons.
pub fn bar_of(w: &World, owner: u32) -> Option<usize> {
    if owner == GAME_ICON {
        return Some(6);
    }
    if w.icon_kit(owner) {
        return Some(5); // H205: a kit EvidenceIcon
    }
    [(IDEA_ICON, 0), (class::ACTION_ICON, 1), (class::INVENTORY, 2), (class::EMOTION_ICON, 3), (class::EVIDENCE_ICON, 4)]
        .into_iter()
        .find(|&(c, _)| w.h.has(c, owner))
        .map(|(_, b)| b)
}

/// Hit band of a bar: true = bottom (y ≥ 330), false = top (y < 90).
pub fn bar_bottom(bar: usize) -> bool {
    matches!(bar, 1..=5)
}

/// Placement state of the bars (count, cursor) and the rect each placed icon got.
#[derive(Clone, Debug, Default)]
pub struct Bars {
    count: [u32; 7],
    cur: [Option<(i32, i32)>; 7],
    pub rects: HashMap<u32, (usize, [i16; 4])>,
    /// icons placed and not removed, placement order per bar (= the app kind list order, H131)
    pub placed: Vec<u32>,
}

impl Bars {
    /// The display manager with state 1: place `owner` (size w×h) in `bar`.
    pub fn place(&mut self, bar: usize, owner: u32, (w, h): (i32, i32), alt: bool) {
        if self.placed.contains(&owner) {
            return;
        }
        let cfg = BARS[bar];
        let start = cfg.start[alt as usize];
        let (cx, cy) = self.cur[bar].unwrap_or(start);
        let (x, y) = if cfg.fixed { (cx, cy) } else { (cx, cy - h) };
        self.rects.insert(owner, (bar, [x as i16, y as i16, (x + w) as i16, (y + h) as i16]));
        self.cur[bar] = Some(match cfg.dir {
            2 => (cx + w + cfg.spacing, cy),
            3 => (cx - (w + cfg.spacing), cy),
            1 => (cx, cy - (h + cfg.spacing)),
            _ => (cx, cy + h + cfg.spacing),
        });
        self.count[bar] += 1;
        self.placed.push(owner);
    }

    /// The display manager with state 0: the count drops; at 0 the cursor goes back to the start. The rect is kept
    /// (the hit test needs the object to be active anyway).
    pub fn remove(&mut self, owner: u32) {
        let Some(i) = self.placed.iter().position(|&o| o == owner) else { return };
        self.placed.remove(i);
        if let Some(&(bar, _)) = self.rects.get(&owner) {
            self.count[bar] = self.count[bar].saturating_sub(1);
            if self.count[bar] == 0 {
                self.cur[bar] = None;
            }
        }
    }
}

impl World<'_> {
    /// The AssetRef an icon shows (H150: first enabled AssetRef of f30).
    pub fn icon_picture(&self, owner: u32) -> Option<u32> {
        let cls = [IDEA_ICON, class::ACTION_ICON, class::INVENTORY, class::EMOTION_ICON, class::EVIDENCE_ICON]
            .into_iter()
            .find(|&c| self.h.has(c, owner))?;
        let list = self.h.read(cls, owner)?.u32("f30");
        self.first_enabled_asset(list)
    }

    /// Window aid (H309): an icon's roll-over picture (first enabled AssetRef of f2c) and roll-over text (f3c), as
    /// (f2c asset, f3c asset)
    pub fn icon_rollover(&self, owner: u32) -> (Option<u32>, Option<u32>) {
        let cls = [IDEA_ICON, class::ACTION_ICON, class::INVENTORY, class::EMOTION_ICON, class::EVIDENCE_ICON]
            .into_iter()
            .find(|&c| self.h.has(c, owner));
        let Some(r) = cls.and_then(|c| self.h.read(c, owner)) else { return (None, None) };
        (self.first_enabled_asset(r.u32("f2c")), self.first_enabled_asset(r.u32("f3c")))
    }

    /// H150: bar mode — (closed captioning on, rollover text on), from the variables RegPreferences hands over (the
    /// "variables → preferences" path of H81; not traced which way the preferences go).
    pub fn bar_modes(&self) -> (bool, bool) {
        let on = |v: u32| self.vars.get(v).is_some_and(|x| x != 0);
        (on(VAR_CLOSED_CAPTIONING), on(VAR_ROLLOVER_TEXT))
    }

    /// H340: Pref_bClosedCaptioning (var 0xa0d ≠ 0) — the movies' text tracks are shown
    pub fn closed_captioning(&self) -> bool {
        self.bar_modes().0
    }

    /// H155 helper: an inventory value is held (RegCurrInvVar's variable ≠ its "none" value; the none
    /// value is var 0x90a Inventory_None = 0 in XFiles.gam).
    pub fn inventory_held(&self) -> bool {
        self.curr_inv_var.and_then(|v| self.vars.get(v)).is_some_and(|x| x != self.curr_inv_none)
    }

    /// The current inventory value (RegCurrInvVar's variable), or the "none" value.
    pub fn inventory_value(&self) -> i32 {
        self.curr_inv_var.and_then(|v| self.vars.get(v)).unwrap_or(self.curr_inv_none)
    }

    /// HYPOTHESIS H362 — the held object's Deactivation (the original's FUN_00437c20, the same for every kind-0x18
    /// class): RegCurrInvVar's variable is set to the "none" value (type 1).
    pub fn inventory_value_reset(&mut self) {
        if let Some(v) = self.curr_inv_var {
            self.vars.set_native(v, Some(self.curr_inv_none), 1);
        }
    }

    /// HYPOTHESIS H360 / H154 — the reaction test (the original's FUN_004d1f70): with a value held, some event-0 trigger
    /// of `tl` has a direct ActionList action whose condition has RegCurrInvVar's variable as one operand (kind 0) and
    /// whose other operand's value equals the variable's value (compared for equality whatever the condition's op is).
    /// DoActionList targets are not followed. `ev`: the event the triggers are tested for (Mouse Click from the
    /// CharView click; Roll Into from the CharView's hover handler, H820).
    pub fn reacts_to_held(&self, tl: u32, ev_tested: u8) -> bool {
        let Some(cur) = self.curr_inv_var else { return false };
        if tl == 0 || !self.inventory_held() {
            return false;
        }
        let value = self.inventory_value();
        for (trigger, ev, al) in self.triggers(tl) {
            if ev != ev_tested {
                continue;
            }
            for a in self.h.list_of(class::ACTION_LIST, al) {
                let Some(c) = super::action::Action::load(self.h, a).and_then(|a| a.condition()) else { continue };
                let other = |v: u32, k: u8| self.vars.operand_in(v, k, Some(trigger)) == Some(value);
                if (c.lhs_kind == 0 && c.lhs == cur && other(c.rhs, c.rhs_kind))
                    || (c.rhs_kind == 0 && c.rhs == cur && other(c.lhs, c.lhs_kind))
                {
                    return true;
                }
            }
        }
        false
    }

    /// HYPOTHESIS H360 — the CharView's refusal slots, as its activation fills them (the original's FUN_0043d3d0): the
    /// first member of the f48 ConversationList, if it is disabled (C15) and its f2c is 0, gives slots 0..6 = its f38
    /// AssetRefList in order (an AssetRef of kind f40 & 7 = 4 leaves its slot empty), slot 7 = AssetRef 0x4cc8, slot 8 =
    /// 0x4cc6. None = no refusal Conversation.
    pub fn refusal_slots(&self, cv: u32) -> Option<[u32; 9]> {
        let f48 = self.h.read(class::CHAR_VIEW, cv)?.u32("f48");
        let first = *self.h.list_of(class::CONVERSATION_LIST, f48).first()?;
        let conv = self.h.read(class::CONVERSATION, first)?;
        if self.is_enabled(class::CONVERSATION, first) || conv.u32("f2c") != 0 {
            return None;
        }
        let mut slots = [0u32; 9];
        let list = conv.u32("f38");
        if list != 0 && self.h.has(class::ASSET_REF_LIST, list) {
            for (i, a) in self.h.list_of(class::ASSET_REF_LIST, list).into_iter().take(7).enumerate() {
                let kind4 = self.h.read(class::ASSET_REF, a).is_some_and(|r| r.u32("f40") & 7 == 4);
                if !kind4 {
                    slots[i] = a;
                }
            }
            slots[7] = 0x4cc8;
            slots[8] = 0x4cc6;
        }
        Some(slots)
    }

    /// HYPOTHESIS H360 — the refusal slot for the held inventory value; None = no refusal
    /// (nothing held, no refusal slots, or a value without one: talk instead). Value 5 takes slot 3 when it holds a
    /// video, else slot 5 (the original picks at random when both do).
    pub fn refusal_slot(&self, slots: Option<&[u32; 9]>) -> Option<usize> {
        let slots = slots?;
        let value = self.inventory_value();
        if value == self.curr_inv_none {
            return None;
        }
        let (_, refusal) = REFUSALS.iter().find(|(values, _)| values.contains(&value))?;
        Some(match *refusal {
            Refusal::Slot(n) => n,
            Refusal::PreferSlot(first, other) => {
                if slots[first] != 0 {
                    first
                } else {
                    other
                }
            }
        })
    }

    /// HYPOTHESIS H820 — the CharView's hover handler (static, post-hoc fo-081 61.62 REG 0x4e15 on the clerk, fo-091
    /// 104.993 REG 0x4e25 on Skinner): after its Roll Into fired, with a value held, unless a Roll Into trigger of its TL
    /// reacts to the value (H360's test with event 4), a CharView with refusal slots holding the gun (9), handcuffs (10)
    /// or value 0x17 plays refusal slot 5 (a CharView-wrapper player posted) — the "don't point that at me" video; any
    /// other value → slot 10 = nothing.
    pub fn char_view_rolled_into(&mut self, cv: u32) {
        if !self.rules.h820_char_view_hover_refusal || !self.char_views.contains(&cv) || !self.inventory_held() {
            return;
        }
        let tl = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f54"));
        if self.reacts_to_held(tl, event::ROLL_INTO) {
            return;
        }
        let Some(slots) = self.refusal_slots(cv) else { return };
        if !matches!(self.inventory_value(), 9 | 10 | 0x17) {
            return;
        }
        let asset = slots[5];
        if asset != 0 {
            self.cv_assets.push_back((asset, cv));
            self.out.push(Out::PlayerPost { asset });
        }
    }

    /// HYPOTHESIS H821 — the gun's shot AssetRef: the application start (the original's FUN_0041b060) registers gun
    /// cursor 0x1bf6 and shot 0x87b7 (XS/92148.amv, "FBI Issue/gun.amv", the gunshot); RegGunAss would replace
    /// it (its only call, 0x164b5 RegGunAss(0, 0), is in the never-run debug list). 0 if the HDB has no such AssetRef.
    pub fn gun_shot_asset(&self) -> u32 {
        if self.h.has(class::ASSET_REF, 0x87b7) {
            0x87b7
        } else {
            0
        }
    }

    /// CharViews built for the current View (H74), build order.
    pub fn char_views(&self) -> &[u32] {
        &self.char_views
    }

    /// HYPOTHESIS H155 — what follows the Mouse Click on a CharView (the original's FUN_00436080): no value held →
    /// DisplayDBox iff an enabled Conversation of f48 or an enabled IdeaIcon of an IdeaMap of f4c exists; value held →
    /// H360: nothing more if the CharView reacts to it, else the refusal slot's AssetRef is played (a player posted; the
    /// AssetRef registers when it runs) or, with no refusal for that value, DisplayDBox as above.
    /// H701 (provisional): a click while an IdeaIcon is picked up / on an IdeaIcon. True = the click was the drop on a
    /// CharView (its own Mouse Click and H155 are skipped).
    pub fn idea_click(&mut self, owner: u32) -> bool {
        if !self.rules.h701_idea_drop {
            return false;
        }
        if let Some(icon) = self.idea_drag.take() {
            if self.char_views.contains(&owner) && self.idea_drop(owner, icon) {
                return true;
            }
        }
        if self.icon_objs.iter().any(|o| o.cls == IDEA_ICON && o.id == owner) {
            self.idea_drag = Some(owner);
        }
        false
    }

    /// HYPOTHESIS H2170 — the held camera's click (the original's FUN_004f26a0) at screen point `at`; `explorables`:
    /// the active Explorables in kind-list order. Returns the owners to get a Mouse Click (first reacting CharView, then
    /// first reacting Explorable) and posts the refusal-slot-2 player. Nothing when the camera has no viewfinder
    /// (iCurrViewSpecial = 1 at its selection — `dev.held_overlay`).
    /// `under`: the video under the viewfinder and its position (ms) — the playing video, else the View's looping
    /// background (C306)
    pub fn camera_shot(&mut self, at: (i32, i32), explorables: &[u32], under: Option<(u32, u64)>) -> Vec<u32> {
        if !self.dev.held_overlay {
            self.out.push(Out::Unimplemented { action: 0, what: "H2170: held camera click, no viewfinder (inert)".into() });
            return Vec::new();
        }
        // Camera.amv 0x87bb (DeadShtr.amv 0xdf10 at 24) restarted — sound not modelled
        // below 24 the game state's camera count + 1 (one exposure used)
        if self.pc.camera < 0x18 {
            self.pc.camera += 1;
            // C306: the shot is a VCPhoto of that video's frame (f64 = its AssetRef, f68 = its time in 1/600 s, rect = the
            // 320 × 240 crop at the viewfinder's left, x0 = pointer x − 160 in 0..280)
            if let Some((asset, ms)) = under.filter(|_| self.rules.c306_shot_photo) {
                let x0 = (at.0 - 160).clamp(0, 280) as i16;
                self.pc.photo_frames.resize(self.pc.photos.len(), None);
                self.pc.photos.push((asset, false));
                self.pc.photo_frames.push(Some(((ms * 600 / 1000) as u32, [x0, 0, x0 + 320, 240])));
            }
        }
        // the viewfinder's display rect (x.pff #242, 320 × 241 centred on the pointer, clipped to the band; H1773)
        let vf = [(at.0 - 160).max(20), (at.1 - 120).max(90), (at.0 + 160).min(620), (at.1 + 121).min(330)];
        let overlaps = |r: [i16; 4]| {
            let [x1, y1, x2, y2] = r.map(|v| v as i32);
            x1 < vf[2] && vf[0] < x2 && y1 < vf[3] && vf[1] < y2
        };
        let region = |cls: u32, id: u32, f: &str| match self.h.read(cls, id).and_then(|r| r.field(f).cloned()) {
            Some(crate::hdb::Value::Region { rect, .. }) => Some(rect),
            _ => None,
        };
        let mut fire = Vec::new();
        // a reaction = an event-0 trigger with an action conditioned on the held value (H154)
        let cv = self.char_views.iter().copied().find(|&cv| {
            let tl = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f54"));
            region(class::CHAR_VIEW, cv, "f2c").is_some_and(overlaps) && self.reacts_to_held(tl, event::MOUSE_CLICK)
        });
        fire.extend(cv);
        let ex = explorables.iter().copied().find(|&e| {
            let tl = self.registered.get(&e).copied().unwrap_or(0);
            region(class::EXPLORABLE, e, "region").is_some_and(overlaps) && self.reacts_to_held(tl, event::MOUSE_CLICK)
        });
        fire.extend(ex);
        // the third walk: the first CharView whose refusal slot 2 holds an AssetRef not playing now plays it
        if let Some((asset, cv)) = self.char_views.iter().find_map(|&cv| {
            let a = self.refusal_slots(cv)?[2];
            (a != 0 && !self.live_players.contains(&a)).then_some((a, cv))
        }) {
            self.cv_assets.push_back((asset, cv));
            self.out.push(Out::PlayerPost { asset });
        }
        self.out.push(Out::Unimplemented {
            action: 0,
            what: format!("H2170: camera shot at {at:?} (count {}), reacting {fire:x?}; shutter sound not modelled", self.pc.camera),
        });
        fire
    }

    pub fn char_view_clicked(&mut self, cv: u32) {
        if !self.char_views.contains(&cv) {
            return;
        }
        if self.inventory_held() {
            if !self.rules.h360_char_view_held_click {
                self.out.push(Out::Unimplemented { action: 0, what: format!("H155: CharView {cv:#x} clicked with an inventory value held (reaction / refusal video not modelled)") });
                return;
            }
            let tl = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f54"));
            if self.reacts_to_held(tl, event::MOUSE_CLICK) {
                return;
            }
            let slots = self.refusal_slots(cv);
            let asset = match (self.refusal_slot(slots.as_ref()), slots) {
                (Some(slot), Some(s)) => s[slot],
                _ => 0,
            };
            if asset != 0 {
                self.cv_assets.push_back((asset, cv));
                if self.inventory_value() == 9 && self.rules.h821_gun_shot_sound && self.gun_shot_asset() != 0 {
                    // H821: value 9 → the gun's shot player (AssetRef 0x87b7) played to its end first; the refusal
                    // player is posted after it (runner)
                    self.dev.requests.push(super::devices::DevReq::GunShotThenPlayer(asset));
                } else {
                    self.out.push(Out::PlayerPost { asset });
                }
                return;
            }
        }
        let Some(r) = self.h.read(class::CHAR_VIEW, cv) else { return };
        let f48 = r.u32("f48");
        let talk = f48 != 0
            && self.h.list_of(class::CONVERSATION_LIST, f48).into_iter().any(|c| self.is_enabled(class::CONVERSATION, c));
        if talk || self.char_view_has_ideas(cv) {
            self.display_dbox(cv);
        }
    }

    /// H155: an enabled IdeaIcon (table 9) named by f28 of a VCIdeaMap of the CharView's f4c VCIdeaMapList.
    pub(super) fn char_view_has_ideas(&self, cv: u32) -> bool {
        let f4c = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f4c"));
        f4c != 0
            && self.h.list_of(IDEA_MAP_LIST, f4c).into_iter().any(|m| {
                let icon = self.h.read(IDEA_MAP, m).map_or(0, |r| r.u32("f28"));
                icon != 0 && self.is_enabled(IDEA_ICON, icon)
            })
    }

    /// HYPOTHESIS H450 — the gun's click, reached by a View click that hits no CharView / Explorable / nav-hotspot while
    /// the gun (held type 7) is held: (1) the current Location picks a game-over video ([`GUN_SCENES`]); (2) the shot
    /// (0x87b7, H821 — played by the runner before this call, to its end when a video follows); (3) the click goes on
    /// to the View's Mouse Click; (4) the video's player is posted. Returns the video (0 = none).
    pub fn gun_view_click(&mut self) -> u32 {
        let asset = self.gun_view_video();
        if let Some(v) = self.view {
            self.fire(v, event::MOUSE_CLICK);
        }
        if asset != 0 && self.h.has(class::ASSET_REF, asset) {
            self.cv_assets.push_back((asset, 0));
            self.out.push(Out::PlayerPost { asset });
        }
        asset
    }

    /// H450 (1): the game-over video the current Location picks for a View shot (0 = none).
    pub fn gun_view_video(&self) -> u32 {
        /// the warehouse ending only plays at these story points: (node, view state or None = any)
        const WAREHOUSE_ENDING_AT: [(i32, Option<u8>); 2] = [(12, None), (11, Some(b'd'))];
        let loc = self.view.and_then(|v| self.viewpoint_of(v)).and_then(|vp| {
            self.h.class(class::LOCATION)?.objects.iter().map(|o| o.0).find(|&l| self.h.list_of(class::LOCATION, l).contains(&vp))
        });
        let loc = loc.or(self.location).unwrap_or(0);
        let node = self.node_vars.0.and_then(|v| self.vars.get(v)).unwrap_or(0);
        let state = self.view_state_var.and_then(|v| self.vars.get(v)).unwrap_or(0);
        let Some(scene) = GUN_SCENES.iter().find(|s| s.locations.contains(&loc)) else { return 0 };
        let allowed = !scene.story_gated
            || WAREHOUSE_ENDING_AT.iter().any(|&(n, st)| n == node && st.is_none_or(|c| state == c as i32));
        if allowed {
            scene.video
        } else {
            0
        }
    }

    /// HYPOTHESIS H451 — the tool comment (the original's FUN_004e2160) after a tool selection changed the value:
    /// binoculars 2 / flashlight 8 → refusal slot 2, night vision 0xd → slot 3, other values nothing; the first CharView
    /// of the View (build order; the registry test taken as "built") whose slot holds an AssetRef gets a 2000 ms timer
    /// that posts that AssetRef's player. Returns (asset, CharView) for the runner's clock.
    pub fn tool_comment(&self, value: i32) -> Option<(u32, u32)> {
        let slot = match value {
            2 | 8 => 2,
            0xd => 3,
            _ => return None,
        };
        self.char_views.iter().find_map(|&cv| {
            let a = self.refusal_slots(cv)?[slot];
            (a != 0).then_some((a, cv))
        })
    }

    /// Queue a CharView-wrapper player posted from outside a dispatch (H451 timer): the runner posts
    /// `Cmd::Player`, `run_player` registers it.
    pub(super) fn queue_char_view_player(&mut self, asset: u32, cv: u32) {
        self.cv_assets.push_back((asset, cv));
    }

    /// HYPOTHESIS H152 — the selection changes from `old` to `item` (the original's FUN_004b0710): the old item's
    /// cursor object (f38) gets state 0 and is removed when the new item has another cursor; the new cursor object is
    /// built (REG TL 0)
    /// and activated with its picture (as the startup cursors, H69) unless `exists(cursor)`.
    pub fn inventory_select(&mut self, old: Option<u32>, item: u32, exists: &dyn Fn(u32) -> bool) {
        let cursor = |w: &Self, i: u32| w.h.read(class::INVENTORY, i).map_or(0, |r| r.u32("f38"));
        let (oc, nc) = (old.map_or(0, |o| cursor(self, o)), cursor(self, item));
        if oc == nc {
            return;
        }
        if oc != 0 && exists(oc) {
            self.cursor_deact(oc, exists);
        }
        if nc != 0 && self.h.has(class::CURSOR, nc) && !exists(nc) {
            self.out.push(Out::Reg { owner: nc, tl: Some(0), cause: RegCause::Startup });
            let list = self.h.read(class::CURSOR, nc).map_or(0, |r| r.u32("f38"));
            if let Some(a) = self.first_enabled_asset(list) {
                if !exists(a) {
                    self.out.push(Out::Reg { owner: a, tl: Some(0), cause: RegCause::Startup });
                    self.out.push(Out::Fire { owner: a, event: event::ACTIVATION });
                }
            }
            self.out.push(Out::Fire { owner: nc, event: event::ACTIVATION });
        }
    }

    /// The deselect: the old selection's cursor object gets state 0 and is removed (Deactivation); H363: its picture
    /// AssetRef (the first enabled one of the cursor's f38, built with it) goes too.
    pub(super) fn cursor_deact(&mut self, cursor: u32, exists: &dyn Fn(u32) -> bool) {
        self.out.push(Out::Fire { owner: cursor, event: event::DEACTIVATION });
        if self.rules.h363_cursor_asset_deact {
            let list = self.h.read(class::CURSOR, cursor).map_or(0, |r| r.u32("f38"));
            if let Some(a) = self.first_enabled_asset(list).filter(|&a| exists(a)) {
                self.out.push(Out::Fire { owner: a, event: event::DEACTIVATION });
            }
        }
    }

    /// HYPOTHESIS H153 — OnInvClick(var) (the original's FUN_0049b410): if RegCurrInvVar's variable differs from the
    /// argument's value, the inventory panel is hidden, the held object (app kind 0x18) removed, the variable set to the
    /// value and, if a value is now held, a new held object (type 0, no registration) posted. Trigger level: only the
    /// variable write.
    pub(super) fn on_inv_click(&mut self, var: u32) {
        let Some(cur) = self.curr_inv_var else { return };
        let Some(v) = self.vars.get(var) else { return };
        if self.vars.get(cur) != Some(v) {
            self.vars.set_native(cur, Some(v), 1);
        }
    }
}

/// HYPOTHESIS H203–H206 — icon objects of the type-3 Enable / Disable (the original's FUN_004201a0, classes 0x47 / 0x48
/// / 0x49 / 0x4a; static, RE_NOTES H203–H206). Every icon object registers with TL = f40; posting the object gives its
/// Activation, which for every icon class builds the first enabled AssetRef of f30, f2c, f3c (as H60). The icon's
/// *state* is separate: a queued state command places the icon in its bar (H150) on a change to 1 and takes it out on 0.
/// The world tracks: built objects (`icon_objs`, list order; EmotionIcons keep their own `icons` list, H60), the ids in state
/// 1 (`icon_state1`) and the EvidenceIcon display flag (`ev_icons_shown`); `Out::IconState` tells the runner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconObj {
    pub cls: u32,
    pub id: u32,
    /// EvidenceIcon kind (set when the object is built): true = kit icon (bar 5)
    pub kit: bool,
}

impl World<'_> {
    /// A kit EvidenceIcon object (bar 5)
    pub fn icon_kit(&self, id: u32) -> bool {
        self.icon_objs.iter().any(|o| o.id == id && o.kit)
    }

    /// The icon's state is 1 (placed in its bar)
    pub fn icon_state(&self, id: u32) -> bool {
        self.icon_state1.contains(&id)
    }

    /// Icon objects built by the type-3 / start paths (classes 0x47 / 0x49 / 0x4a), list order
    pub fn icon_objects(&self) -> &[IconObj] {
        &self.icon_objs
    }

    /// The state command for `icon` — only a change reaches the bar (H150's state handler)
    pub(super) fn icon_set_state(&mut self, id: u32, on: bool) {
        if on {
            // a kit EvidenceIcon ignores state 1 while the EvidenceIcon display flag is set
            if self.icon_kit(id) && self.ev_icons_shown {
                return;
            }
            if self.icon_state1.contains(&id) {
                return;
            }
            self.icon_state1.push(id);
        } else {
            let Some(i) = self.icon_state1.iter().position(|&x| x == id) else { return };
            self.icon_state1.remove(i);
        }
        self.out.push(Out::IconState { icon: id, state: on });
    }

    /// Build + post: REG (TL f40), Activation (its assets built at the Activation, `create_children`).
    pub(super) fn icon_build(&mut self, cls: u32, id: u32, kit: bool, cause: RegCause) {
        if cls != class::EMOTION_ICON {
            self.icon_objs.push(IconObj { cls, id, kit });
        }
        let tl = self.trigger_list_of(id);
        self.out.push(Out::Reg { owner: id, tl, cause });
        self.pending_children.push(id);
        self.out.push(Out::Fire { owner: id, event: event::ACTIVATION });
    }

    /// The removal of an icon object: state 0 first if it is 1 (the callers queue that state command), then the
    /// removal — an activated object gets its Deactivation, then its assets (as H61).
    pub(super) fn icon_remove(&mut self, id: u32) {
        self.icon_set_state(id, false);
        self.icon_objs.retain(|o| o.id != id);
        self.icons.retain(|&x| x != id);
        self.pending_children.retain(|&o| o != id);
        if let Some(i) = self.icons_active.iter().position(|x| x.0 == id) {
            let (icon, assets) = self.icons_active.remove(i);
            self.out.push(Out::Fire { owner: icon, event: event::DEACTIVATION });
            for a in assets {
                if !self.icon_asset_held(a, icon) {
                    self.out.push(Out::Fire { owner: a, event: event::DEACTIVATION });
                }
            }
        }
    }

    /// The type-3 Enable / Disable of classes 0x47 / 0x48 / 0x49 / 0x4a, run only when the VCEnabled flag changes (the
    /// dispatcher compares the old flag with the new value first). True = handled here.
    pub(super) fn icon_type3(&mut self, action: u32, cls: u32, id: u32, on: bool) -> bool {
        let r = self.rules;
        let exists = |w: &Self| w.icon_objs.iter().any(|o| o.cls == cls && o.id == id);
        match cls {
            // H203: ActionIcon — Enable: an object built and posted; its activation queues state 1 (bar 1).
            // Disable: the ActionIcon object → state 0 if 1, then removed.
            class::ACTION_ICON if r.h203_action_icon_type3 => {
                if on {
                    if exists(self) {
                        self.out.push(Out::Unimplemented { action, what: format!("H203: ActionIcon {id:#x} built twice (second object not modelled)") });
                        return true;
                    }
                    self.icon_build(cls, id, false, RegCause::IconEnable { action });
                    self.icon_set_state(id, true);
                } else if exists(self) {
                    self.icon_remove(id);
                }
                true
            }
            // H204: EmotionIcon Enable: an object in state 0 → state 1; else a new object posted + state 1.
            // Disable: state 0 if 1, then removed (the object leaves the "exists" list).
            class::EMOTION_ICON if r.h204_emotion_icon_type3 => {
                let built = self.icons.contains(&id);
                if on {
                    if built && !self.icon_state(id) {
                        self.icon_set_state(id, true);
                    } else if built {
                        self.out.push(Out::Unimplemented { action, what: format!("H204: EmotionIcon {id:#x} in state 1 enabled again (a second object, not modelled)") });
                    } else {
                        self.icons.push(id);
                        self.icon_build(cls, id, false, RegCause::IconEnable { action });
                        self.icon_set_state(id, true);
                    }
                } else if built {
                    self.icon_remove(id);
                }
                true
            }
            // H205: EvidenceIcon Enable: a non-kit object built and posted, no state command. Disable: kit or
            // state 0 → removed; state 1 → every non-kit icon state 0, removal, every other non-kit icon state 1 (bar 4
            // re-placed from its start)
            class::EVIDENCE_ICON if r.h205_evidence_icon_type3 => {
                if on {
                    self.icon_build(cls, id, false, RegCause::IconEnable { action });
                    return true;
                }
                let Some(o) = self.icon_objs.iter().copied().find(|o| o.cls == cls && o.id == id) else { return true };
                if o.kit || !self.icon_state(id) {
                    self.icon_remove(id);
                    return true;
                }
                let plain: Vec<u32> = self.icon_objs.iter().filter(|o| o.cls == cls && !o.kit).map(|o| o.id).collect();
                for &x in &plain {
                    self.icon_set_state(x, false);
                }
                self.icon_remove(id);
                for &x in plain.iter().filter(|&&x| x != id) {
                    self.icon_set_state(x, true);
                }
                true
            }
            // H206: IdeaIcon Enable: only while the dialogue box exists in state 1: an object built (REG); kept,
            // posted and given state 1 iff the box's CharView has it in an IdeaMap of f4c, else deleted. Disable:
            // state 0 → removed; state 1 → state 0, removed, every
            // other IdeaIcon state 1.
            IDEA_ICON if r.h206_idea_icon_type3 => {
                if on {
                    let Some(cv) = self.dbox.as_ref().map(|d| d.0).filter(|_| self.dbox_shown) else { return true };
                    if self.char_view_has_idea(cv, id) {
                        self.icon_build(cls, id, false, RegCause::IconEnable { action });
                        self.icon_set_state(id, true);
                    } else {
                        let tl = self.trigger_list_of(id);
                        self.out.push(Out::Reg { owner: id, tl, cause: RegCause::IconEnable { action } });
                    }
                    return true;
                }
                if !exists(self) {
                    return true;
                }
                let was_on = self.icon_state(id);
                self.icon_remove(id);
                if was_on {
                    let others: Vec<u32> = self.icon_objs.iter().filter(|o| o.cls == cls).map(|o| o.id).collect();
                    for x in others {
                        self.icon_set_state(x, true);
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// H700 (the original's FUN_00460360): the IdeaIcons (f28 of each VCIdeaMap of the CharView's f4c list, list order)
    /// enabled in table 9
    pub(super) fn char_view_enabled_ideas(&self, cv: u32) -> Vec<u32> {
        let f4c = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f4c"));
        if f4c == 0 {
            return Vec::new();
        }
        self.h
            .list_of(IDEA_MAP_LIST, f4c)
            .into_iter()
            .map(|m| self.h.read(IDEA_MAP, m).map_or(0, |r| r.u32("f28")))
            .filter(|&i| i != 0 && self.is_enabled(IDEA_ICON, i))
            .collect()
    }

    /// An enabled IdeaIcon `icon` named by f28 of a VCIdeaMap of the CharView's f4c list
    fn char_view_has_idea(&self, cv: u32, icon: u32) -> bool {
        let f4c = self.h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f4c"));
        f4c != 0
            && self.h.list_of(IDEA_MAP_LIST, f4c).into_iter().any(|m| {
                self.h.read(IDEA_MAP, m).map_or(0, |r| r.u32("f28")) == icon && self.is_enabled(IDEA_ICON, icon)
            })
    }

    /// HYPOTHESIS H205 — DispEvIcons(n) (the original's FUN_004efb20): every active kit EvidenceIcon state 0, the
    /// EvidenceIcon display flag set to n ≠ 0, every non-kit EvidenceIcon state (n ≠ 0).
    pub(super) fn disp_ev_icons(&mut self, n: u32) {
        let objs: Vec<IconObj> = self.icon_objs.iter().copied().filter(|o| o.cls == class::EVIDENCE_ICON).collect();
        for o in objs.iter().filter(|o| o.kit) {
            if self.icons_active.iter().any(|x| x.0 == o.id) {
                self.icon_set_state(o.id, false);
            }
        }
        self.ev_icons_shown = n != 0;
        for o in objs.iter().filter(|o| !o.kit) {
            self.icon_set_state(o.id, n != 0);
        }
    }

    /// H203–H206 part of NukeAllIcons (H187): every tracked Action / Idea / Evidence icon object state 0 and removed.
    pub(super) fn nuke_icon_objects(&mut self) {
        let r = self.rules;
        let ids: Vec<u32> = self
            .icon_objs
            .iter()
            .filter(|o| match o.cls {
                class::ACTION_ICON => r.h203_action_icon_type3,
                class::EVIDENCE_ICON => r.h205_evidence_icon_type3,
                _ => r.h206_idea_icon_type3,
            })
            .map(|o| o.id)
            .collect();
        for id in ids {
            self.icon_remove(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hdb::Hdb;
    use crate::world::{Msg, Rules};
    use std::path::PathBuf;

    fn data() -> Option<(Hdb, Hdb)> {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
        if !d.join("XFiles.hdb").exists() {
            return None;
        }
        Some((Hdb::open(d.join("XFiles.hdb")).unwrap(), Hdb::open(d.join("XFiles.gam")).unwrap()))
    }

    fn run(w: &mut World, action: u32) -> Vec<Out> {
        let n = w.out.len();
        w.execute(Msg { action, trigger: 0 });
        w.out[n..].to_vec()
    }

    fn events(o: &[Out]) -> Vec<String> {
        o.iter()
            .filter_map(|x| match x {
                Out::Reg { owner, .. } => Some(format!("REG {owner:x}")),
                Out::Fire { owner, event } => Some(format!("{} {owner:x}", if *event == 8 { "Act" } else { "Deact" })),
                Out::IconState { icon, state } => Some(format!("S{} {icon:x}", *state as u8)),
                _ => None,
            })
            .collect()
    }

    /// H203: Enable 0x938c of ActionIcon 0x5ff5 (gam flag 0) → REG (TL f40) + Act + state 1; the Activation builds the
    /// icon's assets; Disable 0x938f → state 0, Deact icon, then its assets. Off → flag only.
    #[test]
    fn h203_action_icon_enable_disable() {
        let Some((h, g)) = data() else { return };
        let mut w = World::new(&h, &g, Rules::all());
        let o = events(&run(&mut w, 0x938c));
        assert_eq!(o, ["REG 5ff5", "Act 5ff5", "S1 5ff5"]);
        assert_eq!(w.trigger_list_of(0x5ff5), h.read(class::ACTION_ICON, 0x5ff5).map(|r| r.u32("f40")));
        let n = w.out.len();
        w.fire(0x5ff5, event::ACTIVATION);
        let assets: Vec<u32> = w.out[n..].iter().filter_map(|x| if let Out::Reg { owner, .. } = x { Some(*owner) } else { None }).collect();
        assert!(!assets.is_empty());
        let o = events(&run(&mut w, 0x938f));
        let mut want = vec!["S0 5ff5".to_string(), "Deact 5ff5".to_string()];
        want.extend(assets.iter().map(|a| format!("Deact {a:x}")));
        assert_eq!(o, want);
        assert!(w.icon_objects().is_empty());
        // a second Disable is no flag change: nothing
        assert!(events(&run(&mut w, 0x938f)).is_empty());
        let mut w = World::new(&h, &g, Rules { h203_action_icon_type3: false, ..Rules::all() });
        assert!(events(&run(&mut w, 0x938c)).is_empty());
    }

    /// H205: two EvidenceIcons enabled (no state), DispEvIcons(1) gives both state 1; Disable of the first (state 1) → both
    /// state 0, Deact of the first, the second state 1 again (bar 4 re-placed).
    #[test]
    fn h205_evidence_icon_replacement() {
        let Some((h, g)) = data() else { return };
        let mut w = World::new(&h, &g, Rules::all());
        assert_eq!(events(&run(&mut w, 0x7e31)), ["REG 443d", "Act 443d"]);
        assert_eq!(events(&run(&mut w, 0x7e37)), ["REG 4441", "Act 4441"]);
        w.fire(0x443d, event::ACTIVATION);
        w.fire(0x4441, event::ACTIVATION);
        w.disp_ev_icons(1);
        assert!(w.icon_state(0x443d) && w.icon_state(0x4441));
        w.icon_type3(0, class::EVIDENCE_ICON, 0x443d, false);
        let o = events(&w.out);
        let tail: Vec<&str> = o.iter().map(|s| s.as_str()).skip_while(|s| *s != "S0 443d").filter(|s| !s.starts_with("Deact") || *s == "Deact 443d").collect();
        assert_eq!(tail, ["S0 443d", "S0 4441", "Deact 443d", "S1 4441"]);
        assert_eq!(w.icon_objects().len(), 1);
    }

    /// H206: IdeaIcon 0x557f Enable (0x67b5) does nothing without a shown dialogue box; with CharView 0x584e's box shown
    /// (its f4c list holds IdeaMap 0x5979 → 0x557f) the object is built with state 1; Disable 0x6412 → state 0, Deact.
    #[test]
    fn h206_idea_icon_needs_box() {
        let Some((h, g)) = data() else { return };
        let mut w = World::new(&h, &g, Rules::all());
        assert!(events(&run(&mut w, 0x67b5)).is_empty());
        let mut w = World::new(&h, &g, Rules::all());
        w.dbox = Some((0x584e, 0x5968, Vec::new()));
        w.dbox_shown = true;
        assert_eq!(events(&run(&mut w, 0x67b5)), ["REG 557f", "Act 557f", "S1 557f"]);
        w.fire(0x557f, event::ACTIVATION);
        let o = events(&run(&mut w, 0x6412));
        assert_eq!(&o[..2], ["S0 557f", "Deact 557f"]);
    }

    /// H700: 0x557f enabled while no box is shown (flag only); CharView 0x584e's box state 1 builds it (REG,
    /// Act, S1); state 0 removes it (S0, Deact); with the rule off the box shows no IdeaIcon.
    #[test]
    fn h700_box_builds_enabled_ideas() {
        let Some((h, g)) = data() else { return };
        for on in [true, false] {
            let mut w = World::new(&h, &g, Rules { h700_dbox_idea_icons: on, ..Rules::all() });
            assert!(events(&run(&mut w, 0x67b5)).is_empty());
            assert!(w.is_enabled(IDEA_ICON, 0x557f));
            w.dbox = Some((0x584e, 0x5968, Vec::new()));
            let n = w.out.len();
            w.dbox_state(true);
            let o = events(&w.out[n..]);
            if !on {
                assert!(o.is_empty());
                continue;
            }
            assert!(o.windows(3).any(|x| x == ["REG 557f", "Act 557f", "S1 557f"]), "{o:?}");
            w.fire(0x557f, event::ACTIVATION);
            let n = w.out.len();
            w.dbox_state(false);
            let o = events(&w.out[n..]);
            assert_eq!(&o[..2], ["S0 557f", "Deact 557f"]);
            assert!(w.icon_objects().iter().all(|x| x.cls != IDEA_ICON));
        }
    }

    /// H204: Enable of a preloaded (active, state 0) EmotionIcon gives state 1 and no event (fo-005..fo-011: no REG / Act
    /// after 0xda6e..0xda70); Disable 0x7a41 = state 0, Deact icon + assets (H61); H207: the Deactivation clears the flag, so
    /// the later Disable 0x745a is nothing and a new Enable builds a new object.
    #[test]
    fn h204_h207_emotion_icon() {
        let Some((h, g)) = data() else { return };
        let mut w = World::new(&h, &g, Rules { h207_icon_deact_clears_flag: true, ..Rules::all() });
        w.icons.push(0x46fc);
        w.pending_children.push(0x46fc);
        w.fire(0x46fc, event::ACTIVATION);
        assert_eq!(events(&run(&mut w, 0xda6e)), ["S1 46fc"]);
        let o = events(&run(&mut w, 0x7a41));
        assert_eq!(&o[..2], ["S0 46fc", "Deact 46fc"]);
        assert!(o.len() > 2);
        assert!(!w.is_enabled(class::EMOTION_ICON, 0x46fc));
        assert!(events(&run(&mut w, 0x745a)).is_empty());
        assert_eq!(events(&run(&mut w, 0xda6e)), ["REG 46fc", "Act 46fc", "S1 46fc"]);
    }
}
