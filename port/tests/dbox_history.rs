//! H500–H503 (static decode + post-hoc + player report, NOT pre-registered): the dialogue box after its topics
//! are exhausted (removal H501, HISTORY tab H502, replay H503) and a document layout shown at the end of an answer
//! (drawn over the box, H500). Closed loop from the original's saves (RE_NOTES H500–H503).
use std::path::PathBuf;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::world::{dbox, event, Rules};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
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

fn click(s: &mut Session, x: i32, y: i32) {
    s.hover_at(x, y);
    s.click_at(x, y);
    xf::front::settle_timers(s, 100, 900_000);
}

/// fo-033-end.x (box 0x590d of CharView 0x590c, history [0x590e]; talk rows 0x5914 (239,305) / 0x591a (239,327)):
/// asking 0x5914 then 0x591a exhausts the topics. H501: after 0x591a's answer the box is removed (each Conversation
/// deactivated, then its asset) instead of staying hidden; H502: the next click on the CharView (lips) opens a new box on
/// the HISTORY tab with the three asked lines (build order). Ablation (reported): without H501 the box stays hidden
/// and the CharView click opens nothing (DisplayDBox of the same CharView is a no-op, FUN_00481660).
#[test]
fn h501_h502_exhausted_topics_open_history() {
    load("fo-033-end.x", Rules::all(), |s| {
        assert_eq!(s.r.w.dbox_layout().map(|l| l.tab), Some(0));
        click(s, 260, 311); // 0x5914
        let l = s.r.w.dbox_layout().expect("re-shown with 0x591a");
        assert_eq!(l.lines.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0x591a]);
        let l0 = s.r.lifecycle.len();
        click(s, 260, 311); // 0x591a, the last topic
        assert!(s.r.w.dbox_layout().is_none(), "H501: removed");
        let d = event::DEACTIVATION;
        let deacts: Vec<u32> = s.r.lifecycle[l0..].iter().filter(|x| x.1 == d).map(|x| x.0).collect();
        for o in [0x590e, 0x4503, 0x5914, 0x4505, 0x591a, 0x4507] {
            assert!(deacts.contains(&o), "{o:#x} deactivated by the removal");
        }
        assert_eq!(s.r.w.conv_history, vec![0x590e, 0x5914, 0x591a]);
        let r0 = s.r.regs.len();
        click(s, 350, 200); // CharView 0x590c (the lips)
        assert!(s.r.regs[r0..].contains(&0x590d), "DisplayDBox: box object registered again");
        let l = s.r.w.dbox_layout().expect("H502: a box again");
        assert_eq!(l.tab, 1);
        assert_eq!(l.lines.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0x590e, 0x5914, 0x591a]);
        assert_eq!(l.lines[0].1, [239, 305, 449, 320]); // the history panel = the talk panel's rect, H262 rows
        assert_eq!(s.r.w.dbox_line_texts().len(), 3);
    });
    load("fo-033-end.x", Rules { h501_dbox_removed_when_empty: false, ..Rules::all() }, |s| {
        click(s, 260, 311);
        click(s, 260, 311);
        assert!(s.r.w.dbox_layout().is_none());
        let r0 = s.r.regs.len();
        click(s, 350, 200);
        assert!(s.r.w.dbox_layout().is_none(), "the reported bug: the lips open nothing");
        assert!(!s.r.regs[r0..].contains(&0x590d));
    });
    load("fo-033-end.x", Rules { h502_dbox_history_tab: false, ..Rules::all() }, |s| {
        click(s, 260, 311);
        click(s, 260, 311);
        click(s, 350, 200);
        let l = s.r.w.dbox_layout().expect("box");
        assert_eq!(l.tab, 0);
        assert!(l.lines.is_empty(), "without H502: an empty talk panel");
    });
}

