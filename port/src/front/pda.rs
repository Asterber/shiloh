//! Display of the screens opened from the inventory: the darkened scene under a full-screen layout (H620)
//! and the PDA note pad's text (H624). Display only — the state is the core's (`world::pda`).
use super::{blit, text, ui, Session, SCREEN_H, SCREEN_W};
use crate::hdb::Value;
use crate::world::{class, pda};

/// HYPOTHESIS H620 — the scene darkened under a shown layout. Static (the layout show, FUN_0042b250; RE_NOTES H83
/// update): before its items are built, a shown layout dims every runtime object of kinds 4 (the View with its
/// picture / video), 0xc / 0xe / 0xf / 0x12 (icons), 0x1f (InterfaceItems of layouts shown before), 0x22 (the AI
/// icon), 0x10 (inventory) and 0x1b (players) with a filter (command type 21) — unless the layout is one of 0xc24a,
/// 0xc275, 0xc277, 0xc279, 0x11ce5, 0xc51c (or in the AddIFLExcept list, not modelled); the layout's hide removes the
/// filters. Explorables and the dialogue box (other kinds) are not dimmed. The as-View close-ups (0xdefa, 0x14968,
/// 0x14992, 0xe4b9, 0x11b38 — the ids the layout show tests first) are exempted here although the static path dims for them:
/// `traces/fo-060.jsonl.shots/k11.png` and fo-063 k00 / k01 show the drawer close-up 0x14968 with the AI icon at
/// full brightness (AG check BH2). The filter's look, measured: each channel v → (15·v + 32) >> 6 (≈ 0.234) —
/// `traces/fo-076.jsonl.shots/k04.png` → `k05.png` (photo layout 0x1095d over View 0x4d5, 63 000 channel samples: 32 →
/// 8, 64 → 15, 128 → 30, 200 → 47, 248 → 58), fo-057 k13 (PDA map 0x84dc over View 0x2bb), fo-077 k28 → k29. An already
/// dimmed AI icon (H621, the same filter) is not darkened twice (fo-057 k13 = fo-064 k12 at the icon).
pub fn dim(v: u8) -> u8 {
    ((15 * v as u32 + 32) >> 6) as u8
}

/// Darken a whole frame (H620).
pub fn dim_frame(fb: &mut [u8]) {
    for v in fb.iter_mut() {
        *v = dim(*v);
    }
}

/// layouts whose show does not dim the scene (H620)
pub const NO_DIM_LAYOUTS: [u32; 6] = [0xc24a, 0xc275, 0xc277, 0xc279, 0x11ce5, 0xc51c];
/// the as-View close-up layouts (exempted, see H620)
pub const AS_VIEW_LAYOUTS: [u32; 5] = [0xdefa, 0x14968, 0x14992, 0xe4b9, 0x11b38];

/// H620: the index (in show order, `World::layouts`) of the dimming layout, if the rule is on: the most recently shown
/// one, or with H894 the first one — a second dimming layout over it does not darken it (R37: the address picker 0x9dd3
/// over the compose screen 0x9516, `traces/fo-116.jsonl.burst/k15-0.png` = k14-0 outside the picker).
pub fn dim_layout(s: &Session) -> Option<usize> {
    if !s.r.w.rules.h620_dim_under_layout {
        return None;
    }
    let dims = |(l, _): &(u32, Vec<(u32, Option<u32>)>)| !NO_DIM_LAYOUTS.contains(l) && !AS_VIEW_LAYOUTS.contains(l);
    if s.r.w.rules.h894_dim_once {
        s.r.w.layouts().iter().position(dims)
    } else {
        s.r.w.layouts().iter().rposition(dims)
    }
}

