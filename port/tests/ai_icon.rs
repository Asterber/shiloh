//! The AI icon's three states and what a click shows (H1920–H1923, C292; H1922 = H1940). States: dimmed (no hint
//! applies, H621), lit (a list action's condition holds, H621), spinning (SetAIGlow, H622 / C73 — `tests/screens.rs`
//! `h622_glow_spins_and_click_travels`). A lit icon's click posts the list's next held hint (H623 / C69): a type-5 card
//! (fo-089), a type-1 still in the movie area (fo-111), a type-3 ActionIcon (fo-132), or nothing visible (fo-180: the
//! still's AssetRef is disabled). Pixel values measured on the original's screenshots. Skipped without the game data.
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Load `save`, run `steps` (front::script syntax), settle after each, then `f` on the session.
fn session_at<T>(save: &str, steps: &str, rules: Rules, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
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
        s.render();
    }
    Some(f(&mut s))
}

fn px(fb: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * front::SCREEN_W + x) * 3;
    [fb[o], fb[o + 1], fb[o + 2]]
}

/// The movie band (600×240 at (20,90)).
fn band(fb: &[u8]) -> Vec<u8> {
    (90..330).flat_map(|y| fb[(y * 640 + 20) * 3..(y * 640 + 620) * 3].to_vec()).collect()
}

fn ticks(s: &mut Session, ms: u64) {
    for _ in 0..ms / 20 {
        s.tick(20);
    }
}

/// The icon's centre and two rim pixels: lit = XG/56700.pff entry 0, dimmed = H620's factor (fo-063 k01 / fo-064 k12).
const LIT: [[u8; 3]; 3] = [[115, 227, 230], [39, 62, 97], [40, 73, 116]];
const DIM: [[u8; 3]; 3] = [[27, 53, 54], [9, 15, 23], [9, 17, 27]];
fn icon(fb: &[u8]) -> [[u8; 3]; 3] {
    [px(fb, 563, 63), px(fb, 555, 60), px(fb, 570, 66)]
}

/// H1920: the icon's state per AI list, as the original's screenshots — lit at Smolnikoff's 0x14b6 (list 0xfca5,
/// `traces/fo-180.jsonl.shots/k03.png`), lit at Willmore's room 0x1994 with the sleep ActionIcon after the click (list
/// 0x16c6e, `traces/fo-132.jsonl.shots/k05.png`), dimmed at the Field Office desk after the Lone Gunmen (empty list,
/// `traces/fo-204.jsonl.shots/k01.png`; C204: the click posts nothing).
#[test]
fn h1920_icon_state_per_ai_list() {
    let cases = [
        ("fo-179-end.x", "p:570,210;p:320,210;p:562,63", 0xfca5, LIT),
        ("fo-131-end.x", "m:20,440;p:21,440;p:278,192;p:326,348;p:562,63", 0x16c6e, LIT),
        ("fo-203-end.x", "p:562,63", 0, DIM),
    ];
    for (save, steps, list, want) in cases {
        let Some((fb, ai)) = session_at(save, steps, front::rules_hyp(), |s| (s.render(), s.r.w.ai.clone())) else {
            return;
        };
        assert_eq!(ai.list, list, "{save}");
        assert_eq!(icon(&fb), want, "{save}");
    }
    // fo-132 k05: the sleep ActionIcon 0x5ff5 the click enabled lies at (520,340) (the bed picture)
    let Some(fb) = session_at("fo-131-end.x", "m:20,440;p:21,440;p:278,192;p:326,348;p:562,63", front::rules_hyp(), |s| s.render())
    else {
        return;
    };
    let lit = (345..395).flat_map(|y| (525..595).map(move |x| (x, y))).filter(|&(x, y)| px(&fb, x, y) != [0, 0, 0]).count();
    assert!(lit > 2500, "ActionIcon pixels {lit}");
    // ablation: without H621 the empty-list icon is drawn lit
    let off = Rules { h621_ai_icon: false, ..front::rules_hyp() };
    let Some(fb) = session_at("fo-203-end.x", "", off, |s| s.render()) else { return };
    assert_eq!(icon(&fb)[0], LIT[0]);
}

