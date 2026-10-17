//! Content tables read at runtime from the user's own `XFiles.exe`.
//!
//! The port keeps only each table's virtual address and row layout; the values (HDB object ids the developers listed
//! in the exe's read-only data) come from the exe beside the opened `XFiles.hdb`. Two builds are mapped, identified by
//! size, PE timestamp and a 64-bit FNV-1a of the whole file: the RU 1.00 exe (FileVersion 1.00.12, the traced one) and
//! the English DVD Edition's (FileVersion 2.05.00, H2300: the same tables, row for row, at other addresses — located
//! statically, not traced). Any other exe or no exe at all gives empty tables: the features that read them (ING
//! records, media search, cheat codes, phone numbers and refusals, suicide notes, e-mail replies, volume bars, the
//! media kinds of file extensions, the shootout's pick Views, the gun-hit layouts, the computer's lists, cursors, save
//! slots, note days) do nothing, and one line says so on stderr.
use std::path::Path;

/// RU 1.00 `XFiles.exe` (SHA-256 eae65c27…59ce)
const RU100_SIZE: usize = 3_360_768;
const RU100_TIMESTAMP: u32 = 0x3561_09e8;
const RU100_FNV: u64 = 0xed9a_5dbb_96ba_0e25;
/// EN DVD Edition `XFiles.exe` (`English/MININST/XFILES.EXE`, FileVersion 2.05.00; SHA-256 1c7385b1…a2ad)
const ENDVD_SIZE: usize = 3_379_200;
const ENDVD_TIMESTAMP: u32 = 0x378c_fe55;
const ENDVD_FNV: u64 = 0x7752_e441_ea65_6257;

/// A mapped PE32 image: image base + (rva, virtual size, raw size, raw offset) per section.
pub struct Image<'a> {
    d: &'a [u8],
    base: u32,
    timestamp: u32,
    secs: Vec<(u32, u32, u32, u32)>,
}

