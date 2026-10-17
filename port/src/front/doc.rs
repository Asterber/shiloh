//! Document text: XT/*.xtx texts (journal and Bible pages, notes, letters, e-mails) drawn into the InterfaceItem that
//! shows them, as the original's text object does (static decode, RE_NOTES H530–H536 — hypotheses, no trace).
//!
//! The original (all static, `XFiles.exe`):
//! - a text AssetRef shown by an item stores the item's text settings (colour, font style, shadow, alignment, wrap
//!   mode); the item's display routine (FUN_0040b6c0, case 3) hands them to the text object and sets its draw rect to
//!   the item's region;
//! - the text object draws with **DrawTextA(format = alignment | DT_WORDBREAK | DT_NOPREFIX)** (FUN_004052c0),
//!   alignment 0 → DT_CENTER, 1 → DT_LEFT, 2 → DT_RIGHT, transparent background, into the item's rect
//!   (DrawText clips to it); with the shadow flag first in black one pixel right and down;
//! - fonts: CreateFontIndirectA(lfHeight = −size, weight 400, no italic, DEFAULT_QUALITY) (FUN_00430c00); the journal
//!   style = "ZonkersHand" 14 (Jrn.ttr), the Bible style = Arial 12 (BibleInit); both pass black, no shadow, left,
//!   wrap mode 1 (JournalFlip / BibleInit push 1, 1, 0, style, colour). The game registers Dlg / Jrn / Phn / Hcd .ttr
//!   with AddFontResourceA.
//!
//! The port never ships game fonts: the .ttr files are read from the install directory at run time.
use super::text::Face;
use crate::media::MediaLib;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Text alignment of the item: 0 centre, 1 left, 2 right (the draw routine's DT_ mapping).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Center,
    Left,
    Right,
}

/// Font faces the documents use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DocFont {
    /// Jrn.ttr, "ZonkersHand" (handwriting: the journal, the suicide notes)
    Jrn,
    /// Hcd.ttr, "Typist" (typewriter; used by an app object whose documents are not decoded)
    Hcd,
    /// Arial (the font-style switch's case 3)
    Arial,
    /// Courier New (case 4)
    Courier,
    /// GetStockObject(SYSTEM_FONT) (case 0): an item's default style. GUESS for its
    /// looks under Wine: Arial bold 16 px cell (H534).
    System,
}

/// The text settings an item hands to its text object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocStyle {
    pub font: DocFont,
    /// lfHeight = −size (character height in pixels)
    pub size: u32,
    pub colour: [u8; 3],
    pub shadow: bool,
    pub align: Align,
}

/// HYPOTHESIS H531: the journal pages (JournalInit / JournalFlip into items 0xb522 / 0xb523)
pub const JOURNAL: DocStyle =
    DocStyle { font: DocFont::Jrn, size: 14, colour: [0, 0, 0], shadow: false, align: Align::Left };
/// HYPOTHESIS H532: the Bible pages (BibleInit / BibleFlip into items 0x13be4 / 0x13be5)
pub const BIBLE: DocStyle =
    DocStyle { font: DocFont::Arial, size: 12, colour: [0, 0, 0], shadow: false, align: Align::Left };
/// HYPOTHESIS H534: an item's default text settings (colour 0x12 black, stock SYSTEM_FONT, no shadow,
/// left) — used for text files shown by an item's own asset list
pub const DEFAULT: DocStyle =
    DocStyle { font: DocFont::System, size: 16, colour: [0, 0, 0], shadow: false, align: Align::Left };

/// The text settings of `item` when it shows a text file: the procedures that set them (H531 / H532), else the item's
/// default (H534).
pub fn item_style(item: u32) -> DocStyle {
    match item {
        0xb522 | 0xb523 => JOURNAL,
        0x13be4 | 0x13be5 => BIBLE,
        _ => DEFAULT,
    }
}

/// A face at a pixel size with GDI's text metrics.
pub struct DocFace {
    pub face: Face,
    /// tmHeight = tmAscent + tmDescent: DrawText's line advance (no DT_EXTERNALLEADING)
    pub line_h: i32,
}

