//! UI drawing (front::ui / front::text) against screenshots of the original: frames rendered headless at the states of
//! the bot's screenshots, key pixels and per-row text pixel counts compared with numbers measured on the PNGs.
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Load `save`, run `steps` (front::script syntax), settle after each, render.
fn frame_at(save: &str, steps: &str) -> Option<Vec<u8>> {
    session_at(save, steps, |s| s.render())
}

/// Load `save`, run `steps`, then `f` on the session.
fn session_at<T>(save: &str, steps: &str, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
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
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
        s.render(); // a frame after each step, as the window draws continuously (UI state between frames)
    }
    Some(f(&mut s))
}

fn px(fb: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * front::SCREEN_W + x) * 3;
    [fb[o], fb[o + 1], fb[o + 2]]
}

/// (y, count of text-coloured pixels in the talk panel's columns) of every row with text
fn text_rows(fb: &[u8], x0: usize) -> Vec<(usize, usize)> {
    (305..395)
        .map(|y| (y, (x0 + 11..x0 + 223).filter(|&x| px(fb, x, y) == front::ui::TEXT).count()))
        .filter(|r| r.1 > 0)
        .collect()
}

/// Every row's text pixel count within `tol` of the reference's (a row missing on one side counts 0).
fn rows_match(got: &[(usize, usize)], want: &[(usize, usize)], tol: usize) {
    let get = |v: &[(usize, usize)], y: usize| v.iter().find(|r| r.0 == y).map_or(0, |r| r.1);
    for y in 300..400 {
        let (a, b) = (get(got, y), get(want, y));
        assert!(a.abs_diff(b) <= tol, "row {y}: {a} pixels, screenshot {b}");
    }
}

/// fo-055 k01 (`traces/fo-055.jsonl.shots/k01.png`): Skinner's box after Load fo-054-end.x + click (105,190): box picture
/// at (20,285) (frame (30,30,30) at (21,350)), three rows (0x57ca, 0x57a3 on two lines, 0x57c1) in (205,205,205); text
/// pixel rows measured on the PNG.
#[test]
fn fo055_k01_dialogue_box() {
    let Some(fb) = frame_at("fo-054-end.x", "p:105,190") else { return };
    assert_eq!(px(&fb, 21, 350), [30, 30, 30]);
    assert_eq!(px(&fb, 22, 400), [0, 0, 0]);
    // H300: the body darkens the View under it (k01 (22,297) = (8,8,7) over k00 (25,24,20))
    let p = px(&fb, 100, 300);
    assert!(p.iter().all(|&v| v < 30), "{p:?}");
    if front::text::courier12().is_none() {
        return; // no Courier New on this machine: the fallback font is not compared
    }
    let want = [
        (309, 6),
        (310, 4),
        (311, 41),
        (312, 22),
        (313, 28),
        (314, 20),
        (315, 18),
        (316, 43),
        (317, 1),
        (318, 3),
        (331, 6),
        (332, 2),
        (333, 91),
        (334, 49),
        (335, 65),
        (336, 41),
        (337, 43),
        (338, 95),
        (339, 1),
        (340, 3),
        (346, 2),
        (347, 3),
        (348, 40),
        (349, 21),
        (350, 31),
        (351, 17),
        (352, 17),
        (353, 40),
        (354, 1),
        (355, 3),
        (367, 1),
        (368, 9),
        (369, 5),
        (370, 94),
        (371, 49),
        (372, 55),
        (373, 45),
        (374, 44),
        (375, 97),
        (376, 2),
        (377, 6),
    ];
    rows_match(&text_rows(&fb, 20), &want, 3);
}

/// fo-055 k02: then click (505,187) → Shanks's box at (374,285) with one two-line row (0x5691).
#[test]
fn fo055_k02_dialogue_box() {
    let Some(fb) = frame_at("fo-054-end.x", "p:105,190;p:505,187") else { return };
    assert_eq!(px(&fb, 375, 350), [30, 30, 30]);
    assert_eq!(px(&fb, 21, 350), [0, 0, 0], "Skinner's box is gone");
    if front::text::courier12().is_none() {
        return;
    }
    let want = [
        (309, 10),
        (310, 5),
        (311, 89),
        (312, 50),
        (313, 54),
        (314, 39),
        (315, 44),
        (316, 84),
        (317, 3),
        (318, 5),
        (325, 2),
        (326, 20),
        (327, 8),
        (328, 14),
        (329, 8),
        (330, 8),
        (331, 23),
        (332, 2),
    ];
    rows_match(&text_rows(&fb, 374), &want, 3);
}