fn u16le(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32le(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

impl<'a> Image<'a> {
    pub fn new(d: &'a [u8]) -> Option<Image<'a>> {
        let pe = u32le(d, 0x3c)? as usize;
        if d.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let nsec = u16le(d, pe + 6)? as usize;
        let opt = u16le(d, pe + 20)? as usize;
        let timestamp = u32le(d, pe + 8)?;
        let base = u32le(d, pe + 24 + 28)?;
        let secs = (0..nsec)
            .map(|i| {
                let s = pe + 24 + opt + 40 * i;
                Some((u32le(d, s + 12)?, u32le(d, s + 8)?, u32le(d, s + 16)?, u32le(d, s + 20)?))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Image { d, base, timestamp, secs })
    }
    /// The dword at virtual address `va` (only bytes present in the file; .bss reads as None).
    pub fn u32_at(&self, va: u32) -> Option<u32> {
        let rva = va.checked_sub(self.base)?;
        let &(sva, _, rs, ro) = self.secs.iter().find(|&&(sva, vs, rs, _)| sva <= rva && rva + 4 <= sva + vs.max(rs))?;
        let o = rva - sva;
        if o + 4 > rs {
            return None;
        }
        u32le(self.d, (ro + o) as usize)
    }
    /// The NUL-terminated byte string at virtual address `va` (at most 256 bytes, all present in the file).
    pub fn cstr_at(&self, va: u32) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for k in 0..256u32 {
            let rva = (va + k).checked_sub(self.base)?;
            let &(sva, _, _, ro) = self.secs.iter().find(|&&(sva, _, rs, _)| sva <= rva && rva < sva + rs)?;
            match *self.d.get((ro + rva - sva) as usize)? {
                0 => return Some(out),
                b => out.push(b),
            }
        }
        None
    }
    /// `n` rows of `W` dwords from `va`, rows `stride` bytes apart.
    pub fn rows<const W: usize>(&self, va: u32, n: usize, stride: u32) -> Option<Vec<[u32; W]>> {
        (0..n as u32)
            .map(|r| {
                let mut row = [0u32; W];
                for (k, x) in row.iter_mut().enumerate() {
                    *x = self.u32_at(va + r * stride + 4 * k as u32)?;
                }
                Some(row)
            })
            .collect()
    }
}

/// An identified image: table reads take the RU 1.00 address and read the build's own ([`Build::va`]).
struct Tables<'a> {
    img: Image<'a>,
    build: Build,
}

impl Tables<'_> {
    fn rows<const W: usize>(&self, ru: u32, n: usize, stride: u32) -> Option<Vec<[u32; W]>> {
        self.img.rows(self.build.va(ru)?, n, stride)
    }
    /// the text at this build's address of the RU 1.00 text at `ru`
    fn text_at(&self, ru: u32) -> Option<Vec<u8>> {
        self.img.cstr_at(self.build.va(ru)?)
    }
}

fn fnv1a(d: &[u8]) -> u64 {
    d.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// The build an exe is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// RU 1.00 (the addresses in `va` / `pc_va`)
    Ru100,
    /// the English DVD Edition, 2.05.00 (the RU addresses moved by [`EN_DVD_VA`])
    EnDvd,
}

/// H2300 (static): RU 1.00 table address → the EN DVD Edition's. Each EN table holds the RU table's dwords unchanged
/// (string pointers aside); located by the code that references it (the RU code around each reference, relocated
/// dwords and call / jump offsets masked, matched once in the EN `.text`) and by the table's own bytes.
const EN_DVD_VA: &[(u32, u32)] = &[
    (0x659ac0, 0x65e1a0), // ING_RECORDS
    (0x658da8, 0x65d148), // MEDIA_KEYS
    (0x6548d0, 0x65bdb8), // CHEATS
    (0x656040, 0x65ecb0), // PHONES
    (0x659a00, 0x657930), // SUICIDE_NOTES
    (0x659f70, 0x65e650), // ING_REPLIES
    (0x658de8, 0x65d188), // MEDIA_REPLIES
    (0x658518, 0x658f88), // NOTE_REPLIES
    (0x6549a8, 0x65be90), // MEL_SHOT_LAYOUTS
    (0x6549a0, 0x65be88), // SUICIDE_NOTE_LAYOUTS
    (0x6599d0, 0x657900), // SMOL_PICK_VIEWS
    (0x654c10, 0x65a558), // CURSORS
    (0x65b768, 0x658e48), // SAVE_SLOT_ITEMS
    (0x65b778, 0x658e58), // LOAD_SLOT_ITEMS
    (0x6584b8, 0x658f28), // NOTE_DAYS (H2302: the EN code reads it through a day-order table)
    (0x658520, 0x658f90), // NOTE_KEYS
    (0x658620, 0x659090), // NOTE_DAY3_EXTRA
    (0x655fa0, 0x6581e0), // VOLUME_BARS
    (0x659aa0, 0x65e180), // ING_VALUE_ITEMS / ING_VALUES
    (0x659a80, 0x65e160), // ING_LABEL_ITEMS / ING_LABELS
    (0x6560e0, 0x65ed50), // PHONE_REFUSALS
    (0x656100, 0x65ed70), // PHONE_LONG_DISTANCE
    (0x65c380, 0x6609f8), // EXT_KINDS
    (0x659a10, 0x65e0f0), // ING_PHONES
    (0x659a70, 0x65e150), // ING_PLATES
    (0x657660, 0x65dc40), // SBASE_DOTS
    (0x6581e0, 0x657940), // PHOTO_SLOTS
    (0x658628, 0x659098), // NOTE_LETTERS
    (0x6587e8, 0x6584d0), // RECIPIENTS
    (0x6b1d40, 0x6b63a0), // ENCLOSURES (.data, +0x4660 as all the .data tables below)
    (0x6b1cf0, 0x6b6350), // ENCLOSURE_EXT
    (0x6b1e30, 0x6b6490), // ADDR_TO_BOX
    (0x6b1e40, 0x6b64a0), // ADDR_CC_BOX
    (0x6b1e4c, 0x6b64ac), // ADDR_PDA_ONE
    (0x6b1e50, 0x6b64b0), // ADDR_PDA_ROWS
    (0x6b1e78, 0x6b64d8), // ADDR_TO_ROWS
    (0x6b1e90, 0x6b64f0), // ADDR_TO_MARKS
    (0x6b1ea8, 0x6b6508), // ADDR_CC_ROWS
    (0x6b1ec0, 0x6b6520), // ADDR_CC_MARKS
    (0x6b1d70, 0x6b63d0), // MAIL_ROWS_PC
    (0x6b1dd0, 0x6b6430), // MAIL_ROWS_PDA
];

/// H2301 (static): the EN build has no "Article" / "Field Notes" literals; its code shows an AssetRef's text instead
/// (kind 0, a string of `XfilesT.dll`): these two
const ENDVD_ARTICLE_SUBJECT_ASSET: u32 = 0x172a1;
const ENDVD_FIELD_NOTES_ASSET: u32 = 0x10767;

impl Build {
    /// The address of the RU 1.00 table at `ru` in this build
    fn va(self, ru: u32) -> Option<u32> {
        match self {
            Build::Ru100 => Some(ru),
            Build::EnDvd => EN_DVD_VA.iter().find(|&&(r, _)| r == ru).map(|&(_, e)| e),
        }
    }
}

pub fn identify(d: &[u8]) -> Result<Build, String> {
    let img = Image::new(d).ok_or("not a PE32 image")?;
    if d.len() == RU100_SIZE && img.timestamp == RU100_TIMESTAMP && fnv1a(d) == RU100_FNV {
        return Ok(Build::Ru100);
    }
    if d.len() == ENDVD_SIZE && img.timestamp == ENDVD_TIMESTAMP && fnv1a(d) == ENDVD_FNV {
        return Ok(Build::EnDvd);
    }
    Err(format!(
        "unknown build (size {}, PE timestamp {:#x}, fnv {:#x}); only RU 1.00 and the EN DVD Edition are mapped",
        d.len(),
        img.timestamp,
        fnv1a(d)
    ))
}

/// Virtual addresses (RU 1.00; other builds: [`Build::va`]) and row layouts. Values: see the tables' users and RE_NOTES.
mod va {
    /// H575: (name-key item, record item, photo item, frame item, field notes AssetRef) × 60
    pub const ING_RECORDS: (u32, usize) = (0x659ac0, 60);
    /// H790: the media index's 16 key items
    pub const MEDIA_KEYS: (u32, usize) = (0x658da8, 16);
    /// H191: (trigger, action list) per cheat code (the codes are item 0x1190d's texts in the HDB)
    pub const CHEATS: (u32, usize) = (0x6548d0, 26);
    /// (number text AssetRef, trigger, action list) per phone number
    pub const PHONES: (u32, usize) = (0x656040, 13);
    /// H1863: the three suicide-note AssetRefs (Suicide1..3.txt)
    pub const SUICIDE_NOTES: (u32, usize) = (0x659a00, 3);
    /// H797: ING record name AssetRef → reaction item
    pub const ING_REPLIES: (u32, usize) = (0x659f70, 18);
    /// H797: media article → reaction item
    pub const MEDIA_REPLIES: (u32, usize) = (0x658de8, 10);
    /// H797: note pad day item → reaction item (stride 12: the third dword is not read here)
    pub const NOTE_REPLIES: (u32, usize) = (0x658518, 22);
    /// the "Mel shot" layouts (hotspot 666)
    pub const MEL_SHOT_LAYOUTS: (u32, usize) = (0x6549a8, 5);
    /// the suicide-note layouts (hotspots 100 / 500)
    pub const SUICIDE_NOTE_LAYOUTS: (u32, usize) = (0x6549a0, 2);
    /// H730: each Smolnikoff goon's Views by pick (stride 16: the fourth dword is unused)
    pub const SMOL_PICK_VIEWS: (u32, usize) = (0x6599d0, 3);
    /// H69: the VCCursor ids the application builds at start
    pub const CURSORS: (u32, usize) = (0x654c10, 12);
    /// H72 / H142: the save screen's three slot text items
    pub const SAVE_SLOT_ITEMS: (u32, usize) = (0x65b768, 3);
    /// H72: the load screen's three slot text items
    pub const LOAD_SLOT_ITEMS: (u32, usize) = (0x65b778, 3);
    /// H624: the note pad's day items (InterfaceItems whose AssetRefList holds a day's notes)
    pub const NOTE_DAYS: (u32, usize) = (0x6584b8, 23);
    /// H624: each day's key note (stride 12, first dword; the last row is not an AssetRef)
    pub const NOTE_KEYS: (u32, usize) = (0x658520, 23);
    /// H624: the two extra notes day 3's address link needs
    pub const NOTE_DAY3_EXTRA: (u32, usize) = (0x658620, 2);
    /// H189: the four volume bars' level pictures (dialog, music, effect, game) × 10 levels
    pub const VOLUME_BARS: (u32, usize) = (0x655fa0, 4);
    /// H190: the ING record's eight value text items and their eight label items
    pub const ING_VALUE_ITEMS: (u32, usize) = (0x659aa0, 8);
    pub const ING_LABEL_ITEMS: (u32, usize) = (0x659a80, 8);
    /// H166: the phone's four refusal recordings (trigger, action list), then the long-distance recording
    pub const PHONE_REFUSALS: (u32, usize) = (0x6560e0, 4);
    pub const PHONE_LONG_DISTANCE: (u32, usize) = (0x656100, 1);
    /// H51: media file extension (string pointer) → media kind
    pub const EXT_KINDS: (u32, usize) = (0x65c380, 45);
}

/// The typed tables. Empty when no known exe was found (the features that use them then do nothing).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExeTables {
    pub build: Option<Build>,
    pub ing_records: Vec<[u32; 5]>,
    pub media_keys: Vec<u32>,
    pub cheats: Vec<(u32, u32)>,
    pub phones: Vec<(u32, u32, u32)>,
    pub suicide_notes: Vec<u32>,
    pub ing_replies: Vec<(u32, u32)>,
    pub media_replies: Vec<(u32, u32)>,
    pub note_replies: Vec<(u32, u32)>,
    pub mel_shot_layouts: Vec<u32>,
    pub suicide_note_layouts: Vec<u32>,
    pub smol_pick_views: Vec<[u32; 3]>,
    /// the FBI computer's item lists and directories (world/computer.rs)
    pub pc: PcTables,
    pub cursors: Vec<u32>,
    pub save_slot_items: Vec<u32>,
    pub load_slot_items: Vec<u32>,
    pub note_days: Vec<u32>,
    pub note_keys: Vec<u32>,
    pub note_day3_extra: Vec<u32>,
    pub volume_bars: Vec<[u32; 10]>,
    pub ing_value_items: Vec<u32>,
    pub ing_label_items: Vec<u32>,
    pub phone_refusals: Vec<(u32, u32)>,
    pub phone_long_distance: Option<(u32, u32)>,
    /// (upper-case extension, media kind)
    pub ext_kinds: Vec<(String, u32)>,
}

