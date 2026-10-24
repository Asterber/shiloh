//! The player's devices: PDA / phone inventory clicks, the phone's dialling, the PDA Enable, held objects, the inventory
//! panel, the top-right menu object, right click and key focus (H165–H174; static decode of XFiles.exe + HDB data, NOT
//! traced — RE_NOTES H165–H174).
//!
//! - H165 OnIntInvClick(var) (the original's FUN_00467fb0): value 0xe (PDA) and a video on screen → var 0x149d set to 1
//!   plus refusal sound 0x127a8 and the selection cleared; otherwise players / timers paused (no trigger effect). Every
//!   value: held object removed, panel hidden (prefs 'H'), selection cleared.
//! - H166 phone procedures (PhoneInit, PhoneButton, PhoneClear, PhoneMenuInit, PhoneFlipName, PhoneKeyPress, PhoneSend) and
//!   DoActionListID.
//! - H167 Enable of the PDA 0x442b: every inventory object removed, PDA first, the other enabled ones rebuilt.
//! - H168 DeselectInv with a held object (drops it); H171 On{Binoc,Camera,FLight,Gun,NVision}Click.
//! - H169 / H170 / H172 / H174: runner side (`Runner::menu_open`, the panel, `Runner::right_click`, `Runner::key_press`).
use super::run::{Clickable, Cmd, Runner};
use super::{class, event, Msg, Out, PostSrc, RegCause, World};

/// VCInventory objIds of the PDA and the phone
pub const PDA: u32 = 0x442b;
pub const PHONE: u32 = 0x442f;
/// "Inventory_iPDA" value (var 0x165)
const PDA_VALUE: i32 = 0xe;
/// " bDisablePDA"
const VAR_DISABLE_PDA: u32 = 0x149d;
/// "Pref_cInventory (H=hide,V=visible)" (an app preference)
pub const VAR_PREF_INVENTORY: u32 = 0x3d7;
/// "IFL_bPhoneHasNOTCalledLoneGunmen"
const VAR_LONE_GUNMEN: u32 = 0xa3f;
/// the phone's display item and the phone-book card list item (f44 = cards)
pub const PHONE_DISPLAY: u32 = 0x9a59;
const PHONE_CARDS_ITEM: u32 = 0xb0fd;
/// H166: the local area code
const AREA: &str = "206";
/// H166: the refusal recording (index into `Hdb::exe.phone_refusals`) for a too long, barred or bad number
const BAD_NUMBER: usize = 2;
/// H166: the refusal recording of an unlisted 7 / 10 / 11 character number, by its first digit (a non-digit → 3)
const REFUSAL_BY_FIRST_DIGIT: [usize; 10] = [2, 1, 2, 0, 2, 2, 3, 2, 3, 3];
/// H166: the refusal recording of an unlisted number of any other length
const INCOMPLETE: usize = 1;
/// the ring (trigger, action list: a random pick of one of three ring sounds)
const RING: (u32, u32) = (0x133c4, 0x133c6);
/// the Lone Gunmen's number (text AssetRef)
const LONE_GUNMEN: u32 = 0x10773;
/// PhoneKeyPress (the original's FUN_004c8340): key → the trigger / action list of the matching key
const PHONE_KEYS: [(u32, u32); 10] = [
    (0xa90a, 0xa90c),
    (0xa8e6, 0xa8e8),
    (0xa8ea, 0xa8ec),
    (0xa8ee, 0xa8f0),
    (0xa8f2, 0xa8f4),
    (0xa8f6, 0xa8f8),
    (0xa8fa, 0xa8fc),
    (0xa8fe, 0xa900),
    (0xa902, 0xa904),
    (0xa906, 0xa908),
];
const PHONE_SEND: (u32, u32) = (0xdf53, 0xdf54);
/// held-object types of the tool procedures (H171)
const HELD_TOOLS: [(&str, u8); 6] =
    [("OnInvClick", 0), ("OnBinocClick", 1), ("OnCameraClick", 2), ("OnFLightClick", 3), ("OnNVisionClick", 4), ("OnGunClick", 7)];

/// Requests of the world to the runner (state the runner owns: selection, panel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevReq {
    /// the selection cleared: the selection's cursor object removed, nothing selected
    Deselect,
    /// DeselectInv(item) with a held object: deselect if item = 0 or the selection is `item`
    DeselectIf(u32),
    /// the inventory panel (kind 0x11) gets state 0
    PanelHide,
    /// H451: the tool comment (AssetRef, CharView) — its player 2000 ms from now
    ToolComment(u32, u32),
    /// H821: the gun's shot played to its end, then the CharView refusal player of this AssetRef
    /// (already queued in the world's CharView player FIFO) posted
    GunShotThenPlayer(u32),
    /// H734: SelectInv(item) — held object dropped, panel shown, a click command on the inventory object posted
    SelectInv(u32),
    /// H736: SelectInv again on the held object's item (SetFLightProps with value 8)
    ReselectHeld,
    /// H890: every running timer paused (periodic: the delay becomes its period; one-shot: the time remaining)
    TimersPause,
}