/// fo-060 k11 (`traces/fo-060.jsonl.shots/k11.png`): Load fo-059-end.x, the first 10 steps of fo-060 (badge taken from the
/// drawer and selected): with the inventory panel hidden (prefs 'H') only the selected badge stays in the bar, at
/// (301,420)–(331,460); the game's EvidenceIcon 0xdb79 (bar 6) at (530,48). The bottom band is black elsewhere. Colours
/// within 4 of the screenshot's for most badge pixels (the original's 16-bit screen / PICT decoding differ slightly).
#[test]
fn fo060_k11_inventory_bar() {
    let steps = "m:20,440;p:21,440;p:239,322;p:326,234;p:161,210;p:215,212;p:318,309;p:236,178;p:201,174;p:316,440";
    let Some(fb) = frame_at("fo-059-end.x", steps) else { return };
    // samples from the PNG: (x, y, rgb)
    let want = [
        (305, 425, [247, 247, 247]),
        (310, 430, [253, 253, 253]),
        (301, 421, [31, 31, 31]),
        (316, 445, [255, 255, 255]),
    ];
    for (x, y, c) in want {
        let p = px(&fb, x, y);
        assert!(p.iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 4), "({x},{y}) {p:?} vs {c:?}");
    }
    // nothing else drawn in the bottom band (y 331..480) outside the badge
    let mut stray = 0;
    for y in 331..480 {
        for x in 0..640 {
            let inside = (301..331).contains(&x) && (420..460).contains(&y);
            if !inside && px(&fb, x, y) != [0, 0, 0] {
                stray += 1;
            }
        }
    }
    assert_eq!(stray, 0);
    // the top-right EvidenceIcon 0xdb79: some non-black pixels in (530,48)–(594,78), as in the screenshot
    let lit =
        (48..78).flat_map(|y| (530..594).map(move |x| (x, y))).filter(|&(x, y)| px(&fb, x, y) != [0, 0, 0]).count();
    assert!(lit > 300, "{lit}");
}

/// fo-056 k20 (`traces/fo-056.jsonl.shots/k20.png`, the end of fo-056's 20 steps from fo-054-end.x): Rudy's box at
/// (185,285) with more rows than the talk panel holds — the scrollbar (H304: bar at full opacity, arrows and thumb at
/// 0.41), the last row cut at the panel's bottom, and no asked line yet → the HISTORY tab dimmed (H305).
#[test]
fn fo056_k20_scrollbar_history_tab() {
    let steps = "p:105,190;h:117,336,57a3;p:505,187;h:499,350,5686;p:536,92;p:570,210;p:287,209;p:220,370;p:314,209;\
        p:70,210;p:470,209;p:215,212;p:424,286;p:2,2;p:460,183;p:70,210;p:475,210;p:193,210;p:322,210;p:308,201";
    let Some(fb) = frame_at("fo-054-end.x", steps) else { return };
    let near = |p: [u8; 3], c: [u8; 3]| p.iter().zip(c).all(|(a, b)| a.abs_diff(b) <= 4);
    // samples from the PNG: bar, below the down arrow's bottom row, thumb over the bar, the dimmed HISTORY tab
    for (x, y, c) in
        [(421, 345, [102, 102, 102]), (421, 398, [0, 0, 0]), (422, 330, [82, 82, 82]), (335, 290, [11, 11, 11])]
    {
        assert!(near(px(&fb, x, y), c), "({x},{y}) {:?} vs {c:?}", px(&fb, x, y));
    }
    if front::text::courier12().is_none() {
        return;
    }
    let want = [
        (308, 3),
        (309, 11),
        (310, 5),
        (311, 96),
        (312, 53),
        (313, 73),
        (314, 48),
        (315, 42),
        (316, 105),
        (317, 3),
        (318, 3),
        (330, 1),
        (331, 10),
        (332, 4),
        (333, 74),
        (334, 36),
        (335, 53),
        (336, 34),
        (337, 34),
        (338, 79),
        (339, 1),
        (340, 3),
        (345, 1),
        (346, 13),
        (347, 7),
        (348, 97),
        (349, 55),
        (350, 69),
        (351, 48),
        (352, 48),
        (353, 112),
        (354, 1),
        (355, 3),
        (360, 2),
        (361, 9),
        (362, 7),
        (363, 109),
        (364, 56),
        (365, 62),
        (366, 48),
        (367, 51),
        (368, 105),
        (369, 4),
        (370, 6),
        (383, 7),
        (384, 5),
        (385, 93),
        (386, 48),
        (387, 72),
        (388, 39),
        (389, 43),
        (390, 110),
        (391, 3),
        (392, 3),
    ];
    rows_match(&text_rows(&fb, 185), &want, 6); // 29 dropout pixels of diagonals missing (H301)
}

