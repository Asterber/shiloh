//! The FBI computer's native screens (static reading + HDB data, partly traced: RE_NOTES H570–H579, H790–H797, C67,
//! C95). Declared inside `procs` (`#[path]`), so it uses the procedures' helpers directly.
//!
//! - **Searches** (ING H575, media index H790): one shape — a query is looked up in a list of *key items* (an item's
//!   AssetRef list: entry 0 = what is found, entries 1.. = the words that find it) or in a directory, a hit shows a
//!   record / article / reply, and the screen answers with the found or not-found post pair.
//! - **Item texts** (H574): texts put into items with their style (Arial / Courier New, size, colour, alignment).
//! - **Text panels** (H576 / H791): the one main scroller shows an asset's text in the current panel item.
//! - **E-mail** (H577 / H796 / H797): inbox rows, forwarding, the address picker, the reply mails, the note-pad letter.
//! - **Carets** (H578): caret items follow the typed text.
//!
//! The item lists and directories come from the user's `XFiles.exe` (`exetables::PcTables`); single item / text ids
//! the original uses directly stay here as named constants.
use super::super::{class, Msg, Out, PostSrc, World};
use crate::exetables::PcTables;

/// Text face (Arial for SetIItemText 'H', else Courier New)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Courier,
    Arial,
}

/// A text object's style: face, weight, pixel size, colour, alignment, wrapping
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub face: Face,
    pub bold: bool,
    /// pixel size
    pub size: u8,
    pub colour: [u8; 3],
    /// 0 centre, 1 left, 2 right ('c' / 'l' / 'r')
    pub align: u8,
    pub wrap: bool,
}

/// Black Courier New 14, left aligned: the computer's default text style
pub const COURIER14: Style = Style { face: Face::Courier, bold: false, size: 14, colour: [0, 0, 0], align: 1, wrap: false };

/// The red Courier New 16 of the "restricted data" message (H893 / R36, fo-115)
const ALARM: Style = Style { size: 16, colour: [255, 0, 0], ..COURIER14 };

/// The engine's colour index → RGB (H893: index 8 is the Windows palette's grey RGB(160,160,164); index 6 is drawn red
/// on the original, R36): 0 white, 1 cyan, 2 magenta, 3 blue, 4 yellow, 5 green, 6 red, 7 grey, 8 (160,160,164); 0x12
/// is taken as black (every "default" text, H574)
pub fn colour(i: u32) -> [u8; 3] {
    match i {
        0 => [255, 255, 255],
        1 => [0, 255, 255],
        2 => [255, 0, 255],
        3 => [0, 0, 255],
        4 => [255, 255, 0],
        5 => [0, 255, 0],
        6 => [255, 0, 0],
        7 => [128, 128, 128],
        8 => [160, 160, 164],
        _ => [0, 0, 0],
    }
}

/// Where an item's text comes from: an AssetRef (kind 0 string resource, kind 4 text, kind 1 / 3 text file) or a native
/// string
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Src {
    Asset(u32),
    Str(String),
    /// H577: header line `n` (0 = "From:", 1 = "Subject:") of a mail AssetRef's text
    MailLine(u32, u8),
    /// H797: several text AssetRefs, one entry each (the note pad's notes of a day in the main scroller)
    Notes(Vec<u32>),
}

#[derive(Clone, Debug, Default)]
pub struct Pc {
    /// item → (text, style): native texts and asset texts put into items
    pub texts: Vec<(u32, Src, Style)>,
    /// item → style: SetIItemText's text style, the text = the item's H176 AssetRef
    pub styles: Vec<(u32, Style)>,
    /// item → (text, style): text panels
    pub panels: Vec<(u32, Src, Style)>,
    /// the current panel item (ScrollTextInit / ScrollTextList / InitEmailRead)
    pub panel_item: Option<u32>,
    /// H575: the ING screen's typed text
    pub ing: String,
    /// H575: a record is shown; INGDisplayCook's one-time post done; the object Forward sends (record name, article,
    /// mail or note-pad day item; 0 = none)
    pub ing_shown: bool,
    pub ing_cook_posted: bool,
    pub last_shown: u32,
    /// H577: EmailInit 'P' (the PDA's mail screen, Courier 12) instead of 'C' (the computer's, Courier 14)
    pub mail_pda: bool,
    /// H790: the media index's typed key words
    pub media: String,
    /// H791: the main text scroller's first line shown (MainScroll over a text panel); the front clamps it and writes
    /// back the largest first line (`scroll_max`) and a page (`scroll_page`, lines) of the laid-out panel
    pub scroll: usize,
    pub scroll_max: Option<usize>,
    pub scroll_page: usize,
    /// H892: the largest first line from which MainScroll 2 (page up) goes to the top; the front writes it
    pub scroll_to_top: usize,
    /// H793: the photo album (VCPhoto objects, class 0x59, in the game state's order): (picture AssetRef, downloaded)
    pub photos: Vec<(u32, bool)>,
    /// C306: per photo (parallel to `photos`) the VCPhoto's f68 / rect — a camera shot's video time (1/600 s) and its
    /// crop of the 600×240 frame; None (or missing) = TakePhoto's picture record (H982's template: f68 0x269, 0,0,300,240)
    pub photo_frames: Vec<Option<(u32, [i16; 4])>>,
    /// H793: the camera's photo count (≤ 24)
    pub camera: u32,
    /// H793: the first downloaded photo shown and the enlarged photo (index into `photos`)
    pub photo_offset: usize,
    pub enlarged: Option<usize>,
    /// H793: PhotoInit ran and PhotoClose did not (the thumbnails exist)
    pub album_open: bool,
    /// H794: SBase's typed text and the number of dots shown
    pub sbase: String,
    pub sbase_dots: usize,
    /// H796: ForwardObject's kind ('I' / 'M' / 'N') and the enclosure name counter
    pub forward: u8,
    pub enclosure: usize,
    /// H797: the address lists — the cards (the enabled AssetRefs of item 0xb0ff, each with its selected flag), To, Cc,
    /// the PDA list
    pub addr_cards: Vec<(u32, bool)>,
    pub addr_to: Vec<u32>,
    pub addr_cc: Vec<u32>,
    pub addr_pda: Vec<u32>,
    /// H797: the first row shown, the last configured picker kind, and the list CreateAddrList fills ('T' To, 'C' Cc,
    /// 'P' the PDA list)
    pub addr_offset: usize,
    pub addr_kind: u8,
    pub addr_target: u8,
}

/// H790: the media screen's typed-text field, its caret, the "Results on" text and the Forward button
pub const MEDIA_FIELD: u32 = 0xc63a;
pub const MEDIA_CARET: u32 = 0xfbba;
pub const MEDIA_RESULTS: u32 = 0xd6ff;
const MEDIA_FORWARD: u32 = 0xd6fc;
/// H790: finding this article (the Coast Guard / Tarakan story) also enables AssetRef 0x524c (Video Node 5 "Hauling
/// Yard" SC80I.mov) through the engine's enable queue
const MEDIA_TUG_ARTICLE: u32 = 0xd927;
const MEDIA_TUG_ASSET: u32 = 0x524c;
/// H794: the SBase caret, at (170 + 10 n, 200), and the field
pub const SBASE_CARET: u32 = 0xfbbc;
const SBASE_FIELD: u32 = 0xda11;
/// H793: the enlargement item; the Download / Delete buttons and their grey pictures
pub const PHOTO_ENLARGED: u32 = 0xc8c4;
const PHOTO_DOWNLOAD: u32 = 0xc8c5;
const PHOTO_DOWNLOAD_GREY: u32 = 0x114c5;
const PHOTO_DELETE: u32 = 0xc8c6;
const PHOTO_DELETE_GREY: u32 = 0x114c7;
/// H793: the album holds 60 downloaded photos (the message box when full); the camera 24
const ALBUM_CAPACITY: usize = 60;
const ALBUM_FULL_MSG: u32 = 0x107b1;
const CAMERA_MAX: u32 = 24;
/// H796: the forward compose screen's subject and enclosure items, its Send button
const FWD_SUBJECT: u32 = 0x9df5;
const FWD_ENCLOSURE: u32 = 0x9df4;
const FWD_SEND: u32 = 0x9e36;
/// H797: the PDA compose screen's subject item; the cards Cook / Shanks; RegCookEmail's variables
const NOTE_SUBJECT: u32 = 0x9fa9;
const CARD_COOK: u32 = 0xb0ea;
const CARD_SHANKS: u32 = 0xb0f2;
const VAR_WONG_CHECKED: u32 = 0x7bc;
const VAR_EMAILS_TO_COOK: u32 = 0x705;
const VAR_EMAILED_COOK_NSA: u32 = 0x10fe;
/// H797: day 2's reaction item (the letter about the NSA car), its extra post and extra mail to Cook
const COOK_NSA_REACTION: u32 = 0xf8b0;
const COOK_NSA_POST: (u32, u32) = (0xac35, 0xac39);
const COOK_NSA_EXTRA_MAIL: u32 = 0xfc0e;