/// H891: the font's VDMX record for `ppem` (first ratio group for a 1:1 device or any ratio): (yMax, −yMin). GDI (and
/// Wine's freetype backend, load_VDMX) take tmAscent / tmDescent from it when present — Arial 12 ppem: 12 / 3 →
/// tmHeight 15 (R34), not the OS/2 scaling's 11 + 3.
fn vdmx_metrics(data: &[u8], ppem: u32) -> Option<(i32, i32)> {
    let be16 = |o: usize| data.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let n = be16(4)? as usize;
    let off = (0..n).find_map(|i| {
        let r = 12 + 16 * i;
        (data.get(r..r + 4)? == b"VDMX").then(|| u32::from_be_bytes(data[r + 8..r + 12].try_into().unwrap()) as usize)
    })?;
    let ratios = be16(off + 4)? as usize;
    for r in 0..ratios {
        let rr = off + 6 + 4 * r;
        let (x, ys, ye) = (*data.get(rr + 1)?, *data.get(rr + 2)?, *data.get(rr + 3)?);
        if !(x == 0 || (ys <= x && x <= ye)) {
            continue; // Windows matches the device ratio 1:1 (or a record for every ratio, xRatio 0)
        }
        let g = off + be16(off + 6 + 4 * ratios + 2 * r)? as usize;
        let recs = be16(g)? as usize;
        for k in 0..recs {
            let e = g + 4 + 6 * k;
            if be16(e)? as u32 == ppem {
                return Some((be16(e + 2)? as i16 as i32, -(be16(e + 4)? as i16 as i32)));
            }
        }
        return None;
    }
    None
}

/// GDI-style metrics: the VDMX record for the ppem if the font has one (H891), else from the OS/2 table: tmAscent =
/// round(usWinAscent × ppem / upem), tmDescent likewise.
fn gdi_metrics(data: &[u8], px: f32) -> Option<(i32, i32)> {
    use skrifa::raw::TableProvider;
    if px.fract() == 0.0 {
        if let Some(m) = vdmx_metrics(data, px as u32) {
            return Some(m);
        }
    }
    let f = skrifa::FontRef::new(data).ok()?;
    let upem = f.head().ok()?.units_per_em() as f32;
    let os2 = f.os2().ok()?;
    let s = px / upem;
    Some(((os2.us_win_ascent() as f32 * s).round() as i32, (os2.us_win_descent() as f32 * s).round() as i32))
}

fn system_font_files(font: DocFont) -> &'static [&'static str] {
    match font {
        DocFont::Arial => &["Arial.ttf", "arial.ttf"],
        DocFont::Courier => &["Courier New.ttf", "cour.ttf", "Courier_New.ttf"],
        DocFont::System => &["Arial Bold.ttf", "arialbd.ttf", "Arial.ttf", "arial.ttf"],
        DocFont::Jrn | DocFont::Hcd => &[],
    }
}

/// Bytes of the font file: the game's .ttr from the install dir, system fonts from the font directories.
fn font_bytes(lib: &MediaLib, font: DocFont, alt: Option<&std::path::Path>) -> Option<Vec<u8>> {
    if let Some(n) = game_font_file(font) {
        return crate::io::read(alt.or_else(|| lib.find(n).map(|p| p.as_path()))?).ok();
    }
    // the same dirs as the UI faces (text.rs): XF_FONT_DIR, the web front's `fonts/` (xf-serve), the system dirs
    let dirs = super::text::font_dirs();
    system_font_files(font)
        .iter()
        .flat_map(|f| dirs.iter().map(move |d| d.join(f)))
        .find_map(|p| crate::io::read(&p).ok())
}

/// The game's own font file of `font` (.ttr in the install dir), None for system fonts.
fn game_font_file(font: DocFont) -> Option<&'static str> {
    match font {
        DocFont::Jrn => Some("Jrn.ttr"),
        DocFont::Hcd => Some("Hcd.ttr"),
        _ => None,
    }
}

/// The face for `font` at `size` px (cached; None if the file is missing).
pub fn face(lib: &MediaLib, font: DocFont, size: u32) -> Option<&'static DocFace> {
    face_from(lib, font, size, None)
}