/// H306 (no screenshot of the original: checks the drawing follows the core's state): the FBI login (fo-045 route from
/// fo-040-end.x, as tests/login.rs) with "SHI" typed: black text in the name field's text item (pre-filled, H570)
/// 0xc67b (260,225)–(447,243), three '*' and the caret in the password item 0xc68c (260,286)–(448,306).
#[test]
fn h306_login_typed_text() {
    let route = "p:70,210;p:570,210;p:570,210;p:251,281;p:320,210;p:83,282;p:320,210;p:466,300;p:320,210;p:70,210;p:320,210;p:412,147;p:320,210;p:171,271;p:320,210;p:187,223;p:320,210;p:570,210;p:2,92;p:70,210;p:570,210;p:318,200";
    let Some(blank) = frame_at("fo-040-end.x", route) else { return };
    // H570: the name is pre-filled and the keys go to the password field (fo-069 k22)
    let typed = frame_at("fo-040-end.x", &format!("{route};t:SHI")).unwrap();
    let black_in = |fb: &[u8], x1: usize, y1: usize, x2: usize| {
        (y1..y1 + 18).flat_map(|y| (x1..x2).map(move |x| (x, y))).filter(|&(x, y)| px(fb, x, y) == [0, 0, 0]).count()
    };
    let name = black_in(&blank, 262, 225, 447) as i64;
    let pass = black_in(&typed, 262, 287, 448) as i64 - black_in(&blank, 262, 287, 448) as i64;
    if front::text::courier12().is_some() {
        assert!(name > 150, "name text pixels {name}");
    }
    assert!(pass > 10, "password text pixels {pass}");
    // only the first 7 + 3 characters' cells hold the asterisks and the caret; the rest of the field is untouched
    assert_eq!(black_in(&typed, 300, 287, 448), black_in(&blank, 300, 287, 448));
}

/// H306: the phone (inventory 0x442f clicked from fo-059-end.x, keys 5 5 5 0 → PhoneKeyPress, H166) shows the number in
/// its display item 0x9a59 (273,139)–(390,196).
#[test]
fn h306_phone_number() {
    let Some(before) = frame_at("fo-059-end.x", "m:20,440;o:442f;m:300,200") else { return };
    let after = frame_at("fo-059-end.x", "m:20,440;o:442f;m:300,200;k:0x35;k:0x35;k:0x35;k:0x30").unwrap();
    let changed = (139..196)
        .flat_map(|y| (273..390).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&before, x, y) != px(&after, x, y))
        .count();
    assert!(changed > 30, "{changed}");
}

/// H307: the cursor follows the region kind under the pointer — fo-054-end.x (View 0x379): the left nav 0x388 at (25,200) (kind
/// 0x11c) → the left hand, Skinner's CharView 0x579c (kind 0x122) → the lips, the bottom band → none; with the badge
/// selected (fo-060 k9 state) → the badge's cursor (Inventory f38).
#[test]
fn h307_cursors() {
    let id = |save: &str, steps: &str| session_at(save, steps, |s| xf::front::ui::cursor_id(s));
    let Some(left) = id("fo-054-end.x", "m:25,200") else { return };
    assert_eq!(left, Some(0x11c));
    assert_eq!(id("fo-054-end.x", "m:105,190").unwrap(), Some(0x122));
    assert_eq!(id("fo-054-end.x", "m:320,450").unwrap(), None);
    let steps =
        "m:20,440;p:21,440;p:239,322;p:326,234;p:161,210;p:215,212;p:318,309;p:236,178;p:201,174;p:316,440;m:320,200";
    if let Some(badge) = id("fo-059-end.x", steps) {
        let c = badge.expect("a cursor with the badge selected");
        assert!(![0x11c, 0x11d, 0x11e, 0x122].contains(&c), "{c:#x}");
    }
    // drawn: the hand's pixels around the pointer (hotPoint (3,3))
    let fb = session_at("fo-054-end.x", "m:25,200", |s| {
        s.game_cursor = true;
        s.render()
    })
    .unwrap();
    let plain = frame_at("fo-054-end.x", "m:25,200").unwrap();
    let changed = (197..221)
        .flat_map(|y| (22..60).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&fb, x, y) != px(&plain, x, y))
        .count();
    assert!(changed > 200, "{changed}");
}