/// H620: compose everything after the movie area with the dimmed objects darkened (replaces the region pictures +
/// `ui::draw` of `Session::render`): items of earlier layouts and the icon bars, then the darkening, then the
/// Explorable pictures and the dialogue box (not dimmed), the dimming layout's and later layouts' items, texts, cursor.
/// Returns false (nothing drawn) when no layout dims.
pub fn compose_dimmed(s: &mut Session, fb: &mut [u8]) -> bool {
    let Some(idx) = dim_layout(s) else { return false };
    let layouts: Vec<(u32, Vec<u32>)> = s.r.w.layouts().iter().map(|l| (l.0, l.1.iter().map(|x| x.0).collect())).collect();
    let above: Vec<u32> = layouts[idx..].iter().flat_map(|l| l.1.iter().copied()).collect();
    // H799: items of earlier layouts that are not as-View close-ups lie above the icon bars (a full-screen computer
    // layout keeps covering the AI icon when a second layout opens over it); each layout's texts are drawn with its
    // pictures (a later layout covers them)
    let layered = s.r.w.rules.h799_layouts_over_bars;
    let as_view = |s: &Session, l: u32| AS_VIEW_LAYOUTS.contains(&l) || s.r.w.layout_as_view(l);
    let mut layer_of: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for (i, l) in layouts.iter().enumerate() {
        for &it in &l.1 {
            layer_of.entry(it).or_insert(i);
        }
    }
    let screen_layer = |s: &Session, i: usize| layered && i < idx && !as_view(s, layouts[i].0);
    let (mut low, mut mid) = (Vec::new(), Vec::new());
    let mut by_layer: Vec<Vec<(i32, i32, u32)>> = vec![Vec::new(); layouts.len()];
    let h = s.r.w.h;
    for c in s.r.clickables().into_iter().rev() {
        if c.cls != class::INTERFACE_ITEM && c.cls != class::EXPLORABLE {
            continue;
        }
        let list = h.read(c.cls, c.owner).map(|r| r.u32("f44")).unwrap_or(0);
        for a in h.list_of(class::ASSET_REF_LIST, list) {
            if s.r.active.contains(&a) && s.picture(a).is_some() {
                // H578: a caret item is drawn where its field's text ends (as `Session::region_pictures_of`)
                let (x, y) = s.r.w.pc_item_pos(c.owner).unwrap_or((c.rect[0] as i32, c.rect[1] as i32));
                let at = (x, y, super::computer::caret_asset(s, c.owner, a)); // H792
                let layer = layer_of.get(&c.owner).copied();
                if c.cls == class::EXPLORABLE {
                    mid.push(at);
                } else if let Some(i) = layer.filter(|&i| above.contains(&c.owner) || screen_layer(s, i)) {
                    by_layer[i].push(at);
                } else {
                    low.push(at);
                }
            }
        }
    }
    let draw = |s: &mut Session, fb: &mut [u8], v: Vec<(i32, i32, u32)>| {
        for (x, y, a) in v {
            if let Some(p) = s.picture(a).cloned() {
                blit(fb, x, y, &p);
            }
        }
    };
    let mut texts_done: Vec<usize> = Vec::new();
    // one layout: its pictures, then (H799) its texts
    let layer = |s: &mut Session, fb: &mut [u8], i: usize, pics: Vec<(i32, i32, u32)>, done: &mut Vec<usize>| {
        draw(s, fb, pics);
        if layered {
            let items: std::collections::HashSet<u32> = layouts[i].1.iter().copied().collect();
            super::computer::draw_only(s, fb, &|it| items.contains(&it));
            if items.contains(&pda::NOTE_PANEL) {
                draw_note_pad(s, fb);
            }
            done.push(i);
        }
    };
    draw(s, fb, low);
    let screen = s.r.w.layouts().iter().any(|l| ui::FULL_SCREEN_LAYOUTS.contains(&l.0));
    if !screen {
        ui::draw_bars(s, fb);
    }
    for i in 0..idx {
        if screen_layer(s, i) {
            let pics = std::mem::take(&mut by_layer[i]);
            layer(s, fb, i, pics, &mut texts_done);
        }
    }
    dim_frame(fb);
    draw(s, fb, mid);
    // H500: the box under the shown layouts' items (off: over them, as before H500)
    let under = s.r.w.rules.h500_layouts_over_dbox;
    if !screen && under {
        ui::draw_dbox(s, fb);
    }
    for i in idx..layouts.len() {
        let pics = std::mem::take(&mut by_layer[i]);
        layer(s, fb, i, pics, &mut texts_done);
    }
    if !screen && !under {
        ui::draw_dbox(s, fb);
    }
    ui::draw_captions(s, fb);
    ui::draw_video_text(s, fb);
    ui::draw_item_texts(s, fb);
    // texts of the layouts not drawn by layer (as-View close-ups; all of them without H799)
    let note_done = layer_of.get(&pda::NOTE_PANEL).is_some_and(|i| texts_done.contains(i));
    super::computer::draw_only(s, fb, &|it| !layer_of.get(&it).is_some_and(|i| texts_done.contains(i))); // H570–H578
    super::doc::draw_documents(s, fb); // H530
    if !note_done {
        draw_note_pad(s, fb);
    }
    if s.game_cursor {
        ui::draw_cursor(s, fb);
    }
    true
}