/// [`face`] in the session's game language: H1835 — the EN .ttr fonts are other files than the RU ones (other upem /
/// metrics, no Cyrillic), so English texts use the English data set's font.
pub fn face_lang(s: &super::Session, font: DocFont, size: u32) -> Option<&'static DocFace> {
    let alt = match (s.game_lang, &s.en_data, game_font_file(font)) {
        (crate::media::lang::GameLang::En, Some(en), Some(n)) => en.index.find(n).cloned(),
        _ => None,
    };
    face_from(&s.lib, font, size, alt)
}

fn face_from(lib: &MediaLib, font: DocFont, size: u32, alt: Option<std::path::PathBuf>) -> Option<&'static DocFace> {
    // keyed by the font file itself: the RU and the EN data sets' .ttr differ (H1835)
    let alt = alt.or_else(|| game_font_file(font).and_then(|n| lib.find(n).cloned()));
    static CACHE: OnceLock<Mutex<HashMap<(DocFont, u32, Option<std::path::PathBuf>), Option<&'static DocFace>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().ok()?;
    *c.entry((font, size, alt.clone())).or_insert_with(|| {
        let data = font_bytes(lib, font, alt.as_deref())?;
        // SYSTEM_FONT: a 16-px cell font (lfHeight > 0 = cell height) — the em is smaller; GUESS 13 px em
        let px = if font == DocFont::System { size as f32 * 0.8 } else { size as f32 };
        let (asc, desc) = gdi_metrics(&data, px)?;
        let mut face = Face::from_bytes(data, px)?;
        face.ascent = asc;
        face.proportional = true;
        Some(&*Box::leak(Box::new(DocFace { face, line_h: asc + desc })))
    })
}

/// Windows-1251 → Unicode for 0x80..0xbf (0x98 unused); 0xc0..0xff = А..я.
const CP1251_HI: [char; 64] = [
    'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', //
    'ђ', '‘', '’', '“', '”', '•', '–', '—', '\u{98}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', //
    '\u{a0}', 'Ў', 'ў', 'Ј', '¤', 'Ґ', '¦', '§', 'Ё', '©', 'Є', '«', '¬', '\u{ad}', '®', 'Ї', //
    '°', '±', 'І', 'і', 'ґ', 'µ', '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї',
];

/// The game's texts are cp1251 bytes (the RU build).
pub fn decode_cp1251(b: &[u8]) -> String {
    b.iter()
        .map(|&c| match c {
            0..=0x7f => c as char,
            0x80..=0xbf => CP1251_HI[(c - 0x80) as usize],
            _ => char::from_u32(0x410 + (c - 0xc0) as u32).unwrap_or('?'),
        })
        .collect()
}

/// cp1252 bytes 0x80..=0x9f (Windows Latin 1; 0xa0.. = Latin-1)
const CP1252_C1: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', //
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// H2306: the English data's texts are cp1252 bytes (e.g. the EN DVD's XT files: 'é', curly quotes).
pub fn decode_cp1252(b: &[u8]) -> String {
    b.iter().map(|&c| if (0x80..0xa0).contains(&c) { CP1252_C1[(c - 0x80) as usize] } else { c as char }).collect()
}

/// A text file's bytes in its data set's code page (H2306): cp1251 for Russian data, cp1252 for English.
pub fn decode_text(b: &[u8], lang: crate::media::lang::GameLang) -> String {
    match lang {
        crate::media::lang::GameLang::Ru => decode_cp1251(b),
        crate::media::lang::GameLang::En => decode_cp1252(b),
    }
}

/// Text of a text-file AssetRef (XT/*.xtx), decoded in the HDB's code page; None for other assets or a missing file.
pub fn xtx_text(lib: &MediaLib, h: &crate::hdb::Hdb, asset: u32) -> Option<String> {
    let clip = lib.resolve(h, asset)?;
    if !matches!(clip.ext().as_str(), "xtx" | "txt") {
        return None;
    }
    Some(decode_text(&crate::io::read(&clip.file).ok()?, crate::media::lang::hdb_lang(h)))
}