/// H1920 / C175 (H1300 REFUTED): at Smolnikoff's 0x14b6 the lit icon's click posts the hint 0xfca6 only
/// (`traces/fo-180.jsonl` 59.943 act_post from FUN_005097e0, action_exec type 0x81) and the original plays nothing — no
/// trig_register / mv_new between 59.94 and 66.05: its AssetRefList 0xfca7 holds only 0x13c4 (the 2nd-floor still), which
/// is disabled. The port: the same post, no player, the band unchanged, the list position past it; the next click goes
/// on to the following held hint, which shows its still.
#[test]
fn h1920_fo180_click_posts_the_hint_without_a_picture() {
    let Some(r) = session_at("fo-179-end.x", "p:570,210;p:320,210", front::rules_hyp(), |s| {
        let before = band(&s.render());
        let (n, np) = (s.r.posts.len(), s.r.plays.len());
        s.hover_at(562, 63);
        assert_eq!(s.click_at(562, 63).map(|c| c.owner), Some(0xdb79));
        ticks(s, 3000);
        let posted: Vec<u32> = s.r.posts[n..].iter().map(|m| m.action).collect();
        let played = s.r.plays.len() - np;
        let same = band(&s.render()) == before;
        let cursor = s.r.w.ai.cursor;
        // second click: the next held action of 0xfca5 = [0xfca6, 0xfca8, 0xfcac, 0xfcaa]
        let n = s.r.posts.len();
        s.hover_at(562, 63);
        s.click_at(562, 63);
        ticks(s, 500);
        let second: Vec<u32> = s.r.posts[n..].iter().map(|m| m.action).collect();
        let shown = band(&s.render()) != before;
        (posted, played, same, cursor, second, shown)
    }) else {
        return;
    };
    let (posted, played, same, cursor, second, shown) = r;
    assert_eq!(posted.first(), Some(&0xfca6));
    assert!(!posted.iter().any(|a| [0xfca8, 0xfcac, 0xfcaa].contains(a)), "one hint per click: {posted:x?}");
    assert_eq!(played, 0, "0x13c4 is disabled: nothing to play");
    assert!(same, "the band is unchanged");
    assert_eq!(cursor, 1);
    assert!(second.iter().any(|a| [0xfca8, 0xfcac, 0xfcaa].contains(a)), "{second:x?}");
    assert!(shown, "the next hint shows its picture");
}

/// H1920 (fo-111, the warehouse): the lit icon's click posts 0xe16b (`traces/fo-111.jsonl` 284.308), whose AssetRef
/// 0xe2d (NAV2 32001, the crates) is played — trig_register 0xe2d + mv_new 284.309, the 3 s still timer 284.314 — and
/// the View comes back after it (view_reshow 287.436; C270's 3 s nav stills). The port shows the still in the band, then
/// the View again.
#[test]
fn h1920_fo111_click_shows_the_hint_still() {
    let steps = "p:320,312;p:263,318;m:20,440;p:141,440;p:301,209;p:320,144;p:598,210;p:320,210;p:570,210;p:162,308;\
                 p:239,219;m:20,440";
    let Some((posted, at1, at25, at4, view)) = session_at("fo-110-end.x", steps, front::rules_hyp(), |s| {
        s.hover_at(562, 63);
        let view = band(&s.render());
        let n = s.r.posts.len();
        s.click_at(562, 63);
        ticks(s, 500);
        let at1 = band(&s.render());
        ticks(s, 2000);
        let at25 = band(&s.render());
        ticks(s, 1500);
        let at4 = band(&s.render());
        (s.r.posts[n..].iter().map(|m| m.action).collect::<Vec<_>>(), at1, at25, at4, view)
    }) else {
        return;
    };
    assert_eq!(posted.last(), Some(&0xe16b), "{posted:x?}");
    assert_ne!(at1, view, "the still at +0.5 s");
    assert_eq!(at1, at25, "the still stays (+2.5 s)");
    assert_eq!(at4, view, "the View again at +4 s");
}

