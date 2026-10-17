//! The FBI computer's texts on screen (H570–H578 — static decode, the looks measured on the
//! original's login screenshots fo-069 k22 / k23 only): the native item texts and SetIItemText styles of
//! `world::procs::computer`, the login fields, text panels (ING field notes, the e-mail reader) and the inbox rows.
//!
//! HYPOTHESIS H574 (display): an item text = GDI text without anti-aliasing (as H301) in the style's face (Courier New /
//! Arial Bold) at `size` px, origin = the item rect's left (fo-069 k23: the 14-character user name's glyph cells from
//! x 260 = rect left of 0xc67b, 8 px apart), one line 1 px below the rect's top (name field rect 225..243 → line top 226, password
//! field 286..306 → top 287; ascent 12), wrapped text from the rect's top (`doc.rs`); clipped to the rect. Text panels (H576) wrap at spaces to the rect width.
//!
//! The text of a kind-1 / 3 AssetRef is its XT/*.xtx file (cp1251, FUN_00447430 normalisation). Wrapped texts (the
//! panels: ING field notes, the e-mail reader) are laid out and drawn by the DrawText model (`doc.rs`); single
//! lines (wrap mode 0) are drawn here, placed as measured on fo-069.
use super::text::Face;
use super::Session;
use crate::world::procs::computer::{self as pc, Src, Style};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// A face of `style` (Courier New / Arial Bold at its size), loaded once per (face, bold, size)
fn face(st: &Style) -> Option<&'static Face> {
    static FACES: OnceLock<Mutex<HashMap<(bool, bool, u8), Option<&'static Face>>>> = OnceLock::new();
    let key = (st.face == pc::Face::Arial, st.bold, st.size);
    let mut m = FACES.get_or_init(|| Mutex::new(HashMap::new())).lock().ok()?;
    *m.entry(key).or_insert_with(|| {
        let files: &[&str] = match key {
            (true, true, _) => &["Arial Bold.ttf", "arialbd.ttf"],
            (true, false, _) => &["Arial.ttf", "arial.ttf"],
            (false, true, _) => &["Courier New Bold.ttf", "courbd.ttf"],
            (false, false, _) => &["Courier New.ttf", "cour.ttf", "Courier_New.ttf"],
        };
        let mut f = Face::load(files, st.size as f32)?;
        f.proportional = key.0;
        Some(&*Box::leak(Box::new(f)))
    })
}

/// Line height of a face at `size` px: GDI tmAscent + tmDescent (Courier New 1705 + 615, Arial 1854 + 434 units of 2048)
fn line_h(st: &Style) -> i32 {
    let (a, d) = if st.face == pc::Face::Arial { (1854.0, 434.0) } else { (1705.0, 615.0) };
    let s = st.size as f32 / 2048.0;
    (a * s).round() as i32 + (d * s).round() as i32
}

fn width(f: Option<&Face>, st: &Style, t: &str) -> i32 {
    match f {
        Some(f) => f.width(t),
        None => t.chars().count() as i32 * (st.size as i32 * 6 / 10),
    }
}

/// The text file of a kind-1 / 3 AssetRef, read once per language (cp1251 → Unicode, the original's text normalisation; H1836:
/// the game language's XT file)
pub fn file_text(s: &Session, asset: u32) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<(u32, crate::media::lang::GameLang), Option<String>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().ok()?;
    c.entry((asset, s.game_lang))
        .or_insert_with(|| {
            if !pc::is_text_file(&s.r.w, asset) {
                return None;
            }
            let clip = s.lib.resolve(s.r.w.h, asset)?;
            let b = crate::io::read(&s.lang_file(&clip, s.game_lang)).ok()?;
            // H2306: an English file is cp1252
            let en = s.file_lang(s.game_lang) == crate::media::lang::GameLang::En;
            Some(pc::normalize(&if en { pc::cp1252(&b) } else { pc::cp1251(&b) }))
        })
        .clone()
}