impl ExeTables {
    /// The tables of an exe image; Err = not a known build (or a table outside the file).
    pub fn parse(d: &[u8]) -> Result<ExeTables, String> {
        let build = identify(d)?;
        let img = Tables { img: Image::new(d).ok_or("not a PE32 image")?, build };
        let bad = |what: &str| format!("table {what} is outside the image");
        let col = |(a, n): (u32, usize), what: &str| -> Result<Vec<u32>, String> {
            Ok(img.rows::<1>(a, n, 4).ok_or_else(|| bad(what))?.into_iter().map(|r| r[0]).collect())
        };
        let pairs = |(a, n): (u32, usize), stride: u32, what: &str| -> Result<Vec<(u32, u32)>, String> {
            Ok(img.rows::<2>(a, n, stride).ok_or_else(|| bad(what))?.into_iter().map(|r| (r[0], r[1])).collect())
        };
        Ok(ExeTables {
            build: Some(build),
            ing_records: img.rows::<5>(va::ING_RECORDS.0, va::ING_RECORDS.1, 20).ok_or_else(|| bad("ING_RECORDS"))?,
            media_keys: col(va::MEDIA_KEYS, "MEDIA_KEYS")?,
            cheats: pairs(va::CHEATS, 8, "CHEATS")?,
            phones: img
                .rows::<3>(va::PHONES.0, va::PHONES.1, 12)
                .ok_or_else(|| bad("PHONES"))?
                .into_iter()
                .map(|r| (r[0], r[1], r[2]))
                .collect(),
            suicide_notes: col(va::SUICIDE_NOTES, "SUICIDE_NOTES")?,
            ing_replies: pairs(va::ING_REPLIES, 8, "ING_REPLIES")?,
            media_replies: pairs(va::MEDIA_REPLIES, 8, "MEDIA_REPLIES")?,
            note_replies: pairs(va::NOTE_REPLIES, 12, "NOTE_REPLIES")?,
            mel_shot_layouts: col(va::MEL_SHOT_LAYOUTS, "MEL_SHOT_LAYOUTS")?,
            suicide_note_layouts: col(va::SUICIDE_NOTE_LAYOUTS, "SUICIDE_NOTE_LAYOUTS")?,
            smol_pick_views: img
                .rows::<3>(va::SMOL_PICK_VIEWS.0, va::SMOL_PICK_VIEWS.1, 16)
                .ok_or_else(|| bad("SMOL_PICK_VIEWS"))?,
            pc: PcTables::parse(&img)?,
            cursors: col(va::CURSORS, "CURSORS")?,
            save_slot_items: col(va::SAVE_SLOT_ITEMS, "SAVE_SLOT_ITEMS")?,
            load_slot_items: col(va::LOAD_SLOT_ITEMS, "LOAD_SLOT_ITEMS")?,
            note_days: col(va::NOTE_DAYS, "NOTE_DAYS")?,
            note_keys: img
                .rows::<1>(va::NOTE_KEYS.0, va::NOTE_KEYS.1, 12)
                .ok_or_else(|| bad("NOTE_KEYS"))?
                .into_iter()
                .map(|r| r[0])
                .collect(),
            note_day3_extra: col(va::NOTE_DAY3_EXTRA, "NOTE_DAY3_EXTRA")?,
            volume_bars: img.rows::<10>(va::VOLUME_BARS.0, va::VOLUME_BARS.1, 40).ok_or_else(|| bad("VOLUME_BARS"))?,
            ing_value_items: col(va::ING_VALUE_ITEMS, "ING_VALUE_ITEMS")?,
            ing_label_items: col(va::ING_LABEL_ITEMS, "ING_LABEL_ITEMS")?,
            phone_refusals: pairs(va::PHONE_REFUSALS, 8, "PHONE_REFUSALS")?,
            phone_long_distance: pairs(va::PHONE_LONG_DISTANCE, 8, "PHONE_LONG_DISTANCE")?.first().copied(),
            ext_kinds: img
                .rows::<2>(va::EXT_KINDS.0, va::EXT_KINDS.1, 8)
                .ok_or_else(|| bad("EXT_KINDS"))?
                .into_iter()
                .map(|[p, k]| Some((String::from_utf8(img.img.cstr_at(p)?).ok()?, k)))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| bad("EXT_KINDS strings"))?,
        })
    }

    /// The tables of `XFiles.exe` in `dir` (any letter case); empty, with one stderr line, when it is missing or not
    /// a known build.
    pub fn beside(dir: &Path) -> ExeTables {
        let exe = crate::io::read_dir(dir).ok().and_then(|es| {
            es.into_iter().find(|e| !e.is_dir && e.name.eq_ignore_ascii_case("XFiles.exe")).map(|e| e.path)
        });
        let Some(p) = exe else {
            eprintln!("xf: no XFiles.exe in {}: the exe's tables are empty (ING, media search, cheats, phone, notes, e-mail replies disabled)", dir.display());
            return ExeTables::default();
        };
        match crate::io::read(&p).map_err(|e| e.to_string()).and_then(|d| ExeTables::parse(&d)) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("xf: {}: {e}: the exe's tables are empty (ING, media search, cheats, phone, notes, e-mail replies disabled)", p.display());
                ExeTables::default()
            }
        }
    }
}