/// H797: the card item whose enabled list is the address cards
const ADDR_CARDS_ITEM: u32 = 0xb0ff;

/// H795: RegUberVars' second variable (iUberParanoia)
const VAR_UBER_PARANOIA: u32 = 0x35b;

/// H575 / H790: what both search screens post on a hit / a miss
const POST_FOUND: (u32, u32) = (0x16245, 0x16246);
const POST_MISSING: (u32, u32) = (0x15fb5, 0x15fb6);
/// H575: the James Wong record (its position in the record list) answers only after SHANKSN2_bDoneWithWong
const WONG_RECORD: usize = 13;
const VAR_DONE_WITH_WONG: u32 = 0x83b;
/// H575: INGDisplayCook's record (Mark Cook), its gate variable and its one-time post
const COOK_RECORD: usize = 3;
const VAR_COOK_GATE: u32 = 0x13e7;
const COOK_POST: (u32, u32) = (0x160f7, 0x160f8);
/// H575: the phone directory's two fixed replies (the warehouse address — with its post and SHANKSN2_bFoundWarehouseInING
/// — and Seattle PD)
const WAREHOUSE_TEXT: u32 = 0xa4f1;
const WAREHOUSE_POST: (u32, u32) = (0x127f6, 0x127f7);
const VAR_FOUND_WAREHOUSE: u32 = 0x13bb;
const POLICE_TEXT: u32 = 0x15f07;
/// H575 / H795: the NSA plate's PDA note and IFL_bINGHaveSearchedForNSACar
const NSA_NOTE: u32 = 0xf73e;
const VAR_NSA_CAR: u32 = 0x1269;
/// ING screen items: the typed-text field, its caret, the message / result area, the Forward button
pub const ING_FIELD: u32 = 0xc636;
pub const ING_CARET: u32 = 0xfbe1;
pub const ING_MESSAGE: u32 = 0xe3e8;
const ING_FORWARD: u32 = 0x8de0;
/// IFL_cINGSearchCategory / IFL_cINGSearchDatabase
const VAR_CATEGORY: u32 = 0x5b2;
const VAR_DATABASE: u32 = 0x731;
/// the blank text a record without field notes shows
const ING_NO_NOTES: u32 = 0x94f5;
/// the "not found" text (ING / media index search without a match) and the "net congestion" error text
const NOT_FOUND_TEXT: u32 = 0xe3ec;
const CONGESTION_TEXT: u32 = 0x16437;
/// H578: the login caret 0xfbe5 and the start of each field's text (text item left, the centred line's top; the password
/// one measured on fo-069 k22: caret pixels at x 261..262, y 288..301)
pub const LOGIN_CARET: u32 = 0xfbe5;
const LOGIN_CARET_BASE: [(u32, i32, i32); 2] = [(0xc63b, 260, 226), (0xc63c, 260, 288)];
/// Courier New 14: 8 px per character (fo-069 k23: the 14-character user name's cells 8 px apart)
pub const COURIER14_ADVANCE: i32 = 8;

/// H797: a picker kind's row items and marker items: 'A' the To box, 'B' the Cc box, 'C' / 'T' the Cc / To pickers
/// (with markers), 'D' the PDA compose screen's address, 'P' the PDA picker
fn addr_tables(t: &PcTables, k: u8) -> Option<(&[u32], &[u32])> {
    Some(match k {
        b'A' => (&t.addr_to_box, &[]),
        b'B' => (&t.addr_cc_box, &[]),
        b'C' => (&t.addr_cc_rows, &t.addr_cc_marks),
        b'D' => (&t.addr_pda_one, &[]),
        b'P' => (&t.addr_pda_rows, &[]),
        b'T' => (&t.addr_to_rows, &t.addr_to_marks),
        _ => return None,
    })
}

/// One ING record (H575): the key item (entry 0 = the databases' letters, 1.. = the names), the item whose list holds
/// the record's texts, the photo and frame items, the field notes (0 = none)
#[derive(Clone, Copy)]
struct IngRecord {
    key: u32,
    texts: u32,
    photo: u32,
    frame: u32,
    notes: u32,
}

impl From<&[u32; 5]> for IngRecord {
    fn from(&[key, texts, photo, frame, notes]: &[u32; 5]) -> Self {
        IngRecord { key, texts, photo, frame, notes }
    }
}

/// Where a phone-directory entry leads (H575)
enum PhoneTarget {
    Record(usize),
    Warehouse,
    PoliceDept,
}

impl PhoneTarget {
    fn of(code: i32) -> Option<PhoneTarget> {
        match code {
            -1 => Some(PhoneTarget::Warehouse),
            -2 => Some(PhoneTarget::PoliceDept),
            n => usize::try_from(n).ok().map(PhoneTarget::Record),
        }
    }
}

/// What an ING lookup came to
enum IngOutcome {
    /// a record is on screen (Forward usable)
    Record,
    /// a fixed reply is in the message area, nothing to forward
    Reply,
    Missing,
    Congested,
}

/// The text of an AssetRef as Unicode: kind 4 = the blob's cp1251 text, kind 0 = an XfilesT.dll string (H260), other
/// kinds: None (files are read by the front-end, [`text_file_of`])
pub fn asset_string(w: &World, asset: u32) -> Option<String> {
    let r = w.h.read(class::ASSET_REF, asset)?;
    match r.u32("f40") & 7 {
        4 => {
            let b = w.h.blob_bytes(r.blob("asset")?)?;
            let n = *b.first()? as usize;
            let len: usize = std::str::from_utf8(b.get(1..1 + n)?).ok()?.parse().ok()?;
            Some(cp1251(b.get(1 + n..1 + n + len)?))
        }
        0 => super::super::devices::asset_text(w, asset),
        _ => None,
    }
}

/// An AssetRef of kind 1 / 3 whose file is a text (.txt / .xtx entries)
pub fn is_text_file(w: &World, asset: u32) -> bool {
    let Some(r) = w.h.read(class::ASSET_REF, asset) else { return false };
    matches!(r.u32("f40") & 7, 1 | 3)
        && r.blob("asset").and_then(|b| w.h.blob_bytes(b)).is_some_and(|b| b.windows(4).any(|x| x.eq_ignore_ascii_case(b".xtx")))
}

/// Unicode → cp1251 bytes (inverse of [`cp1251`] for ASCII, А..я, Ё, ё; anything else '?') (H650)
pub fn to_cp1251(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            '\0'..='\x7f' => c as u8,
            'А'..='я' => (c as u32 - 0x410 + 0xc0) as u8,
            'Ё' => 0xa8,
            'ё' => 0xb8,
            _ => b'?',
        })
        .collect()
}

/// H2306: cp1252 bytes (English data) → Unicode as [`cp1251`] does it: the same punctuation to ASCII, the rest Latin-1
pub fn cp1252(b: &[u8]) -> String {
    b.iter()
        .map(|&c| match c {
            0x96 | 0x97 => '-',
            0x93 | 0x94 | 0xab | 0xbb => '"',
            0x91 | 0x92 => '\'',
            _ => c as char,
        })
        .collect()
}

