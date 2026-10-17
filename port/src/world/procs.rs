//! Type-6 procedures decoded statically (H175–H198; static decode of XFiles.exe + HDB data, NOT traced —
//! RE_NOTES H175–H198). Handler = the procedure-name table's entry; every effect below is a HYPOTHESIS behind its
//! [`super::Rules`] switch. `World::exec_proc` calls [`World::ab_proc`] for the names its own arms do not handle.
//!
//! Native state the procedures keep (globals of XFiles.exe) lives in [`Procs`] (`World::px`).
use super::devices::asset_text;

/// H570–H578: the FBI computer — a child of this module so it uses the procedures' helpers
#[path = "computer.rs"]
pub mod computer;
use super::{class, event, Msg, Out, PostSrc, RegCause, TransSrc, World};
use std::collections::{HashMap, VecDeque};

/// H189: the four volume bars' variables (dialog, music, effect, game); each bar's ten level pictures are the exe's
/// table (`Hdb::exe.volume_bars`)
const VOLUME_VARS: [u32; 4] = [0x3d0, 0x3d1, 0x3d2, 0x3d3];
/// H189: boolean preferences toggled by their procedure (var := !var, type 2)
const PREF_TOGGLES: [(&str, u32); 13] = [
    ("Pref3DSound", 0x3d4),
    ("PrefRollover", 0x3d6),
    ("PrefClosedCap", 0xa0d),
    ("PrefSubliminal", 0x3d9),
    ("PrefTransition", 0x3da),
    ("PrefTranslucnt", 0x3db),
    ("PrefSmoothText", 0x3dc),
    ("PrefArtIntuit", 0x369),
    ("PrefAutoRewind", 0x3e0),
    ("PrefSkipIntro", 0x3e2),
    ("PrefSkipVideo", 0x3e3),
    ("PrefHighVideo", 0x1072),
    ("PrefLiveEnviro", 0x13b6),
];
/// H189: character preferences cycled through a table (next char; unknown → the second entry; type 0)
const PREF_CYCLES: [(&str, u32, &str); 7] = [
    ("PrefAutoDepth", 0x3d5, "NALH"),
    ("PrefInventory", 0x3d7, "HV"),
    ("PrefFlashBrght", 0x3dd, "MBD"),
    ("PrefFlashBeam", 0x3de, "MWS"),
    ("PrefActionDiff", 0x3df, "SHE"),
    ("PrefSaveOnExit", 0x3e1, "AYN"),
    ("PrefAutoRes", 0xa0c, "AYN"),
];
/// H189: PrefDefaults presets ('P', 'Q') as (variable, value, type)
const PRESET_P: [(u32, i32, u8); 11] = [
    (0x3d5, b'L' as i32, 0),
    (0xa0d, 0, 2),
    (0x3d9, 0, 2),
    (0x3da, 0, 2),
    (0x3db, 0, 2),
    (0x3dc, 0, 2),
    (0x1072, 0, 2),
    (0x13b6, 0, 2),
    (0x3de, b'S' as i32, 0),
    (0x3d1, 0, 1),
    (0x3d4, 0, 2),
];
const PRESET_Q: [(u32, i32, u8); 15] = [
    (0x3d4, 1, 2),
    (0x3d5, b'H' as i32, 0),
    (0xa0c, b'Y' as i32, 0),
    (0x3de, b'W' as i32, 0),
    (0x1072, 1, 2),
    (0x13b6, 1, 2),
    (0x3d6, 1, 2),
    (0x3dc, 1, 2),
    (0x3d9, 1, 2),
    (0x3da, 1, 2),
    (0x3db, 1, 2),
    (0x3d0, 80, 1),
    (0x3d2, 80, 1),
    (0x3d1, 40, 1),
    (0x3d3, 70, 1),
];
/// H191: the login fields' text items (n = 1..4) and the field a Tab moves to
const LOGIN_ITEMS: [u32; 4] = [0xc67b, 0xc68c, 0xc897, 0xc898];
const LOGIN_FIELDS: [u32; 4] = [0xc63b, 0xc63c, 0xc63d, 0xc63e];
/// H191: the cheat-code item (f44 = the code texts); the (trigger, action list) per code is the exe's table
/// 0x6548d0 (`Hdb::exe.cheats`)
const CHEAT_ITEM: u32 = 0x1190d;
/// H191: PswdConnect's known wrong (user, password) pairs → message item
const PSWD_HINTS: [(u32, u32, u32); 4] =
    [(0x167aa, 0x167ac, 0x167b9), (0x167ae, 0x167b7, 0x167c1), (0x167b3, 0x167b7, 0x167c9), (0x167b5, 0x167b7, 0x167d1)];
/// H185: what one of Shanks's investigation flags means: 1 while the game is in
/// `node`, every `done` variable is 1 and the `open` items are not finished yet.
struct ShanksFlag {
    flag: u32,
    /// None: any node
    node: Option<i32>,
    done: &'static [u32],
    open: Open,
}