/// The text an AssetRef shows (kind 4 / 0 from the HDB / XfilesT.dll, kind 1 / 3 from its file)
pub fn asset_text(s: &Session, asset: u32) -> Option<String> {
    pc::asset_string(&s.r.w, asset).map(|t| pc::normalize(&t)).or_else(|| file_text(s, asset))
}

fn src_text(s: &Session, src: &Src) -> String {
    match src {
        Src::Str(t) => t.clone(),
        Src::Asset(a) => asset_text(s, *a).unwrap_or_default(),
        Src::MailLine(a, n) => {
            let t = asset_text(s, *a).unwrap_or_default();
            let (from, subject, _) = pc::mail_header(&t);
            if *n == 0 {
                from
            } else {
                subject
            }
        }
        // H797: one scroller entry per note (GUESS: entries separated by a line break)
        Src::Notes(v) => v.iter().filter_map(|&a| asset_text(s, a)).collect::<Vec<_>>().join("\n"),
    }
}

fn item_rect(s: &Session, item: u32) -> Option<[i32; 4]> {
    match s.r.w.h.read(crate::world::class::INTERFACE_ITEM, item)?.field("region")? {
        crate::hdb::Value::Region { rect, .. } => Some(rect.map(|v| v as i32)),
        _ => None,
    }
}

/// Items of the shown layouts (with the AssetRef each shows)
fn shown_items(s: &Session) -> HashMap<u32, Option<u32>> {
    s.r.w.layouts().iter().flat_map(|l| l.1.iter().copied()).collect()
}

/// Word-wrap at spaces to `w` px (a word wider than the box stays whole)
fn wrap(f: Option<&Face>, st: &Style, line: &str, w: i32) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in line.split(' ') {
        let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && width(f, st, &cand) > w {
            out.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = cand;
        }
    }
    out.push(cur);
    out
}

/// A wrapped text (text panels, SetIItemText 'y') through the DrawText model (`doc.rs`, H530 / H533)
fn draw_wrapped(s: &Session, fb: &mut [u8], r: [i32; 4], text: &str, st: &Style) -> bool {
    use super::doc;
    let font = if st.face == pc::Face::Arial { doc::DocFont::Arial } else { doc::DocFont::Courier };
    let align = match st.align {
        0 => doc::Align::Center,
        2 => doc::Align::Right,
        _ => doc::Align::Left,
    };
    let ds = doc::DocStyle { font, size: st.size as u32, colour: st.colour, shadow: false, align };
    let Some(f) = doc::face_lang(s, font, st.size as u32) else { return false };
    doc::draw_text(fb, f, r, text, &ds);
    true
}

/// Draw `text` into rect `r` in `st` (H574): one line centred vertically, else from the top; aligned; clipped
pub fn draw_text(fb: &mut [u8], r: [i32; 4], text: &str, st: &Style, panel: bool) {
    let f = face(st);
    let mut lines: Vec<String> = Vec::new();
    for l in text.split('\n') {
        if st.wrap || panel {
            lines.extend(wrap(f, st, l, r[2] - r[0]));
        } else {
            lines.push(l.to_string());
        }
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) && lines.len() > 1 {
        lines.pop();
    }
    let lh = line_h(st);
    // H574: a single line 1 px below the rect's top (fo-069: name rect top 225 → caps from 230 = top 226 + 4; password
    // rect top 286 → '*' rows 290..294 = top 287 + 3; vertical centring would give 288 there)
    let top = if lines.len() == 1 && !panel { r[1] + 1 } else { r[1] };
    for (i, l) in lines.iter().enumerate() {
        let y = top + i as i32 * lh;
        if y >= r[3] {
            break;
        }
        let w = width(f, st, l);
        let x = match st.align {
            0 => r[0] + (r[2] - r[0] - w) / 2,
            2 => r[2] - w,
            _ => r[0],
        };
        // whole characters inside the rect's right edge (the bitmap fallback font); a face is clipped to the rect's
        // right edge pixel-wise, as DrawText (H893 / R36: fo-115 k14's message ends in a half-drawn 'а' at x 606)
        let mut fit = String::new();
        for ch in l.chars() {
            let mut t = fit.clone();
            t.push(ch);
            if f.is_none() && x + width(f, st, &t) > r[2] + 1 {
                break;
            }
            fit = t;
        }
        match f {
            Some(f) => {
                f.draw_clip_rect(fb, x, y, &fit, st.colour, r[2], r[3]);
            }
            None => {
                super::font::text(fb, x, y + 2, &fit, st.colour);
            }
        }
    }
}

