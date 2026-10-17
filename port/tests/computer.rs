//! The FBI computer (H570–H578 — static decodes; the login looks checked against the original's screenshots
//! `traces/fo-069.jsonl.shots/k22.png` / `k23.png`). Route to the login screen = fo-045's k0–k21 from
//! `data/bot/fo-040-end.x` (traced); the right-click bypass on the welcome picture = C63 (fo-077). Search keys, names and
//! numbers are read from the user's game by AssetRef id (`common::game_texts`) and the exe's tables, never spelled out.
mod common;
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::procs::computer::Src;

const LOGIN: &str = "p:70,210;p:570,210;p:570,210;p:251,281;p:320,210;p:83,282;p:320,210;p:466,300;p:320,210;p:70,210;p:320,210;p:412,147;p:320,210;p:171,271;p:320,210;p:187,223;p:320,210;p:570,210;p:2,92;p:70,210;p:570,210;p:318,200";
const ING: &str = ";r:106,54;p:68,110";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn session_at<T>(steps: &str, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    session_with(front::rules_hyp(), steps, f)
}

fn session_with<T>(rules: xf::world::Rules, steps: &str, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-040-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, rules);
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
    }
    Some(f(&mut s))
}

fn dark(fb: &[u8], x: usize, y: usize) -> bool {
    let o = (y * front::SCREEN_W + x) * 3;
    (fb[o] as u32 + fb[o + 1] as u32 + fb[o + 2] as u32) < 200
}

/// Mismatching pixels of the frame against a bitmap of the original ('#' = dark) from (x0, y0)
fn mismatches(fb: &[u8], x0: usize, y0: usize, rows: &[&str]) -> usize {
    let mut n = 0;
    for (j, r) in rows.iter().enumerate() {
        for (i, c) in r.chars().enumerate() {
            n += (dark(fb, x0 + i, y0 + j) != (c == '#')) as usize;
        }
    }
    n
}

/// A frame region as dark (1) / light (0) pixels: one hex string per row, the leftmost pixel in the top bit, each row
/// padded to whole hex digits, rows joined by '\n'
fn hex_rows(fb: &[u8], x0: usize, y0: usize, width: usize, height: usize) -> String {
    let rows: Vec<String> = (0..height)
        .map(|j| {
            (0..width.div_ceil(4))
                .map(|d| {
                    let bit = |k: usize| (d * 4 + k < width && dark(fb, x0 + d * 4 + k, y0 + j)) as u32;
                    char::from_digit(bit(0) << 3 | bit(1) << 2 | bit(2) << 1 | bit(3), 16).unwrap()
                })
                .collect()
        })
        .collect();
    rows.join("\n")
}

/// Text AssetRefs used as typed input: the login user, Cook's and Wong's ING names, a media key word of the tug
/// article, the NSA plate, the warehouse's and Cook's ING directory numbers
const USER: u32 = 0xe231;
const COOK: u32 = 0xed7a;
const WONG: u32 = 0xeda7;
const TUG_KEY: u32 = 0xe195;
const NSA_PLATE: u32 = 0x15a48;
const WAREHOUSE_NO: u32 = 0x10771;
const COOK_NO: u32 = 0x106cf;

/// A 10-digit number typed with dashes, 3-3-4 (the search keeps the digits)
fn dashed(n: &str) -> String {
    format!("{}-{}-{}", &n[..3], &n[3..6], &n[6..])
}

/// fo-069 k23, x 258..371, y 228..239: the pre-filled name of the original's screenshot as `hex_rows` (114 × 12),
/// compared by hash ([`common::fnv64`]) so the game's text is not spelled out as a bitmap
const NAME_K23_FNV: u64 = 0xba0d_6c95_5b36_f117;
/// The 4 pixels (x, y in the region) the original's name has and the port's glyph rendering does not
const NAME_K23_EXTRA: [(usize, usize); 4] = [(7, 3), (39, 3), (15, 7), (103, 7)];

/// `hex_rows` text with pixel (x, y) set to dark
fn set_pixel(rows: &str, x: usize, y: usize) -> String {
    let mut lines: Vec<Vec<char>> = rows.split('\n').map(|l| l.chars().collect()).collect();
    let c = &mut lines[y][x / 4];
    *c = char::from_digit(c.to_digit(16).unwrap() | 8 >> (x % 4), 16).unwrap();
    lines.iter().map(|l| l.iter().collect::<String>()).collect::<Vec<_>>().join("\n")
}
/// fo-069 k23, x 258..371, y 289..295: 14 '*' after typing the 14-character user text into the password field
const STARS_K23: [&str; 7] = [
    "..................................................................................................................",
    ".....#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#....",
    ".....#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#....",
    "...#####...#####...#####...#####...#####...#####...#####...#####...#####...#####...#####...#####...#####...#####..",
    ".....#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#.......#....",
    "....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#.....#.#...",
    "..................................................................................................................",
];