/// The items a Shanks flag waits for.
enum Open {
    Nothing,
    /// every listed variable is still 0
    All(&'static [u32]),
    /// at least one listed variable is still 0
    Any(&'static [u32]),
}

const fn flag(flag: u32, node: i32, done: &'static [u32], open: Open) -> ShanksFlag {
    ShanksFlag { flag, node: Some(node), done, open }
}

const SHANKS_FLAGS: [ShanksFlag; 16] = [
    flag(0xf49, 2, &[], Open::Any(&[0x834, 0x835])),
    flag(0x838, 2, &[], Open::All(&[0x837])),
    flag(0x13bd, 2, &[0x835], Open::All(&[0x13be])),
    flag(0x836, 2, &[0xf48], Open::All(&[0x11c9])),
    flag(0x83c, 2, &[0x83a], Open::All(&[0x83b])),
    flag(0x83e, 2, &[0xf48, 0xf42, 0xf44, 0xf43], Open::Any(&[0xf45, 0xf46, 0xf47])),
    flag(0x842, 2, &[0x83b], Open::All(&[0x7c6, 0x841])),
    ShanksFlag { flag: 0x840, node: None, done: &[], open: Open::Nothing },
    flag(0x844, 3, &[], Open::All(&[0x843])),
    flag(0x847, 3, &[0x845], Open::All(&[0x846])),
    flag(0x84a, 3, &[0x846], Open::All(&[0x849])),
    flag(0x84d, 3, &[0x849], Open::All(&[0x84b])),
    flag(0x84e, 3, &[0x84b], Open::Any(&[0xf53, 0xf54, 0xf55])),
    flag(0x850, 3, &[0xf53], Open::Any(&[0xf56, 0xf57, 0xf58])),
    flag(0x852, 6, &[0x851], Open::All(&[0x1423])),
    flag(0x855, 6, &[0x853], Open::All(&[0x854])),
];

/// Three-valued "and": false as soon as one term is known false, unknown if some term is unknown, else true.
fn and3(terms: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut unknown = false;
    for t in terms {
        match t {
            Some(false) => return Some(false),
            None => unknown = true,
            Some(true) => {}
        }
    }
    (!unknown).then_some(true)
}

/// Three-valued "or".
fn or3(terms: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    and3(terms.into_iter().map(|t| t.map(|b| !b))).map(|b| !b)
}

impl ShanksFlag {
    /// The flag's value in `node` with the variables `get` (None = unknown).
    fn value(&self, node: i32, get: impl Fn(u32) -> Option<i32>) -> Option<bool> {
        let is = |v: u32, x: i32| get(v).map(|y| y == x);
        let open = match self.open {
            Open::Nothing => Some(true),
            Open::All(vs) => and3(vs.iter().map(|&v| is(v, 0))),
            Open::Any(vs) => or3(vs.iter().map(|&v| is(v, 0))),
        };
        let in_node = Some(self.node.is_none_or(|n| n == node));
        and3([in_node, open].into_iter().chain(self.done.iter().map(|&v| is(v, 1))))
    }
}

/// H185: the warehouse help e-mail ("GShanksHelpWarehouse.txt") and the variables that gate it
const WAREHOUSE_MAIL: u32 = 0x15978;
const WAREHOUSE_ASKED: u32 = 0x13bb;
const WAREHOUSE_MAILED: u32 = 0x13bc;

/// H186 / H866: the item that opens a mail in the office computer's read view; any other item is the home view
const OFFICE_MAIL_VIEW: u32 = 0xd6a6;

/// H866: the controls a mail with attachments switches on (true) / off (false) when it is opened, per read view, in
/// the order of the engine messages
struct MailControls {
    mail: u32,
    office: &'static [(u32, bool)],
    home: &'static [(u32, bool)],
}

const MAIL_CONTROLS: [MailControls; 3] = [
    MailControls {
        mail: 0xf14f,
        office: &[(0x11c09, true), (0x11c06, true), (0x11c0b, true), (0x11c12, true), (0x10f97, false)],
        home: &[(0x11c25, true), (0x11c38, true), (0x11c2f, true), (0x1672b, true), (0x116cc, false)],
    },
    MailControls {
        mail: 0xf145,
        office: &[(0x11c09, true), (0x11c06, true), (0x11c13, true), (0x16723, true), (0x10f97, false)],
        home: &[(0x11c25, true), (0x11c38, true), (0x11c27, true), (0x11c37, true), (0x116cc, false)],
    },
    MailControls {
        mail: 0xfc0f,
        office: &[(0x11c09, true), (0x11c06, true), (0x1639e, true), (0x1637f, true)],
        home: &[(0x11c25, true), (0x11c38, true), (0x11c2f, true), (0x1672b, true)],
    },
];

/// H193 / C215: the isolation chamber as its consoles show it.
#[derive(Clone, Copy)]
enum Chamber {
    Dark,
    BothOpen,
    EastOpen,
    WestOpen,
    Closed,
}

impl Chamber {
    /// The two console picture flags (0-based among the eight) that are 1 in this state.
    fn pictures(self) -> [usize; 2] {
        match self {
            Chamber::Dark => [0, 3],
            Chamber::BothOpen => [2, 4],
            Chamber::EastOpen => [1, 6],
            Chamber::WestOpen => [2, 5],
            Chamber::Closed => [1, 7],
        }
    }
}

/// Native state of the procedures (H175–H198).
#[derive(Clone, Debug)]
pub struct Procs {
    /// H179: hit rects (l, t, r, b) set by RelocateIFace / RelocateIFacID on built InterfaceItems
    pub item_rects: HashMap<u32, [i16; 4]>,
    /// H178: the game state's VCPDANotes list (EnablePDANote)
    pub notes: Vec<u32>,
    /// H186: the game state's e-mail lists (VCEmail inbox, VCEmailRead, pending) and the display list's scroll offset /
    /// entries
    pub inbox: Vec<u32>,
    pub read: Vec<u32>,
    pub pending: Vec<u32>,
    mail_offset: usize,
    mail_shown: Vec<u32>,
    /// H182: three native flags (GetPlayFBIShot / GetPlayBones / GetPlayCancer) — 1 until first read
    play_once: [bool; 3],
    /// H192: engine-built type-4 Set View messages pending (action id 0)
    pub engine_views: VecDeque<u32>,
    /// H182 / H191: the game ended (EndGame) / the main menu object was posted (ReturnMainMenu, ReturnFromHelp)
    pub game_over: bool,
    /// H895: a game-end object is pending for the main loop (FUN_0048cd70) — its blk_reset is not a game start (no
    /// VCGameState / VCTitle registration)
    pub game_end_object: bool,
    /// H183: a CharTeleport travel keeps the media players (SwapDisc's removal of every player is not part of it)
    pub(super) keep_players: bool,
    /// H189: volume bar levels 0..9 (dialog, music, effect, game)
    vol: [usize; 4],
    /// H191: login buffers (user, password, cheat), field state, PswdConnect counters (failures, first success) and
    /// PwbkOK's count
    user: String,
    pass: String,
    cheat: String,
    pswd_fail: u32,
    pswd_done: bool,
    pwbk_count: u32,
    /// H197: the journal and the Bible, each open at a spread
    journal: Book,
    bible: Book,
    /// H195: photos taken (TakePhoto, native count; the photos themselves are not modelled)
    photos: u32,
    /// H193: RegNode2/4/5Vars, RegLink2/3Vars bindings
    binds: HashMap<&'static str, Vec<u32>>,
    /// H730: the Smolnikoff shootout's native globals
    pub smol: super::shootout::Smol,
}

impl Default for Procs {
    fn default() -> Procs {
        Procs {
            item_rects: HashMap::new(),
            notes: Vec::new(),
            inbox: Vec::new(),
            read: Vec::new(),
            pending: Vec::new(),
            mail_offset: 0,
            mail_shown: Vec::new(),
            play_once: [true; 3],
            engine_views: VecDeque::new(),
            game_over: false,
            game_end_object: false,
            keep_players: false,
            vol: [0; 4],
            user: String::new(),
            pass: String::new(),
            cheat: String::new(),
            pswd_fail: 0,
            pswd_done: false,
            pwbk_count: 0,
            journal: Book::default(),
            bible: Book::default(),
            photos: 0,
            binds: HashMap::new(),
            smol: Default::default(),
        }
    }
}

/// An AssetRef's kind (4 = text; FUN_00447340). f40 & 7: 4 → 4, 5 → 2, 0 / 1 / 3 → by the file (a file-type table;
/// the file-type index is not decoded — HYPOTHESIS: a `.txt` file is kind 4).
pub fn asset_is_text(w: &World, asset: u32) -> bool {
    let Some(r) = w.h.read(class::ASSET_REF, asset) else { return false };
    match r.u32("f40") & 7 {
        4 => true,
        // H573: a kind-0 string resource (type "6" → file-type index 0x1a → kind 4) is a text
        0 if w.rules.h573_string_text && asset_text(w, asset).is_some() => true,
        0 | 1 | 3 => r
            .blob("asset")
            .and_then(|b| w.h.blob_bytes(b))
            .map_or(false, |b| b.windows(4).any(|x| x.eq_ignore_ascii_case(b".txt"))),
        _ => false,
    }
}

/// An AssetRef played by an audio-only media (authoring path under `Sounds/`, e.g. FO.amv, *.mus, *.dmv) — HYPOTHESIS
/// standing for the original's "empty display rect" test (StopEnvSounds / StopPlayAssets).
pub fn asset_is_sound(w: &World, asset: u32) -> bool {
    w.h.read(class::ASSET_REF, asset)
        .and_then(|r| r.blob("asset"))
        .and_then(|b| w.h.blob_bytes(b))
        .map_or(false, |b| b.get(1..).map_or(false, |s| s.windows(7).take(4).any(|x| x == b"Sounds\x7f")))
}

impl World<'_> {
    /// Window aid (H191 / H260 / H166): the typed text an InterfaceItem shows when its asset is the engine's text asset
    /// (slot asset 0, `item_text_asset`): the login name / password buffers, the phone number. Returns (item, text,
    /// the item's input field has the key focus) for every shown item with typed text.
    pub fn typed_texts(&self) -> Vec<(u32, String, bool)> {
        let mut v = Vec::new();
        for (_, items) in self.layouts() {
            for &(item, asset) in items {
                if asset != Some(0) {
                    continue;
                }
                if let Some(i) = LOGIN_ITEMS.iter().position(|&x| x == item) {
                    let t = if i % 2 == 0 { &self.px.user } else { &self.px.pass };
                    v.push((item, t.clone(), self.dev.focus == Some(LOGIN_FIELDS[i])));
                } else if item == super::devices::PHONE_DISPLAY {
                    v.push((item, self.dev.number.clone(), false));
                } else if self.rules.h403_save_screen_data {
                    // H403: save slots 0x1427c..e (the selected one has the caret), load slots 0x1438a..c
                    if let Some(i) = [0x1427c, 0x1427d, 0x1427e].iter().position(|&x| x == item) {
                        v.push((item, self.save_slot_text(i), i == self.save_slot));
                    } else if let Some(i) = [0x1438a, 0x1438b, 0x1438c].iter().position(|&x| x == item) {
                        v.push((item, self.save_slot_text(i), false));
                    }
                }
            }
        }
        v
    }
}

impl<'a> World<'a> {
    fn ab_unimpl(&mut self, action: u32, what: String) {
        self.out.push(Out::Unimplemented { action, what });
    }

    fn engine_enable(&mut self, item: u32, state: u8) {
        // an engine-built type-3 message on (0x4f, item) with `state`, posted (FUN_0042abc0)
        self.engine_enables.push_back((class::INTERFACE_ITEM, item, state));
        self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
    }

    fn write_if_changed(&mut self, var: u32, v: Option<i32>, typ: u8) {
        if v.is_none() || self.vars.get(var) != v {
            self.vars.set_native(var, v, typ);
        }
    }