/// Native device state (globals of XFiles.exe).
#[derive(Clone, Debug, Default)]
pub struct Devices {
    /// the phone number buffer (string variable 0xa3e "IFL_scPhoneNumber"; the core keeps strings natively)
    pub number: String,
    /// a phone-book card is shown (the next digit starts a new number)
    pub card_shown: bool,
    cards: Vec<u32>,
    card_index: usize,
    /// input (H165): a player shows a video (non-empty display rect) — the PDA is refused
    pub video_visible: bool,
    /// H168 / H171: a held object (kind 0x18) exists, with its type
    pub held: Option<u8>,
    /// H174: the focused InterfaceItem (SetIItemFocus) and the last key code
    pub focus: Option<u32>,
    pub key: Option<u32>,
    /// inventory objects activated and not deactivated (activation order)
    pub inv_active: Vec<u32>,
    pub requests: Vec<DevReq>,
    /// H890: the PDA click paused the timers; they resume after a scheduler pass that ends with no layout shown
    pub pda_paused: bool,
    /// H1040: SetLBoxTint(r, g, b) sets the renderer's tint slot 0; ClearLBoxTint empties it. The View's movies are
    /// multiplied by colour / 255 (C-BX1, `traces/fo-132.jsonl.shots/k05.png`)
    pub lbox_tint: Option<[u8; 3]>,
    /// H93 / H1042: SetFLightProps' three values (n = 2; c1 = 'O'; c2: 'B' 0, 'D' 2, else 1); all 0 until the first
    /// call or a Load (H1603: saved in the VCGameState record)
    pub flight: [u8; 3],
    /// H1603: `flight` was set (a SetFLightProps or a Load) — a save writes it then, else keeps the base save's bytes
    pub flight_known: bool,
    /// H1772 / H1773: the held object draws its overlay (the binoculars' mask: [`binoc_kind`] 0; the camera's
    /// viewfinder: iCurrViewSpecial ≠ 1), decided at the selection
    pub held_overlay: bool,
}

/// The binoculars' kind (the original's view switch FUN_0051bc70, kept in the binocular object) for RegViewSpecial's
/// value: 1 for 0x4cc / 0x4d4 / 0x14953, 2 for 1 and the background pairs / the 0x5bbe player swap, else 0.
pub fn binoc_kind(special: Option<i32>) -> u8 {
    match special {
        Some(0x4cc | 0x4d4 | 0x14953) => 1,
        Some(1 | 0x1ec6 | 0x1ec7 | 0x4ce8 | 0x11436 | 0x4ce4 | 0x11438 | 0x4cec | 0x118ea | 0x5bbe | 0x11432) => 2,
        _ => 0,
    }
}

/// The numbers of the phone's table (the exe's table 0x656040 text AssetRefs, `Hdb::exe.phones`, then the Lone Gunmen's) as `PhoneSend` compares them —
/// a data aid for tools that dial every number (xfplan hunt); no rule
pub fn phone_numbers(w: &World) -> Vec<String> {
    w.h.exe.phones.iter().map(|t| t.0).chain([LONE_GUNMEN]).filter_map(|a| asset_text(w, a)).collect()
}

/// Text of a text AssetRef (f40 & 7 = 4): blob `[n][n decimal digits = length][text]…`; None for other assets
/// (the original gives "" for them).
pub fn asset_text(w: &World, asset: u32) -> Option<String> {
    let r = w.h.read(class::ASSET_REF, asset)?;
    if r.u32("f40") & 7 == 0 && w.rules.h260_login_text {
        // H260: Pascal strings (resource type, id); type "6" = RT_STRING of XfilesT.dll
        let b = w.h.blob_bytes(r.blob("asset")?)?;
        let n1 = *b.first()? as usize;
        let ty = std::str::from_utf8(b.get(1..1 + n1)?).ok()?;
        let n2 = *b.get(1 + n1)? as usize;
        let id: u32 = std::str::from_utf8(b.get(2 + n1..2 + n1 + n2)?).ok()?.parse().ok()?;
        // H1836: the game language's string table first (same ids as the HDB's)
        let lang = w.lang_strings.as_ref().and_then(|t| t.get(&id));
        return (ty == "6").then(|| lang.or_else(|| w.h.res_strings.get(&id)).cloned()).flatten();
    }
    if r.u32("f40") & 7 != 4 {
        return None;
    }
    let b = w.h.blob_bytes(r.blob("asset")?)?;
    let n = *b.first()? as usize;
    let len: usize = std::str::from_utf8(b.get(1..1 + n)?).ok()?.parse().ok()?;
    Some(String::from_utf8_lossy(b.get(1 + n..1 + n + len)?).into_owned())
}

/// The last `n` characters of `s` (all of it when shorter).
fn right(s: &str, n: usize) -> String {
    let skip = s.chars().count().saturating_sub(n);
    s.chars().skip(skip).collect()
}

/// What a dialled number does (H166).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialled {
    /// more than 11 characters: the bad-number recording
    TooLong,
    /// 10 characters outside the local area: the long-distance recording
    LongDistance,
    /// the Lone Gunmen's number before they may be called: the bad-number recording
    Barred,
    /// a phone-book entry: its (trigger, action list) become the call, and the phone rings
    Listed(u32, u32),
    /// no entry: the refusal recording with this index
    Unlisted(usize),
}

/// Classify a dialled `number`. `book` = the exe's phone book (number text AssetRef, trigger, action list), `text` =
/// an AssetRef's text, `lone_gunmen_ok` = the Lone Gunmen may be called.
pub fn dial(book: &[(u32, u32, u32)], number: &str, text: &dyn Fn(u32) -> String, lone_gunmen_ok: bool) -> Dialled {
    let len = number.chars().count();
    if len > 11 {
        return Dialled::TooLong;
    }
    if len == 10 && !number.starts_with(AREA) {
        return Dialled::LongDistance;
    }
    if !lone_gunmen_ok && number == text(LONE_GUNMEN) {
        return Dialled::Barred;
    }
    // exact, or a local number: the last 7 characters of an entry in the local area
    let reaches = |entry: &str| entry == number || (len == 7 && entry.starts_with(AREA) && entry.ends_with(number));
    if let Some(&(_, trigger, list)) = book.iter().find(|e| reaches(&text(e.0))) {
        return Dialled::Listed(trigger, list);
    }
    if !matches!(len, 7 | 10 | 11) {
        return Dialled::Unlisted(INCOMPLETE);
    }
    let digit = number.chars().next().and_then(|c| c.to_digit(10));
    Dialled::Unlisted(digit.map_or(3, |d| REFUSAL_BY_FIRST_DIGIT[d as usize]))
}