/// H1922 = H1940 (main's fix): NAV2.NMV's second video track ('jpeg') has several sample descriptions; sample 2 (movie time 33 s) is a 'cvid'
/// key frame — the AI hint still 0xe2e (list 0xe138's 0xe170, the Morley butt on the warehouse floor) — and is
/// decoded with Cinepak (before: "jpeg: invalid JPEG format", an empty band).
#[test]
fn h1922_cvid_sample_in_a_jpeg_track() {
    let d = root().join("data/InstalledGame");
    let Ok(m) = xf::media::qt::Movie::open(&d.join("NAV2.NMV")) else { return };
    let t = &m.tracks[1];
    assert_eq!(&t.fourcc, b"jpeg");
    assert_eq!(&t.sample_fourcc(2), b"cvid");
    assert_eq!(&t.sample_fourcc(1), b"jpeg");
    let Some(fb) = session_at("fo-110-end.x", "", front::rules_hyp(), |s| {
        s.play_preview(0xe2e);
        s.tick(40);
        let fb = s.render();
        assert!(!s.log.iter().any(|l| l.contains("jpeg:")), "{:?}", s.log);
        fb
    }) else {
        return;
    };
    let fill = (90..330).flat_map(|y| (20..620).map(move |x| (x, y))).filter(|&(x, y)| px(&fb, x, y) == [40, 40, 48]).count();
    assert!(fill < 1000, "plain fill pixels {fill}");
    let distinct: std::collections::HashSet<[u8; 3]> =
        (90..330).step_by(4).flat_map(|y| (20..620).step_by(4).map(move |x| (x, y))).map(|(x, y)| px(&fb, x, y)).collect();
    assert!(distinct.len() > 50, "a picture, not a flat frame: {} colours", distinct.len());
}

/// C292 / H1923 (fo-220, the finale): after the key the View 0x387e has the lure ActionIcon 0x5ffe (enabled at the
/// arrival by 0x10586, trace 56.144) and the AI icon dimmed (`traces/fo-220.jsonl.shots/s02-0.png`); the AI icon click
/// (+0.87 s) posts nothing; the ActionIcon's movie XG/24485.xmv is built at its activation (mv_new 56.145) and started
/// only when the pointer reaches it (mv_start 57.754, from the roll handler, FUN_004631b0); its click
/// runs DoActionList 0x10379 (57.817). Ablation: H1923 off → the movie runs without the pointer (H311).
#[test]
fn c292_fo220_action_icon_animates_while_hovered() {
    let run = |rules: Rules| {
        session_at("fo-219-end.x", "", rules, |s| {
            s.hover_at(335, 200);
            s.click_at(335, 200);
            let mut t = 0;
            while s.r.w.view != Some(0x387e) && t < 30_000 {
                s.tick(20);
                t += 20;
            }
            ticks(s, 300);
            let fb = s.render();
            let placed = s.r.clickables().iter().any(|c| c.owner == 0x5ffe);
            let n = s.r.posts.len();
            s.hover_at(562, 63);
            s.click_at(562, 63);
            ticks(s, 100);
            let ai_posts = s.r.posts.len() - n;
            let crop = |fb: &[u8]| (340..400).flat_map(|y| fb[(y * 640 + 520) * 3..(y * 640 + 600) * 3].to_vec()).collect::<Vec<u8>>();
            s.hover_at(320, 450);
            let a = crop(&s.render());
            let mut idle_moved = false;
            for _ in 0..10 {
                ticks(s, 100);
                idle_moved |= crop(&s.render()) != a;
            }
            s.hover_at(560, 370);
            let b = crop(&s.render());
            let mut hover_moved = false;
            for _ in 0..10 {
                ticks(s, 100);
                hover_moved |= crop(&s.render()) != b;
            }
            let n = s.r.posts.len();
            s.click_at(560, 370);
            ticks(s, 100);
            let clicked: Vec<u32> = s.r.posts[n..].iter().map(|m| m.action).collect();
            (s.r.w.view, icon(&fb), placed, ai_posts, idle_moved, hover_moved, clicked)
        })
    };
    let Some((view, ai, placed, ai_posts, idle_moved, hover_moved, clicked)) = run(front::rules_hyp()) else { return };
    assert_eq!(view, Some(0xb321), "the lure chain's first View (trace 57.834)");
    assert_eq!(ai, DIM, "s02-0: the AI icon dimmed");
    assert!(placed, "ActionIcon 0x5ffe in the bar");
    assert_eq!(ai_posts, 0, "the AI click posts nothing");
    assert!(!idle_moved, "the ActionIcon's movie stands while the pointer is elsewhere");
    assert!(hover_moved, "and plays while the pointer is on it");
    assert!(clicked.contains(&0x10379), "{clicked:x?}");
    let off = Rules { h1923_icon_movie_on_hover: false, ..front::rules_hyp() };
    let Some((_, _, _, _, idle_moved, _, _)) = run(off) else { return };
    assert!(idle_moved, "ablation: H311 loops the movie without the pointer");
}