    /// The registered owner whose TriggerList holds `trigger`.
    fn owner_of_trigger(&self, trigger: u32) -> Option<u32> {
        let mut owners: Vec<u32> = self
            .registered
            .iter()
            .filter(|(_, &tl)| tl != 0 && self.triggers(tl).iter().any(|t| t.0 == trigger))
            .map(|(&o, _)| o)
            .collect();
        owners.sort_unstable();
        owners.first().copied()
    }

    /// Enabled AssetRefs of InterfaceItem `item`'s f44 list.
    fn item_assets(&self, item: u32) -> Vec<u32> {
        let list = self.h.read(class::INTERFACE_ITEM, item).map_or(0, |r| r.u32("f44"));
        if list == 0 {
            return Vec::new();
        }
        self.h.list_of(class::ASSET_REF_LIST, list).into_iter().filter(|&x| self.is_enabled(class::ASSET_REF, x)).collect()
    }

    /// `item` shows `asset` only if it is a text asset (kind 4) and the item's object exists.
    fn item_show_text(&mut self, item: u32, asset: u32) {
        if asset_is_text(self, asset) {
            self.item_show_asset(item, asset);
        }
    }

    /// The message box (FUN_004be800) — engine type-5 show of 0xead3, then an engine Enable of the item.
    fn message_box(&mut self, item: u32) {
        self.engine_layout(0xead3, 0);
        if item != 0 {
            self.engine_enable(item, 1);
        }
    }