/// cp1251 bytes → Unicode (Cyrillic А..я, Ё, ё; ASCII as is)
pub fn cp1251(b: &[u8]) -> String {
    b.iter()
        .map(|&c| match c {
            0xc0..=0xff => char::from_u32(0x410 + c as u32 - 0xc0).unwrap_or('?'),
            0xa8 => 'Ё',
            0xb8 => 'ё',
            0x96 | 0x97 => '-',
            0x93 | 0x94 | 0xab | 0xbb => '"',
            0x91 | 0x92 => '\'',
            0xb9 => '№',
            _ => c as char,
        })
        .collect()
}

/// A text file's text as shown (H576): a tab → 4 spaces, a CR next to an LF dropped, a lone CR → LF
pub fn normalize(t: &str) -> String {
    let c: Vec<char> = t.chars().collect();
    let mut out = String::new();
    for (i, &ch) in c.iter().enumerate() {
        match ch {
            '\t' => out.push_str("    "),
            '\r' => {
                let lf = (i > 0 && c[i - 1] == '\n') || c.get(i + 1) == Some(&'\n');
                if !lf {
                    out.push('\n');
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// H577 / H798: a mail text's header lines — (line 2, line 3, the rest) split at "\n" (the line after "To:" = "From: …",
/// then "Subject: …")
pub fn mail_header(text: &str) -> (String, String, String) {
    let t = normalize(text);
    let mut it = t.splitn(4, '\n');
    let _to = it.next();
    let from = it.next().unwrap_or("").to_string();
    let subject = it.next().unwrap_or("").to_string();
    let rest = it.next().unwrap_or("").to_string();
    (from, subject, rest)
}

/// The computer's key filter (shared with LoginKeyPress, H174 / H867): digits, A–Z, space, '-', ',', '.' kept, a–z
/// upper-cased. Keys are virtual-key codes: VK_OEM_MINUS / PERIOD / COMMA (0xbd / 0xbe / 0xbc) are dropped — their
/// WM_CHAR '-' / '.' / ',' arrives as a second key event and is kept; the VK codes equal to '(' ')' (VK_DOWN /
/// VK_SELECT) are no characters
fn ing_char(k: u32) -> Option<char> {
    match char::from_u32(k)? {
        c @ ('0'..='9' | 'A'..='Z' | ' ' | '-' | ',' | '.') => Some(c),
        c @ 'a'..='z' => Some(c.to_ascii_uppercase()),
        _ => None,
    }
}

/// A key on a typed buffer: backspace removes the last character, a kept character is appended below `max`
fn edit_typed(buf: &mut String, key: u32, max: usize) {
    if key == 8 {
        buf.pop();
    } else if let Some(c) = ing_char(key) {
        if buf.chars().count() < max {
            buf.push(c);
        }
    }
}

/// A phone number or plate as compared: without ' ', '-', '(' and ')'
fn bare_number(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, ' ' | '-' | '(' | ')')).collect()
}

/// The last `n` characters of `s`
fn last_chars(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    c[c.len().saturating_sub(n)..].iter().collect()
}

impl World<'_> {
    fn pc_set(list: &mut Vec<(u32, Src, Style)>, item: u32, src: Src, style: Style) {
        list.retain(|x| x.0 != item);
        list.push((item, src, style));
    }

    /// A native text object in `item` (REG owner 0 + Activation as H102) with `text`
    fn pc_text(&mut self, item: u32, text: &str, style: Style) {
        self.item_text_asset(item);
        Self::pc_set(&mut self.pc.texts, item, Src::Str(text.to_string()), style);
    }

    /// `item` shows the text AssetRef `asset` (H176's object) in `style`
    fn pc_asset_text(&mut self, item: u32, asset: u32, style: Style) {
        if super::asset_is_text(self, asset) {
            self.item_show_asset(item, asset);
            Self::pc_set(&mut self.pc.texts, item, Src::Asset(asset), style);
        }
    }

    /// The current text panel shows the asset's text (0: empty); the scroller restarts at the top (H791)
    fn pc_panel(&mut self, asset: u32, style: Style) {
        if let Some(item) = self.pc.panel_item {
            let src = if asset == 0 { Src::Str(String::new()) } else { Src::Asset(asset) };
            Self::pc_set(&mut self.pc.panels, item, src, style);
            if self.rules.h791_text_scroll {
                self.pc.scroll = 0;
            }
        }
    }

    /// An InterfaceItem's whole f44 AssetRefList
    fn item_list(&self, item: u32) -> Vec<u32> {
        let list = self.h.read(class::INTERFACE_ITEM, item).map_or(0, |r| r.u32("f44"));
        if list == 0 {
            return Vec::new();
        }
        self.h.list_of(class::ASSET_REF_LIST, list)
    }

    /// Adds a mail to the pending-mail list unless it is pending or in the inbox already (as SendEmail, H186)
    fn queue_mail(&mut self, mail: u32) {
        if !self.px.pending.contains(&mail) && !self.px.inbox.contains(&mail) {
            self.px.pending.push(mail);
        }
    }

    // ---- searches ------------------------------------------------------------------------------------------------

    /// The first of `keys` (in order) one of whose words — list entries 1.. — equals `query` (lower case) ignoring
    /// letter case: (its position, its whole list). A list of fewer than two entries finds nothing.
    fn first_keyed(&self, keys: &[u32], query: &str) -> Option<(usize, Vec<u32>)> {
        keys.iter().enumerate().find_map(|(pos, &key)| {
            let list = self.item_list(key);
            let words = list.get(1..).unwrap_or(&[]);
            words.iter().any(|&a| asset_string(self, a).is_some_and(|t| t.to_lowercase() == query)).then_some((pos, list))
        })
    }

    fn ing_records(&self) -> Vec<IngRecord> {
        self.h.exe.ing_records.iter().map(IngRecord::from).collect()
    }

    /// Hides every record's photo and frame item
    fn ing_hide_pictures(&mut self) {
        for r in self.ing_records() {
            self.engine_enable(r.photo, 0);
            self.engine_enable(r.frame, 0);
        }
    }

    /// The record view emptied (also INGClear, H190): pictures and labels off, value texts and the panel empty
    fn ing_clear(&mut self) {
        self.ing_hide_pictures();
        let h = self.h;
        for &it in &h.exe.pc.ing_values {
            self.pc_text(it, "", COURIER14);
        }
        self.pc_panel(0, COURIER14);
        for &it in &h.exe.pc.ing_labels {
            self.engine_enable(it, 0);
        }
    }

    /// Shows record `pos`: its pictures, field notes, 8 texts and the labels; Forward will send its name
    fn ing_show_record(&mut self, pos: usize) {
        let Some(rec) = self.ing_records().get(pos).copied() else { return };
        let h = self.h;
        self.pc_text(ING_MESSAGE, "", COURIER14);
        self.ing_hide_pictures();
        self.engine_enable(rec.photo, 1);
        self.engine_enable(rec.frame, 1);
        self.pc_panel(if rec.notes == 0 { ING_NO_NOTES } else { rec.notes }, COURIER14);
        let texts = self.item_list(rec.texts);
        for (&it, &a) in h.exe.pc.ing_values.iter().zip(&texts) {
            self.pc_asset_text(it, a, COURIER14);
        }
        for &it in &h.exe.pc.ing_labels {
            self.engine_enable(it, 1);
        }
        self.pc.last_shown = texts.first().copied().unwrap_or(0);
        self.pc.ing_shown = true;
    }

    /// A fixed reply instead of a record: the view cleared, `text` in the message area, nothing to forward
    fn ing_show_reply(&mut self, text: u32, style: Style) {
        self.ing_clear();
        self.pc_asset_text(ING_MESSAGE, text, style);
    }

    /// Nothing to forward: the shown object forgotten, Forward off
    fn ing_forget_shown(&mut self) {
        self.pc.last_shown = 0;
        self.engine_enable(ING_FORWARD, 0);
        self.pc.ing_shown = false;
    }

    /// INGSearch (H575): look the typed text up in the chosen category, then answer
    fn ing_search(&mut self, action: u32) {
        let query = self.pc.ing.to_lowercase();
        let category = self.vars.get(VAR_CATEGORY).unwrap_or(0) as u8;
        let database = self.vars.get(VAR_DATABASE).unwrap_or(0) as u8;
        let outcome = match category {
            b'N' => self.ing_by_name(&query, database),
            b'P' => self.ing_by_phone(&query, action),
            b'L' => self.ing_by_plate(&query, database),
            _ => IngOutcome::Missing,
        };
        match outcome {
            IngOutcome::Record | IngOutcome::Reply => {
                self.do_action_list(action, POST_FOUND);
                if self.pc.ing_shown {
                    self.engine_enable(ING_FORWARD, 1);
                }
                self.pc.ing.clear();
                self.pc_text(ING_FIELD, "", COURIER14);
            }
            IngOutcome::Missing | IngOutcome::Congested => {
                // the typed text stays as typed
                self.do_action_list(action, POST_MISSING);
                self.ing_clear();
                let text = if matches!(outcome, IngOutcome::Congested) { CONGESTION_TEXT } else { NOT_FOUND_TEXT };
                self.pc_asset_text(ING_MESSAGE, text, COURIER14);
                self.engine_enable(ING_FORWARD, 0);
            }
        }
    }

    /// Category 'N': the first record named `query`, shown when it lives in `database`
    fn ing_by_name(&mut self, query: &str, database: u8) -> IngOutcome {
        let keys: Vec<u32> = self.ing_records().iter().map(|r| r.key).collect();
        let Some((pos, list)) = self.first_keyed(&keys, query) else { return IngOutcome::Missing };
        if pos == WONG_RECORD && self.vars.get(VAR_DONE_WITH_WONG) == Some(0) {
            return IngOutcome::Congested;
        }
        let letters = list.first().and_then(|&a| asset_string(self, a)).unwrap_or_default();
        if database == 0 || !letters.contains(database as char) {
            return IngOutcome::Missing;
        }
        self.ing_show_record(pos);
        IngOutcome::Record
    }

    /// The directory entry whose number ends in the 10 characters `number`; None also when an entry before it cannot
    /// be read
    fn ing_phone_entry(&self, number: &str) -> Option<i32> {
        for &(text, target) in &self.h.exe.pc.ing_phones {
            if last_chars(&asset_string(self, text)?, 10) == number {
                return Some(target);
            }
        }
        None
    }

    /// Category 'P': a 10-character number (spaces, '-', parentheses ignored) in the phone directory
    fn ing_by_phone(&mut self, query: &str, action: u32) -> IngOutcome {
        let number = bare_number(query);
        if number.chars().count() != 10 {
            return IngOutcome::Missing;
        }
        match self.ing_phone_entry(&number).and_then(PhoneTarget::of) {
            None => IngOutcome::Missing,
            Some(PhoneTarget::Record(pos)) => {
                self.ing_show_record(pos);
                IngOutcome::Record
            }
            Some(PhoneTarget::Warehouse) => {
                self.ing_show_reply(WAREHOUSE_TEXT, COURIER14);
                self.do_action_list(action, WAREHOUSE_POST);
                if self.vars.has(VAR_FOUND_WAREHOUSE) {
                    self.vars.set_native(VAR_FOUND_WAREHOUSE, Some(1), 0);
                }
                self.ing_forget_shown();
                IngOutcome::Reply
            }
            Some(PhoneTarget::PoliceDept) => {
                self.ing_show_reply(POLICE_TEXT, COURIER14);
                self.ing_forget_shown();
                IngOutcome::Reply
            }
        }
    }

    /// Category 'L': the rental car's plate in Civilian ('Z') or the NSA car's in Gov't/Military ('G')
    fn ing_by_plate(&mut self, query: &str, database: u8) -> IngOutcome {
        let &[rental, rental_file, nsa, restricted] = self.h.exe.pc.ing_plates.as_slice() else { return IngOutcome::Missing };
        let (Some(rental_plate), Some(nsa_plate)) = (asset_string(self, rental), asset_string(self, nsa)) else {
            return IngOutcome::Missing;
        };
        let plate = bare_number(query);
        match database {
            b'Z' if plate == rental_plate => {
                self.ing_show_reply(rental_file, COURIER14);
                self.ing_forget_shown();
                IngOutcome::Reply
            }
            b'G' if plate == nsa_plate => {
                self.ing_show_reply(restricted, ALARM);
                self.nsa_plate_found()
            }
            _ => IngOutcome::Missing,
        }
    }

    /// H575 / H795: the NSA plate's consequences — its PDA note, and on the first search iUberParanoia + 2 and
    /// IFL_bINGHaveSearchedForNSACar := 1; without that variable the search ends as not found
    fn nsa_plate_found(&mut self) -> IngOutcome {
        if self.h.has(class::ASSET_REF, NSA_NOTE) && !self.px.notes.contains(&NSA_NOTE) {
            super::super::list_add(&mut self.px.notes, NSA_NOTE, self.rules.h1602_sorted_lists);
        }
        let Some(searched) = self.vars.get(VAR_NSA_CAR) else { return IngOutcome::Missing };
        if searched == 0 {
            if self.rules.h795_uber_paranoia {
                if let Some(p) = self.vars.get(VAR_UBER_PARANOIA) {
                    self.vars.set_native(VAR_UBER_PARANOIA, Some(p + 2), 1);
                }
                self.vars.set_native(VAR_NSA_CAR, Some(1), 2);
            } else {
                self.vars.set_native(VAR_NSA_CAR, Some(1), 0);
            }
        }
        self.ing_forget_shown();
        IngOutcome::Reply
    }

    /// INGDisplayCook (H575): with 0x13e7 = 0 Mark Cook's record (and once a session its post), else the restricted text
    fn ing_display_cook(&mut self, action: u32) {
        let Some(gate) = self.vars.get(VAR_COOK_GATE) else { return };
        if gate == 0 {
            self.ing_show_record(COOK_RECORD);
            if !self.pc.ing_cook_posted {
                self.do_action_list(action, COOK_POST);
                self.pc.ing_cook_posted = true;
            }
        } else if let Some(&restricted) = self.h.exe.pc.ing_plates.get(3) {
            self.pc_asset_text(ING_MESSAGE, restricted, ALARM);
        }
    }

    /// INGKeyPressed (H575): Enter posts (0x9dcb, 0x9f86); else the key edits the typed text (max 30) and the field
    /// shows it
    fn ing_key(&mut self, action: u32) {
        let Some(key) = self.dev.key else {
            self.ab_unimpl(action, "H575: INGKeyPressed without a key".into());
            return;
        };
        if key == 0xd {
            self.do_action_list(action, (0x9dcb, 0x9f86));
            return;
        }
        edit_typed(&mut self.pc.ing, key, 30);
        let t = self.pc.ing.clone();
        self.pc_text(ING_FIELD, &t, COURIER14);
    }

    /// MSKeyPressed (H790): Enter posts (0xe90c, 0xe90d); else the key edits the typed words (max 25), the field shows
    /// them and the caret follows
    fn media_key(&mut self, action: u32) {
        let Some(key) = self.dev.key else {
            self.ab_unimpl(action, "H790: MSKeyPressed without a key".into());
            return;
        };
        if key == 0xd {
            self.do_action_list(action, (0xe90c, 0xe90d));
            return;
        }
        edit_typed(&mut self.pc.media, key, 25);
        let t = self.pc.media.clone();
        self.pc_text(MEDIA_FIELD, &t, COURIER14);
    }

    /// MediaSearch (H790): "Results on" shows the typed words; the first key item they match gives the article for the
    /// panel and for Forward; the typed words are emptied either way
    fn media_search(&mut self, action: u32) {
        let typed = self.pc.media.clone();
        self.pc_text(MEDIA_RESULTS, &typed, COURIER14);
        let keys = self.h.exe.media_keys.clone();
        match self.first_keyed(&keys, &typed.to_lowercase()).map(|(_, list)| list[0]) {
            Some(article) => {
                self.pc_panel(article, COURIER14);
                self.pc.last_shown = article;
                self.engine_enable(MEDIA_FORWARD, 1);
                self.do_action_list(action, POST_FOUND);
                if article == MEDIA_TUG_ARTICLE {
                    self.engine_enables.push_back((class::ASSET_REF, MEDIA_TUG_ASSET, 1));
                    self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
                }
            }
            None => {
                self.do_action_list(action, POST_MISSING);
                self.pc_panel(NOT_FOUND_TEXT, COURIER14);
                self.pc.last_shown = 0;
                self.engine_enable(MEDIA_FORWARD, 0);
            }
        }
        self.pc.media.clear();
        self.pc_text(MEDIA_FIELD, "", COURIER14);
    }

    // ---- the photo album ----------------------------------------------------------------------------------------

    /// H793: a save's VCPhoto records (class 0x59: f64 = the picture AssetRef, f95 = downloaded), in record order; the
    /// camera holds the ones not downloaded (TakePhoto +1, PhotoDownload := those left). No save of the original with a
    /// photo exists yet: NOT checked.
    pub(in crate::world) fn pc_seed(&mut self) {
        let Some(c) = self.gam.class(0x59) else { return };
        let ids: Vec<u32> = c.objects.iter().map(|x| x.0).collect();
        let recs: Vec<_> = ids
            .into_iter()
            .filter_map(|id| self.gam.read(0x59, id))
            .filter(|r| r.u32("f64") != 0)
            .map(|r| {
                let rect = match r.field("rect") {
                    Some(crate::hdb::Value::Rect(q)) => *q,
                    _ => [0, 0, 300, 240],
                };
                ((r.u32("f64"), r.u32("f95") != 0), (r.u32("f68"), rect))
            })
            .collect();
        self.pc.photos = recs.iter().map(|x| x.0).collect();
        // C306: a shot's f68 / rect kept (loaded and written back as they were)
        self.pc.photo_frames = recs.iter().map(|x| Some(x.1).filter(|_| self.rules.c306_shot_photo)).collect();
        self.pc.camera = self.pc.photos.iter().filter(|p| !p.1).count().min(CAMERA_MAX as usize) as u32;
    }

    /// H793: the downloaded photos the album shows (indices into `photos`): from the offset, one per thumbnail slot
    pub fn album_shown(&self) -> Vec<usize> {
        let down: Vec<usize> = (0..self.pc.photos.len()).filter(|&i| self.pc.photos[i].1).collect();
        down.into_iter().skip(self.pc.photo_offset).take(self.h.exe.pc.photo_slots.len()).collect()
    }

    /// The thumbnail slot item of shown photo `k` (H793)
    pub fn album_slot(&self, k: usize) -> Option<u32> {
        self.h.exe.pc.photo_slots.get(k).copied()
    }

    /// Download usable (its grey picture hidden) or not
    fn photo_download_button(&mut self, usable: bool) {
        self.engine_enable(PHOTO_DOWNLOAD_GREY, (!usable) as u8);
        self.engine_enable(PHOTO_DOWNLOAD, usable as u8);
    }

    /// Delete usable (its grey picture hidden) or not
    fn photo_delete_button(&mut self, usable: bool) {
        self.engine_enable(PHOTO_DELETE_GREY, (!usable) as u8);
        self.engine_enable(PHOTO_DELETE, usable as u8);
    }

    /// PhotoInit (H793): offset and enlargement reset; Download usable iff a photo is still in the camera; Delete not
    fn photo_init(&mut self) {
        self.pc.photo_offset = 0;
        self.pc.enlarged = None;
        let in_camera = self.pc.photos.iter().any(|p| !p.1);
        self.photo_download_button(in_camera);
        self.photo_delete_button(false);
        self.pc.album_open = true;
    }

    /// PhotoDownload (H793): the camera's photos move into the album until it holds 60 (then the "album full" box); an
    /// empty camera greys Download; the album shows from its first row
    fn photo_download(&mut self) {
        let mut in_album = self.pc.photos.iter().filter(|p| p.1).count();
        if in_album == self.pc.photos.len() {
            return;
        }
        let in_camera: Vec<usize> = (0..self.pc.photos.len()).filter(|&i| !self.pc.photos[i].1).collect();
        for i in in_camera {
            if in_album >= ALBUM_CAPACITY {
                self.message_box(ALBUM_FULL_MSG);
                break;
            }
            self.pc.photos[i].1 = true;
            in_album += 1;
        }
        let left = self.pc.photos.len() - in_album;
        if left == 0 {
            self.photo_download_button(false);
        }
        self.pc.camera = left as u32;
        self.pc.photo_offset = 0;
    }

    // ---- e-mail ---------------------------------------------------------------------------------------------------

    /// The mail screens' text style: Courier New 14 on the computer, 12 on the PDA
    fn mail_style(&self) -> Style {
        Style { size: if self.pc.mail_pda { 12 } else { 14 }, ..COURIER14 }
    }

    /// ForwardObject (H796): the compose screen's subject / enclosure for the forwarded object; Send disabled until a
    /// recipient is chosen (CreateAddrList)
    fn forward_object(&mut self, k: u8) {
        self.pc.forward = k;
        let style = self.mail_style();
        let h = self.h;
        match k {
            b'I' => {
                let shown = self.pc.last_shown;
                self.pc_asset_text(FWD_SUBJECT, shown, style);
                let names = &h.exe.pc.enclosures;
                if let Some(name) = names.get(self.pc.enclosure % names.len().max(1)) {
                    self.pc_text(FWD_ENCLOSURE, name, style);
                }
                // the counter runs on for the whole session
                self.pc.enclosure = (self.pc.enclosure + 1) % names.len().max(1);
                self.engine_enable(FWD_SEND, 0);
            }
            b'M' => {
                // H2301: the EN build's subject is an AssetRef's text
                match h.exe.pc.article_subject_asset {
                    Some(a) => self.pc_asset_text(FWD_SUBJECT, a, style),
                    None => self.pc_text(FWD_SUBJECT, &h.exe.pc.article_subject, style),
                }
                let shown = self.pc.last_shown;
                self.pc_panel(shown, style);
                self.engine_enable(FWD_SEND, 0);
            }
            b'N' => {
                // the note pad's current day item (GUESS H797: taken here, from the current day)
                if let Some(&d) = self.h.exe.note_days.get(self.notes_ui.day) {
                    self.pc.last_shown = d;
                }
            }
            _ => {}
        }
    }

    /// H797: the letter chosen for `card` on note-pad day index `day`: Some((subject, body)) for a written letter,
    /// Some(None) for the "Field Notes" fallback, None when nothing happens at all
    fn note_letter_for(&self, day: usize, card: u32) -> Option<Option<(u32, u32)>> {
        if day == 1 && card == CARD_COOK {
            let checked = self.vars.get(VAR_WONG_CHECKED)?;
            if checked == 0 {
                return Some(Some((0x10601, 0xf150)));
            }
        }
        if day == 4 && card == CARD_SHANKS {
            let body = if self.px.notes.contains(&0xf738) { 0xf149 } else { 0xf148 };
            return Some(Some((0x10607, body)));
        }
        let row = self.h.exe.pc.note_letters.iter().find(|r| r[0] as usize == day && r[1] == card);
        Some(row.map(|r| (r[3], r[2])))
    }

    /// AddrSelPDA's letter (H797): the PDA compose screen's subject and body for the chosen recipient; a letter to Cook
    /// is counted at once (iNumWillmoreEMailsToCook + 1; on day index 1 also IFL_bPDAEmailedCookAboutNSA)
    fn note_letter(&mut self, card: u32) {
        let day = self.notes_ui.day;
        let Some(letter) = self.note_letter_for(day, card) else { return };
        let style = Style { size: 12, ..COURIER14 };
        match letter {
            Some((subject, body)) => {
                self.pc_asset_text(NOTE_SUBJECT, subject, style);
                self.pc_panel(body, style);
            }
            None => {
                // H2301: the EN build's subject is an AssetRef's text
                match self.h.exe.pc.field_notes_asset {
                    Some(a) => self.pc_asset_text(NOTE_SUBJECT, a, style),
                    None => {
                        let words = self.h.exe.pc.field_notes.clone();
                        self.pc_text(NOTE_SUBJECT, &words, style);
                    }
                }
                let notes = self.note_assets();
                if let Some(item) = self.pc.panel_item {
                    Self::pc_set(&mut self.pc.panels, item, Src::Notes(notes), style);
                    self.pc.scroll = 0;
                }
            }
        }
        if card == CARD_COOK {
            if day == 1 && self.vars.has(VAR_EMAILED_COOK_NSA) {
                self.vars.set_native(VAR_EMAILED_COOK_NSA, Some(1), 2);
            }
            if let Some(n) = self.vars.get(VAR_EMAILS_TO_COOK) {
                self.vars.set_native(VAR_EMAILS_TO_COOK, Some(n + 1), 1);
            }
        }
    }

    /// H797: 'C' / 'P' / 'T' also choose the list CreateAddrList fills (Cc / the PDA list / To); 'A' / 'B' / 'D' keep it
    fn addr_kind(&mut self, k: u8) {
        if addr_tables(&self.h.exe.pc, k).is_none() {
            return;
        }
        self.pc.addr_kind = k;
        if matches!(k, b'C' | b'P' | b'T') {
            self.pc.addr_target = k;
        }
    }

    /// H797: the list the rows show: the cards for the pickers, else To / Cc / the PDA list
    fn addr_shown(&self) -> Vec<u32> {
        match self.pc.addr_kind {
            b'A' => self.pc.addr_to.clone(),
            b'B' => self.pc.addr_cc.clone(),
            b'D' => self.pc.addr_pda.clone(),
            _ => self.pc.addr_cards.iter().map(|c| c.0).collect(),
        }
    }

    fn addr_markers_off(&mut self) {
        let h = self.h;
        if let Some((_, marks)) = addr_tables(&h.exe.pc, self.pc.addr_kind) {
            for &m in marks {
                self.engine_enable(m, 0);
            }
        }
    }

    /// H797: from the offset, each row shows its entry's address (the mail style), a selected card's marker on; the rows
    /// left show ""
    fn addr_rows(&mut self) {
        let h = self.h;
        let Some((rows, marks)) = addr_tables(&h.exe.pc, self.pc.addr_kind) else { return };
        let style = self.mail_style();
        let shown = self.addr_shown();
        let off = self.pc.addr_offset;
        for (j, &row) in rows.iter().enumerate() {
            match shown.get(off + j) {
                Some(&a) => {
                    let selected = !marks.is_empty() && self.pc.addr_cards.get(off + j).is_some_and(|c| c.1);
                    if selected {
                        self.engine_enable(marks[j], 1);
                    }
                    self.pc_asset_text(row, a, style);
                }
                None => self.pc_text(row, "", style),
            }
        }
    }

    /// The list CreateAddrList fills := the selected cards
    fn addr_take_selected(&mut self) {
        let sel: Vec<u32> = self.pc.addr_cards.iter().filter(|c| c.1).map(|c| c.0).collect();
        match self.pc.addr_target {
            b'T' => self.pc.addr_to = sel,
            b'C' => self.pc.addr_cc = sel,
            b'P' => self.pc.addr_pda = sel,
            _ => {}
        }
    }

    /// EmailSend (H797): with a To or PDA recipient — the recipients (the PDA list, else To then the Cc entries not in
    /// To) as positions in the recipients list; 'I' / 'M': the reaction item of the shown record / article, its list's
    /// mail per recipient position queued; 'N': the note pad's replies
    fn email_send(&mut self, action: u32) {
        if self.pc.addr_to.is_empty() && self.pc.addr_pda.is_empty() {
            return;
        }
        let rec: Vec<u32> = if !self.pc.addr_pda.is_empty() {
            self.pc.addr_pda.clone()
        } else {
            let mut r = self.pc.addr_to.clone();
            r.extend(self.pc.addr_cc.iter().filter(|c| !self.pc.addr_to.contains(c)));
            r
        };
        let cards = &self.h.exe.pc.recipients;
        let idx: Vec<usize> = rec.iter().flat_map(|a| (0..cards.len()).filter(move |&i| cards[i] == *a)).collect();
        let shown = self.pc.last_shown;
        let reaction = match self.pc.forward {
            b'I' => self.h.exe.ing_replies.iter().find(|x| x.0 == shown).map(|x| x.1),
            b'M' => self.h.exe.media_replies.iter().find(|x| x.0 == shown).map(|x| x.1),
            b'N' => {
                self.note_send(action, &idx);
                None
            }
            _ => None,
        };
        let Some(reaction) = reaction else { return };
        let replies = self.item_list(reaction);
        for i in idx {
            if let Some(&mail) = replies.get(i) {
                self.queue_mail(mail);
            }
        }
    }

    /// EmailSend for a note-pad letter (H797): the day item's reaction, its reply per recipient; the NSA letter to Cook
    /// is answered only while bWongBackgroundChecked ≠ 1 (then also a post and an extra mail), and without that variable
    /// the sending stops
    fn note_send(&mut self, action: u32, idx: &[usize]) {
        let shown = self.pc.last_shown;
        let Some(&(_, reaction)) = self.h.exe.note_replies.iter().find(|x| x.0 == shown) else { return };
        let replies = self.item_list(reaction);
        for &i in idx {
            let to_cook = self.h.exe.pc.recipients.get(i) == Some(&CARD_COOK);
            if reaction == COOK_NSA_REACTION && to_cook {
                let Some(checked) = self.vars.get(VAR_WONG_CHECKED) else { return };
                if checked == 1 {
                    continue;
                }
                self.do_action_list(action, COOK_NSA_POST);
                if let Some(&mail) = replies.get(i) {
                    self.queue_mail(mail);
                }
                self.queue_mail(COOK_NSA_EXTRA_MAIL);
            } else if let Some(&mail) = replies.get(i) {
                self.queue_mail(mail);
            }
        }
    }

    /// H577: the 8 shown inbox rows: header texts and read markers
    fn mail_redisplay(&mut self) {
        let h = self.h;
        let table = if self.pc.mail_pda { &h.exe.pc.mail_rows_pda } else { &h.exe.pc.mail_rows_pc };
        let [from_items, subject_items, markers] = table.as_slice() else { return };
        let style = self.mail_style();
        let shown: Vec<u32> = self.px.mail_shown.iter().skip(self.px.mail_offset).take(8).copied().collect();
        for j in 0..8 {
            match shown.get(j) {
                Some(&a) => {
                    self.item_text_asset(from_items[j]);
                    Self::pc_set(&mut self.pc.texts, from_items[j], Src::MailLine(a, 0), style);
                    self.item_text_asset(subject_items[j]);
                    Self::pc_set(&mut self.pc.texts, subject_items[j], Src::MailLine(a, 1), style);
                    let read = self.px.read.contains(&a);
                    self.engine_enable(markers[j], read as u8);
                }
                None => {
                    self.pc_text(from_items[j], "", style);
                    self.pc_text(subject_items[j], "", style);
                    self.engine_enable(markers[j], 0);
                }
            }
        }
    }

    /// H570–H578: the computer's procedures; true = handled (else the older handlers run)
    pub(in crate::world) fn pc_proc(&mut self, action: u32, trigger: u32, name: &str, args: &[u32]) -> bool {
        let r = self.rules;
        match (name, args) {
            // H574: SetIItemText's style; the text itself = H176 (+ H573 for kind-0 strings)
            ("SetIItemText", &[item, _asset, c, f, size, wrap, align, ..]) if r.h574_item_text_style => {
                let style = Style {
                    face: if f == 'H' as u32 { Face::Arial } else { Face::Courier },
                    bold: f == 'H' as u32,
                    size: size as u8,
                    colour: match c as u8 {
                        b'w' => colour(0),
                        _ => colour(0x12),
                    },
                    align: match align as u8 {
                        b'c' => 0,
                        b'r' => 2,
                        _ => 1,
                    },
                    wrap: wrap == 'y' as u32,
                };
                self.pc.styles.retain(|x| x.0 != item);
                self.pc.styles.push((item, style));
                false
            }
            ("ScrollTextInit", &[item, ..]) if r.h576_text_panels => {
                // an empty black Courier New panel in the item's rect
                self.pc.panel_item = Some(item);
                if r.h791_text_scroll {
                    // one main scroller: a new panel replaces the old one
                    self.pc.panels.clear();
                    Self::pc_set(&mut self.pc.panels, item, Src::Str(String::new()), COURIER14);
                    self.pc.scroll = 0;
                }
                false
            }
            // H791: ScrollTextList(item, asset, colour 'b' / 'w' / 'x', face 'H' = Arial bold else Courier New, size):
            // the main scroller in the item's rect with the asset's whole text; it becomes the current panel
            ("ScrollTextList", &[item, asset, c, f, size, ..]) if r.h791_text_scroll => {
                let style = Style {
                    face: if f == 'H' as u32 { Face::Arial } else { Face::Courier },
                    bold: f == 'H' as u32,
                    size: size as u8,
                    colour: if c == 'w' as u32 { colour(0) } else { colour(0x12) },
                    align: 1,
                    wrap: true,
                };
                self.pc.panel_item = Some(item);
                let src = if asset == 0 { Src::Str(String::new()) } else { Src::Asset(asset) };
                self.pc.panels.clear();
                Self::pc_set(&mut self.pc.panels, item, src, style);
                self.pc.scroll = 0;
                true
            }
            // H791: the main scroller (one for every text panel): 0 one line up, 1 one line down, 2 a page up, 3 a page
            // down, only while the text is longer than the panel (the front measures it: `scroll_max`, `scroll_page`)
            ("MainScroll", &[dir, ..]) if r.h791_text_scroll && !self.notes_ui.open && self.pc.panel_item.is_some() => {
                let max = self.pc.scroll_max.unwrap_or(usize::MAX);
                let page = self.pc.scroll_page.max(1);
                let s = self.pc.scroll.min(max);
                self.pc.scroll = match dir {
                    0 => s.saturating_sub(1),
                    1 => (s + 1).min(max),
                    // H892 / R35: a page up goes to the top when the text's top is at most the view's height less
                    // min(line, gap) above it (fo-114 k15: line 5 → 0)
                    2 if r.h892_page_up_top && s <= self.pc.scroll_to_top => 0,
                    2 => s.saturating_sub(page),
                    3 => s.saturating_add(page).min(max),
                    _ => s,
                };
                true
            }
            // ---- H790: the media index ----------------------------------------------------------------------------
            ("MediaInit", _) if r.h790_media_index => {
                // nothing to forward, Forward disabled, the caret placed, the field focused
                self.pc.last_shown = 0;
                self.engine_enable(MEDIA_FORWARD, 0);
                self.dev.focus = Some(MEDIA_FIELD);
                true
            }
            ("MSKeyPressed", _) if r.h790_media_index => {
                self.media_key(action);
                true
            }
            ("MediaSearch", _) if r.h790_media_index => {
                self.media_search(action);
                true
            }
            // ---- H794: SBase --------------------------------------------------------------------------------------
            ("SBaseInit", _) if r.h794_sbase => {
                // no dots, the text emptied, every dot item disabled, the field focused
                self.pc.sbase_dots = 0;
                self.pc.sbase.clear();
                let h = self.h;
                for &d in &h.exe.pc.sbase_dots {
                    self.engine_enable(d, 0);
                }
                self.dev.focus = Some(SBASE_FIELD);
                true
            }
            ("SBaseKeyPress", _) if r.h794_sbase => {
                // the key filter (max 31); one more dot shown per added character, one hidden per removed
                if let Some(key) = self.dev.key {
                    edit_typed(&mut self.pc.sbase, key, 31);
                }
                let dots = self.h.exe.pc.sbase_dots.clone();
                let n = self.pc.sbase.chars().count();
                if n < 32 && self.pc.sbase_dots < n {
                    if let Some(&d) = dots.get(self.pc.sbase_dots) {
                        self.engine_enable(d, 1);
                    }
                    self.pc.sbase_dots += 1;
                }
                if self.pc.sbase_dots != 0 && n < self.pc.sbase_dots {
                    self.pc.sbase_dots -= 1;
                    if let Some(&d) = dots.get(self.pc.sbase_dots) {
                        self.engine_enable(d, 0);
                    }
                }
                true
            }
            // ---- H793: the photo album ----------------------------------------------------------------------------
            ("TakePhoto", &[asset, ..]) if r.h793_photo_album => {
                // below 24 photos in the camera, a VCPhoto of the AssetRef's picture is added
                if self.pc.camera != CAMERA_MAX && self.h.has(class::ASSET_REF, asset) {
                    self.pc.photos.push((asset, false));
                    self.pc.photo_frames.resize(self.pc.photos.len(), None);
                    if self.pc.camera < CAMERA_MAX {
                        self.pc.camera += 1;
                    }
                }
                true
            }
            ("PhotoInit", _) if r.h793_photo_album => {
                self.photo_init();
                true
            }
            ("PhotoDownload", _) if r.h793_photo_album => {
                self.photo_download();
                true
            }
            ("PhotoEnlarge", &[i, ..]) if r.h793_photo_album => {
                // shown thumbnail i enlarged; Delete usable
                let shown = self.album_shown();
                if let Some(&p) = shown.get(i as usize) {
                    self.pc.enlarged = Some(p);
                    self.photo_delete_button(true);
                }
                true
            }
            ("PhotoDelete", _) if r.h793_photo_album => {
                // the enlarged photo leaves the game state; the thumbnails move up one slot
                if let Some(p) = self.pc.enlarged.take() {
                    if p < self.pc.photos.len() {
                        self.pc.photos.remove(p);
                        if p < self.pc.photo_frames.len() {
                            self.pc.photo_frames.remove(p);
                        }
                    }
                    self.photo_delete_button(false);
                }
                true
            }
            ("PhotoScroll", &[dir, ..]) if r.h793_photo_album => {
                // 0: a row up while not at the top; 1: a row down while 20 thumbnails are shown
                if dir == 0 {
                    self.pc.photo_offset = self.pc.photo_offset.saturating_sub(5);
                } else if self.album_shown().len() >= 20 {
                    self.pc.photo_offset += 5;
                }
                true
            }
            ("PhotoClose", _) if r.h793_photo_album => {
                // the thumbnails removed (the enlarged object stays until the next PhotoInit)
                self.pc.album_open = false;
                true
            }
            // ---- H796: forwarding ---------------------------------------------------------------------------------
            ("ForwardObject", &[k, ..]) if r.h796_forward => {
                self.forward_object(k as u8);
                true
            }
            ("ScrollTextKill", _) if r.h576_text_panels => {
                if let Some(i) = self.pc.panel_item.take() {
                    self.pc.panels.retain(|x| x.0 != i);
                }
                false
            }
            ("INGClear", _) if r.h575_ing => {
                self.ing_clear();
                self.pc_text(ING_MESSAGE, "", COURIER14);
                self.pc_text(ING_FIELD, "", COURIER14);
                self.pc.ing.clear();
                self.engine_enable(ING_FORWARD, 0);
                true
            }
            ("INGInit", _) if r.h575_ing => {
                self.engine_enable(ING_FORWARD, 0);
                self.pc_text(ING_FIELD, "", COURIER14);
                self.dev.focus = Some(ING_FIELD);
                true
            }
            ("INGKeyPressed", _) if r.h575_ing => {
                self.ing_key(action);
                true
            }
            ("INGSearch", _) if r.h575_ing => {
                self.ing_search(action);
                true
            }
            ("INGDisplayCook", _) if r.h575_ing => {
                self.ing_display_cook(action);
                true
            }
            ("EmailInit", &[c, ..]) if r.h577_mail_rows && (c == 'C' as u32 || c == 'P' as u32) => {
                self.pc.mail_pda = c == 'P' as u32;
                if r.h797_addr_lists {
                    // every list emptied; the cards = the enabled AssetRefs of 0xb0ff, none selected
                    let cards = self.item_assets(ADDR_CARDS_ITEM);
                    let pc = &mut self.pc;
                    pc.addr_cards = cards.into_iter().map(|a| (a, false)).collect();
                    pc.addr_to.clear();
                    pc.addr_cc.clear();
                    pc.addr_pda.clear();
                    pc.addr_kind = 0;
                    pc.addr_target = 0;
                }
                false
            }
            // ---- H797: the address lists and EmailSend -----------------------------------------------------------
            ("CreateAddrList", &[k, ..]) if r.h797_addr_lists => {
                // the target list := the selected cards; Send usable iff To or Cc is not empty
                self.addr_kind(k as u8);
                self.addr_take_selected();
                if k as u8 != b'D' {
                    let any = !(self.pc.addr_to.is_empty() && self.pc.addr_cc.is_empty());
                    self.engine_enable(FWD_SEND, any as u8);
                }
                true
            }
            ("DisplayAddrs", &[k, ..]) if r.h797_addr_lists => {
                // every card unselected, offset 0, the markers off, the rows shown
                for c in self.pc.addr_cards.iter_mut() {
                    c.1 = false;
                }
                self.addr_kind(k as u8);
                self.pc.addr_offset = 0;
                self.addr_markers_off();
                self.addr_rows();
                true
            }
            ("ScrollAddrs", &[k, dir, ..]) if r.h797_addr_lists => {
                // only with more entries than rows; 1 = one down, else one up
                self.addr_kind(k as u8);
                let rows = addr_tables(&self.h.exe.pc, self.pc.addr_kind).map_or(0, |t| t.0.len());
                let n = self.addr_shown().len();
                if n > rows {
                    let o = self.pc.addr_offset;
                    let moved = if dir == 1 { (n - rows > o).then_some(o + 1) } else { o.checked_sub(1) };
                    if let Some(o) = moved {
                        self.pc.addr_offset = o;
                        self.addr_markers_off();
                        self.addr_rows();
                    }
                }
                true
            }
            ("AddrSelected", &[i, ..]) if r.h797_addr_lists => {
                // with markers, card offset + i toggles its flag and marker i
                let h = self.h;
                let Some((_, marks)) = addr_tables(&h.exe.pc, self.pc.addr_kind) else { return true };
                let j = self.pc.addr_offset + i as usize;
                if !marks.is_empty() && j < self.pc.addr_cards.len() {
                    let on = !self.pc.addr_cards[j].1;
                    self.pc.addr_cards[j].1 = on;
                    if let Some(&m) = marks.get(i as usize) {
                        self.engine_enable(m, on as u8);
                    }
                }
                true
            }
            ("AddrSelPDA", &[i, ..]) if r.h797_addr_lists => {
                // card i selected; the PDA list (the target after DisplayAddrs('P')) := the selected cards; every flag
                // cleared; the PDA compose screen's address row shows the recipient; then the letter's subject / body
                let i = i as usize;
                if i < self.pc.addr_cards.len() {
                    self.pc.addr_cards[i].1 = true;
                    let card = self.pc.addr_cards[i].0;
                    self.addr_kind(b'D');
                    self.addr_take_selected();
                    for c in self.pc.addr_cards.iter_mut() {
                        c.1 = false;
                    }
                    self.pc.addr_offset = 0;
                    self.addr_rows();
                    self.note_letter(card);
                }
                true
            }
            ("EmailSend", _) if r.h797_addr_lists => {
                self.email_send(action);
                true
            }
            ("DisplayInBox", _) if r.h577_mail_rows => {
                self.ab_proc(action, trigger, name, args);
                self.out.retain(|o| !matches!(o, Out::Unimplemented { action: a, .. } if *a == action));
                self.mail_redisplay();
                true
            }
            ("ScrollHeaders", _) if r.h577_mail_rows => {
                self.ab_proc(action, trigger, name, args);
                self.out.retain(|o| !matches!(o, Out::Unimplemented { action: a, .. } if *a == action));
                if self.px.mail_shown.len() > 8 {
                    self.mail_redisplay();
                }
                true
            }
            ("InitEmailRead", &[item, var, ..]) if r.h577_mail_rows => {
                self.ab_proc(action, trigger, name, args);
                // H576: the mail's text in a panel of the item (the three special mails use 0x11c36 / 0x11c39)
                let v = self.vars.get(var).unwrap_or(0) as u32;
                let item = match (v, item) {
                    (0xf14f | 0xf145 | 0xfc0f, 0xd6a6) => 0x11c36,
                    (0xf14f | 0xf145 | 0xfc0f, _) => 0x11c39,
                    _ => item,
                };
                let style = Style { wrap: true, ..self.mail_style() };
                let src = if v == 0 { Src::Str(String::new()) } else { Src::Asset(v) };
                if r.h791_text_scroll {
                    self.pc.panels.clear();
                }
                Self::pc_set(&mut self.pc.panels, item, src, style);
                self.pc.last_shown = v;
                if r.h791_text_scroll {
                    self.pc.panel_item = Some(item);
                    self.pc.scroll = 0;
                }
                true
            }
            _ => false,
        }
    }

    /// H578: where a caret item is drawn instead of its rect's top-left
    pub fn pc_item_pos(&self, item: u32) -> Option<(i32, i32)> {
        match item {
            LOGIN_CARET if self.rules.h578_carets => {
                let f = self.dev.focus?;
                let &(_, x, y) = LOGIN_CARET_BASE.iter().find(|b| b.0 == f)?;
                let n = if f == 0xc63b { self.px.user.chars().count() } else { self.px.pass.chars().count() };
                (n < 24).then_some((x + COURIER14_ADVANCE * n as i32, y))
            }
            // H575: (332 + text width, 252) while the text is shorter than 31
            ING_CARET if self.rules.h578_carets => {
                let n = self.pc.ing.chars().count();
                (n < 31).then_some((332 + COURIER14_ADVANCE * n as i32, 252))
            }
            // H790: (377 + text width, 50) while the text is shorter than 26
            MEDIA_CARET if self.rules.h790_media_index => {
                let n = self.pc.media.chars().count();
                (n < 26).then_some((377 + COURIER14_ADVANCE * n as i32, 50))
            }
            // H794: (170 + 10 n, 200) while n < 31 (SBaseInit does not move it)
            SBASE_CARET if self.rules.h794_sbase => {
                let n = self.pc.sbase.chars().count();
                (n < 31).then_some((170 + 10 * n as i32, 200))
            }
            _ => None,
        }
    }

    /// H570 / H578: the login fields' texts for the front-end: (text item, text, password masked)
    pub fn pc_login_texts(&self) -> [(u32, String); 2] {
        [(0xc67b, self.px.user.clone()), (0xc68c, "*".repeat(self.px.pass.chars().count()))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_header_lines() {
        let (f, s, rest) = mail_header("To: Craig\r\nFrom: Jim <j@x>\r\nSubject: Hi\r\n\r\nBody\r\n");
        assert_eq!((f.as_str(), s.as_str()), ("From: Jim <j@x>", "Subject: Hi"));
        assert_eq!(rest, "\nBody\n");
    }

    #[test]
    fn normalize_tabs_and_cr() {
        assert_eq!(normalize("a\tb\rc\r\nd"), "a    b\nc\nd");
    }

    #[test]
    fn phone_number_and_key_filter() {
        assert_eq!(bare_number("(206) 000-0005"), "2060000005");
        assert_eq!(last_chars("12020000006", 10), "2020000006");
        assert_eq!(ing_char('a' as u32), Some('A'));
        assert_eq!(ing_char(0xbe), None);
        assert_eq!(ing_char('.' as u32), Some('.'));
        assert_eq!(ing_char(0xbd), None);
        assert_eq!(ing_char('-' as u32), Some('-'));
        assert_eq!(ing_char(0x28), None);
    }
}