/// H570 / H574 / H578 vs fo-069 k22 / k23: name pre-filled (Pref_bArtificialIntuition on), focus and caret in the
/// password field, the typed password as '*' at the original's pixels
#[test]
fn h570_login_matches_fo069() {
    let Some([name]) = common::game_texts([USER]) else { return };
    let Some((focus, user, fb)) = session_at(LOGIN, |s| (s.r.w.dev.focus, s.r.w.pc_login_texts()[0].1.clone(), s.render()))
    else {
        return;
    };
    assert_eq!(focus, Some(0xc63c));
    assert_eq!(user, name.to_uppercase());
    let mut name_px = hex_rows(&fb, 258, 228, 114, 12);
    for &(x, y) in &NAME_K23_EXTRA {
        assert!(!dark(&fb, 258 + x, 228 + y), "pixel ({x},{y}) of the name is drawn now: update NAME_K23_EXTRA");
        name_px = set_pixel(&name_px, x, y);
    }
    assert_eq!(common::fnv64(&name_px), NAME_K23_FNV, "name pixels differ from fo-069 k23");
    // k22: the caret's two black columns at x 261..262, y 288..301
    for y in 288..302 {
        assert!(dark(&fb, 261, y) && dark(&fb, 262, y) && !dark(&fb, 260, y) && !dark(&fb, 263, y), "caret row {y}");
    }
    assert_eq!(name.chars().count(), 14);
    let fb = session_at(&format!("{LOGIN};t:{name}"), |s| s.render()).unwrap();
    assert_eq!(mismatches(&fb, 258, 289, &STARS_K23), 0);
}

/// H575: Phone, the warehouse's number (directory text 0x10771, typed with dashes) → the warehouse address (0xa4f1),
/// SHANKSN2_bFoundWarehouseInING 0x13bb := 1
#[test]
fn h575_ing_phone_warehouse() {
    let Some([no]) = common::game_texts([WAREHOUSE_NO]) else { return };
    let Some((v, msg, field)) = session_at(&format!("{LOGIN}{ING};p:338,300;t:{};k:13", dashed(&no)), |s| {
        let t = |i: u32| s.r.w.pc.texts.iter().find(|x| x.0 == i).map(|x| x.1.clone());
        (s.r.w.vars.get(0x13bb), t(0xe3e8), t(0xc636))
    }) else {
        return;
    };
    assert_eq!(v, Some(1));
    assert_eq!(msg, Some(Src::Asset(0xa4f1)));
    assert_eq!(field, Some(Src::Str(String::new())));
}

/// H575: Name = Cook's (key word 0xed7a, typed in lower case) — FBI shows record 3 (texts 0x8ae8.. from 0x13842's
/// list), Civilian finds nothing (0xe3ec)
#[test]
fn h575_ing_name_database() {
    let Some([cook]) = common::game_texts([COOK]) else { return };
    let cook = cook.to_lowercase();
    let Some((shown, name)) = session_at(&format!("{LOGIN}{ING};p:338,373;t:{cook};k:13"), |s| {
        (s.r.w.pc.ing_shown, s.r.w.pc.texts.iter().find(|x| x.0 == 0x8ae8).map(|x| x.1.clone()))
    }) else {
        return;
    };
    assert!(shown);
    assert_eq!(name, Some(Src::Asset(0x92c8)));
    let (shown, msg, typed) = session_at(&format!("{LOGIN}{ING};t:{cook};k:13"), |s| {
        (s.r.w.pc.ing_shown, s.r.w.pc.texts.iter().find(|x| x.0 == 0xe3e8).map(|x| x.1.clone()), s.r.w.pc.ing.clone())
    })
    .unwrap();
    assert!(!shown);
    assert_eq!(msg, Some(Src::Asset(0xe3ec)));
    assert_eq!(typed, cook.to_uppercase(), "a failed search keeps the typed text");
}

/// H575: Wong's record (key word 0xeda7) needs SHANKSN2_bDoneWithWong (0 in fo-040-end.x) → the network-congestion
/// message 0x16437
#[test]
fn h575_ing_wong_congestion() {
    let Some([wong]) = common::game_texts([WONG]) else { return };
    let Some(msg) = session_at(&format!("{LOGIN}{ING};p:338,415;t:{};k:13", wong.to_lowercase()), |s| {
        s.r.w.pc.texts.iter().find(|x| x.0 == 0xe3e8).map(|x| x.1.clone())
    }) else {
        return;
    };
    assert_eq!(msg, Some(Src::Asset(0x16437)));
}