impl World<'_> {
    /// H170: prefs 'H' (the panel hides itself) — var 0x3d7
    pub fn inventory_hides(&self) -> bool {
        self.vars.get(VAR_PREF_INVENTORY) == Some(b'H' as i32)
    }

    /// Bookkeeping of inventory objects (World::fire hook).
    pub(super) fn dev_fired(&mut self, owner: u32, ev: u8) {
        if !self.h.has(class::INVENTORY, owner) {
            return;
        }
        match ev {
            event::ACTIVATION => {
                self.dev.inv_active.retain(|&o| o != owner);
                self.dev.inv_active.push(owner);
            }
            event::DEACTIVATION => {
                self.dev.inv_active.retain(|&o| o != owner);
                self.inv_assets.retain(|x| x.0 != owner); // H1181
            }
            _ => {}
        }
    }

    pub(super) fn do_action_list(&mut self, action: u32, (trigger, al): (u32, u32)) {
        for act in self.h.list_of(class::ACTION_LIST, al) {
            self.post(Msg { action: act, trigger }, PostSrc::Proc { action });
        }
    }

    /// Item `item` (of a shown layout) shows AssetRef `asset` (as H96's `pda_show`).
    pub(super) fn item_show_asset(&mut self, item: u32, asset: u32) {
        let Some(slot) = self.layouts.iter_mut().flat_map(|l| l.1.iter_mut()).find(|x| x.0 == item) else { return };
        let old = slot.1;
        if old == Some(asset) {
            return;
        }
        slot.1 = Some(asset);
        let tl = self.trigger_list_of(asset);
        self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::ObjectAsset { of: item } });
        if let Some(o) = old {
            self.out.push(Out::Fire { owner: o, event: event::DEACTIVATION });
        }
        self.out.push(Out::Fire { owner: asset, event: event::ACTIVATION });
    }

    fn phone_card(&mut self) {
        let Some(&card) = self.dev.cards.get(self.dev.card_index) else { return };
        self.dev.card_shown = true;
        self.item_show_asset(PHONE_DISPLAY, card);
        self.dev.number = right(&asset_text(self, card).unwrap_or_default(), 10);
    }

    /// The held object is removed: its Deactivation — H362: it resets RegCurrInvVar's variable; H1420: the binoculars'
    /// (type 1) then switch the View back (mode 0). False if no object was held.
    pub fn held_drop(&mut self) -> bool {
        let Some(typ) = self.dev.held.take() else { return false };
        if self.rules.h362_drop_resets_inv_var {
            self.inventory_value_reset();
        }
        if typ == 1 {
            self.binoc_view_swap(0, false);
        }
        true
    }

    /// HYPOTHESIS H1420 — the binoculars' View switch (the original's FUN_0051bc70; static; C230 for 0x1ec7 ↔ 0x1ec6,
    /// `traces/fo-198.jsonl`): by the value of RegViewSpecial's variable: 0x4cc / 0x4d4 / 0x14953 / 1 → handled,
    /// nothing switched; a background pair → mode 1 picks the second, else the first: with a View background object
    /// two type-3 messages are posted: Disable the current background's AssetRef, Enable the new one (the H53
    /// re-choice starts it, H1220 removes the old); 0x5bbe ↔ 0x11432 → a playing player swapped (not modelled).
    /// True = handled (OnBinocClick then skips H451's comment).
    pub(super) fn binoc_view_swap(&mut self, action: u32, on: bool) -> bool {
        if !self.rules.h1420_binoc_view_swap {
            return false;
        }
        let Some(v) = self.view_special_var.and_then(|v| self.vars.get(v)) else { return false };
        let pair = match v {
            0x4cc | 0x4d4 | 0x14953 | 1 => return true,
            0x1ec6 | 0x1ec7 => (0x1ec7, 0x1ec6),
            0x4ce8 | 0x11436 => (0x4ce8, 0x11436),
            0x4ce4 | 0x11438 => (0x4ce4, 0x11438),
            0x4cec | 0x118ea => (0x4cec, 0x118ea),
            0x5bbe | 0x11432 => {
                let (from, to) = if on { (0x5bbe, 0x11432) } else { (0x11432, 0x5bbe) };
                self.out.push(Out::Unimplemented { action, what: format!("H1420: binocular player swap {from:#x} → {to:#x} not modelled") });
                return true;
            }
            _ => return false,
        };
        let new = if on { pair.1 } else { pair.0 };
        if let Some(old) = self.background {
            for (target, state) in [(old, 0), (new, 1)] {
                self.engine_enables.push_back((class::ASSET_REF, target, state));
                self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
            }
        }
        true
    }

    /// Type-6 procedures of the devices; true = handled.
    pub(super) fn device_proc(&mut self, action: u32, name: &str, args: &[u32]) -> bool {
        let r = self.rules;
        match (name, args) {
            // H165
            ("OnIntInvClick", &[var, ..]) if r.h165_on_int_inv_click => {
                let Some(v) = self.vars.get(var) else {
                    self.out.push(Out::Unimplemented { action, what: format!("H165: OnIntInvClick value of {var:#x} unknown") });
                    return true;
                };
                if v == PDA_VALUE {
                    if self.dev.video_visible {
                        self.vars.set_native(VAR_DISABLE_PDA, Some(1), 2);
                        self.out.push(Out::Unimplemented { action, what: "H165: PDA refused — sound 0x127a8 (modal) not modelled".into() });
                        self.dev.requests.push(DevReq::Deselect);
                        return true;
                    }
                    if r.h890_timer_pause {
                        // H890 / C98: every timer paused, the PDA's pause remembered
                        self.dev.requests.push(DevReq::TimersPause);
                        self.dev.pda_paused = true;
                        self.out.push(Out::Unimplemented { action, what: "H165: players paused (not modelled)".into() });
                    } else {
                        self.out.push(Out::Unimplemented { action, what: "H165: players / timers paused (not modelled)".into() });
                    }
                }
                self.held_drop(); // H362 / H1420: the held object's Deactivation
                if r.h200_deselect_before_panel_hide {
                    // H200: the deselect runs at once, the panel's state 0 is a queued command (fo-037 175.06 → 175.74)
                    self.dev.requests.push(DevReq::Deselect);
                }
                if self.inventory_hides() {
                    self.dev.requests.push(DevReq::PanelHide);
                }
                if !r.h200_deselect_before_panel_hide {
                    self.dev.requests.push(DevReq::Deselect);
                }
                true
            }
            // H153 / H171 (the original's OnInvClick, FUN_0049b410): the variable takes the value if it differs; the
            // old held object goes, a new one (type by procedure) exists iff a value is held
            (n, &[var, ..]) if r.h171_held_tools && HELD_TOOLS.iter().any(|t| t.0 == n) => {
                let typ = HELD_TOOLS.iter().find(|t| t.0 == n).map_or(0, |t| t.1);
                let (Some(cur), Some(v)) = (self.curr_inv_var, self.vars.get(var)) else { return true };
                if self.vars.get(cur) != Some(v) {
                    // H1772 / H1773: the overlay by RegViewSpecial's value before the selection changes anything
                    let special = self.view_special_var.and_then(|v| self.vars.get(v));
                    let overlay = match typ {
                        1 => binoc_kind(special) == 0,
                        2 => special != Some(1),
                        _ => true,
                    };
                    // H1420: the View switch (mode 1) first (OnBinocClick only); a handled View skips H451's comment
                    let swapped = typ == 1 && self.binoc_view_swap(action, true);
                    self.on_inv_click(var);
                    if self.inventory_hides() {
                        self.dev.requests.push(DevReq::PanelHide); // prefs 'H': panel state 0
                    }
                    self.dev.held = self.inventory_held().then_some(typ);
                    self.dev.held_overlay = overlay;
                    // H451: the tool comment at the end of OnBinocClick / OnCameraClick / OnFLightClick / OnNVisionClick;
                    // OnCameraClick: no comment when iCurrViewSpecial = 1
                    let camera_quiet = typ == 2 && special == Some(1);
                    if r.h451_tool_comment && matches!(typ, 1 | 2 | 3 | 4) && !swapped && !camera_quiet {
                        if let Some((a, cv)) = self.tool_comment(self.inventory_value()) {
                            self.dev.requests.push(DevReq::ToolComment(a, cv));
                        }
                    }
                }
                true
            }
            // H168: only with a held object
            ("DeselectInv", &[item, ..]) if r.h168_deselect_held => {
                if self.dev.held.is_some() {
                    self.dev.requests.push(DevReq::DeselectIf(item));
                }
                true
            }
            // H174: keyboard focus (H95 keeps its no-op arm)
            ("SetIItemFocus", &[item, ..]) if r.h174_key_focus => {
                self.dev.focus = Some(item);
                false
            }
            // H166 PhoneInit: empty display text, number cleared, tone players (not modelled)
            ("PhoneInit", _) if r.h166_phone => {
                self.item_text_asset(PHONE_DISPLAY);
                self.dev.number.clear();
                true
            }
            // H166 PhoneButton
            ("PhoneButton", &[c, _, item, ..]) if r.h166_phone => {
                let ch = self.vars.get(c).map(|v| v as u8 as char);
                if let Some(d) = ch.filter(|c| c.is_ascii_digit()) {
                    if self.dev.number.chars().count() < 13 || self.dev.card_shown {
                        if self.dev.card_shown {
                            self.dev.card_shown = false;
                            self.dev.number.clear();
                        }
                        self.dev.number.push(d);
                        self.item_text_asset(PHONE_DISPLAY);
                    }
                }
                if item != 0 {
                    // the key's "pressed" picture item disabled — 100 ms later in the original (H201: at the queue's back)
                    self.engine_enables.push_back((class::INTERFACE_ITEM, item, 0));
                    if r.h201_phone_key_post_back {
                        self.post_back(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
                    } else {
                        self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
                    }
                }
                true
            }
            // H166 PhoneClear
            ("PhoneClear", _) if r.h166_phone => {
                self.item_text_asset(PHONE_DISPLAY);
                self.dev.number.clear();
                true
            }
            // H166 PhoneMenuInit: the phone-book cards = enabled AssetRefs of item 0xb0fd's f44
            ("PhoneMenuInit", _) if r.h166_phone => {
                let list = self.h.read(class::INTERFACE_ITEM, PHONE_CARDS_ITEM).map_or(0, |r| r.u32("f44"));
                self.dev.cards = if list == 0 { Vec::new() } else { self.h.list_of(class::ASSET_REF_LIST, list) }
                    .into_iter()
                    .filter(|&x| self.is_enabled(class::ASSET_REF, x))
                    .collect();
                if self.dev.cards.is_empty() {
                    self.item_text_asset(PHONE_DISPLAY);
                    self.dev.number.clear();
                } else {
                    self.dev.card_index %= self.dev.cards.len();
                    self.phone_card();
                }
                true
            }
            // H166 PhoneFlipName
            ("PhoneFlipName", &[dir, ..]) if r.h166_phone => {
                let n = self.dev.cards.len();
                if self.dev.card_shown && n > 1 {
                    self.dev.card_index = if dir == 0 { (self.dev.card_index + 1) % n } else { (self.dev.card_index + n - 1) % n };
                    self.phone_card();
                }
                true
            }
            // H166 PhoneKeyPress: the key of the current key event (H174)
            ("PhoneKeyPress", _) if r.h166_phone => {
                match self.dev.key {
                    Some(0xd) => self.do_action_list(action, PHONE_SEND),
                    Some(k @ (0x30..=0x39 | 0x60..=0x69)) => self.do_action_list(action, PHONE_KEYS[(k & 0xf) as usize]),
                    Some(_) => {}
                    None => self.out.push(Out::Unimplemented { action, what: "H166: PhoneKeyPress without a key".into() }),
                }
                true
            }
            // H166: the dialled number calls, rings or plays a refusal recording
            ("PhoneSend", &[_, tvar, alvar, ..]) if r.h166_phone => {
                let node = self.node_vars.0.and_then(|v| self.vars.get(v)).unwrap_or(0);
                let lone_gunmen_ok = node >= 2 && self.vars.get(VAR_LONE_GUNMEN).is_some_and(|v| v != 0);
                let h = self.h;
                let text = |a: u32| asset_text(self, a).unwrap_or_default();
                let refusal = |i: usize| h.exe.phone_refusals.get(i).copied();
                let post = match dial(&h.exe.phones, &self.dev.number, &text, lone_gunmen_ok) {
                    Dialled::TooLong | Dialled::Barred => refusal(BAD_NUMBER),
                    Dialled::LongDistance => h.exe.phone_long_distance,
                    Dialled::Unlisted(i) => refusal(i),
                    Dialled::Listed(t, al) => {
                        for (var, v) in [(tvar, t), (alvar, al)] {
                            if self.vars.get(var) != Some(v as i32) {
                                self.vars.set_native(var, Some(v as i32), 1);
                            }
                        }
                        Some(RING)
                    }
                };
                if let Some(p) = post {
                    self.do_action_list(action, p);
                }
                true
            }
            // H166: DoActionList(value of a, value of b)
            ("DoActionListID", &[a, b, ..]) if r.h166_phone => {
                match (self.vars.get(a), self.vars.get(b)) {
                    (Some(t), Some(al)) => self.do_action_list(action, (t as u32, al as u32)),
                    _ => self.out.push(Out::Unimplemented { action, what: "H166: DoActionListID values unknown".into() }),
                }
                true
            }
            ("PhoneDestroy", _) if r.h166_phone => true,
            _ => false,
        }
    }

    /// HYPOTHESIS H167 (the original's FUN_004a1c70): Enable of the PDA — every inventory object removed (Deact), the
    /// PDA built first, then every other enabled Inventory record (objId order).
    pub(super) fn pda_enable(&mut self, action: u32) {
        self.held_drop(); // H362 / H1420
        for o in std::mem::take(&mut self.dev.inv_active) {
            self.out.push(Out::Fire { owner: o, event: event::DEACTIVATION });
        }
        let others: Vec<u32> = self.enabled_of(class::INVENTORY).into_iter().filter(|&i| i != PDA).collect();
        for id in std::iter::once(PDA).chain(others) {
            let tl = self.trigger_list_of(id);
            self.out.push(Out::Reg { owner: id, tl, cause: RegCause::InventoryEnable { action } });
            self.pending_children.push(id);
            self.out.push(Out::Fire { owner: id, event: event::ACTIVATION });
        }
        if self.inventory_hides() {
            self.dev.requests.push(DevReq::PanelHide);
        }
    }

    /// The deselect on the world's side: the old selection's cursor (f38) gets its Deactivation if it exists.
    fn inventory_deselect(&mut self, old: Option<u32>, exists: &dyn Fn(u32) -> bool) {
        let oc = old.and_then(|o| self.h.read(class::INVENTORY, o)).map_or(0, |r| r.u32("f38"));
        if oc != 0 && exists(oc) {
            self.cursor_deact(oc, exists);
        }
    }
}