/// The FBI computer's tables (RU 1.00 addresses, other builds: [`Build::va`]; values: RE_NOTES H575 / H577 / H793 / H794 /
/// H796 / H797)
mod pc_va {
    /// H575: (phone-number text, target) × 12 — target = a record position, or −1 the warehouse, −2 Seattle PD
    pub const ING_PHONES: (u32, usize) = (0x659a10, 12);
    /// H575: rental plate text, its file text, NSA plate text, the "restricted" text
    pub const ING_PLATES: (u32, usize) = (0x659a70, 4);
    /// H575: the record view's 8 label items and 8 value items
    pub const ING_LABELS: (u32, usize) = (0x659a80, 8);
    pub const ING_VALUES: (u32, usize) = (0x659aa0, 8);
    /// H794: SBase's dot items
    pub const SBASE_DOTS: (u32, usize) = (0x657660, 31);
    /// H793: the album's thumbnail slots (row-major, 5 per row)
    pub const PHOTO_SLOTS: (u32, usize) = (0x6581e0, 20);
    /// H797: (note-pad day index, recipient card, body text, subject text), 16 bytes a row
    pub const NOTE_LETTERS: (u32, usize) = (0x658628, 7);
    /// H797: the recipients' address cards (position = reply index)
    pub const RECIPIENTS: (u32, usize) = (0x6587e8, 9);
    /// H796: pointers to the 9 enclosure names; the ".DAT" suffix; the article subject word
    pub const ENCLOSURES: (u32, usize) = (0x6b1d40, 9);
    pub const ENCLOSURE_EXT: u32 = 0x6b1cf0;
    pub const ARTICLE_SUBJECT: u32 = 0x6b1ed8;
    /// H797: the note-pad letter's fallback subject
    pub const FIELD_NOTES: u32 = 0x6b1a58;
    /// H797: address rows / markers per picker kind
    pub const ADDR_TO_BOX: (u32, usize) = (0x6b1e30, 3);
    pub const ADDR_CC_BOX: (u32, usize) = (0x6b1e40, 3);
    pub const ADDR_PDA_ONE: (u32, usize) = (0x6b1e4c, 1);
    pub const ADDR_PDA_ROWS: (u32, usize) = (0x6b1e50, 9);
    pub const ADDR_TO_ROWS: (u32, usize) = (0x6b1e78, 5);
    pub const ADDR_TO_MARKS: (u32, usize) = (0x6b1e90, 5);
    pub const ADDR_CC_ROWS: (u32, usize) = (0x6b1ea8, 5);
    pub const ADDR_CC_MARKS: (u32, usize) = (0x6b1ec0, 5);
    /// H577: inbox rows (From × 8, Subject × 8, read marker × 8) of the computer and of the PDA
    pub const MAIL_ROWS_PC: u32 = 0x6b1d70;
    pub const MAIL_ROWS_PDA: u32 = 0x6b1dd0;
}