    /// Type-6 procedures of H175–H198; true = handled.
    pub(super) fn ab_proc(&mut self, action: u32, trigger: u32, name: &str, args: &[u32]) -> bool {
        let r = self.rules;
        match (name, args) {
            // ---- H176: item text / text panel -------------------------------------------------------------------
            // a kind-4 asset shown in the item (as H96)
            ("SetIItemText", &[item, asset, ..]) if r.h176_item_text => self.item_show_text(item, asset),
            // a random one (draw 0..=2: three entries) of the exe's suicide-note table
            // (Suicide1..3.txt, `Hdb::exe.suicide_notes`) in the activated item (event-context key 1), font "ZonkersHand" (not
            // modelled). H1863: fo-238 shows 0xf598's text on the note (`traces/fo-238.jsonl.burst/p001.5.png`).
            ("SuicideNote", _) if r.h176_item_text && r.h1863_suicide_note_text => {
                let item = self.owner_of_trigger(trigger);
                let i = self.live_rand_value(0, 2).unwrap_or(2).clamp(0, 2) as usize;
                match (item, self.h.exe.suicide_notes.get(i)) {
                    (Some(item), Some(&note)) => self.item_show_text(item, note),
                    (Some(_), None) => self.ab_unimpl(action, "H1863: SuicideNote without the exe's note table".into()),
                    (None, _) => self.ab_unimpl(action, "H1863: SuicideNote without its item".into()),
                }
            }
            ("SuicideNote", _) if r.h176_item_text => {
                self.ab_unimpl(action, "H176: SuicideNote shows a random note (table 0x659a00) — not modelled".into())
            }
            // ---- H177 -----------------------------------------------------------------------------------------
            // runs the actions of trigger 0x1692f, action list 0x16934
            ("PageTurnSound", _) if r.h177_page_turn_sound => self.do_action_list(action, (0x1692f, 0x16934)),
            // ---- H178: PDA notes --------------------------------------------------------------------------------
            ("EnablePDANote", &[asset, ..]) if r.h178_pda_notes => {
                if self.h.has(class::ASSET_REF, asset) && !self.px.notes.contains(&asset) {
                    super::list_add(&mut self.px.notes, asset, r.h1602_sorted_lists);
                }
            }
            // H624 (pda.rs): the note pad's procedures and its scroller
            ("EnableAllNotes" | "NoteFlipDay" | "NotePadInit" | "MainScroll" | "ScrollTextKill", _)
                if r.h624_note_pad && self.note_proc(name, args) => {}
            ("EnableAllNotes" | "NoteFlipDay" | "NotePadInit", _) if r.h178_pda_notes => {
                self.ab_unimpl(action, format!("H178: {name} (day tables 0x658520 / 0x658620 not modelled)"))
            }
            // ---- H179: hit rects --------------------------------------------------------------------------------
            ("RelocateIFace", &[l, t, rr, b, _key, ..]) if r.h179_relocate_iface => {
                // the item = event-context key 1 of the item's own Activation (HYPOTHESIS: the activated item)
                match self.owner_of_trigger(trigger).filter(|&o| self.h.has(class::INTERFACE_ITEM, o)) {
                    Some(item) => {
                        self.px.item_rects.insert(item, [l as i16, t as i16, rr as i16, b as i16]);
                    }
                    None => self.ab_unimpl(action, "H179: RelocateIFace item (event-context key 1) unknown".into()),
                }
            }
            ("RelocateIFacID", &[l, t, rr, b, item, ..]) if r.h179_relocate_iface => {
                if self.layouts.iter().any(|x| x.1.iter().any(|i| i.0 == item)) {
                    self.px.item_rects.insert(item, [l as i16, t as i16, rr as i16, b as i16]);
                }
            }
            // ---- H180: native display / debug procedures --------------------------------------------------------
            // H733: FadeToBlack (FUN_004ccb50): the event context's player held, renderer fade, then a busy wait of
            // 2000 ms before the player is released — the main loop (queue, media tasks) stands still; fo-082 64.076
            // proc6 → 66.111 the action's removal
            ("FadeToBlack", _) if r.h733_fade_stall => self.out.push(Out::Stall { action, ms: 2000 }),
            // H1700: SaveGameState (FUN_004d9450) — the rewind snapshot ("Try again?" YES, C245)
            ("SaveGameState", _) if r.h1700_rewind_box => self.out.push(Out::SaveGameState { action }),
            // H1040: the View tint (the renderer's tint slot 0; drawn by front::light)
            ("SetLBoxTint", &[r_, g, b, ..]) if r.h1040_lbox_tint => {
                self.dev.lbox_tint = Some([r_ as u8, g as u8, b as u8]);
            }
            ("ClearLBoxTint", _) if r.h1040_lbox_tint => self.dev.lbox_tint = None,
            (
                "MainScroll" | "ScrollTextList" | "ScrollTextKill" | "ScrollTextInit" | "TextOn" | "FadeToBlack"
                | "SetLBoxTint" | "ClearLBoxTint" | "FixJumpCut" | "SaveGameState" | "PrepBombTime" | "SmAttack"
                | "WillmoreSleeps" | "BreakPoint" | "RegDlgAss" | "RegXCursorAss" | "RegGunAss" | "RegFLightAss"
                | "RegCameraAss" | "RefNVisionAss" | "RegBinocAss" | "RegAIIcon" | "ClearIFLExcept" | "AddIFLExcept"
                | "PswdInit" | "PwbkInit" | "PrefInit" | "InitAddrList" | "PhotoForward" | "PhotoScroll" | "PhotoClose"
                | "JournalDestroy" | "INGDestroy" | "MediaDestroy" | "RollOnEvKit",
                _,
            ) if r.h180_native_noops => {}
            ("RegNavBeepAss", &[asset, ..]) if r.h180_native_noops => {
                if asset != 0 {
                    self.ab_unimpl(action, format!("H180: RegNavBeepAss({asset:#x}) builds an asset object"));
                }
            }
            // ---- H181: stopping players -------------------------------------------------------------------------
            ("StopEnvSounds", _) if r.h181_stop_players => self.stop_players(|w, a| asset_is_sound(w, a)),
            ("StopPlayAssets", &[var, ..]) if r.h181_stop_players => {
                if self.vars.get(var) == Some(1) {
                    self.ab_unimpl(action, "H181: StopPlayAssets(1) holds queued players (+8) — not modelled".into());
                }
                self.stop_players(|w, a| !asset_is_sound(w, a));
            }
            // ---- H182: end of game ------------------------------------------------------------------------------
            ("EndGame", &[var, ..]) if r.h182_end_game => {
                let v = self.vars.get(var).unwrap_or(0);
                let rewind = v != 0 && self.vars.get(0x3e0).unwrap_or(0) != 0;
                if rewind {
                    self.write_if_changed(var, Some(0), 2);
                }
                self.px.game_over = true;
                if !rewind {
                    self.px.game_end_object = true; // H895
                }
                self.out.push(Out::GameEnd { action, rewind });
            }
            ("GetPlayFBIShot" | "GetPlayBones" | "GetPlayCancer", &[var, ..]) if r.h182_end_game => {
                let i = ["GetPlayFBIShot", "GetPlayBones", "GetPlayCancer"].iter().position(|&n| n == name).unwrap();
                let v = std::mem::take(&mut self.px.play_once[i]) as i32;
                self.write_if_changed(var, Some(v), 2);
            }
            // ---- H183 -------------------------------------------------------------------------------------------
            ("CharTeleport", &[_, _, node, loc, vp, view, ..]) if r.h183_char_teleport => {
                self.char_teleport(action, trigger, [node, loc, vp, view])
            }
            // ---- H184 -------------------------------------------------------------------------------------------
            // var := (the held object is InterfaceItem `item`) — the object the button-down hit, until the button-up
            // clears it (FUN_004b3850); H1814: `World::held_item`, set by a front's press (the core's
            // instantaneous clicks: never held)
            ("IsIItemHeld", &[item, var, ..]) if r.h184_iitem_held => {
                let held = self.held_item == Some(item);
                self.write_if_changed(var, Some(held as i32), 2)
            }
            // ---- H185 -------------------------------------------------------------------------------------------
            ("SetShanksVar", &[var, ..]) if r.h185_shanks_var => self.set_shanks_var(var),
            // ---- H186: e-mail -----------------------------------------------------------------------------------
            ("EmailInit", &[c, ..]) if r.h186_email => {
                let al = match c as u8 {
                    b'a' => Some((0x15124, 0x15126)),
                    b'b' => Some((0x1512a, 0x1512c)),
                    _ => None,
                };
                if let Some(p) = al {
                    if self.px.inbox.len() != self.px.read.len() {
                        self.do_action_list(action, p);
                    }
                }
            }
            ("GetCurrEmail", &[i, var, ..]) if r.h186_email => {
                let v = self.px.mail_shown.get(self.px.mail_offset + i as usize).map_or(0, |&a| a as i32);
                self.write_if_changed(var, Some(v), 1);
            }
            ("MarkEmailRead", &[i, ..]) if r.h186_email => {
                if let Some(&a) = self.px.mail_shown.get(self.px.mail_offset + i as usize) {
                    if !self.px.read.contains(&a) {
                        self.px.read.push(a);
                    }
                }
            }
            ("EmailDelete", &[var, ..]) if r.h186_email => {
                if let Some(v) = self.vars.get(var) {
                    self.px.inbox.retain(|&a| a != v as u32);
                    self.px.read.retain(|&a| a != v as u32);
                }
            }
            ("DisplayInBox", &[var, ..]) if r.h186_email => {
                self.px.inbox.retain(|&a| a != 0xf87b);
                self.px.mail_shown = self.px.inbox.clone();
                let n = self.px.mail_shown.len();
                let sel = self.vars.get(var).unwrap_or(0) as u32;
                self.px.mail_offset = match self.px.mail_shown.iter().position(|&a| a == sel) {
                    Some(p) if sel != 0 && n > 8 => p.min(n - 8),
                    _ => n.saturating_sub(8),
                };
                self.ab_unimpl(action, "H186: DisplayInBox rows (text assets, read markers; tables at run time) not modelled".into());
            }
            ("ScrollHeaders", &[d, ..]) if r.h186_email => {
                let n = self.px.mail_shown.len();
                if n > 8 {
                    let o = self.px.mail_offset;
                    self.px.mail_offset = match d {
                        0 => o.saturating_sub(1),
                        1 if o < n - 8 => o + 1,
                        10 => o.saturating_sub(7),
                        11 if o < n - 8 => (o + 7).min(n - 8),
                        _ => o,
                    };
                    self.ab_unimpl(action, "H186: ScrollHeaders redisplay not modelled".into());
                }
            }
            ("InitEmailRead", &[item, var, ..]) if r.h186_email => {
                // H866: only three mails have controls to switch; any other mail sends no engine message (fo-106 k23)
                let mail = self.vars.get(var).unwrap_or(0) as u32;
                if let Some(m) = MAIL_CONTROLS.iter().find(|m| m.mail == mail) {
                    let controls = if item == OFFICE_MAIL_VIEW { m.office } else { m.home };
                    for &(it, on) in controls {
                        self.engine_enable(it, on as u8);
                    }
                }
            }
            ("ForwardObject" | "EmailSend" | "CreateAddrList", _) if r.h186_email => {
                self.ab_unimpl(action, format!("H186: {name} (address / forward lists) not modelled"))
            }
            // ---- H187 -------------------------------------------------------------------------------------------
            ("NukeAllIcons", _) if r.h187_nuke_icons => self.nuke_all_icons(action),
            // ---- H188 -------------------------------------------------------------------------------------------
            ("EnableKitIcon", &[icon, ..]) if r.h188_kit_icon => {
                // the icon's C15 flag := 1, an EvidenceIcon object built (TL f40) and posted (FUN_004effd0)
                if self.h.has(class::EVIDENCE_ICON, icon) {
                    self.enabled.insert((class::EVIDENCE_ICON, icon), 1);
                    // H1582: the object is a kit icon of the app's icon list — a save lists it in l118
                    // (fo-110-end.x l118 = [0xca9b] after EnableKitIcon 0xcaa1, `traces/fo-110.jsonl` 127.449)
                    if r.h1582_kit_icon_listed && !self.icon_objs.iter().any(|o| o.id == icon) {
                        self.icon_objs.push(super::icons::IconObj { cls: class::EVIDENCE_ICON, id: icon, kit: true });
                    }
                    let tl = self.trigger_list_of(icon);
                    self.out.push(Out::Reg { owner: icon, tl, cause: RegCause::Proc { action } });
                    self.out.push(Out::Fire { owner: icon, event: event::ACTIVATION });
                }
            }
            ("EvKitIconClick", _) if r.h188_kit_icon => {
                self.ab_unimpl(action, "H188: EvKitIconClick (roll-over object, panel) not modelled".into())
            }
            // ---- H189: preferences ------------------------------------------------------------------------------
            ("PrefDialogVol" | "PrefMusicVol" | "PrefEffectVol" | "PrefVolume", &[_, n, ..]) if r.h189_prefs => {
                let bar = ["PrefDialogVol", "PrefMusicVol", "PrefEffectVol", "PrefVolume"].iter().position(|&x| x == name);
                self.volume_step(bar.unwrap(), n);
            }
            ("AudioInit", _) if r.h189_prefs => {
                for bar in [0, 2, 1, 3] {
                    let l = (self.vars.get(VOLUME_VARS[bar]).unwrap_or(0) / 11).clamp(0, 9) as usize;
                    self.px.vol[bar] = l;
                    if let Some(item) = self.volume_item(bar, l) {
                        self.engine_enable(item, 1);
                    }
                }
            }
            ("PrefDefaults", &[c, ..]) if r.h189_prefs => {
                let preset: &[(u32, i32, u8)] = if c as u8 == b'Q' { &PRESET_Q } else { &PRESET_P };
                for &(v, x, t) in preset {
                    self.write_if_changed(v, Some(x), t);
                }
            }
            (n, &[var, ..]) if r.h189_prefs && PREF_TOGGLES.iter().any(|t| t.0 == n) => {
                let x = self.vars.get(var).map(|v| (v == 0) as i32);
                self.vars.set_native(var, x, 2);
            }
            (n, &[var, ..]) if r.h189_prefs && PREF_CYCLES.iter().any(|t| t.0 == n) => {
                let table = PREF_CYCLES.iter().find(|t| t.0 == n).unwrap().2.as_bytes();
                let cur = self.vars.get(var);
                let next = match cur.and_then(|c| table.iter().position(|&x| x as i32 == c)) {
                    Some(i) => table[(i + 1) % table.len()],
                    None => table[1],
                };
                self.vars.set_native(var, Some(next as i32), 0);
            }
            // ---- H190: search terminals -------------------------------------------------------------------------
            ("INGClear", _) if r.h190_terminals => {
                self.ing_clear_tables();
                self.item_text_asset(0xe3e8);
                self.item_text_asset(0xc636);
                self.engine_enable(0x8de0, 0);
            }
            ("INGInit", _) if r.h190_terminals => {
                self.engine_enable(0x8de0, 0);
                self.item_text_asset(0xc636);
                self.dev.focus = Some(0xc636);
            }
            ("MediaInit", _) if r.h190_terminals => {
                self.engine_enable(0xd6fc, 0);
                self.dev.focus = Some(0xc63a);
            }
            ("SBaseInit", _) if r.h190_terminals => {
                self.ab_unimpl(action, "H190: SBaseInit dot items (table 0x657660) not modelled".into());
                self.dev.focus = Some(0xda11);
            }
            ("INGKeyPressed" | "MSKeyPressed", _) if r.h190_terminals => {
                let (enter, text) = if name == "INGKeyPressed" { ((0x9dcb, 0x9f86), 0xc636) } else { ((0xe90c, 0xe90d), 0xc63a) };
                match self.dev.key {
                    Some(0xd) => self.do_action_list(action, enter),
                    Some(_) => {
                        self.item_text_asset(text);
                        self.ab_unimpl(action, format!("H190: {name} typed text not modelled"));
                    }
                    None => self.ab_unimpl(action, format!("H190: {name} without a key")),
                }
            }
            ("INGSearch" | "MediaSearch" | "SBaseKeyPress" | "INGDisplayCook", _) if r.h190_terminals => {
                self.ab_unimpl(action, format!("H190: {name} depends on the typed text / search tables (not modelled)"))
            }
            // ---- H191: login screens, menu ----------------------------------------------------------------------
            ("LoginClick", &[n, ..]) if r.h191_login && (1..=4).contains(&n) => {
                self.dev.focus = Some(LOGIN_FIELDS[n as usize - 1]);
            }
            ("LoginKeyPress", &[n, ..]) if r.h191_login => self.login_key(action, n),
            ("PswdConnect", &[var, ..]) if r.h191_login => self.pswd_connect(action, var),
            ("PwbkOK", &[var, ..]) if r.h191_login => self.pwbk_ok(action, var),
            ("LoginInit", &[0x50, ..]) if r.h191_login => self.login_init_p(),
            ("ReturnMainMenu" | "ReturnFromHelp", _) if r.h191_login => self.out.push(Out::MainMenu { action }),
            // ---- H730 (`shootout.rs`) before H192 ---------------------------------------------------------------
            ("DoSmolShootout" | "SmolGoonShot1" | "SmIsWillShot1" | "SmGoonShoots", _)
                if r.h730_smol_shootout && self.smol_proc(action, name, args) => {}
            // ---- H192: shootouts, engine Set View ---------------------------------------------------------------
            ("SmolGoonShot1", _) if r.h192_shootout => {
                for v in [0xe9b, 0xe9c, 0xe9d] {
                    if self.vars.get(v) != Some(1) {
                        self.vars.set_native(v, None, 2); // the random target's flag := 1
                    }
                }
            }
            ("SmGoonShoots", &[a, b, n, ..]) if r.h192_shootout => match self.vars.get(b) {
                Some(2 | 3) => {
                    self.vars.set_native(a, None, 2);
                }
                Some(_) => {
                    self.engine_timer_stops.push_back(n as u8);
                    self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
                    self.write_if_changed(a, Some(0), 2);
                }
                None => self.ab_unimpl(action, "H192: SmGoonShoots count unknown".into()),
            },
            ("DoSmolShootout" | "SmIsWillShot1" | "AlienBlast", _) if r.h192_shootout => {
                self.ab_unimpl(action, format!("H192: {name} (random target / queue freeze) not modelled"))
            }
            ("DrawGunRauch", &[a, b, ..]) if r.h192_shootout => match self.vars.get(a) {
                Some(1) => {
                    self.write_if_changed(b, Some(b'b' as i32), 0);
                    self.engine_set_view(0x9b42);
                }
                Some(_) => {}
                None => self.ab_unimpl(action, "H192: DrawGunRauch value unknown".into()),
            },
            // ---- H193: node / link done, iso chamber, ranges ----------------------------------------------------
            ("SetIsNode2Done" | "SetIsNode4Done" | "SetIsNode5Done" | "SetIsLink2Done" | "SetIsLink3Done", _)
                if r.h193_node_link_done =>
            {
                self.node_link_done(name)
            }
            ("IsoChamberStat", a) if r.h193_node_link_done && a.len() >= 11 => self.iso_chamber(a),
            ("RandGenRange", &[var, lo, hi, ..]) if r.h193_node_link_done => {
                let v = if lo == hi { Some(lo as i32) } else { self.live_rand_value(lo as i32, hi as i32) };
                self.vars.set_native(var, v, 1);
            }
            // ---- H194 -------------------------------------------------------------------------------------------
            ("PlayAssetAt", &[asset, ..]) if r.h194_play_asset_at => {
                if self.h.has(class::ASSET_REF, asset) && !self.looping_players.contains(&asset) {
                    let tl = self.trigger_list_of(asset);
                    self.played.insert(asset);
                    self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::PlayAsset { action } });
                    self.out.push(Out::Fire { owner: asset, event: event::ACTIVATION });
                }
            }
            // ---- H195: photos -----------------------------------------------------------------------------------
            ("PhotoInit", _) if r.h195_photos => {
                // u = photos not yet downloaded (the core keeps only the count taken; none downloaded)
                let u = self.px.photos;
                for (it, s) in [(0x114c5, (u == 0) as u8), (0xc8c5, (u != 0) as u8), (0x114c7, 1), (0xc8c6, 0)] {
                    self.engine_enable(it, s);
                }
            }
            ("TakePhoto", _) if r.h195_photos => {
                if self.px.photos < 24 {
                    self.px.photos += 1;
                }
            }
            ("PhotoEnlarge" | "PhotoDownload" | "PhotoDelete", _) if r.h195_photos => {
                self.ab_unimpl(action, format!("H195: {name} (photo album) not modelled"))
            }
            // ---- H196: PDA address book -------------------------------------------------------------------------
            ("AddrSelPDA" | "ScrollAddrs" | "DisplayAddrs" | "AddrSelected", _) if r.h196_addr_book => {
                self.ab_unimpl(action, format!("H196: {name} (address rows) not modelled"))
            }
            // ---- H197: journal, bible ---------------------------------------------------------------------------
            ("JournalInit", &[var, ..]) if r.h197_journal_bible => {
                let pages = self.item_assets(JOURNAL_POOL);
                let at_end = self.vars.get(var).unwrap_or(0) == 0;
                self.px.journal = Book::journal(pages, at_end);
                self.journal_show();
            }
            ("JournalFlip", &[dir, first_var, last_var, ..]) if r.h197_journal_bible => {
                if self.px.journal.pages.len() > 2 {
                    self.px.journal.turn(dir == 0);
                    let (first, last) = (self.px.journal.is_first(), self.px.journal.is_last());
                    self.write_if_changed(first_var, Some(first as i32), 0);
                    self.write_if_changed(last_var, Some(last as i32), 0);
                    self.journal_show();
                }
            }
            ("BibleInit", _) if r.h197_journal_bible => {
                self.px.bible = Book::bible(self.item_assets(BIBLE_POOL));
                self.engine_enable(BIBLE_PREV, 0);
                self.engine_enable(BIBLE_NEXT, 1);
                self.bible_show();
            }
            ("BibleFlip", &[dir, ..]) if r.h197_journal_bible => {
                self.engine_enable(BIBLE_PREV, 1);
                self.engine_enable(BIBLE_NEXT, 1);
                if self.px.bible.turn(dir != 1) {
                    if self.px.bible.is_first() {
                        self.engine_enable(BIBLE_PREV, 0);
                    }
                    if self.px.bible.is_last() {
                        self.engine_enable(BIBLE_NEXT, 0);
                    }
                    self.bible_show();
                }
            }
            // ---- H198 -------------------------------------------------------------------------------------------
            // H734: the synthetic click (Runner, `DevReq::SelectInv`)
            ("SelectInv", &[item, ..]) if r.h734_select_inv => {
                if item != 0 && self.dev.inv_active.contains(&item) {
                    self.dev.requests.push(super::devices::DevReq::SelectInv(item));
                }
            }
            ("SelectInv", &[item, ..]) if r.h198_select_inv => {
                if item != 0 && self.dev.inv_active.contains(&item) {
                    self.ab_unimpl(action, format!("H198: SelectInv({item:#x}) synthetic click not modelled"));
                }
            }
            _ => return false,
        }
        true
    }

    /// H178 / H186: a save's VCGameState names its VCPDANotes (f3c), VCEmail (f40), VCEmailRead (f44) and pending (f48)
    /// objects; their data blobs are little-endian AssetRef ids (the new game's gam has none: empty).
    pub(super) fn ab_seed(&mut self) {
        let Some(&(id, _)) = self.gam.class(class::GAME_STATE).and_then(|c| c.objects.first()) else { return };
        let Some(r) = self.gam.read(class::GAME_STATE, id) else { return };
        let list = |cls: u32, obj: u32| -> Vec<u32> {
            self.gam
                .read(cls, obj)
                .and_then(|x| x.blob("data"))
                .and_then(|b| if b.len == 0 { Some(&[][..]) } else { self.gam.blob_bytes(b) })
                .map(|b| b.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect())
                .unwrap_or_default()
        };
        if self.rules.h262_dbox_rows {
            self.conv_history = list(0x56, r.u32("f38")); // H262: VCConversationHistory (fo-054-end.x: 0x5697, 0x57a0, …)
        }
        if self.rules.h178_pda_notes {
            self.px.notes = list(0x58, r.u32("f3c"));
        }
        if self.rules.h186_email {
            self.px.inbox = list(0x5a, r.u32("f40"));
            self.px.read = list(0x5b, r.u32("f44"));
            self.px.pending = list(0x5c, r.u32("f48"));
        }
    }

    /// H186: SendEmail(asset) adds to the pending list, EmailFlushMess moves pending into the inbox (native state the
    /// H92 no-op arm keeps; called before that arm).
    pub(super) fn ab_native_state(&mut self, name: &str, args: &[u32]) {
        if self.rules.h193_node_link_done && self.rules.h80_reg_vars {
            // H193: the bindings the SetIs*Done procedures read (the H80 arm keeps the no-op)
            match (name, args) {
                ("RegNode2Vars", &[a, b, c, ..]) => self.bind_vars("n2", &[a, b, c]),
                ("RegNode4Vars", &[a, b, ..]) => self.bind_vars("n4", &[a, b]),
                ("RegNode5Vars", &[a, b, c, ..]) => self.bind_vars("n5", &[a, b, c]),
                ("RegLink2Vars", &[a, b, ..]) => self.bind_vars("l2", &[a, b]),
                ("RegLink3Vars", &[a, b, ..]) => self.bind_vars("l3", &[a, b]),
                _ => {}
            }
        }
        if self.rules.h730_smol_shootout && name == "InitSmShootout" {
            self.smol_init(args); // H730: InitSmShootout's native globals (the H91 arm writes the variables)
        }
        if self.rules.h191_login && name == "LoginInit" && args.first() == Some(&0x43) {
            self.px.user.clear(); // H191: LoginInit 'C' starts with empty fields (H102 builds their empty texts)
            self.px.pass.clear();
            self.login_prefill(); // H260
        }
        if !self.rules.h186_email {
            return;
        }
        match (name, args) {
            ("SendEmail", &[asset, ..]) if self.h.has(class::ASSET_REF, asset) => {
                if !self.px.pending.contains(&asset) && !self.px.inbox.contains(&asset) {
                    self.px.pending.push(asset);
                }
            }
            ("EmailFlushMess", _) => {
                let p = std::mem::take(&mut self.px.pending);
                self.px.inbox.extend(p);
            }
            _ => {}
        }
    }

    /// H181: the media players the core knows (looping ones, played ones not deactivated) whose AssetRef passes `pick`
    /// are stopped (removed: ends like a media end, H100 mode 3).
    fn stop_players(&mut self, pick: impl Fn(&World, u32) -> bool) {
        let mut v: Vec<u32> = self.looping_players.iter().chain(self.played.iter()).copied().collect();
        v.sort_unstable();
        v.dedup();
        for a in v {
            if pick(self, a) {
                let known = self.looping_players.remove(&a);
                if known {
                    self.played.remove(&a);
                }
                self.out.push(Out::PlayerStop { asset: a, known });
            }
        }
    }

    /// H183: CharTeleport (FUN_004c56a0).
    fn char_teleport(&mut self, action: u32, trigger: u32, [node, loc, vp, view]: [u32; 4]) {
        let h = self.h;
        if !(h.has(class::VIEW, view) && h.has(class::VIEWPOINT, vp) && h.has(class::LOCATION, loc) && h.has(class::NODE, node)) {
            return;
        }
        // the CharView under the click (event-context keys 0 / 1 = the point; the trigger's owner here) →
        // its character key f28 → the enabled member of the target View's f30 with that key
        let Some(cv) = self.owner_of_trigger(trigger).filter(|&o| h.has(class::CHAR_VIEW, o)) else {
            self.ab_unimpl(action, "H183: CharTeleport without a clicked CharView".into());
            return;
        };
        let key = h.read(class::CHAR_VIEW, cv).map_or(0, |r| r.u32("f28"));
        let list = h.read(class::VIEW, view).map_or(0, |r| r.u32("f30"));
        let found = h.list_of(class::CHAR_VIEW_LIST, list).into_iter().find(|&c| {
            h.read(class::CHAR_VIEW, c).map_or(false, |r| r.u32("f28") == key) && self.is_enabled(class::CHAR_VIEW, c)
        });
        // only inside the current Node; each of View / ViewPoint / Location replaced only where it differs
        let Some(target) = found.filter(|_| self.node == Some(node) && self.view != Some(view)) else {
            return;
        };
        // H601: the transition records the target CharView
        self.teleport_cv = Some((view, target));
        if self.location != Some(loc) {
            let tl = self.trigger_list_of(loc);
            self.out.push(Out::Reg { owner: loc, tl, cause: RegCause::TravelLocation { action } });
            self.travel = Some([self.viewpoint, self.location, None, None, Some(loc)]);
            self.px.keep_players = true;
            self.location = Some(loc);
        }
        self.transition(view, 0, TransSrc::CharTeleport { action }, Some(vp), &[]);
    }

    /// H185 SetShanksVar(flag): the flag is recomputed (type 2, also when unchanged); then, in node 2, the warehouse
    /// help mail arrives once nothing of that lead is done.
    fn set_shanks_var(&mut self, var: u32) {
        let node = self.node_vars.0.and_then(|v| self.vars.get(v)).unwrap_or(0);
        if let Some(f) = SHANKS_FLAGS.iter().find(|f| f.flag == var) {
            let v = f.value(node, |x| self.vars.get(x));
            if self.vars.has(f.flag) {
                self.vars.set_native(f.flag, v.map(i32::from), 2);
            }
        }
        let gate = [0xf49, WAREHOUSE_ASKED, WAREHOUSE_MAILED];
        if node == 2
            && self.vars.has(WAREHOUSE_ASKED)
            && self.vars.has(WAREHOUSE_MAILED)
            && gate.iter().all(|&v| self.vars.get(v) == Some(0))
        {
            if !self.px.inbox.contains(&WAREHOUSE_MAIL) {
                self.px.inbox.push(WAREHOUSE_MAIL);
            }
            self.vars.set_native(WAREHOUSE_MAILED, Some(1), 2);
        }
    }

    /// H187 (FUN_0049f3c0, on kinds 0xe, 0xc, 0xf, 0x18, 0x10, 0x12, 0xe, 0x22, 0x1e): each object of the
    /// kind lists gets a state-0 command if active (Deactivation) and is removed. Modelled: held object, EmotionIcons,
    /// inventory objects, layouts (as a hide); ActionIcons / IdeaIcons / EvidenceIcons are not tracked by the world.
    fn nuke_all_icons(&mut self, action: u32) {
        self.dev.held = None;
        for icon in self.icons.clone() {
            self.icons_off.push(icon);
            self.icon_off(icon);
        }
        for o in std::mem::take(&mut self.dev.inv_active) {
            self.out.push(Out::Fire { owner: o, event: event::DEACTIVATION });
        }
        let shown: Vec<u32> = self.layouts.iter().map(|l| l.0).collect();
        for l in shown {
            self.layout_cmd(action, l, 1);
        }
        let r = self.rules;
        if r.h203_action_icon_type3 && r.h205_evidence_icon_type3 && r.h206_idea_icon_type3 {
            self.nuke_icon_objects(); // H203–H206
        } else {
            self.ab_unimpl(action, "H187: NukeAllIcons — Action / Idea / Evidence icon objects not tracked".into());
        }
    }

    /// H189 (FUN_004c7840): n = 0 down, 0xb up, else level n − 1; the old level's item disabled, the new one's enabled
    /// (only on a change); the variable := level · 11 (≤ 100), type 1, always.
    fn volume_step(&mut self, bar: usize, n: u32) {
        let var = VOLUME_VARS[bar];
        let old = self.px.vol[bar];
        let new = match n {
            0 => old.saturating_sub(1),
            0xb => (old + 1).min(9),
            n => (n as usize).saturating_sub(1).min(9),
        };
        if new != old {
            if let (Some(off), Some(on)) = (self.volume_item(bar, old), self.volume_item(bar, new)) {
                self.engine_enable(off, 0);
                self.engine_enable(on, 1);
            }
            self.px.vol[bar] = new;
        }
        self.vars.set_native(var, Some((new as i32 * 11).min(100)), 1);
    }

    /// The picture item of volume `bar` at `level` (exe table; None without the RU 1.00 exe).
    fn volume_item(&self, bar: usize, level: usize) -> Option<u32> {
        self.h.exe.volume_bars.get(bar).map(|items| items[level])
    }

    /// H190: INGClear empties the record screen: every record's photo and frame items disabled (engine messages),
    /// the eight value items get the empty text, the eight label items disabled. Tables: `Hdb::exe` (ING records,
    /// value / label items).
    fn ing_clear_tables(&mut self) {
        let h = self.h;
        for r in &h.exe.ing_records {
            self.engine_enable(r[2], 0);
            self.engine_enable(r[3], 0);
        }
        for &it in &h.exe.ing_value_items {
            self.item_text_asset(it);
        }
        for &it in &h.exe.ing_label_items {
            self.engine_enable(it, 0);
        }
    }

    /// H191 (FUN_004248b0): LoginKeyPress(n) with the key of the current key event (H174).
    fn login_key(&mut self, action: u32, n: u32) {
        let Some(key) = self.dev.key else {
            self.ab_unimpl(action, "H191: LoginKeyPress without a key".into());
            return;
        };
        if n == 5 {
            if key == 0xd {
                let texts: Vec<String> =
                    self.item_assets(CHEAT_ITEM).into_iter().map(|a| asset_text(self, a).unwrap_or_default()).collect();
                let typed = std::mem::take(&mut self.px.cheat);
                if let Some(i) = texts.iter().position(|t| t.eq_ignore_ascii_case(&typed)) {
                    if let Some(&p) = self.h.exe.cheats.get(i) {
                        self.do_action_list(action, p);
                    }
                }
            } else if let Some(c) = login_char(key) {
                if self.px.cheat.len() < 0x32 {
                    self.px.cheat.push(c);
                }
            }
            return;
        }
        if !(1..=4).contains(&n) {
            return;
        }
        let i = n as usize - 1;
        match key {
            0xd => self.do_action_list(action, if n <= 2 { (0x9a33, 0x9a35) } else { (0x9aa0, 0x9aa2) }),
            9 => self.dev.focus = Some(LOGIN_FIELDS[i ^ 1]),
            _ => {
                let max = [0x17, 0x17, 0x15, 0x11][i];
                let buf = if n % 2 == 1 { &mut self.px.user } else { &mut self.px.pass };
                match key {
                    8 | 0x2e => {
                        buf.pop();
                    }
                    k => {
                        if let Some(c) = login_char(k) {
                            if buf.len() < max {
                                buf.push(c);
                            }
                        }
                    }
                }
                self.item_text_asset(LOGIN_ITEMS[i]);
            }
        }
    }

    /// H191 (FUN_0045a3a0).
    fn pswd_connect(&mut self, action: u32, var: u32) {
        let t = |w: &World, a: u32| asset_text(w, a).unwrap_or_default();
        let (user, pass) = (self.px.user.clone(), self.px.pass.clone());
        let ok_user = user.eq_ignore_ascii_case(&t(self, 0xe231));
        if ok_user && pass.eq_ignore_ascii_case(&t(self, 0x10d33)) {
            self.vars.set_native(var, Some(1), 2);
            self.px.pswd_fail = 0;
            if !self.px.pswd_done {
                self.px.pswd_done = true;
                if self.vars.has(0x1352) {
                    self.vars.set_native(0x1352, Some(1), 2);
                }
            }
            if self.rules.h260_login_text {
                return; // H260 (c): the success branch ends here (no LoginInit 'C')
            }
        } else {
            self.vars.set_native(var, Some(0), 2);
            if ok_user {
                self.px.pswd_fail += 1;
            }
            let hint = PSWD_HINTS.iter().find(|h| {
                user.eq_ignore_ascii_case(&t(self, h.0)) && pass.eq_ignore_ascii_case(&t(self, h.1))
            });
            let msg = match hint {
                Some(h) => h.2,
                None if self.px.pswd_fail < 3 => 0x1079f,
                None => 0x107ac,
            };
            self.message_box(msg);
        }
        // then LoginInit 'C' (H102)
        self.px.user.clear();
        self.px.pass.clear();
        if self.rules.h260_login_text {
            self.login_prefill();
        } else {
            self.dev.focus = Some(0xc63b);
        }
        if self.vars.has(0x1352) {
            for item in [0xc67b, 0xc68c] {
                self.item_text_asset(item);
            }
        }
        let _ = action;
    }

    /// H260 (b) (LoginInit 'C', FUN_00468f00): only while VCVariable 0x1352 exists; the first-success flag := its value;
    /// if it is ≠ 0 (or a second preference flag, H570) the name buffer := text 0xe231 upper-cased and the focus goes to
    /// the password field 0xc63c, else to the name field 0xc63b.
    fn login_prefill(&mut self) {
        if !self.rules.h260_login_text || !self.vars.has(0x1352) {
            return;
        }
        let logged = self.vars.get(0x1352).is_some_and(|v| v != 0);
        self.px.pswd_done = logged;
        // H570: the second condition, the preference RegPreferences binds as its 2nd argument = Pref_bArtificialIntuition
        // 0x369 (H81 / H189: the prefs mirror the variables)
        let art_intuit = self.rules.h570_login_art_intuit && self.vars.get(0x369).is_some_and(|v| v != 0);
        if logged || art_intuit {
            self.px.user = asset_text(self, 0xe231).unwrap_or_default().to_uppercase();
            self.dev.focus = Some(0xc63c);
        } else {
            self.dev.focus = Some(0xc63b);
        }
    }

    /// H191: PwbkOK (FUN_004b2db0), then LoginInit 'P'.
    fn pwbk_ok(&mut self, _action: u32, var: u32) {
        let t = |w: &World, a: u32| asset_text(w, a).unwrap_or_default();
        if self.px.user.eq_ignore_ascii_case(&t(self, 0x10d31)) && self.px.pass.eq_ignore_ascii_case(&t(self, 0x10d2e)) {
            self.vars.set_native(var, Some(1), 2);
        }
        self.px.pwbk_count += 1;
        self.message_box(if self.px.pwbk_count < 4 { 0x1079f } else { 0x15e1b });
        self.login_init_p();
    }

    /// H191: LoginInit 'P' (FUN_004b2970): user := the password book's user text, password cleared, text assets on
    /// 0xc897 / 0xc898, focus 0xc63d (caret 0xfbe9 moved: rect only); after more than 3 tries the two fields disabled.
    fn login_init_p(&mut self) {
        self.px.user = asset_text(self, 0x10d31).unwrap_or_default();
        self.px.pass.clear();
        self.item_text_asset(0xc897);
        self.item_text_asset(0xc898);
        self.dev.focus = Some(0xc63d);
        if self.px.pwbk_count > 3 {
            self.engine_enable(0xc63d, 0);
            self.engine_enable(0xc63e, 0);
        }
    }

    /// H199 (FUN_00436c20): type-3 Disable of the VCInventory `target` — held → dropped and the selection cleared;
    /// active → state 0 (Deactivation); removed. Prefs 'V': every other active inventory object gets state 0, then
    /// state 1 (and is placed anew) — Deactivation then Activation.
    pub(super) fn inventory_disable(&mut self, target: u32) {
        if self.dev.held.is_some() {
            self.dev.requests.push(super::devices::DevReq::DeselectIf(target));
        }
        if let Some(i) = self.dev.inv_active.iter().position(|&o| o == target) {
            self.dev.inv_active.remove(i);
            self.out.push(Out::Fire { owner: target, event: event::DEACTIVATION });
            // H1181: the removal deactivates the object's assets unless another holder keeps them
            // (fo-162 265.285: 0xa186's Deact, then 0xa182 / 0xa184 / 0xd545; 216.266 0x130b2's: none)
            if self.rules.h1181_shared_icon_asset {
                let assets = self.inv_assets.iter().find(|x| x.0 == target).map(|x| x.1.clone()).unwrap_or_default();
                for a in assets {
                    if !self.icon_asset_held(a, target) {
                        self.out.push(Out::Fire { owner: a, event: event::DEACTIVATION });
                    }
                }
            }
        }
        if !self.inventory_hides() {
            let others = self.dev.inv_active.clone();
            for &o in &others {
                self.out.push(Out::Fire { owner: o, event: event::DEACTIVATION });
            }
            for &o in &others {
                self.out.push(Out::Fire { owner: o, event: event::ACTIVATION });
            }
        }
    }

    /// H192: an engine-built type-4 Set View message (action id 0).
    pub(super) fn engine_set_view(&mut self, view: u32) {
        self.px.engine_views.push_back(view);
        self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
    }

    /// H192: the engine type-4 message runs (as type 4, C18): a transition to the View if it is not the current one.
    pub(super) fn run_engine_view(&mut self, view: u32) {
        self.out.push(Out::Exec { action: 0, typ: 4 });
        if self.view != Some(view) {
            let vp = self.viewpoint_of(view);
            self.transition(view, 0, TransSrc::SetView { action: 0 }, vp, &[]);
        }
    }

    /// H193: RegNode2/4/5Vars, RegLink2/3Vars bind only with every variable present.
    fn bind_vars(&mut self, key: &'static str, vars: &[u32]) {
        if vars.iter().all(|&v| self.vars.has(v)) {
            self.px.binds.insert(key, vars.to_vec());
        }
    }

    /// H193: SetIsNode2/4/5Done, SetIsLink2/3Done (SetIsNode2Done = FUN_004ad400, the others alike): the last bound
    /// variable := formula (type 2) — nothing unless RegCurrNode and the Reg* binding exist.
    fn node_link_done(&mut self, name: &str) {
        let (key, node) = match name {
            "SetIsNode2Done" => ("n2", 2),
            "SetIsNode4Done" => ("n4", 4),
            "SetIsNode5Done" => ("n5", 5),
            "SetIsLink2Done" => ("l2", 0xc),
            _ => ("l3", 0xd),
        };
        let (Some(cur), Some(b)) = (self.node_vars.0, self.px.binds.get(key).cloned()) else { return };
        let g = |v: u32| self.vars.get(v);
        let cur = g(cur);
        let v = match key {
            "n2" => (|| Some(cur? == node && g(b[0])? == 1 && g(b[1])? > 2))(),
            "n5" => (|| Some(cur? == node && g(b[1])? == 1 && g(b[0])? == 1))(),
            _ => (|| Some(cur? == node && g(b[0])? == 1))(),
        };
        let target = *b.last().unwrap();
        self.vars.set_native(target, v.map(|x| x as i32), 2);
    }

    /// H193 / C215 IsoChamberStat(east door, west door, lights, eight console picture flags): exactly the two flags of
    /// the chamber's state end 1, the others 0 (type 2, written only on a change).
    fn iso_chamber(&mut self, a: &[u32]) {
        let args = &a[..11];
        if !args.iter().all(|&v| self.vars.has(v)) {
            return;
        }
        let (inputs, pictures) = args.split_at(3);
        let g = |v: u32| self.vars.get(v);
        let state = match g(inputs[2]) {
            None => None,
            Some(lights) if lights != 1 => Some(Chamber::Dark),
            Some(_) => match (g(inputs[0]), g(inputs[1])) {
                (Some(east), Some(west)) => Some(match (east != 0, west != 0) {
                    (true, true) => Chamber::BothOpen,
                    (true, false) => Chamber::EastOpen,
                    (false, true) => Chamber::WestOpen,
                    (false, false) => Chamber::Closed,
                }),
                _ => None,
            },
        };
        for (i, &flag) in pictures.iter().enumerate() {
            let v = state.map(|s| s.pictures().contains(&i) as i32);
            self.write_if_changed(flag, v, 2);
        }
    }

    /// The journal's spread on its two page items; a missing page shows the item's own text
    fn journal_show(&mut self) {
        for (item, page) in JOURNAL_PAGES.into_iter().zip(self.px.journal.facing()) {
            match page {
                Some(p) => self.item_show_text(item, p),
                None => self.item_text_asset(item),
            }
        }
    }

    /// The Bible's spread on its two page items; a missing page leaves the item as it is
    fn bible_show(&mut self) {
        for (item, page) in BIBLE_PAGES.into_iter().zip(self.px.bible.facing()) {
            if let Some(p) = page {
                self.item_show_text(item, p);
            }
        }
    }
}