/// H502: the tabs switch the panels (TALK (238,285)–(330,297), HISTORY (338,285)–(430,297) for box x 228); a
/// box with talk lines opens on TALK.
#[test]
fn h502_tabs() {
    load("fo-033-end.x", Rules::all(), |s| {
        let l = s.r.w.dbox_layout().expect("box rebuilt by the Load");
        assert_eq!(l.tabs, [[238, 285, 330, 297], [338, 285, 430, 297]]);
        assert_eq!(s.r.hit(380, 290).map(|c| c.kind), Some(dbox::KIND_TAB_HISTORY));
        click(s, 380, 290);
        let l = s.r.w.dbox_layout().expect("box");
        assert_eq!(l.tab, 1);
        assert_eq!(l.lines, vec![(0x590e, [239, 305, 449, 320])]);
        assert_eq!(s.r.hit(300, 311).map(|c| c.owner), Some(0x590e));
        click(s, 280, 290);
        let l = s.r.w.dbox_layout().expect("box");
        assert_eq!(l.tab, 0);
        assert_eq!(l.lines.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0x5914, 0x591a]);
    });
}

/// H503 / H863: a history line replays — box state 0, f30 0x4e75, f38 0x529a (list 0x5913) registered, no f34 0x4e77
/// (fo-096 106.53–109.65, R31: FUN_004e9440's count path, its likeliest draw), the Mouse Click posts 0x1262a again; afterwards (no talk line left after exhausting) the box stays on the HISTORY tab
/// (the box's history flag set → not removed, FUN_0041c740) and is shown again. Without H503 the line plays f30 / f34 alike (H135).
#[test]
fn h503_history_replay() {
    load("fo-033-end.x", Rules::all(), |s| {
        click(s, 260, 311);
        click(s, 260, 311);
        click(s, 350, 200);
        assert_eq!(s.r.w.dbox_layout().map(|l| l.tab), Some(1));
        let (r0, p0) = (s.r.regs.len(), s.r.posts.len());
        click(s, 300, 311); // 0x590e in the history panel
        assert_eq!(s.r.regs[r0..].to_vec(), vec![0x4e75, 0x529a]);
        assert_eq!(s.r.posts[p0..].iter().map(|m| m.action).collect::<Vec<_>>(), vec![0x1262a]);
        let l = s.r.w.dbox_layout().expect("H501: on the HISTORY tab the box is not removed");
        assert_eq!(l.tab, 1);
        assert_eq!(l.lines.len(), 3);
        assert_eq!(s.r.w.conv_history, vec![0x590e, 0x5914, 0x591a]);
    });
    load("fo-033-end.x", Rules { h503_history_replay: false, ..Rules::all() }, |s| {
        click(s, 260, 311);
        click(s, 260, 311);
        click(s, 350, 200);
        let r0 = s.r.regs.len();
        click(s, 300, 311);
        assert_eq!(s.r.regs[r0..].to_vec(), vec![0x4e75, 0x4e77]);
    });
}

/// H500 (fo-067 k0 from data/bot/fo-066-end.x: the clerk's row 0x587d at (314,372); trace `traces/fo-067.jsonl` 65.467:
/// the answer 0x4e59's Deactivation shows layout 0x1467b (the car note) and the box gets state 1 again at 65.523): the
/// box is open and shown in the core, but the document covers it — inside the note's picture the frame equals the frame
/// with the box closed; without H500 the box's rows are drawn over the note (the reported bug).
#[test]
fn h500_document_covers_box() {
    for on in [true, false] {
        load("fo-066-end.x", Rules { h500_layouts_over_dbox: on, ..Rules::all() }, |s| {
            click(s, 314, 372);
            assert!(s.r.w.layouts().iter().any(|l| l.0 == 0x1467b), "car note shown");
            assert!(s.r.w.dbox_layout().is_some(), "box re-shown (fo-067 65.523 set_state 1)");
            assert!(s.r.hit(320, 240).is_some_and(|c| c.owner == 0x14681), "the note's item takes the click (fo-067 71.810)");
            let with_box = s.render();
            s.r.w.dbox_close();
            let without = s.render();
            let diff = (300..385)
                .flat_map(|y| (215..440).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let o = (y * xf::front::SCREEN_W + x) * 3;
                    with_box[o..o + 3] != without[o..o + 3]
                })
                .count();
            if on {
                assert_eq!(diff, 0, "H500: the note covers the box");
            } else {
                assert!(diff > 1000, "without H500 the box is drawn over the note ({diff})");
            }
        });
    }
}