/// HYPOTHESIS H792 (static, the item display routine FUN_0040b6c0 case 2: an item showing a multi-entry PFF range
/// resets it to frame 0, sets the loop flag and starts it; XG/69328.pff = 2 entries, 500 ms each, black bar / white): an item's picture that is a PFF
/// range of more than one entry (the 11 such items: the carets 0xfbba / 0xfbbc / 0xfbe1 / 0xfbe5 / 0xfbe9 / 0x14281, the
/// mail reader's paper clips 0x11c09 / 0x11c25 — "the flashing paper clip" of the walkthroughs —, 0xcc22, 0xcc24,
/// 0xd01e) is the animation's current frame, timed from the first rendered frame that drew the item (not drawn by the
/// previous render = shown anew). The returned key is `asset` for entry `start`, else a cached synthetic key (bit 31 +
/// entry) holding that entry's picture.
pub fn caret_asset(s: &mut Session, owner: u32, asset: u32) -> u32 {
    if !s.r.w.rules.h792_caret_blink {
        return asset;
    }
    if !s.clip(asset).is_some_and(|c| c.ext() == "pff" && c.end > c.start) {
        return asset;
    }
    let (now, frame) = (s.now, s.ui.frame);
    let t0 = {
        let e = s.ui.carets.entry(owner).or_insert((now, frame));
        if now < e.0 || e.1 + 1 < frame {
            *e = (now, frame);
        }
        e.1 = frame;
        e.0
    };
    let Some(c) = s.clip(asset).filter(|c| c.ext() == "pff" && c.end > c.start) else { return asset };
    let Some(pff) = s.pffs.entry(c.file.clone()).or_insert_with(|| crate::media::pict::Pff::open(&c.file).ok()).as_ref() else {
        return asset;
    };
    let (a, b) = (c.start as usize, (c.end as usize).min(pff.len().saturating_sub(1)));
    let d = pff.durations();
    let durs: Vec<u32> = (a..=b).map(|i| d.get(i).copied().unwrap_or(0)).collect();
    let i = a + super::cursor::frame_at(&durs, now - t0);
    if i == a {
        return asset;
    }
    let key = 0x8000_0000 | ((i as u32) << 20) | (asset & 0xf_ffff);
    if !s.pictures.contains_key(&key) {
        let pic = pff.picture(i).ok();
        s.pictures.insert(key, pic);
    }
    key
}

/// Items whose text this module draws (the generic H306 item texts of `ui.rs` skip them)
pub fn handles(s: &Session, item: u32) -> bool {
    let w = &s.r.w;
    (w.rules.h578_carets && matches!(item, 0xc67b | 0xc68c))
        || w.pc.texts.iter().any(|x| x.0 == item)
        || w.pc.styles.iter().any(|x| x.0 == item)
}

/// Draw the computer's texts (after the region pictures, with the other UI texts)
pub fn draw(s: &mut Session, fb: &mut [u8]) {
    draw_only(s, fb, &|_| true);
}