/// H197: the journal's page pool and its left / right page items
const JOURNAL_POOL: u32 = 0xc10e;
const JOURNAL_PAGES: [u32; 2] = [0xb522, 0xb523];
/// H197: the Bible's page pool, page items and "previous" / "next" hotspots
const BIBLE_POOL: u32 = 0x13c0b;
const BIBLE_PAGES: [u32; 2] = [0x13be4, 0x13be5];
const BIBLE_PREV: u32 = 0x13be6;
const BIBLE_NEXT: u32 = 0x13be7;
/// H197: the Bible stops at its fifth spread (pages 8 / 9), whatever its pool holds
const BIBLE_LAST_SPREAD: usize = 4;

/// A book seen one spread at a time: spread `s` faces pages 2s and 2s + 1.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Book {
    pages: Vec<u32>,
    spread: usize,
    last: usize,
}

impl Book {
    /// The journal: it ends at the spread holding its last page; opened there (`at_end`) or at the first spread
    fn journal(pages: Vec<u32>, at_end: bool) -> Book {
        let last = pages.len().saturating_sub(1) / 2;
        Book { pages, spread: if at_end { last } else { 0 }, last }
    }

    /// The Bible, opened at the first spread
    fn bible(pages: Vec<u32>) -> Book {
        Book { pages, spread: 0, last: BIBLE_LAST_SPREAD }
    }