/// [`xtx_text`] in the session's game language (H1836: the English data set's XT file of the same name).
pub fn xtx_text_lang(s: &super::Session, asset: u32) -> Option<String> {
    let clip = s.lib.resolve(s.r.w.h, asset)?;
    if !matches!(clip.ext().as_str(), "xtx" | "txt") {
        return None;
    }
    Some(decode_text(&crate::io::read(&s.lang_file(&clip, s.game_lang)).ok()?, s.file_lang(s.game_lang)))
}

/// HYPOTHESIS H533: DrawTextA's DT_WORDBREAK line breaking (Wine's TEXT_NextLine / TEXT_WordBreak): a line ends at
/// CR LF / LF CR / a lone CR or LF; a line wider than `width` breaks after the last space that still fits, the spaces
/// at the break are dropped; a single word wider than the box stays whole (clipped by the rect); spaces at a paragraph's
/// start are kept.
pub fn layout(text: &str, width: i32, measure: impl Fn(&str) -> i32) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let n = chars.len();
    // paragraphs
    let mut paras: Vec<String> = Vec::new();
    let mut cur = String::new();
    while i < n {
        let c = chars[i];
        if c == '\r' || c == '\n' {
            paras.push(std::mem::take(&mut cur));
            let pair = if c == '\r' { '\n' } else { '\r' };
            i += if i + 1 < n && chars[i + 1] == pair { 2 } else { 1 };
        } else {
            cur.push(c);
            i += 1;
        }
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    for p in paras {
        let pc: Vec<char> = p.chars().collect();
        let mut s = 0;
        if pc.is_empty() {
            out.push(String::new());
            continue;
        }
        while s < pc.len() {
            // the longest prefix that fits
            let mut fit = s;
            while fit < pc.len() && measure(&pc[s..=fit].iter().collect::<String>()) <= width {
                fit += 1;
            }
            if fit == pc.len() {
                out.push(pc[s..].iter().collect());
                break;
            }
            // break at the last space within [s, fit]: the char that does not fit may itself be the space
            let brk = (s..=fit).rev().find(|&k| pc[k] == ' ' && k > s);
            let (end, next) = match brk {
                Some(b) => {
                    let mut e = b;
                    while e > s && pc[e - 1] == ' ' {
                        e -= 1;
                    }
                    (e, b)
                }
                None => {
                    // the word does not fit: keep it whole up to the next space
                    let e = (fit..pc.len()).find(|&k| pc[k] == ' ').unwrap_or(pc.len());
                    (e, e)
                }
            };
            out.push(pc[s..end].iter().collect());
            let mut k = next;
            while k < pc.len() && pc[k] == ' ' {
                k += 1;
            }
            s = k;
        }
    }
    out
}

/// Draw `text` into `rect` (x1, y1, x2, y2) with `style` as DrawText does (H530 / H533); returns the laid-out lines.
pub fn draw_text(fb: &mut [u8], f: &DocFace, rect: [i32; 4], text: &str, style: &DocStyle) -> Vec<String> {
    let w = rect[2] - rect[0];
    let lines = layout(text, w, |s| f.face.width(s));
    // the glyphs go to a scratch frame (marker colours), then the pixels inside the rect are copied: DrawText clips
    let mut scratch = vec![0u8; super::SCREEN_W * super::SCREEN_H * 3];
    const TEXT: [u8; 3] = [255, 0, 0];
    const SHADOW: [u8; 3] = [0, 255, 0];
    for pass in [SHADOW, TEXT] {
        if pass == SHADOW && !style.shadow {
            continue;
        }
        let d = (pass == SHADOW) as i32;
        for (i, l) in lines.iter().enumerate() {
            let top = rect[1] + i as i32 * f.line_h;
            if top >= rect[3] {
                break;
            }
            let lw = f.face.width(l);
            let x = match style.align {
                Align::Left => rect[0],
                Align::Right => rect[2] - lw,
                Align::Center => rect[0] + (w - lw) / 2,
            };
            f.face.draw(&mut scratch, x + d, top + d, l, pass);
        }
    }
    for y in rect[1].max(0)..rect[3].min(super::SCREEN_H as i32) {
        for x in rect[0].max(0)..rect[2].min(super::SCREEN_W as i32) {
            let o = (y as usize * super::SCREEN_W + x as usize) * 3;
            match [scratch[o], scratch[o + 1], scratch[o + 2]] {
                TEXT => fb[o..o + 3].copy_from_slice(&style.colour),
                SHADOW => fb[o..o + 3].copy_from_slice(&[0, 0, 0]),
                _ => {}
            }
        }
    }
    lines
}