/// H308: StartVideoText's caption text (FUN_004e1490): New Game's call 0x100af = text 0xe25b, day variable 0xe9e = 1, time
/// 0xb6b = 914, flag 0x25d = 1.
#[test]
fn h308_video_text_format() {
    use xf::front::ui::video_text_lines;
    assert_eq!(video_text_lines("CAPTION LINE", 1, 914, 1), ["CAPTION LINE", "APRIL 2, 1996", "9:14 AM"]);
    assert_eq!(video_text_lines("X", 0, 1305, 1), ["X", "MARCH 29, 1996", "1:05 PM"]);
    assert_eq!(video_text_lines("X", 13, 0, 0), ["X", "APRIL 4, 1996"]);
}

/// H312: the typewriter timing — nothing for 200 ms, then 3 characters per 50 ms tick, the whole text, removed 4000 ms after
/// the tick that found it complete.
#[test]
fn h312_typewriter_timing() {
    use xf::front::ui::video_text_shown;
    assert_eq!(video_text_shown(38, 0), None);
    assert_eq!(video_text_shown(38, 199), None);
    assert_eq!(video_text_shown(38, 200), Some(0));
    assert_eq!(video_text_shown(38, 250), Some(3));
    assert_eq!(video_text_shown(38, 499), Some(15)); // 5 ticks
    assert_eq!(video_text_shown(38, 850), Some(38)); // 13 ticks
    assert_eq!(video_text_shown(38, 4899), Some(38));
    assert_eq!(video_text_shown(38, 4900), None); // tick 14 (900 ms) + 4000 ms
}

/// H308 / H312: New Game — frames at t = 0, 0.5 s, 2 s show no caption, a partial one, the whole one (white, at the View's
/// bottom-left); it is gone 4.9 s after the call.
#[test]
fn h308_intro_caption_drawn() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, front::rules_hyp());
    s.r.new_game();
    let white = |fb: &[u8]| {
        (270..318)
            .flat_map(|y| (35..160).map(move |x| (x, y)))
            .filter(|&(x, y)| px(fb, x, y) == [255, 255, 255])
            .count()
    };
    let at = |s: &mut Session, t: u64| {
        while s.now < t {
            s.tick(50);
        }
        s.render()
    };
    let f0 = at(&mut s, 0);
    let f1 = at(&mut s, 500);
    let f2 = at(&mut s, 2000);
    let vt = s.r.w.video_text.clone().expect("StartVideoText ran");
    assert_eq!((vt.asset, vt.day, vt.time, vt.flag), (0xe25b, 1, 914, 1));
    assert_eq!(white(&f0), 0);
    if front::text::courier12().is_some() {
        let (a, b) = (white(&f1), white(&f2));
        assert!(a > 20 && a + 100 < b, "partial {a}, full {b}");
        assert!(b > 150, "{b}");
    }
    let f3 = at(&mut s, vt.at + 4900);
    assert_eq!(white(&f3), 0);
}

fn white_in(fb: &[u8], x1: usize, y1: usize, x2: usize, y2: usize) -> usize {
    (y1..y2).flat_map(|y| (x1..x2).map(move |x| (x, y))).filter(|&(x, y)| px(fb, x, y) == [255, 255, 255]).count()
}

/// H309–H311: New Game, settled (no timer waited): the three EmotionIcons 0x46fc / 0x4700 / 0x6fde are placed at (180,340),
/// (280,340), (380,340) (80×60); their pictures are movies (XG/27698.xmv …) and are drawn; the pointer on the second one
/// shows its roll-over text (string resource 761 of the user's install) under it; elsewhere no caption.
#[test]
fn h309_emotion_icons_and_caption() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, front::rules_hyp());
    s.r.new_game();
    front::settle(&mut s, 100, 900_000);
    let lit = |fb: &[u8], x1: usize| {
        (340..400).flat_map(|y| (x1..x1 + 80).map(move |x| (x, y))).filter(|&(x, y)| px(fb, x, y) != [0, 0, 0]).count()
    };
    s.hover_at(320, 200);
    let fb = s.render();
    for x1 in [180, 280, 380] {
        assert!(lit(&fb, x1) > 3000, "icon at {x1}: {}", lit(&fb, x1));
    }
    assert_eq!(white_in(&fb, 180, 402, 460, 418), 0);
    s.hover_at(310, 370);
    s.tick(0);
    let fb = s.render();
    assert_eq!(xf::front::ui::hovered_icon(&s), Some(0x4700));
    let want = h.res_strings.get(&761).cloned();
    assert!(want.as_ref().is_some_and(|t| !t.is_empty()), "string 761");
    assert_eq!(s.r.w.icon_rollover(0x4700).1.and_then(|a| xf::world::devices::asset_text(&s.r.w, a)), want);
    let n = white_in(&fb, 240, 402, 400, 418);
    assert!(n > 60, "caption pixels {n}");
}