    /// One spread forward or back; false (and the spread stays) at that end of the book
    fn turn(&mut self, forward: bool) -> bool {
        let to = if forward { Some(self.spread + 1).filter(|&s| s <= self.last) } else { self.spread.checked_sub(1) };
        to.map(|s| self.spread = s).is_some()
    }

    fn is_first(&self) -> bool {
        self.spread == 0
    }

    fn is_last(&self) -> bool {
        self.spread >= self.last
    }

    /// The left and right pages of the open spread
    fn facing(&self) -> [Option<u32>; 2] {
        [0, 1].map(|k| self.pages.get(2 * self.spread + k).copied())
    }
}

/// FUN_00486e60: digits, A–Z (a–z upper-cased) and " -(),." are typed; other keys nothing.
pub(super) fn login_char(k: u32) -> Option<char> {
    let c = char::from_u32(k)?;
    match c {
        '0'..='9' | 'A'..='Z' | ' ' | '-' | '(' | ')' | ',' => Some(c),
        'a'..='z' => Some(c.to_ascii_uppercase()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn books_turn_by_spreads() {
        let mut j = Book::journal((1..=5).collect(), true);
        assert_eq!((j.spread, j.facing()), (2, [Some(5), None]));
        assert!(!j.turn(true) && j.is_last());
        assert!(j.turn(false) && j.turn(false) && j.is_first() && !j.turn(false));
        assert_eq!(j.facing(), [Some(1), Some(2)]);
        let mut b = Book::bible((1..=12).collect());
        assert_eq!((0..5).map(|_| b.turn(true)).collect::<Vec<_>>(), [true, true, true, true, false]);
        assert_eq!((b.is_last(), b.facing()), (true, [Some(9), Some(10)]));
    }

    #[test]
    fn login_chars() {
        assert_eq!(login_char('a' as u32), Some('A'));
        assert_eq!(login_char('7' as u32), Some('7'));
        assert_eq!(login_char(0x1b), None);
    }

    #[test]
    fn shanks_flags_cover_every_hdb_argument() {
        for a in [0x836, 0xf49, 0x838, 0x83c, 0x83e, 0x842, 0x840, 0x844, 0x847, 0x84a, 0x84d, 0x84e, 0x850, 0x852, 0x855, 0x13bd] {
            assert!(SHANKS_FLAGS.iter().any(|s| s.flag == a), "{a:#x}");
        }
        let f = |id: u32| SHANKS_FLAGS.iter().find(|s| s.flag == id).unwrap();
        let v = |x: u32| Some(if x == 0x837 { 0 } else { 1 });
        assert_eq!(f(0x838).value(2, v), Some(true));
        assert_eq!(f(0x838).value(3, v), Some(false));
        assert_eq!(f(0x840).value(0, |_| None), Some(true));
        // "at least one open": 0xf45 still 0 is enough; all done → 0
        let w = |x: u32| Some(if x == 0xf45 { 0 } else { 1 });
        assert_eq!(f(0x83e).value(2, w), Some(true));
        assert_eq!(f(0x83e).value(2, |_| Some(1)), Some(false));
        // unknowns matter only when they decide: a wrong node or a known false term gives 0
        assert_eq!(f(0x842).value(3, |_| None), Some(false));
        assert_eq!(f(0x842).value(2, |x| if x == 0x83b { Some(0) } else { None }), Some(false));
        assert_eq!(f(0x842).value(2, |x| if x == 0x83b { Some(1) } else { None }), None);
        assert_eq!(f(0xf49).value(2, |x| if x == 0x835 { Some(0) } else { None }), Some(true));
    }
}
