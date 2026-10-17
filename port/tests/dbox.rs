//! H135–H139 (dialogue box interaction, static decode, NOT traced): closed loop from data/bot/fo-028-end.x (the Load
//! rebuilds CharView 0x590c's box 0x590d with Conversations 0x590e / 0x5914 / 0x591a, H126 — fo-029 129.3–146.3).
//! Checks what the core predicts for the pre-registered dialogue click (RE_NOTES H135; traces/fo-033.md).
use std::path::PathBuf;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::world::{class, event, Rules};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn with_session(rules: Rules, f: impl FnOnce(&mut Session)) {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-028-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    xf::front::settle(&mut s, 100, 900_000);
    f(&mut s);
}

/// H136: the box under CharView 0x590c (region 263,107–439,295): x = 263 + (176 − 246) / 2 = 228, y = 0x11d; talk lines
/// from (239,305) in build order, H161: 14 px high, 7 px apart (y 305 / 326 / 347); close box (462,301)–(469,309);
/// everything above the nav-hotspots.
/// (Without H262 — the H161 rows; with H262 see `h262_rows_fo028`.)
#[test]
fn h136_box_geometry() {
    with_session(Rules { h262_dbox_rows: false, ..Rules::all() }, |s| {
        let l = s.r.w.dbox_layout().expect("box shown after the Load");
        assert_eq!(l.list, 0x590d);
        assert_eq!(l.rect, [228, 285, 474, 405]);
        assert_eq!(l.close, [462, 301, 469, 309]);
        let lines: Vec<(u32, i16)> = l.lines.iter().map(|x| (x.0, x.1[1])).collect();
        assert_eq!(lines, vec![(0x590e, 305), (0x5914, 326), (0x591a, 347)]);
        assert_eq!(s.r.hit(345, 311).map(|c| c.owner), Some(0x590e));
        // H161 pre-registration points (traces/fo-036 candidate): line 1, line 2; the gap between lines hits the body
        assert_eq!(s.r.hit(260, 333).map(|c| c.owner), Some(0x5914));
        assert_eq!(s.r.hit(260, 354).map(|c| c.owner), Some(0x591a));
        assert_eq!(s.r.hit(260, 322).map(|c| c.owner), Some(0x590d));
        assert_eq!(s.r.hit(465, 305).map(|c| (c.owner, c.kind)), Some((0x590d, xf::world::dbox::KIND_CLOSE)));
        assert_eq!(s.r.hit(300, 400).map(|c| c.owner), Some(0x590d)); // body
        assert_eq!(s.r.hit(570, 210).map(|c| c.owner), Some(0x6bdc)); // nav-hotspot beside the box
    });
    with_session(Rules { h136_dbox_hit: false, ..Rules::all() }, |s| {
        assert!(s.r.w.dbox_layout().is_none());
        assert!(!s.r.clickables().iter().any(|c| c.cls == class::CONVERSATION));
    });
}

/// H135: clicking line 0 (0x590e) — no transition; f30 0x4e75 then f34 0x4e77 registered + played (FIFO), the Mouse
/// Click posts 0x1262a (var 0x1104 := var 0x25d); the box hides while the line plays and (H160) is shown again after
/// it with the same three lines (fo-033 184.00–184.52), the Conversations stay. Ablation: without H135 only the post;
/// without H160 the box stays hidden.
#[test]
fn h135_conversation_click() {
    with_session(Rules::all(), |s| {
        let (r0, p0) = (s.r.regs.len(), s.r.posts.len());
        let c = s.click_at(345, 311).expect("hit");
        assert_eq!(c.owner, 0x590e);
        xf::front::settle(s, 100, 900_000);
        assert_eq!(s.r.regs[r0..].to_vec(), vec![0x4e75, 0x4e77]);
        assert_eq!(s.r.posts[p0..].iter().map(|m| m.action).collect::<Vec<_>>(), vec![0x1262a]);
        assert_eq!(s.r.w.view, Some(0x2a24));
        let life: Vec<(u32, u8)> = s.r.lifecycle.iter().rev().take(4).rev().copied().collect();
        assert_eq!(
            life,
            vec![(0x4e75, event::ACTIVATION), (0x4e77, event::ACTIVATION), (0x4e75, event::DEACTIVATION), (0x4e77, event::DEACTIVATION)]
        );
        // H262: the asked line went to the history panel — the talk panel shows the other two (fo-036 G1 (b): (260,333)
        // then hit 0x591a)
        let l = s.r.w.dbox_layout().expect("H160: box shown again after the line");
        assert_eq!(l.lines.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0x5914, 0x591a]);
        assert_eq!(s.r.w.conv_history, vec![0x590e]);
        assert_eq!(s.r.hit(260, 333).map(|c| c.owner), Some(0x591a));
        assert!(s.r.active.contains(&0x590e) && s.r.active.contains(&0x5914) && s.r.active.contains(&0x591a));
    });
    with_session(Rules { h160_dbox_reshow: false, ..Rules::all() }, |s| {
        s.click_at(345, 311).expect("hit");
        xf::front::settle(s, 100, 900_000);
        assert!(s.r.w.dbox_layout().is_none());
    });
    with_session(Rules { h135_conversation_click: false, ..Rules::all() }, |s| {
        let (r0, p0) = (s.r.regs.len(), s.r.posts.len());
        s.click_at(345, 311).expect("hit");
        xf::front::settle(s, 100, 900_000);
        assert!(s.r.regs[r0..].is_empty());
        assert_eq!(s.r.posts[p0..].iter().map(|m| m.action).collect::<Vec<_>>(), vec![0x1262a]);
    });
}