/// H577: the inbox rows show the "From:" / "Subject:" lines of the save's three mails; a row click reads the mail
#[test]
fn h577_mail_rows_and_reader() {
    let Some((rows, fb)) = session_at(&format!("{LOGIN};r:106,54;p:68,193"), |s| {
        let t = |i: u32| s.r.w.pc.texts.iter().find(|x| x.0 == i).map(|x| x.1.clone());
        ([t(0x958a), t(0xa6ed), t(0x958d)], s.render())
    }) else {
        return;
    };
    assert_eq!(rows[0], Some(Src::MailLine(0xfc11, 0)));
    assert_eq!(rows[1], Some(Src::MailLine(0xfc11, 1)));
    assert_eq!(rows[2], Some(Src::Str(String::new())));
    assert!((186..400).any(|x| (98..116).any(|y| dark(&fb, x, y))), "first row text drawn");
    let p = session_at(&format!("{LOGIN};r:106,54;p:68,193;p:390,118"), |s| s.r.w.pc.panels.clone()).unwrap();
    assert!(p.iter().any(|x| x.0 == 0xd6a6 && x.1 == Src::Asset(0xfc11)), "{p:x?}");
}

/// H650 (H605): the ING buttons 0x8de0 / 0x3b16 / 0x8de1 / 0x8de2 / 0x9a84 test ` scGameLanguage` 0xf9d != 0xf9f in their
/// Activation (0x10df3 …) — texts 'english' / 'english' are equal, so no RelocateIFace moves their hit rects; with raw
/// objIds (0x14d1 ≠ 0xf9e, rule off) they are moved. AG check: AG-BG4.
#[test]
fn h650_ing_buttons_stay_in_place() {
    const BUTTONS: [u32; 5] = [0x8de0, 0x3b16, 0x8de1, 0x8de2, 0x9a84];
    let probe = |s: &mut Session| {
        let shown: Vec<u32> = s.r.w.layouts().iter().flat_map(|l| l.1.iter().map(|i| i.0)).filter(|i| BUTTONS.contains(i)).collect();
        let moved: Vec<u32> = BUTTONS.iter().copied().filter(|i| s.r.w.px.item_rects.contains_key(i)).collect();
        (shown, moved, s.r.w.vars.text(0xf9d))
    };
    let Some((shown, moved, lang)) = session_at(&format!("{LOGIN}{ING}"), probe) else {
        return;
    };
    assert!(!shown.is_empty(), "ING buttons shown");
    assert_eq!(lang.as_deref(), Some(&b"english"[..]));
    assert!(moved.is_empty(), "moved: {moved:x?}");
    let mut off = front::rules_hyp();
    off.h650_string_compare = false;
    let (shown_off, moved_off, _) = session_with(off, &format!("{LOGIN}{ING}"), probe).unwrap();
    assert_eq!(shown_off, shown);
    assert_eq!(moved_off, shown, "raw objIds: every shown button relocated");
}

/// Like `session_at`, rendering after every step (the front measures the text panels for MainScroll, H791)
fn session_rendered<T>(steps: &str, prep: impl FnOnce(&mut Session), f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-040-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, front::rules_hyp());
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    prep(&mut s);
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
        let _ = s.render();
    }
    Some(f(&mut s))
}

const MEDIA: &str = ";r:106,54;p:68,136";

/// H790: the tug article's first key word (0xe195) → the Coast Guard article 0xd927 in the media panel 0xd701, Forward
/// 0xd6fc enabled, the AssetRef 0x524c (a Node 5 video) enabled, the field emptied; an unknown word → 0xe3ec, Forward off
#[test]
fn h790_media_search() {
    let Some([key]) = common::game_texts([TUG_KEY]) else { return };
    let Some((panels, shown, tug, fwd, typed, results)) = session_at(&format!("{LOGIN}{MEDIA};t:{key};k:13"), |s| {
        let w = &s.r.w;
        (
            w.pc.panels.clone(),
            w.pc.last_shown,
            w.is_enabled(xf::world::class::ASSET_REF, 0x524c),
            w.is_enabled(xf::world::class::INTERFACE_ITEM, 0xd6fc),
            w.pc.media.clone(),
            w.pc.texts.iter().find(|x| x.0 == 0xd6ff).map(|x| x.1.clone()),
        )
    }) else {
        return;
    };
    assert!(panels.iter().any(|p| p.0 == 0xd701 && p.1 == Src::Asset(0xd927)), "{panels:x?}");
    assert_eq!(shown, 0xd927);
    assert!(tug, "0x524c enabled");
    assert!(fwd, "Forward enabled");
    assert_eq!(typed, "");
    assert_eq!(results, Some(Src::Str(key.to_uppercase())));
    let (panels, shown, fwd) = session_at(&format!("{LOGIN}{MEDIA};t:zebra;k:13"), |s| {
        (s.r.w.pc.panels.clone(), s.r.w.pc.last_shown, s.r.w.is_enabled(xf::world::class::INTERFACE_ITEM, 0xd6fc))
    })
    .unwrap();
    assert!(panels.iter().any(|p| p.0 == 0xd701 && p.1 == Src::Asset(0xe3ec)), "{panels:x?}");
    assert_eq!(shown, 0);
    assert!(!fwd);
}