/// Owner ids of the engine objects without an HDB record (H169 kind 0x1a, H170 kind 0x11)
pub const MENU_OWNER: u32 = 0xffff_ff1a;
pub const PANEL_OWNER: u32 = 0xffff_ff11;
/// H450: the held gun (kind 0x18, type 7) as a click target
pub const GUN_OWNER: u32 = 0xffff_ff18;
/// H452: a playing video player (kind 0x1b) as a click target
pub const VIDEO_OWNER: u32 = 0xffff_ff1b;
/// H2170: the held camera (kind 0x18, type 2) as a click target
pub const CAMERA_OWNER: u32 = 0xffff_ff1c;
/// H404 / H407: the screens the menu opens (save, load, options, help)
pub const MENU_SCREENS: [u32; 4] = [0x14269, 0x142ca, 0xc24a, 0xc51c];
/// H169: kind 0x1a rect — top band
pub const MENU_RECT: [i16; 4] = [501, 0, 640, 34];
/// H156 / H170: kind 0x11 rect — bottom band
pub const PANEL_RECT: [i16; 4] = [0, 420, 640, 480];

/// What the in-game menu (H169) does next — its seven items (H404: rects (472, y)–(638, y + 36) from the top; the
/// original's click handler FUN_004978a0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuChoice {
    /// "New": a new game (in a running game the save question first — the front-end's)
    New,
    Load,
    Save,
    /// "Return": back to the game
    Resume,
    /// "Options": the preferences loaded, then layout 0xc24a shown
    Options,
    /// "Help": layout 0xc51c shown
    Help,
    /// "Quit": the front-end ends the program
    Quit,
}