/// H651 (H215; post-hoc `traces/fo-066.jsonl` 242.893–242.944): fo-066 from fo-065-end.x — the clerk 0x5867's only
/// line 0x586e (k16): its answer's last asset 0x74aa enables four follow-up questions in its Deactivation (0x74ae /
/// 0x74af / 0x74b1 / 0x74b0); the line's end runs behind those posts, so the box is shown again with them (the original:
/// box state 1 with 0x5874 / 0x5877 / 0x587d / 0x587a). Ablation: decided at the Deactivation, no line
/// is left → the box is removed (H501) and the Enables find no box.
#[test]
fn h651_follow_up_questions_reshow_the_box() {
    const STEPS: &str = "p:570,210;p:570,210;p:70,210;p:70,210;p:70,210;p:70,210;p:570,210;p:570,210;p:308,206;p:246,312;\
                         p:215,199;p:198,312;p:570,210;p:157,275;p:320,210;p:327,218;p:310,327";
    for on in [true, false] {
        // C231: 0x586e has f40 = 0 (never moved to the history), so a stale decision would still see it as a talk line
        // and show the box (the Enables then append the follow-ups, its Disable removes it — the same end state); the
        // ablation drops C231 too, so "no line is left" holds as in the H651 analysis
        let rules = Rules { h651_dbox_reshow_queued: on, c231_conv_history_flag: on, ..Rules::all() };
        load("fo-065-end.x", rules, |s| {
            for i in xf::front::script::parse(STEPS).unwrap() {
                xf::front::script::apply(s, &i);
                xf::front::settle_timers(s, 100, 900_000);
            }
            let lines: Option<Vec<u32>> = s.r.w.dbox_layout().map(|l| l.lines.iter().map(|x| x.0).collect());
            if on {
                // the rows that fit the box (H262; the original's k17 shot shows 0x5874 and 0x5877, the rest scrolled)
                let l = lines.expect("box shown again");
                assert_eq!(l[..2], [0x5874, 0x5877]);
                for c in [0x5874, 0x5877, 0x587d, 0x587a] {
                    assert!(s.r.regs.contains(&c), "{c:#x} built");
                }
                if std::env::var("H651_DUMP").is_ok() {
                    let fb = s.render();
                    let mut ppm = format!("P6 {} {} 255\n", xf::front::SCREEN_W, fb.len() / 3 / xf::front::SCREEN_W).into_bytes();
                    ppm.extend_from_slice(&fb);
                    std::fs::write(std::env::var("H651_DUMP").unwrap(), ppm).unwrap();
                    eprintln!("{:?}", s.r.w.dbox_layout().map(|l| l.lines.clone()));
                }
            } else {
                assert_eq!(lines, None, "ablation: the box is removed");
            }
        });
    }
}

/// C76 / H2120 (fo-097 k01-0 / k07: the HISTORY tab's rows are (46,107,140), the TALK tab's (205,205,205); colours
/// of the box's tab drawing, FUN_00483610): fo-033-end.x, 0x5914 asked → on the HISTORY tab its row (and 0x590e) is
/// drawn blue, no talk-coloured text in the panel; before the question the TALK rows are grey-white. Ablation: without
/// the rule the history rows are drawn in the talk colour (the reported bug).
#[test]
fn h2120_history_rows_blue() {
    fn count(fb: &[u8], c: [u8; 3]) -> usize {
        (300..400)
            .flat_map(|y| (229..470).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let o = (y * xf::front::SCREEN_W + x) * 3;
                fb[o..o + 3] == c
            })
            .count()
    }
    use xf::front::ui::{TEXT, TEXT_HISTORY};
    for on in [true, false] {
        load("fo-033-end.x", Rules { h2120_history_row_colour: on, ..Rules::all() }, |s| {
            let before = s.render();
            assert!(count(&before, TEXT) > 300, "TALK rows in the talk colour");
            assert_eq!(count(&before, TEXT_HISTORY), 0);
            click(s, 260, 311); // ask 0x5914
            click(s, 380, 290); // HISTORY tab
            let l = s.r.w.dbox_layout().expect("box");
            assert_eq!(l.tab, 1);
            assert_eq!(l.lines.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0x590e, 0x5914]);
            let after = s.render();
            if on {
                assert!(count(&after, TEXT_HISTORY) > 300, "history rows blue");
                assert_eq!(count(&after, TEXT), 0);
            } else {
                assert_eq!(count(&after, TEXT_HISTORY), 0);
                assert!(count(&after, TEXT) > 300);
            }
        });
    }
}