/// The FBI computer's lists (empty without a known exe)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PcTables {
    pub ing_phones: Vec<(u32, i32)>,
    /// (rental plate, rental file, NSA plate, restricted text); empty when missing
    pub ing_plates: Vec<u32>,
    pub ing_labels: Vec<u32>,
    pub ing_values: Vec<u32>,
    pub sbase_dots: Vec<u32>,
    pub photo_slots: Vec<u32>,
    pub note_letters: Vec<[u32; 4]>,
    pub recipients: Vec<u32>,
    /// the enclosure file names, ".DAT" already appended
    pub enclosures: Vec<String>,
    /// the media Forward's subject: a literal (RU 1.00) or an AssetRef's text (EN, H2301)
    pub article_subject: String,
    pub article_subject_asset: Option<u32>,
    /// the note-pad letter's fallback subject: a literal (RU 1.00) or an AssetRef's text (EN, H2301)
    pub field_notes: String,
    pub field_notes_asset: Option<u32>,
    pub addr_to_box: Vec<u32>,
    pub addr_cc_box: Vec<u32>,
    pub addr_pda_one: Vec<u32>,
    pub addr_pda_rows: Vec<u32>,
    pub addr_to_rows: Vec<u32>,
    pub addr_to_marks: Vec<u32>,
    pub addr_cc_rows: Vec<u32>,
    pub addr_cc_marks: Vec<u32>,
    /// [From items, Subject items, read markers] × 8
    pub mail_rows_pc: Vec<[u32; 8]>,
    pub mail_rows_pda: Vec<[u32; 8]>,
}