/// H790 / H578: the media caret follows the typed text (0x179 + 8 n, 0x32)
#[test]
fn h790_media_caret() {
    let Some(pos) = session_at(&format!("{LOGIN}{MEDIA};t:abc"), |s| s.r.w.pc_item_pos(0xfbba)) else { return };
    assert_eq!(pos, Some((377 + 24, 50)));
}

/// H791: MainScroll over the media article — line down 0xd999, page down 0x10387, line up 0xd998, page up 0x10382;
/// clamped to the text's end; the drawn first line changes
#[test]
fn h791_media_panel_scrolls() {
    let Some([key]) = common::game_texts([TUG_KEY]) else { return };
    let base = format!("{LOGIN}{MEDIA};t:{key};k:13");
    let probe = |s: &mut Session| (s.r.w.pc.scroll, s.r.w.pc.scroll_max, s.r.w.pc.scroll_page, s.render());
    let Some((s0, max, page, fb0)) = session_rendered(&base, |_| {}, probe) else { return };
    assert_eq!(s0, 0);
    let max = max.expect("measured");
    assert!(max > 0 && page > 1, "max {max} page {page}");
    let (s1, _, _, fb1) = session_rendered(&format!("{base};p:606,460"), |_| {}, probe).unwrap();
    assert_eq!(s1, 1);
    assert_ne!(fb0[(160 * 640) * 3..(176 * 640) * 3], fb1[(160 * 640) * 3..(176 * 640) * 3], "first line moved");
    let (s2, ..) = session_rendered(&format!("{base};p:606,460;p:606,442"), |_| {}, probe).unwrap();
    assert_eq!(s2, (1 + page).min(max));
    let (s3, ..) = session_rendered(&format!("{base};p:606,442;p:606,442;p:606,442;p:606,442;p:606,442"), |_| {}, probe).unwrap();
    assert_eq!(s3, max, "clamped at the end");
    let (s4, ..) = session_rendered(&format!("{base};p:606,442;p:606,442;p:606,160"), |_| {}, probe).unwrap();
    assert_eq!(s4, (2 * page).min(max) - 1);
    let (s5, ..) = session_rendered(&format!("{base};p:606,442;p:606,178"), |_| {}, probe).unwrap();
    assert_eq!(s5, 0);
}

/// H892 / R35 / H891 (fo-114, `traces/fo-114.jsonl.burst/k13-0.png`, `k15-0.png`, `traces/fo-114.jsonl.shots/k16.png`):
/// Cook's field notes (ING phone: Cook's directory number 0x106cf) — line down 0 → 1, page down 1 → 5, page up 5 → **0** (the text was
/// scrolled by 5 × 17 = 85 px ≤ the view's 93 px less half a line → FUN_005511b0, to the top); Courier New 14 rows 17 px
/// apart as on the original (ink rows 376, 393, 410, 427, 444 in k16)
#[test]
fn h892_field_notes_page_up_to_top_fo114() {
    let Some([no]) = common::game_texts([COOK_NO]) else { return };
    let base = format!("{LOGIN}{ING};p:338,300;t:{};k:13", dashed(&no));
    let probe = |s: &mut Session| (s.r.w.pc.scroll, s.render());
    let Some((s0, fb0)) = session_rendered(&base, |_| {}, probe) else { return };
    assert_eq!(s0, 0);
    let ink = |fb: &[u8], y: usize| (101..300).filter(|&x| dark(fb, x, y)).count();
    for y in [376, 393, 410, 427, 444] {
        assert!(ink(&fb0, y) > 20, "row at y {y}: {}", ink(&fb0, y));
    }
    let (s1, _) = session_rendered(&format!("{base};p:309,456"), |_| {}, probe).unwrap();
    assert_eq!(s1, 1);
    let (s2, _) = session_rendered(&format!("{base};p:309,456;p:309,442"), |_| {}, probe).unwrap();
    assert_eq!(s2, 5);
    let (s3, fb3) = session_rendered(&format!("{base};p:309,456;p:309,442;p:309,388"), |_| {}, probe).unwrap();
    assert_eq!(s3, 0, "page up from line 5 goes to the top");
    let panel = |fb: &[u8]| -> Vec<bool> { (369..462).flat_map(|y| (101..300).map(move |x| (x, y))).map(|(x, y)| dark(fb, x, y)).collect() };
    assert!(panel(&fb0) == panel(&fb3), "the panel shows the first lines again");
}