/// H624 display (GUESS for the looks — no screenshot of the note pad): black Courier New 12 (the panel style
/// style 4 at size 12), one entry per note, word-wrapped to the panel item's width, a blank half line between
/// notes, from the scroller's first line, clipped to the panel; the day number centred in item 0x1079e.
pub const NOTE_TEXT: [u8; 3] = [0, 0, 0];

/// The text of a note: a kind-0 string resource (H260) / kind-4 text, or a kind-1 .xtx file (`front::doc::xtx_text`,
/// cp1251).
pub fn note_text(s: &mut Session, a: u32) -> Option<String> {
    if let Some(t) = s.r.w.line_text(a) {
        // kind-4 blobs come as cp1251 bytes read as chars; string resources are already Unicode
        return Some(
            t.into_iter()
                .map(|c| if (c as u32) < 0x100 { super::doc::decode_cp1251(&[c as u32 as u8]).chars().next().unwrap_or(c) } else { c })
                .collect(),
        );
    }
    super::doc::xtx_text_lang(s, a)
}

fn item_rect(s: &Session, item: u32) -> Option<[i32; 4]> {
    match s.r.w.h.read(class::INTERFACE_ITEM, item)?.field("region")? {
        Value::Region { rect, .. } => Some(rect.map(|v| v as i32)),
        _ => None,
    }
}

/// H624: the panel's laid-out lines (None = a half-line gap between notes).
pub fn note_lines(s: &mut Session, width: i32) -> Vec<Option<String>> {
    let w = |t: &str| match text::courier12() {
        Some(f) => f.width(t),
        None => t.chars().count() as i32 * crate::world::dbox::CHAR_W,
    };
    let mut out = Vec::new();
    for (i, a) in s.r.w.note_assets().into_iter().enumerate() {
        let Some(t) = note_text(s, a) else { continue };
        if i > 0 {
            out.push(None);
        }
        // H533 (front::doc): DrawText's DT_WORDBREAK line breaking
        for l in super::doc::layout(&t, width, w) {
            out.push(Some(l));
        }
    }
    out
}

pub fn draw_note_pad(s: &mut Session, fb: &mut [u8]) {
    if !s.r.w.rules.h624_note_pad || !s.r.w.notes_ui.open || !s.r.active.contains(&pda::NOTE_PANEL) {
        return;
    }
    let lh = crate::world::dbox::LINE_H_262;
    if let Some(r) = item_rect(s, pda::NOTE_PANEL) {
        let lines = note_lines(s, r[2] - r[0] - 2);
        // MainScroll: the first line shown, clamped so the last page stays full
        let rows = ((r[3] - r[1]) / lh).max(1) as usize;
        let first = s.r.w.notes_ui.scroll.min(lines.len().saturating_sub(rows));
        let mut y = r[1];
        for l in lines.iter().skip(first) {
            match l {
                None => y += lh / 2,
                Some(l) => {
                    if y + lh > r[3] {
                        break;
                    }
                    text::draw_courier_clip(fb, r[0] + 1, y, l, NOTE_TEXT, r[3]);
                    y += lh;
                }
            }
        }
    }
    if s.r.active.contains(&pda::NOTE_DAY_TEXT) {
        if let Some(r) = item_rect(s, pda::NOTE_DAY_TEXT) {
            let t = (s.r.w.notes_ui.day + 1).to_string();
            let w = text::courier12().map_or(t.len() as i32 * 7, |f| f.width(&t));
            let x = ((r[0] + r[2]) / 2 - w / 2).clamp(0, SCREEN_W as i32 - 1);
            let y = ((r[1] + r[3]) / 2 - lh / 2).clamp(0, SCREEN_H as i32 - 1);
            text::draw_courier_clip(fb, x, y, &t, NOTE_TEXT, r[3] + 2);
        }
    }
}