/// H309 / H310: inventory panel (fo-059-end.x, pointer into the band off the icons, then on the third item (56..86, 420..460)): its
/// roll-over picture replaces the normal one and its text appears under it.
#[test]
fn h309_inventory_rollover() {
    let Some(plain) = frame_at("fo-059-end.x", "m:20,440;m:400,470") else { return };
    let hov = frame_at("fo-059-end.x", "m:20,440;m:400,470;m:72,440").unwrap();
    let changed = (420..460)
        .flat_map(|y| (56..86).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&plain, x, y) != px(&hov, x, y))
        .count();
    assert!(changed > 50, "roll-over picture: {changed} pixels differ");
    assert_eq!(white_in(&plain, 0, 462, 200, 478), 0);
    assert!(white_in(&hov, 0, 462, 200, 478) > 60);
}

/// H865 (C79 / R32; screenshots `traces/fo-100.jsonl.shots/k08.png`, `k09.png` of the original): the phone display in
/// Courier New Bold 15 px, ink (27,48,15), from the item's top-left — fo-100's seven dialled digits (k08: 193 ink pixels,
/// rows 142..150, x 274..335) and the first phone-book card after Menu (k09: 824 ink pixels; first name line rows
/// 143..150 x 273..352, second name line 160..167 x 274..324, the number 176..184 x 274..362). The port's frame is
/// pixel-identical there.
#[test]
fn h865_phone_display_as_fo100() {
    let digits = "m:40,440;x:40,440;x:330,373;x:330,373;x:330,373;x:330,446;x:272,337;x:272,409;x:272,337";
    let ink = |fb: &[u8]| -> Vec<(usize, usize)> {
        (139..196).flat_map(|y| (273..390).map(move |x| (x, y))).filter(|&(x, y)| px(fb, x, y) == front::ui::PHONE_TEXT).collect()
    };
    let bbox = |v: &[(usize, usize)], y0: usize, y1: usize| {
        let r: Vec<_> = v.iter().filter(|p| p.1 >= y0 && p.1 <= y1).collect();
        (r.iter().map(|p| p.1).min(), r.iter().map(|p| p.1).max(), r.iter().map(|p| p.0).min(), r.iter().map(|p| p.0).max())
    };
    let Some(k08) = frame_at("fo-2c9-door.x", digits) else { return };
    if xf::front::text::courier15_bold().is_none() {
        return; // no Courier New Bold on this host
    }
    let v = ink(&k08);
    assert_eq!(v.len(), 193);
    assert_eq!(bbox(&v, 139, 195), (Some(142), Some(150), Some(274), Some(335)));
    let k09 = frame_at("fo-2c9-door.x", &format!("{digits};x:375,250")).unwrap();
    let v = ink(&k09);
    assert_eq!(v.len(), 824);
    assert_eq!(bbox(&v, 139, 155), (Some(143), Some(150), Some(273), Some(352)));
    assert_eq!(bbox(&v, 156, 171), (Some(160), Some(167), Some(274), Some(324)));
    assert_eq!(bbox(&v, 172, 195), (Some(176), Some(184), Some(274), Some(362)));
}

/// H1023 (bug report): the PDA map's place selector shown on a dot's hover — the compass x.pff 616 (84×80,
/// alpha plane: white background α 0) at (198,136) and the place's name label (x.pff 81, the warehouse's name, no
/// alpha plane, a white box with a black frame — left as is) at (246,141); fo-118's route (fo-111-end.x, PDA, map key,
/// hover on the warehouse dot (240,174)). No screenshot of the original shows a hovered dot; the rule is the IdeaIcons'
/// (fo-123 k15).
#[test]
fn h1023_pda_map_selector_without_white() {
    let steps = "m:20,440;p:21,440;p:239,322;m:240,174";
    let Some(fb) = frame_at("fo-111-end.x", steps) else { return };
    if let Some(d) = std::env::var_os("XF_DOC_DUMP") {
        front::write_ppm(&PathBuf::from(d).join("pda-map-label.ppm"), &fb).unwrap();
    }
    let label = |x: usize, y: usize| (246..306).contains(&x) && (141..169).contains(&y);
    let white = (136..216)
        .flat_map(|y| (198..282).map(move |x| (x, y)))
        .filter(|&(x, y)| !label(x, y) && px(&fb, x, y) == [255, 255, 255])
        .count();
    assert_eq!(white, 0, "white pixels of the compass picture");
    assert_eq!(px(&fb, 239, 175), [255, 0, 0], "the compass's red centre (α 255)");
}