/// H791: the APB screen's panel 0xc639 shows XT APB.txt (0xd93f) through ScrollTextList when IFL_bOffToFileTheAPB 0xaa3
/// equals 0x25d's value
#[test]
fn h791_apb_scroll_text_list() {
    let Some(panels) = session_rendered(
        &format!("{LOGIN};r:106,54;p:68,221"),
        |s| {
            let v = s.r.w.vars.get(0x25d);
            s.r.w.vars.set_native(0xaa3, v, 2);
        },
        |s| s.r.w.pc.panels.clone(),
    ) else {
        return;
    };
    assert!(panels.iter().any(|p| p.0 == 0xc639 && p.1 == Src::Asset(0xd93f)), "{panels:x?}");
}

/// H792: the login caret blinks — XG/69328.pff entry 0 (black bar) for 500 ms, entry 1 (white) for 500 ms
#[test]
fn h792_caret_blinks() {
    let Some((a, b, c)) = session_at(LOGIN, |s| {
        let a = s.render();
        s.tick(600);
        let b = s.render();
        s.tick(500);
        let c = s.render();
        (a, b, c)
    }) else {
        return;
    };
    let caret = |fb: &[u8]| (288..302).all(|y| dark(fb, 261, y) && dark(fb, 262, y));
    assert!(caret(&a), "shown at first");
    assert!(!caret(&b) && !dark(&b, 261, 295), "off after 600 ms");
    assert!(caret(&c), "on again after 1100 ms");
}

/// H793: a photo of the NSA car's plate (TakePhoto's AssetRef 0x1372e, x.pff 617) — Download marks it downloaded and
/// empties the camera, the thumbnail is the picture at 80×60 in slot 0xc8b0 (138,165), Enlarge 0 shows it at 160×120 in
/// 0xc8c4 (420,32) and enables Delete; Delete removes it
#[test]
fn h793_photo_album() {
    let prep = |s: &mut Session| {
        s.r.w.pc.photos.push((0x1372e, false));
        s.r.w.pc.camera = 1;
    };
    let base = format!("{LOGIN};r:106,54;p:68,165");
    let Some((dl, del)) = session_rendered(&base, prep, |s| {
        let w = &s.r.w;
        (w.is_enabled(xf::world::class::INTERFACE_ITEM, 0xc8c5), w.is_enabled(xf::world::class::INTERFACE_ITEM, 0xc8c6))
    }) else {
        return;
    };
    assert!(dl && !del, "Download on, Delete off");
    let (photos, camera, shown, fb) =
        session_rendered(&format!("{base};p:171,124"), prep, |s| (s.r.w.pc.photos.clone(), s.r.w.pc.camera, s.r.w.album_shown(), s.render()))
            .unwrap();
    assert_eq!(photos, vec![(0x1372e, true)]);
    assert_eq!(camera, 0);
    assert_eq!(shown, vec![0]);
    // the thumbnail: x.pff 617 is 300×240 → pixel (x, y) of the slot = picture (x·300/80, y·240/60)
    let pic = xf::media::pict::Pff::open(&root().join("data/InstalledGame/x.pff")).unwrap().picture(617).unwrap();
    let at = |fb: &[u8], x: usize, y: usize| fb[(y * 640 + x) * 3..][..3].to_vec();
    let src = |x: usize, y: usize| pic.rgb[(y * pic.width + x) * 3..][..3].to_vec();
    for (x, y) in [(0, 0), (40, 30), (79, 59), (13, 51)] {
        assert_eq!(at(&fb, 138 + x, 165 + y), src(x * 300 / 80, y * 240 / 60), "thumb ({x},{y})");
    }
    let (enl, del, fb) = session_rendered(&format!("{base};p:171,124;p:178,195"), prep, |s| {
        (s.r.w.pc.enlarged, s.r.w.is_enabled(xf::world::class::INTERFACE_ITEM, 0xc8c6), s.render())
    })
    .unwrap();
    assert_eq!(enl, Some(0));
    assert!(del, "Delete on");
    for (x, y) in [(0, 0), (80, 60), (159, 119)] {
        assert_eq!(at(&fb, 420 + x, 32 + y), src(x * 300 / 160, y * 240 / 120), "enlarged ({x},{y})");
    }
    let (photos, enl) = session_rendered(&format!("{base};p:171,124;p:178,195;p:252,124"), prep, |s| (s.r.w.pc.photos.clone(), s.r.w.pc.enlarged))
        .unwrap();
    assert!(photos.is_empty());
    assert_eq!(enl, None);
}