/// H137: the close box removes the box — each Conversation deactivated, then its asset (H126's removal order).
#[test]
fn h137_close() {
    with_session(Rules::all(), |s| {
        let l0 = s.r.lifecycle.len();
        s.click_at(465, 305).expect("hit");
        xf::front::settle(s, 100, 900_000);
        let d = event::DEACTIVATION;
        assert_eq!(
            s.r.lifecycle[l0..].to_vec(),
            vec![(0x590e, d), (0x4503, d), (0x5914, d), (0x4505, d), (0x591a, d), (0x4507, d)]
        );
        assert!(s.r.w.dbox_layout().is_none());
        assert_eq!(s.r.hit(345, 311).map(|c| c.owner), None);
    });
    with_session(Rules { h137_dbox_close: false, ..Rules::all() }, |s| {
        let l0 = s.r.lifecycle.len();
        s.click_at(465, 305);
        xf::front::settle(s, 100, 900_000);
        assert_eq!(s.r.lifecycle.len(), l0);
    });
}

/// H262 on the fo-028 box (no history in the save; 0x590c's box x 228, panel 239..451): blocks 15 px per text line, 7 px
/// apart, as wide as their text (strings 50 / 51 / 52: 30, 14, 30 characters × 7 px).
#[test]
fn h262_rows_fo028() {
    with_session(Rules::all(), |s| {
        let l = s.r.w.dbox_layout().expect("box shown after the Load");
        assert_eq!(l.rect, [228, 285, 474, 405]);
        let lines: Vec<(u32, [i16; 4])> = l.lines.clone();
        assert_eq!(
            lines,
            vec![(0x590e, [239, 305, 449, 320]), (0x5914, [239, 327, 337, 342]), (0x591a, [239, 349, 449, 364])]
        );
        assert_eq!(s.r.hit(345, 311).map(|c| c.owner), Some(0x590e));
        assert_eq!(s.r.hit(345, 333).map(|c| c.owner), Some(0x590d)); // right of the short row 0x5914
        assert_eq!(s.r.hit(260, 323).map(|c| c.owner), Some(0x590d)); // gap
    });
}

fn load(save: &str, rules: Rules, f: impl FnOnce(&mut Session)) {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    xf::front::settle_timers(&mut s, 100, 900_000);
    f(&mut s);
}

/// fo-036 (`traces/fo-036.md`, pre-registered, from data/bot/fo-033-end.x whose history = [0x590e]): G1 (a) (345,311) hit
/// the box body, (b) (260,333) hit 0x591a, (c) (260,354) nothing, (d) (260,322) body. H262: talk rows 0x5914 (239,305)–
/// (337,320) and 0x591a (239,327)–(449,342) — all four as traced (H161's 14/7 full-width rows got (a) and (b) wrong).
#[test]
fn h262_fo036_points() {
    load("fo-033-end.x", Rules::all(), |s| {
        assert_eq!(s.r.w.conv_history, vec![0x590e]);
        let l = s.r.w.dbox_layout().expect("box rebuilt by the Load");
        assert_eq!(l.lines, vec![(0x5914, [239, 305, 337, 320]), (0x591a, [239, 327, 449, 342])]);
        assert_eq!(s.r.hit(345, 311).map(|c| c.owner), Some(0x590d));
        assert_eq!(s.r.hit(260, 333).map(|c| c.owner), Some(0x591a));
        assert_eq!(s.r.hit(260, 354).map(|c| c.owner), Some(0x590d));
        assert_eq!(s.r.hit(260, 322).map(|c| c.owner), Some(0x590d));
    });
}