impl PcTables {
    fn parse(img: &Tables) -> Result<PcTables, String> {
        use pc_va::*;
        let outside = |what: &str| format!("table {what} is outside the image");
        let col = |(a, n): (u32, usize), what: &str| -> Result<Vec<u32>, String> {
            Ok(img.rows::<1>(a, n, 4).ok_or_else(|| outside(what))?.into_iter().map(|r| r[0]).collect())
        };
        let text = |va: u32, what: &str| -> Result<String, String> {
            img.text_at(va).map(|b| String::from_utf8_lossy(&b).into_owned()).ok_or_else(|| outside(what))
        };
        let ext = text(ENCLOSURE_EXT, "ENCLOSURE_EXT")?;
        // the enclosure names: pointers (already this build's addresses)
        let enclosures = col(ENCLOSURES, "ENCLOSURES")?
            .into_iter()
            .map(|p| img.img.cstr_at(p).map(|b| String::from_utf8_lossy(&b).into_owned() + &ext).ok_or_else(|| outside("ENCLOSURES")))
            .collect::<Result<_, _>>()?;
        // H2301: a literal in RU 1.00, an AssetRef in the EN build
        let (article_subject, article_subject_asset, field_notes, field_notes_asset) = match img.build {
            Build::Ru100 => (text(ARTICLE_SUBJECT, "ARTICLE_SUBJECT")?, None, text(FIELD_NOTES, "FIELD_NOTES")?, None),
            Build::EnDvd => (String::new(), Some(ENDVD_ARTICLE_SUBJECT_ASSET), String::new(), Some(ENDVD_FIELD_NOTES_ASSET)),
        };
        let mail = |va: u32, what: &str| img.rows::<8>(va, 3, 32).ok_or_else(|| outside(what));
        Ok(PcTables {
            ing_phones: img
                .rows::<2>(ING_PHONES.0, ING_PHONES.1, 8)
                .ok_or_else(|| outside("ING_PHONES"))?
                .into_iter()
                .map(|r| (r[0], r[1] as i32))
                .collect(),
            ing_plates: col(ING_PLATES, "ING_PLATES")?,
            ing_labels: col(ING_LABELS, "ING_LABELS")?,
            ing_values: col(ING_VALUES, "ING_VALUES")?,
            sbase_dots: col(SBASE_DOTS, "SBASE_DOTS")?,
            photo_slots: col(PHOTO_SLOTS, "PHOTO_SLOTS")?,
            note_letters: img.rows::<4>(NOTE_LETTERS.0, NOTE_LETTERS.1, 16).ok_or_else(|| outside("NOTE_LETTERS"))?,
            recipients: col(RECIPIENTS, "RECIPIENTS")?,
            enclosures,
            article_subject,
            article_subject_asset,
            field_notes,
            field_notes_asset,
            addr_to_box: col(ADDR_TO_BOX, "ADDR_TO_BOX")?,
            addr_cc_box: col(ADDR_CC_BOX, "ADDR_CC_BOX")?,
            addr_pda_one: col(ADDR_PDA_ONE, "ADDR_PDA_ONE")?,
            addr_pda_rows: col(ADDR_PDA_ROWS, "ADDR_PDA_ROWS")?,
            addr_to_rows: col(ADDR_TO_ROWS, "ADDR_TO_ROWS")?,
            addr_to_marks: col(ADDR_TO_MARKS, "ADDR_TO_MARKS")?,
            addr_cc_rows: col(ADDR_CC_ROWS, "ADDR_CC_ROWS")?,
            addr_cc_marks: col(ADDR_CC_MARKS, "ADDR_CC_MARKS")?,
            mail_rows_pc: mail(MAIL_ROWS_PC, "MAIL_ROWS_PC")?,
            mail_rows_pda: mail(MAIL_ROWS_PDA, "MAIL_ROWS_PDA")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> Option<Vec<u8>> {
        std::fs::read(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame/XFiles.exe")).ok()
    }

    #[test]
    fn ru100_is_identified_and_tables_load() {
        let Some(d) = exe() else { return };
        let t = ExeTables::parse(&d).expect("RU 1.00");
        assert_eq!(t.build, Some(Build::Ru100));
        assert_eq!(t.ing_records.len(), 60);
        assert_eq!(t.media_keys.len(), 16);
        assert_eq!(t.cheats.len(), 26);
        assert_eq!(t.phones.len(), 13);
        assert_eq!(t.suicide_notes.len(), 3);
        assert_eq!((t.ing_replies.len(), t.media_replies.len(), t.note_replies.len()), (18, 10, 22));
        assert_eq!((t.mel_shot_layouts.len(), t.suicide_note_layouts.len(), t.smol_pick_views.len()), (5, 2, 3));
        let pc = &t.pc;
        assert_eq!((pc.ing_phones.len(), pc.ing_plates.len(), pc.ing_labels.len(), pc.ing_values.len()), (12, 4, 8, 8));
        assert_eq!((pc.sbase_dots.len(), pc.photo_slots.len(), pc.note_letters.len(), pc.recipients.len()), (31, 20, 7, 9));
        assert_eq!(pc.enclosures.len(), 9);
        assert!(pc.enclosures.iter().all(|e| e.len() == 11 && e.ends_with(".DAT")));
        assert_eq!((pc.mail_rows_pc.len(), pc.mail_rows_pda.len()), (3, 3));
        assert!(!pc.article_subject.is_empty() && !pc.field_notes.is_empty());
        assert_eq!((t.cursors.len(), t.save_slot_items.len(), t.load_slot_items.len()), (12, 3, 3));
        assert_eq!((t.note_days.len(), t.note_keys.len(), t.note_day3_extra.len()), (23, 23, 2));
        assert_eq!((t.volume_bars.len(), t.ing_value_items.len(), t.ing_label_items.len()), (4, 8, 8));
        assert_eq!((t.phone_refusals.len(), t.ext_kinds.len()), (4, 45));
        assert!(t.phone_long_distance.is_some());
        // the extension table is text (upper-case names) and holds the video kinds H51 relies on
        assert!(t.ext_kinds.iter().all(|(e, _)| !e.is_empty() && e.bytes().all(|b| b.is_ascii_uppercase())));
        assert!(t.ext_kinds.iter().any(|(e, k)| e == "NMV" && *k == 0x16));
    }

    #[test]
    fn other_exe_gives_no_tables() {
        let Some(mut d) = exe() else { return };
        let n = d.len();
        d[n - 1] ^= 0xff;
        assert!(ExeTables::parse(&d).is_err());
        assert!(ExeTables::parse(b"MZ").is_err());
    }
    /// The tables beside the HDB refer to the HDB's objects: the notes and the phone numbers are AssetRefs
    #[test]
    fn tables_beside_the_hdb_point_into_it() {
        use crate::{hdb::Hdb, world::class};
        let d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame/XFiles.hdb");
        let Ok(h) = Hdb::open(d) else { return };
        assert_eq!(h.exe.build, Some(Build::Ru100));
        assert!(h.exe.suicide_notes.iter().all(|&a| h.has(class::ASSET_REF, a)));
        assert!(h.exe.phones.iter().all(|&(a, _, _)| h.has(class::ASSET_REF, a)));
        assert!(h.exe.ing_records.iter().filter(|r| r[4] != 0).all(|r| h.has(class::ASSET_REF, r[4])));
        let layouts = h.exe.mel_shot_layouts.iter().chain(&h.exe.suicide_note_layouts);
        assert!(layouts.into_iter().all(|&l| h.has(class::IFACE_LAYOUT, l)));
        assert!(h.exe.smol_pick_views.iter().flatten().all(|&v| h.has(class::VIEW, v)));
    }
}