/// H793: 26 downloaded photos — 20 shown; PhotoScroll(1) moves a row (5) on while a page is full, PhotoScroll(0) back
#[test]
fn h793_photo_scroll() {
    let prep = |s: &mut Session| {
        for _ in 0..26 {
            s.r.w.pc.photos.push((0x1372e, true));
        }
    };
    let base = format!("{LOGIN};r:106,54;p:68,165");
    let Some((n, off)) = session_rendered(&base, prep, |s| (s.r.w.album_shown().len(), s.r.w.pc.photo_offset)) else { return };
    assert_eq!((n, off), (20, 0));
    let (n, off) = session_rendered(&format!("{base};p:575,420"), prep, |s| (s.r.w.album_shown().len(), s.r.w.pc.photo_offset)).unwrap();
    assert_eq!((n, off), (20, 5));
    let (n, off) = session_rendered(&format!("{base};p:575,420;p:575,420"), prep, |s| (s.r.w.album_shown().len(), s.r.w.pc.photo_offset)).unwrap();
    assert_eq!((n, off), (16, 10), "a full page scrolls on even past the end (the original tests 20 shown)");
    let (n, off) = session_rendered(&format!("{base};p:575,420;p:575,420;p:576,167"), prep, |s| (s.r.w.album_shown().len(), s.r.w.pc.photo_offset)).unwrap();
    assert_eq!((n, off), (20, 5));
}

/// H796: Forward on Cook's ING record — the compose screen's subject 0x9df5 = the record's name asset, the enclosure
/// 0x9df4 = the exe's first enclosure name, Send 0x9e36 disabled
#[test]
fn h796_forward_ing_record() {
    let Some([cook]) = common::game_texts([COOK]) else { return };
    let steps = format!("{LOGIN}{ING};p:338,373;t:{};k:13;p:383,442", cook.to_lowercase());
    let Some((subj, encl, kind, first)) = session_at(&steps, |s| {
        let t = |i: u32| s.r.w.pc.texts.iter().find(|x| x.0 == i).map(|x| x.1.clone());
        (t(0x9df5), t(0x9df4), s.r.w.pc.forward, s.r.w.h.exe.pc.enclosures[0].clone())
    }) else {
        return;
    };
    assert_eq!(kind, b'I');
    assert_eq!(encl, Some(Src::Str(first)));
    assert_eq!(subj, Some(Src::Asset(0x92c8)));
}

/// H795: the NSA plate (text 0x15a48) in Gov't/Military — first search: iUberParanoia 0x35b + 2 (type 1), 0x1269 := 1
#[test]
fn h795_nsa_plate_paranoia() {
    let Some([plate]) = common::game_texts([NSA_PLATE]) else { return };
    let Some((p1, nsa)) =
        session_at(&format!("{LOGIN}{ING};p:338,320;p:338,387;t:{plate};k:13"), |s| (s.r.w.vars.get(0x35b), s.r.w.vars.get(0x1269)))
    else {
        return;
    };
    let before = session_at(LOGIN, |s| s.r.w.vars.get(0x35b)).unwrap();
    assert!(before.is_some(), "iUberParanoia in the save");
    assert_eq!(nsa, Some(1));
    assert_eq!(p1, before.map(|v| v + 2));
}

/// H893 / R36 (fo-115, `traces/fo-115.jsonl.shots/k14.png`): the restricted-data message 0xe3e8 after the NSA plate in
/// Gov't/Military is red (255,0,0) — the original's text colour 6 (FUN_00428ec0) — one line from x 331, cut pixel-wise at
/// the item's right edge (the original's ink ends at x 606, y 83..95)
#[test]
fn h893_restricted_message_red_clipped_fo115() {
    let Some([plate]) = common::game_texts([NSA_PLATE]) else { return };
    let Some(fb) = session_rendered(&format!("{LOGIN}{ING};p:338,320;p:338,387;t:{plate};k:13"), |_| {}, |s| s.render()) else {
        return;
    };
    let red: Vec<(usize, usize)> = (78..100)
        .flat_map(|y| (320..640).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let o = (y * front::SCREEN_W + x) * 3;
            fb[o] > 200 && fb[o + 1] < 80 && fb[o + 2] < 80
        })
        .collect();
    assert!(red.len() > 300, "red ink {}", red.len());
    let (x0, x1) = (red.iter().map(|p| p.0).min().unwrap(), red.iter().map(|p| p.0).max().unwrap());
    let (y0, y1) = (red.iter().map(|p| p.1).min().unwrap(), red.iter().map(|p| p.1).max().unwrap());
    eprintln!("red ink x {x0}..{x1} y {y0}..{y1} ({} px)", red.len());
    assert_eq!(x0, 331);
    assert!((604..=608).contains(&x1), "clipped at the right edge: {x1}");
}