/// Text files shown by items: (item, asset) of every shown layout item whose current asset is an active text file.
///
/// HYPOTHESIS H535: one asset object per AssetRef (reused on every show) and one draw rect per text
/// object (each show sets it to the showing item's rect): a text shown by several items is drawn once, at the
/// item that showed it last — the page items of JournalFlip / BibleInit (their procedure runs after the pool item's
/// own Activation, e.g. 0x13c0b's first page moves to 0x13be4), else the first item in layout order.
pub fn shown_documents(s: &super::Session) -> Vec<(u32, u32)> {
    let all = shown_text_items(s);
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &(item, asset) in &all {
        if out.iter().any(|d| d.1 == asset) {
            continue;
        }
        let paged = all.iter().find(|d| d.1 == asset && item_style(d.0) != DEFAULT).copied();
        out.push(paged.unwrap_or((item, asset)));
    }
    out
}

fn shown_text_items(s: &super::Session) -> Vec<(u32, u32)> {
    s.r.w
        .layouts()
        .iter()
        .flat_map(|l| l.1.iter())
        .filter_map(|&(i, a)| a.filter(|&a| a != 0 && s.r.active.contains(&a)).map(|a| (i, a)))
        // the FBI computer's item texts carry their own style (H574, `computer.rs`)
        .filter(|&(i, _)| !super::computer::handles(s, i))
        .filter(|&(_, a)| s.lib.resolve(s.r.w.h, a).is_some_and(|c| matches!(c.ext().as_str(), "xtx" | "txt")))
        .collect()
}

/// The item's region rect (x1, y1, x2, y2), screen pixels.
fn item_region(s: &super::Session, item: u32) -> Option<[i32; 4]> {
    match s.r.w.h.read(crate::world::class::INTERFACE_ITEM, item)?.field("region")? {
        crate::hdb::Value::Region { rect, .. } => Some(rect.map(|v| v as i32)),
        _ => None,
    }
}

/// Draw every document text on screen (called by `ui::draw` after the item pictures).
pub fn draw_documents(s: &mut super::Session, fb: &mut [u8]) {
    for (item, asset) in shown_documents(s) {
        let Some(rect) = item_region(s, item) else { continue };
        let Some(text) = xtx_text_lang(s, asset) else { continue };
        let style = item_style(item);
        let Some(f) = face_lang(s, style.font, style.size) else { continue };
        draw_text(fb, f, rect, &text, &style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp1251() {
        assert_eq!(decode_cp1251(b"\xc0\xff \xa8\xb8 \xb9 \x96"), "Ая Ёё № –");
        // H2306: English texts
        assert_eq!(decode_cp1252(b"Rene\xe9\x92s \x93x\x94 \x96"), "Reneé’s “x” –");
    }

    #[test]
    fn wordbreak() {
        let m = |s: &str| s.chars().count() as i32 * 10;
        // 10 px per char
        assert_eq!(layout("aaa bbb ccc", 70, m), vec!["aaa bbb", "ccc"]);
        assert_eq!(layout("aaa bbb ccc", 60, m), vec!["aaa", "bbb", "ccc"]);
        // CR LF paragraphs, an empty line, the indent of a paragraph kept, spaces at a break dropped
        assert_eq!(layout("ab\r\n\r\n    cd ef", 80, m), vec!["ab", "", "    cd", "ef"]);
        // a word wider than the box stays whole
        assert_eq!(layout("abcdefghij k", 50, m), vec!["abcdefghij", "k"]);
    }
}