fn inside(r: [i16; 4], x: i32, y: i32) -> bool {
    x >= r[0] as i32 && x < r[2] as i32 && y >= r[1] as i32 && y < r[3] as i32
}

impl Runner<'_> {
    /// Requests of the world (H165 / H167 / H168): selection and panel state live here.
    pub(super) fn device_requests(&mut self) {
        for req in std::mem::take(&mut self.w.dev.requests) {
            match req {
                DevReq::Deselect => self.deselect(),
                DevReq::DeselectIf(item) => {
                    if item == 0 || self.inv_selected == Some(item) {
                        self.drop_held();
                    }
                }
                DevReq::PanelHide => self.panel_set(false),
                DevReq::ToolComment(a, cv) => self.tool_comment_at = Some((self.now + 2000, a, cv)),
                DevReq::GunShotThenPlayer(a) => {
                    if !self.gun_shot_start(Some(Cmd::Player(a))) {
                        self.post(Cmd::Player(a));
                    }
                }
                DevReq::ReselectHeld => {
                    if let Some(item) = self.inv_selected.filter(|_| self.w.dev.held.is_some()) {
                        self.select_inv(item);
                    }
                }
                DevReq::SelectInv(item) => self.select_inv(item),
                DevReq::TimersPause => self.timers_pause(),
            }
        }
    }

    /// H734 (the original's SelectInv, FUN_0050a390): the held object dropped if there is one, the panel's state-1
    /// command, then a type-2 click command on the inventory object is posted — as a user click (H152)
    fn select_inv(&mut self, item: u32) {
        self.drop_held();
        self.panel_set(true);
        if self.inv_selected != Some(item) && self.active.contains(&item) {
            self.post(Cmd::InvClick(item));
        }
    }

    /// The drop of a held object (the original's FUN_00429f00): the selected item's bar object gets state 0 while the
    /// panel is hidden (prefs 'H'), the held object is removed — H362: its Deactivation resets RegCurrInvVar's
    /// variable — and the selection is cleared. False if no object was held.
    pub(super) fn drop_held(&mut self) -> bool {
        if self.w.dev.held.is_none() {
            return false;
        }
        if let Some(item) = self.inv_selected {
            if self.w.rules.h361_panel_roll_in_drop && self.w.inventory_hides() && !self.inv_panel {
                self.icon_bars.remove(item);
            }
        }
        self.w.held_drop(); // H362 / H1420
        self.deselect();
        true
    }

    fn deselect(&mut self) {
        let old = self.inv_selected.take();
        let active = self.active.clone();
        self.w.inventory_deselect(old, &|o| active.contains(&o));
    }

    /// H170: the panel's state (the original's FUN_00437050): shown → every active inventory object placed (bar 2,
    /// activation order); hidden → every one but the selected taken out of the bar.
    pub(super) fn panel_set(&mut self, shown: bool) {
        if !self.w.rules.h170_inv_panel || self.inv_panel == shown {
            return;
        }
        self.inv_panel = shown;
        let inv: Vec<u32> = self.active.iter().copied().filter(|&o| self.w.h.has(class::INVENTORY, o)).collect();
        for o in inv {
            if shown {
                self.place_icon(o);
            } else if self.inv_selected != Some(o) {
                self.icon_bars.remove(o);
            }
        }
    }

    /// H170: with prefs 'H' an inventory object's Activation places it only while the panel is shown; an object built by a
    /// type-3 Enable shows the panel for 1000 ms (its 500 ms Activation delay not modelled). True = the
    /// normal placement is skipped.
    pub(super) fn panel_activation(&mut self, owner: u32) -> bool {
        if !self.w.rules.h170_inv_panel || !self.w.inventory_hides() || !self.w.h.has(class::INVENTORY, owner) {
            return false;
        }
        let enabled = self.inv_enabled.remove(&owner);
        if self.inv_panel {
            return false;
        }
        if enabled {
            self.panel_set(true); // places every active inventory object, this one included
            self.panel_hide_at = Some(self.now + 1000);
        }
        true
    }

    /// Clock hook (H170): the 1000 ms panel hide.
    pub(super) fn panel_tick(&mut self) {
        if self.panel_hide_at.is_some_and(|t| t <= self.now) {
            self.panel_hide_at = None;
            self.panel_set(false);
        }
    }

    /// Clock hook (H451): the tool comment's timer expired → its player is posted (outside any dispatch: queue back).
    pub(super) fn tool_comment_tick(&mut self) {
        if let Some((t, a, cv)) = self.tool_comment_at.filter(|c| c.0 <= self.now) {
            let _ = t;
            self.tool_comment_at = None;
            self.w.queue_char_view_player(a, cv);
            self.push_back(Cmd::Player(a));
        }
    }

    /// H450: the held gun as a clickable (type 7, its rect = the cursor's, taken as the whole View band).
    pub(super) fn gun_clickable(&self) -> Option<Clickable> {
        (self.w.rules.h450_gun_view_click && self.w.dev.held == Some(7))
            .then_some(Clickable { owner: GUN_OWNER, cls: 0, rect: [0, 90, 640, 330], kind: 0 })
    }

    /// HYPOTHESIS H2170 (the original's hit test FUN_00519e40, category 3): the held camera (type 2) when its rect
    /// (taken as its viewfinder: centred on the pointer, clipped to the band (20, 90)–(620, 330)) holds the point.
    pub(super) fn camera_clickable(&self) -> Option<Clickable> {
        (self.w.rules.h2170_held_camera_click && self.w.dev.held == Some(2))
            .then_some(Clickable { owner: CAMERA_OWNER, cls: 0, rect: [20, 90, 620, 330], kind: 0 })
    }

    /// H2170: the held camera clicked at the last hit point: reacting CharView / Explorable Mouse Clicks.
    fn camera_click(&mut self) {
        let at = self.last_hit.get().unwrap_or((320, 210));
        let ex: Vec<u32> = self.active.iter().copied().filter(|&o| self.w.h.has(class::EXPLORABLE, o)).collect();
        // C306: the video under the viewfinder — the playing one, else the View's looping background — and its position
        let video = |a: &u32| super::sched::media_inputs_h51(self.w.h, *a).is_some_and(|(v, _)| v);
        let under = self.video_playing().or(self.background.filter(video)).map(|a| (a, self.now.saturating_sub(self.movie_start.get(&a).copied().unwrap_or(self.now))));
        for owner in self.w.camera_shot(at, &ex, under) {
            self.push_back(Cmd::Fire { owner, ev: event::MOUSE_CLICK });
        }
    }

    /// H452: a started video player (H51 video kind) takes clicks in the movie area (600×240 at (20,90): the display rect
    /// of a View movie; larger movies not handled).
    pub(super) fn video_player_clickable(&self) -> Option<Clickable> {
        if !self.w.rules.h452_video_player_hit {
            return None;
        }
        let video = self.started().into_iter().any(|a| super::sched::media_inputs_h51(self.w.h, a).is_some_and(|(v, _)| v));
        // H1901: the band's full width (the nav-hotspots at the screen edges covered too)
        let rect = if self.w.rules.h1901_video_band_full_width { [0, 90, 640, 330] } else { [20, 90, 620, 330] };
        video.then_some(Clickable { owner: VIDEO_OWNER, cls: 0, rect, kind: 0 })
    }

    /// H169 / H170: engine clickables without an HDB owner, appended to the band categories.
    pub(super) fn engine_clickables(&self) -> Vec<Clickable> {
        let mut v = Vec::new();
        if self.w.rules.h170_inv_panel && self.w.inventory_hides() {
            v.push(Clickable { owner: PANEL_OWNER, cls: 0, rect: PANEL_RECT, kind: 0 });
        }
        if self.w.rules.h169_game_menu {
            v.push(Clickable { owner: MENU_OWNER, cls: 0, rect: MENU_RECT, kind: 0 });
        }
        v
    }

    /// The pointer moved to (x, y): hover as H134 plus the panel (H170): a hover point inside the panel rect that hits no
    /// icon shows it, a hover point outside hides it.
    ///
    /// H361 (the panel's hover, the original's FUN_00468330): a point entering the strip from outside (the previous
    /// hover point outside it, or none) with an object held drops it first — the panel then shows and the point's
    /// owner is taken from the shown bar. A point moving inside the strip shows nothing (fo-043 k2, fo-061 k2: no
    /// panel_state while the pointer stayed in the strip after a selection).
    pub fn hover_at(&mut self, x: i32, y: i32) -> Option<Clickable> {
        let entered = inside(PANEL_RECT, x, y) && !self.last_hover.is_some_and(|(px, py)| inside(PANEL_RECT, px, py));
        self.last_hover = Some((x, y));
        let dropped = self.w.rules.h361_panel_roll_in_drop && entered && self.w.inventory_held() && self.drop_held();
        let mut c = self.hit(x, y);
        if self.w.rules.h170_inv_panel && self.w.inventory_hides() {
            // H361: the panel shows only for a Roll Into whose previous point lies outside the strip
            let entry_ok = entered || !self.w.rules.h361_panel_roll_in_drop;
            if c.is_some_and(|c| c.owner == PANEL_OWNER) && entry_ok {
                self.panel_set(true);
                self.panel_hide_at = None;
                if dropped {
                    c = self.hit(x, y);
                }
            } else if !inside(PANEL_RECT, x, y) {
                self.panel_set(false);
            }
        }
        let owner = c
            .map(|c| c.owner)
            .filter(|&o| ![PANEL_OWNER, MENU_OWNER, GUN_OWNER, VIDEO_OWNER, CAMERA_OWNER].contains(&o) && !super::moviehot::is_movie_hot(o));
        self.hover(owner);
        c
    }

    /// H169: click on the top-right engine object = Esc: held object dropped, the menu shown (native, not modelled).
    pub(super) fn engine_click(&mut self, owner: u32) -> bool {
        if self.movie_hot_engine_click(owner) {
            return true; // H1281 / H1282
        }
        match owner {
            PANEL_OWNER => true, // swallowed
            VIDEO_OWNER => true, // H452: swallowed like the panel's click
            CAMERA_OWNER => {
                self.camera_click(); // H2170
                true
            }
            GUN_OWNER => {
                self.push_back(Cmd::GunClick); // H450
                true
            }
            MENU_OWNER => {
                self.open_menu();
                self.log.push("H169: in-game menu opened (kind 0x1a)".into());
                true
            }
            _ => false,
        }
    }

    /// H169 (the original's FUN_004525f0: the kind-0x1a click and the Esc key): the held object dropped, the menu
    /// shown.
    pub fn open_menu(&mut self) {
        self.drop_held();
        self.menu_open = true;
    }

    /// H2340: the game is paused by the menu — the menu shows, or a screen it opened (Save / Load / Options / Help) is
    /// still up over it.
    pub fn game_paused(&self) -> bool {
        self.menu_open || self.menu_screen_up()
    }

    /// H2340: a screen the menu opened is still shown over the live menu.
    pub fn menu_screen_up(&self) -> bool {
        self.w.rules.h2340_menu_screen_pauses
            && self.menu_screen
            && !self.menu_open
            && self.w.layouts().iter().any(|l| MENU_SCREENS.contains(&l.0))
    }

    /// H169: the in-game menu's choice — Save (H140: layout 0x14269), Load (H70: 0x142ca), Options (H404: 0xc24a), Help
    /// (H404: 0xc51c) or back to the game; New / Quit close the menu and are the front-end's (a new session / exit).
    pub fn menu_choose(&mut self, c: MenuChoice) {
        if !std::mem::take(&mut self.menu_open) {
            return;
        }
        // H2340: these screens show over the live menu (the game stays paused under them)
        self.menu_screen = matches!(c, MenuChoice::Save | MenuChoice::Load | MenuChoice::Options | MenuChoice::Help);
        match c {
            MenuChoice::Save => self.w.menu_screen(0x14269, true),
            MenuChoice::Load => self.w.menu_screen(0x142ca, true),
            MenuChoice::Options => self.w.engine_layout(0xc24a, 0),
            MenuChoice::Help => self.w.engine_layout(0xc51c, 0),
            // H1703: Return unfreezes the game — a frozen game (H1520) runs again
            MenuChoice::Resume => {
                self.unfreeze();
                self.apply_prefs(); // C311: the menu's Return applies the Preferences
            }
            MenuChoice::New | MenuChoice::Quit => {}
        }
        self.log.push(format!("H169: menu {c:?}"));
        self.drain();
    }

    /// H172: right click = Double Click (event 1) on the hit owner; an inventory item also hides the panel.
    pub fn right_click(&mut self, x: i32, y: i32) -> Option<Clickable> {
        if !self.w.rules.h172_right_click {
            return None;
        }
        let c = self.hit(x, y)?;
        // H2170: the held camera takes the right button too (its Double Click handler not decoded: nothing)
        if c.owner == PANEL_OWNER || c.owner == MENU_OWNER || c.owner == CAMERA_OWNER || !self.active.contains(&c.owner) {
            return Some(c);
        }
        self.push_back(Cmd::Fire { owner: c.owner, ev: event::DOUBLE_CLICK });
        if self.w.h.has(class::INVENTORY, c.owner) && self.w.inventory_hides() {
            self.panel_set(false);
        }
        Some(c)
    }

    /// H174: a key press goes to the focused InterfaceItem as event 3 (its trigger reads the key, e.g. PhoneKeyPress).
    pub fn key_press(&mut self, code: u32) -> bool {
        if !self.w.rules.h174_key_focus {
            return false;
        }
        let Some(f) = self.w.dev.focus.filter(|f| self.active.contains(f)) else { return false };
        self.w.dev.key = Some(code);
        self.push_back(Cmd::Fire { owner: f, ev: event::KEY_PRESS });
        true
    }

    /// H165 input: a player shows a video now (the PDA is refused).
    pub fn set_video_visible(&mut self, on: bool) {
        self.w.dev.video_visible = on;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a made-up phone book (ids 1, 2 and the Lone Gunmen's number asset) for the dialling rules
    const BOOK: [(u32, u32, u32); 3] = [(1, 11, 12), (2, 21, 22), (LONE_GUNMEN, 31, 32)];
    fn text(a: u32) -> String {
        match a {
            1 => "2060000001",
            2 => "13600000002",
            LONE_GUNMEN => "12020000003",
            _ => "x",
        }
        .into()
    }

    #[test]
    fn dialling_rules() {
        use Dialled::*;
        let d = |n: &str, lg: bool| dial(&BOOK, n, &text, lg);
        // 10 digits outside the area never reach the book; inside it they do
        assert_eq!(d("5120000001", true), LongDistance);
        assert_eq!(d("2060000001", true), Listed(11, 12));
        // 7 local digits reach an area entry
        assert_eq!(d("0000001", true), Listed(11, 12));
        assert_eq!(d("13600000002", true), Listed(21, 22));
        // the Lone Gunmen only when allowed
        assert_eq!(d("12020000003", false), Barred);
        assert_eq!(d("12020000003", true), Listed(31, 32));
        // too long / unlisted
        assert_eq!(d("123456789012", true), TooLong);
        assert_eq!(d("123", true), Unlisted(INCOMPLETE));
        assert_eq!(d("", true), Unlisted(INCOMPLETE));
        assert_eq!(d("3334444", true), Unlisted(0));
        assert_eq!(d("1334444", true), Unlisted(1));
        assert_eq!(d("4334444", true), Unlisted(BAD_NUMBER));
        assert_eq!(d("9998888", true), Unlisted(3));
        assert_eq!(d("#998888", true), Unlisted(3));
    }

    #[test]
    fn right_chars() {
        assert_eq!(right("FIRST\r\nSECOND\r\n2060000004", 10), "2060000004");
        assert_eq!(right("12", 10), "12");
    }
}