/// The AI icon (EvidenceIcon 0xdb79, bar 6 at (530,48)) is placed while the login screen is up but not seen: fo-069
/// k22 / k23 have (2,2,2) everywhere in (520,40)–(600,95); the port (H620's order: bars under the dimming layout's items)
/// too — also with the icon glowing (H622 spin)
#[test]
fn login_hides_ai_icon() {
    for glow in [false, true] {
        let Some(fb) = session_rendered(LOGIN, |s| s.r.w.ai.glow = glow, |s| {
            s.tick(250);
            s.render()
        }) else {
            return;
        };
        for y in 40..95 {
            for x in 520..600 {
                let o = (y * 640 + x) * 3;
                assert_eq!(&fb[o..o + 3], &[2, 2, 2], "({x},{y}) glow {glow}");
            }
        }
    }
}

/// H797: Forward Cook's record, TO box → picker 0x9dd3 (the enabled cards of 0xb0ff, alphabetical), row 1 (card 0xfa25)
/// selected (marker 0x9de1), click outside → To = [0xfa25] shown in 0x9dee, Send enabled; Send → the reply of reaction
/// item 0xf8be for recipient index 4 (Seitz, 0xef86) pending
#[test]
fn h797_forward_to_reply() {
    let Some([cook]) = common::game_texts([COOK]) else { return };
    let fwd = format!("{LOGIN}{ING};p:338,373;t:{};k:13;p:383,442", cook.to_lowercase());
    let Some((cards, rows, mark)) = session_at(&format!("{fwd};p:430,120;p:420,126"), |s| {
        let t = |i: u32| s.r.w.pc.texts.iter().find(|x| x.0 == i).map(|x| x.1.clone());
        (s.r.w.pc.addr_cards.clone(), [t(0x9de7), t(0x9de8)], s.r.w.is_enabled(xf::world::class::INTERFACE_ITEM, 0x9de1))
    }) else {
        return;
    };
    assert_eq!(cards.first().map(|c| c.0), Some(0xb0e4), "{cards:x?}");
    assert_eq!(cards.get(1), Some(&(0xfa25, true)));
    assert_eq!(rows, [Some(Src::Asset(0xb0e4)), Some(Src::Asset(0xfa25))]);
    assert!(mark, "marker of row 1 on");
    let (to, row, send) = session_at(&format!("{fwd};p:430,120;p:420,126;p:400,300"), |s| {
        let t = |i: u32| s.r.w.pc.texts.iter().find(|x| x.0 == i).map(|x| x.1.clone());
        (s.r.w.pc.addr_to.clone(), t(0x9dee), s.r.w.is_enabled(xf::world::class::INTERFACE_ITEM, 0x9e36))
    })
    .unwrap();
    assert_eq!(to, vec![0xfa25]);
    assert_eq!(row, Some(Src::Asset(0xfa25)));
    assert!(send, "Send enabled");
    let (pending, inbox) =
        session_at(&format!("{fwd};p:430,120;p:420,126;p:400,300;p:93,376"), |s| (s.r.w.px.pending.clone(), s.r.w.px.inbox.clone()))
            .unwrap();
    assert!(pending.contains(&0xef86) || inbox.contains(&0xef86), "{pending:x?} {inbox:x?}");
}

/// H796 / H797: Forward the tug article (media Forward 0xd6fc at (476,92)) — subject = the exe's article subject, the
/// article in the compose panel 0x9df6; To = Cook (picker row 2) → Send → Cook's reply 0xef4b pending
#[test]
fn h797_forward_article_to_cook() {
    let Some([key]) = common::game_texts([TUG_KEY]) else { return };
    let fwd = format!("{LOGIN}{MEDIA};t:{key};k:13;p:476,92");
    let Some((subj, panels, want)) = session_at(&fwd, |s| {
        let w = &s.r.w;
        (w.pc.texts.iter().find(|x| x.0 == 0x9df5).map(|x| x.1.clone()), w.pc.panels.clone(), w.h.exe.pc.article_subject.clone())
    }) else {
        return;
    };
    assert_eq!(subj, Some(Src::Str(want)));
    assert!(panels.iter().any(|p| p.0 == 0x9df6 && p.1 == Src::Asset(0xd927)), "{panels:x?}");
    let (pending, inbox) =
        session_at(&format!("{fwd};p:430,120;p:420,142;p:400,300;p:93,376"), |s| (s.r.w.px.pending.clone(), s.r.w.px.inbox.clone()))
            .unwrap();
    assert!(pending.contains(&0xef4b) || inbox.contains(&0xef4b), "{pending:x?} {inbox:x?}");
}