/// [`draw`] for the items `only` accepts (H799: one layout's texts with its pictures)
pub fn draw_only(s: &mut Session, fb: &mut [u8], only: &dyn Fn(u32) -> bool) {
    let mut items = shown_items(s);
    items.retain(|it, _| only(*it));
    if items.is_empty() {
        return;
    }
    let enabled = |s: &Session, it: u32| s.r.w.is_enabled(crate::world::class::INTERFACE_ITEM, it);
    // native item texts (the original's item-text procedures)
    let texts = s.r.w.pc.texts.clone();
    for (item, src, st) in &texts {
        if !items.contains_key(item) || !enabled(s, *item) {
            continue;
        }
        let Some(r) = item_rect(s, *item) else { continue };
        let t = src_text(s, src);
        draw_text(fb, r, &t, st, false);
    }
    // SetIItemText texts in their style (H574): the item's shown AssetRef (H176 / H573)
    let styles = s.r.w.pc.styles.clone();
    for (item, st) in &styles {
        let Some(Some(a)) = items.get(item).copied() else { continue };
        if a == 0 || !s.r.active.contains(&a) || !enabled(s, *item) {
            continue;
        }
        let Some(r) = item_rect(s, *item) else { continue };
        let t = asset_text(s, a).unwrap_or_default();
        draw_text(fb, r, &t, st, false);
    }
    // the login fields (H578: Courier New 14, the password as '*')
    if s.r.w.rules.h578_carets {
        for (item, t) in s.r.w.pc_login_texts() {
            if items.get(&item) != Some(&Some(0)) {
                continue;
            }
            let Some(r) = item_rect(s, item) else { continue };
            draw_text(fb, r, &t, &pc::COURIER14, false);
        }
    }
    // text panels (H576); H791: from the main scroller's first line
    let panels = s.r.w.pc.panels.clone();
    let current = s.r.w.pc.panel_item;
    for (item, src, st) in &panels {
        if !items.contains_key(item) {
            continue;
        }
        let Some(r) = item_rect(s, *item) else { continue };
        let t = src_text(s, src);
        if s.r.w.rules.h791_text_scroll {
            // one scroller entry per text (H797: a day's notes = several)
            let entries: Vec<String> = match src {
                Src::Notes(v) => v.iter().filter_map(|&a| asset_text(s, a)).collect(),
                _ => vec![t.clone()],
            };
            let first = if current == Some(*item) { s.r.w.pc.scroll } else { 0 };
            if let Some((max, page, to_top)) = draw_scrolled(s, fb, r, &entries, st, first) {
                if current == Some(*item) {
                    let pc = &mut s.r.w.pc;
                    pc.scroll_max = Some(max);
                    pc.scroll_page = page;
                    pc.scroll_to_top = to_top;
                    pc.scroll = pc.scroll.min(max);
                }
                continue;
            }
        }
        if !draw_wrapped(s, fb, r, &t, st) {
            draw_text(fb, r, &t, st, true);
        }
    }
    draw_photos(s, fb, &items); // H793
}