/// fo-055 (`traces/fo-055.md`, screenshots k01 / k02; save fo-054-end.x, history [0x5697, 0x57a0, 0x57ae, 0x57b5]):
/// Skinner's box (CharView 0x579c, box x clamped to 20): rows 0x57ca (one line), 0x57a3 (two lines), 0x57c1 (one
/// line) — text pixels 309–318 / 331–340 + 346–355 /
/// 367–377 in k01, i.e. blocks 305 / 327–357 / 364; Shanks's box (0x5684, x 374): one two-line row 0x5691.
#[test]
fn h262_fo055_rows() {
    load("fo-054-end.x", Rules::all(), |s| {
        assert_eq!(s.r.w.conv_history, vec![0x5697, 0x57a0, 0x57ae, 0x57b5]);
        s.hover_at(105, 190);
        s.click_at(105, 190).expect("CharView 0x579c");
        xf::front::settle_timers(s, 100, 900_000);
        let l = s.r.w.dbox_layout().expect("Skinner's box");
        assert_eq!(l.rect, [20, 285, 266, 405]);
        assert_eq!(
            l.lines,
            vec![(0x57ca, [31, 305, 115, 320]), (0x57a3, [31, 327, 227, 357]), (0x57c1, [31, 364, 213, 379])]
        );
        s.hover_at(505, 187);
        s.click_at(505, 187).expect("CharView 0x5684");
        xf::front::settle_timers(s, 100, 900_000);
        let l = s.r.w.dbox_layout().expect("Shanks's box");
        assert_eq!(l.rect, [374, 285, 620, 405]);
        assert_eq!(l.lines, vec![(0x5691, [385, 305, 574, 335])]);
    });
}

/// fo-054 (`traces/fo-054.md`, 17 traced steps from data/bot/fo-026-end.x): the dialogue legs (k10–k16) on the original
/// played 0x57ae, 0x57a0 (0x4dae / 0x4db0, 1025-ish), 0x57b5 (0x4dcc / 0x4dce, 1025.62), Shanks's box with 0x5691 +
/// 0x5697, then 0x5697 (0x4dc8 / 0x4dca, 1333.95); the original's save has 0xad1 = 0, 0xaa3 = 0, 0x38f = 2, 0x128d =
/// 0x3c5 = 0x3c7 = 0x1119 = 1 and history [0x5697, 0x57a0, 0x57ae, 0x57b5] (order of the save; the core keeps click order).
/// With H262 the core's plan replays to that state; without it the G2 differences of fo-054.md come back.
#[test]
fn h262_fo054_dialogue_legs() {
    let steps = [
        (42, 209), (265, 179), (556, 281), (460, 183), (70, 210), (475, 210), (512, 209), (299, 209), (70, 210), (305, 210),
        (105, 190), (117, 354), (2, 2), (117, 333), (117, 375), (505, 187), (499, 354),
    ];
    let want: [(u32, i32); 7] = [(0xad1, 0), (0xaa3, 0), (0x38f, 2), (0x128d, 1), (0x3c5, 1), (0x3c7, 1), (0x1119, 1)];
    for (rules, same) in [(Rules::all(), true), (Rules { h262_dbox_rows: false, ..Rules::all() }, false)] {
        load("fo-026-end.x", rules, |s| {
            for (x, y) in steps {
                s.hover_at(x, y);
                s.click_at(x, y);
                xf::front::settle_timers(s, 100, 900_000);
            }
            let got: Vec<(u32, i32)> = want.iter().map(|&(v, _)| (v, s.r.w.vars.get(v).unwrap_or(-1))).collect();
            assert_eq!(got == want, same, "{got:x?}");
            if same {
                let mut h = s.r.w.conv_history.clone();
                h.sort();
                assert_eq!(h, vec![0x5697, 0x57a0, 0x57ae, 0x57b5]);
            }
        });
    }
}