/// A session from `save` with `prep` before the steps (rendering after each step)
fn session_from<T>(save: &str, steps: &str, prep: impl FnOnce(&mut Session), f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, front::rules_hyp());
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    prep(&mut s);
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
        let _ = s.render();
    }
    Some(f(&mut s))
}

/// H797: the PDA note pad's letter — fo-067-end.x with day 1's key note 0xf711 added (the address link 0xf7bf on):
/// link → picker 0x914d (DisplayAddrs 'P'), Cook's row → compose 0x9e3d: 0x9e44 shows Cook's address, the subject = the
/// exe's field-notes subject, the body = the day's notes, iNumWillmoreEMailsToCook 0x705 + 1; Send 0xfb4d → reaction 0xf884's reply for
/// Cook (index 1) pending
#[test]
fn h797_note_pad_letter_to_cook() {
    let prep = |s: &mut Session| s.r.w.px.notes.push(0xf711);
    let base = "m:20,440;p:20,440;p:281,399;p:319,373";
    let Some((cards, n0)) = session_from("fo-067-end.x", base, prep, |s| (s.r.w.pc.addr_cards.clone(), s.r.w.vars.get(0x705))) else {
        return;
    };
    let cook = cards.iter().position(|c| c.0 == 0xb0ea).expect("Cook's card enabled");
    let rows = [(226, 237), (226, 251), (226, 265), (226, 279), (226, 293), (226, 307), (226, 321), (226, 335), (226, 349)];
    let (x, y) = rows[cook];
    let (to, subj, body, n1, want) = session_from("fo-067-end.x", &format!("{base};p:{},{y}", x + 50), prep, |s| {
        let t = |i: u32| s.r.w.pc.texts.iter().find(|x| x.0 == i).map(|x| x.1.clone());
        (t(0x9e44), t(0x9fa9), s.r.w.pc.panels.clone(), s.r.w.vars.get(0x705), s.r.w.h.exe.pc.field_notes.clone())
    })
    .unwrap();
    assert_eq!(to, Some(Src::Asset(0xb0ea)));
    assert_eq!(subj, Some(Src::Str(want)));
    assert!(body.iter().any(|p| p.0 == 0x9fa8 && matches!(&p.1, Src::Notes(v) if v.contains(&0xf709))), "{body:x?}");
    assert_eq!(n1, n0.map(|v| v + 1));
    let reply = session_from("fo-067-end.x", "", |_| {}, |s| s.r.w.h.list_of(xf::world::class::ASSET_REF_LIST,
        s.r.w.h.read(xf::world::class::INTERFACE_ITEM, 0xf884).map_or(0, |r| r.u32("f44")))[1]).unwrap();
    let (pending, inbox) = session_from("fo-067-end.x", &format!("{base};p:{},{y};p:313,373", x + 50), prep, |s| {
        (s.r.w.px.pending.clone(), s.r.w.px.inbox.clone())
    })
    .unwrap();
    assert!(pending.contains(&reply) || inbox.contains(&reply), "{reply:#x}: {pending:x?} {inbox:x?}");
}

/// H894 / R37 (fo-116, `traces/fo-116.jsonl.burst/k15-0.png`): the TO picker 0x9dd3 over the compose screen 0x9516 —
/// the rows start with Skinner's card 0xb0f4 (all 9 cards of 0xb0ff enabled in fo-078-end.x) and the compose screen is
/// not darkened: (300,300) stays (253,253,253) as on the original
#[test]
fn h894_picker_does_not_dim_compose_fo116() {
    let Some([cook]) = common::game_texts([COOK]) else { return };
    let p = format!(
        "m:20,440;p:21,440;p:239,322;p:326,234;p:161,210;p:215,212;p:96,192;p:319,240;r:106,54;p:68,109;p:338,373;t:{};k:13;p:382,441;p:432,123",
        cook.to_lowercase()
    );
    let Some((cards, fb)) = session_from("fo-078-end.x", &p, |_| {}, |s| (s.r.w.pc.addr_cards.clone(), s.render())) else {
        return;
    };
    assert_eq!(cards.first().map(|c| c.0), Some(0xb0f4), "Skinner's card first");
    let o = (300 * front::SCREEN_W + 300) * 3;
    assert_eq!(&fb[o..o + 3], &[253, 253, 253], "the compose screen is not darkened under the picker");
}