/// HYPOTHESIS H791 (display): a text panel = the main scroller's entries laid out as DrawText's DT_WORDBREAK (H533) at
/// the rect's width, one font line (tmHeight + tmExternalLeading) per line, half a line between entries,
/// drawn from row `first` (a line or a gap), clipped to the rect. Returns the
/// largest first line (the text's bottom at the panel's bottom: the scroller stops there) and a page in
/// lines (the panel's height less one line), and the largest first row from which a page
/// up goes to the top (H892: the scroller's page-up goes to the top while the text's top is at most the view's height less
/// min(line, gap) above the view's top)
fn draw_scrolled(s: &Session, fb: &mut [u8], r: [i32; 4], entries: &[String], st: &Style, first: usize) -> Option<(usize, usize, usize)> {
    use super::doc;
    let font = if st.face == pc::Face::Arial { doc::DocFont::Arial } else { doc::DocFont::Courier };
    let f = doc::face_lang(s, font, st.size as u32)?;
    let lh = f.line_h.max(1);
    // rows: the entries' lines, a half-line gap (half the font line) between entries
    let mut lines: Vec<Option<String>> = Vec::new();
    for (k, e) in entries.iter().enumerate() {
        if k > 0 {
            lines.push(None);
        }
        lines.extend(doc::layout(e, r[2] - r[0], |x| f.face.width(x)).into_iter().map(Some));
    }
    let height = |l: &Option<String>| if l.is_some() { lh } else { lh / 2 };
    let view = r[3] - r[1];
    let mut max = lines.len();
    let mut below = 0;
    while max > 0 && below + height(&lines[max - 1]) <= view {
        max -= 1;
        below += height(&lines[max]);
    }
    let rows = (view / lh).max(1) as usize;
    let mut to_top = 0;
    let mut above = 0;
    for (k, l) in lines.iter().enumerate() {
        above += height(l);
        if above > view - lh.min(lh / 2) {
            break;
        }
        to_top = k + 1;
    }
    let first = first.min(max);
    let mut top = r[1];
    for l in lines.iter().skip(first) {
        if top >= r[3] {
            break;
        }
        let Some(l) = l else {
            top += lh / 2;
            continue;
        };
        let w = f.face.width(l);
        let x = match st.align {
            0 => r[0] + (r[2] - r[0] - w) / 2,
            2 => r[2] - w,
            _ => r[0],
        };
        // whole glyphs inside the rect's right edge (DrawText clips to the rect)
        let mut fit = String::new();
        for ch in l.chars() {
            let mut t = fit.clone();
            t.push(ch);
            if x + f.face.width(&t) > r[2] + 1 {
                break;
            }
            fit = t;
        }
        f.face.draw_clip(fb, x, top, &fit, st.colour, r[3]);
        top += lh;
    }
    Some((max, rows.saturating_sub(1).max(1), to_top))
}

/// HYPOTHESIS H793 (display, static, the photo album's draw routine FUN_004e3c50): the album's thumbnails are the photos'
/// pictures scaled to 80×60 at the slot items' top-left (the album's slot table, row-major), the enlarged one scaled to 160×120
/// at 0xc8c4's top-left (420,32); nearest-pixel scaling (StretchBlt COLORONCOLOR is a GUESS)
fn draw_photos(s: &mut Session, fb: &mut [u8], items: &HashMap<u32, Option<u32>>) {
    if !s.r.w.rules.h793_photo_album || !s.r.w.pc.album_open {
        return;
    }
    let mut draws: Vec<(usize, u32, i32, i32)> = Vec::new();
    for (k, &p) in s.r.w.album_shown().iter().enumerate() {
        if let Some(slot) = s.r.w.album_slot(k) {
            draws.push((p, slot, 80, 60));
        }
    }
    if let Some(p) = s.r.w.pc.enlarged.filter(|&p| p < s.r.w.pc.photos.len()) {
        draws.push((p, pc::PHOTO_ENLARGED, 160, 120));
    }
    for (p, slot, w, h) in draws {
        if !items.contains_key(&slot) {
            continue;
        }
        let Some(r) = item_rect(s, slot) else { continue };
        // C306: a camera shot's photo is its video frame's crop
        let frame = s.r.w.pc.photo_frames.get(p).copied().flatten();
        let Some(pic) = s.photo_picture(s.r.w.pc.photos[p].0, frame) else { continue };
        if pic.width == 0 || pic.height == 0 {
            continue;
        }
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = ((x as usize * pic.width) / w as usize, (y as usize * pic.height) / h as usize);
                let (dx, dy) = (r[0] + x, r[1] + y);
                if dx < 0 || dy < 0 || dx >= super::SCREEN_W as i32 || dy >= super::SCREEN_H as i32 {
                    continue;
                }
                let so = (sy * pic.width + sx) * 3;
                let d = (dy as usize * super::SCREEN_W + dx as usize) * 3;
                fb[d..d + 3].copy_from_slice(&pic.rgb[so..so + 3]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn courier14_line() {
        let st = pc::COURIER14;
        assert_eq!(line_h(&st), 16);
        if let Some(f) = face(&st) {
            assert_eq!(f.advance, pc::COURIER14_ADVANCE);
            assert_eq!(f.ascent, 12);
        }
    }
}
