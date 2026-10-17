//! Input and display while a movie plays (H1900 / H1901, H452): the playing video takes the View band, the bands outside
//! it, the dialogue box and the movie-hotspot targets keep working; EmotionIcons show only when the script enables them.
use std::path::PathBuf;
use xf::front::cursor::{self, Shape};
use xf::front::host::{self, Button};
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::devices::VIDEO_OWNER;
use xf::world::{class, moviehot, Rules};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn session(save: &str, rules: Rules) -> Option<Session<'static>> {
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
    Some(s)
}

fn steps(s: &mut Session, steps: &str) {
    for i in script::parse(steps).unwrap() {
        script::apply(s, &i);
        front::settle_timers(s, 100, 900_000);
        s.render();
    }
}

fn ticks(s: &mut Session, ms: u64) {
    for _ in 0..ms / 100 {
        s.tick(100);
        s.render();
    }
}

/// The EmotionIcons placed in the bars now.
fn emotion_icons(s: &Session) -> Vec<u32> {
    s.r.icon_bars.placed.iter().copied().filter(|&o| s.r.w.h.has(class::EMOTION_ICON, o)).collect()
}

/// fo-155's route (C132) from fo-149-end.x to View 0xf02, then the door: Cook's video 0x5aa3 (30.6 s) starts.
fn cook_at_the_door(rules: Rules) -> Option<Session<'static>> {
    let mut s = session("fo-149-end.x", rules)?;
    steps(&mut s, "m:20,440;p:21,440;p:239,322;p:286,179;p:300,211;p:570,210;p:570,210");
    assert_eq!(s.r.w.view, Some(0xf02));
    host::motion(&mut s, Some((406, 273)));
    host::button(&mut s, Button::Left, Some((406, 273)));
    host::release(&mut s, Button::Left, Some((406, 273)));
    s.tick(0);
    assert_eq!(s.r.video_playing(), Some(0x5aa3), "Cook's video");
    Some(s)
}

/// H1900 / H1901 / H452: during a talking movie (Cook's 0x5aa3, before the script's +15 s) no EmotionIcon shows, the
/// whole View band — the screen-edge strips of View 0xf02's nav-hotspots 0xf05 / 0xf07 too — is the video's: the null
/// cursor (crosshair) everywhere in it, a click on a nav-hotspot's edge does nothing (no transition, the video plays on).
/// Ablation H1901 off: the 20-px edges are the nav-hotspots again.
#[test]
fn h1900_talking_movie_no_icons_no_nav() {
    let Some(mut s) = cook_at_the_door(Rules::all()) else { return };
    ticks(&mut s, 5000);
    assert_eq!(s.r.video_playing(), Some(0x5aa3));
    assert!(emotion_icons(&s).is_empty(), "{:x?}", emotion_icons(&s));
    let navs: Vec<u32> = s.r.clickables().into_iter().filter(|c| c.cls == class::NAV).map(|c| c.owner).collect();
    assert!(navs.contains(&0xf05) && navs.contains(&0xf07), "the View's nav-hotspots stay active: {navs:x?}");
    // (the video's own movie-hotspot 600 — H1282: the gun shot at Cook — lies over part of it, as targets do)
    for (x, y) in [(5, 200), (630, 200), (20, 120), (560, 200), (620, 300)] {
        host::motion(&mut s, Some((x, y)));
        assert_eq!(s.r.hit(x, y).map(|c| c.owner), Some(VIDEO_OWNER), "({x},{y})");
        assert_eq!(cursor::shape(&s), Some(Shape::Win(cursor::WIN_CROSSHAIR)), "({x},{y}): no nav cursor");
    }
    let n = s.r.transitions.len();
    let l = host::button(&mut s, Button::Left, Some((5, 200)));
    host::release(&mut s, Button::Left, Some((5, 200)));
    assert!(l.as_deref().unwrap_or("").contains("ignored"), "{l:?}");
    ticks(&mut s, 500);
    assert_eq!(s.r.transitions.len(), n, "no nav during the movie");
    assert_eq!(s.r.video_playing(), Some(0x5aa3), "the click does not end the video");
    // ablation: H1901 off → the strips at the edges are the nav-hotspots
    let Some(mut s) = cook_at_the_door(Rules { h1901_video_band_full_width: false, ..Rules::all() }) else { return };
    ticks(&mut s, 5000);
    assert_eq!(s.r.hit(5, 200).map(|c| c.owner), Some(0xf07));
    assert_eq!(s.r.hit(630, 200).map(|c| c.owner), Some(0xf05));
    assert_eq!(s.r.hit(320, 200).map(|c| c.owner), Some(VIDEO_OWNER));
}

/// C132 / C133 kept (H1900): the script enables Cook's three EmotionIcons 15 s into his video (Timer 0) while it still
/// plays; they are placed and drawn at (180,340) / (280,340) / (380,340), and a click on the middle one during the video
/// goes to it (the front no longer drops it): 0x14b5 bN4SelectedEmotionIcon = 1 as in fo-156. Ablation H1900 off: the
/// click is dropped, 0x14b5 stays 0.
#[test]
fn h1900_c132_cook_icons_during_his_video() {
    let Some(mut s) = cook_at_the_door(Rules::all()) else { return };
    ticks(&mut s, 14_500);
    assert!(emotion_icons(&s).is_empty(), "before +15 s: {:x?}", emotion_icons(&s));
    ticks(&mut s, 1000);
    assert_eq!(s.r.video_playing(), Some(0x5aa3), "still Cook's video");
    let mut icons = emotion_icons(&s);
    icons.sort();
    assert_eq!(icons, vec![0x6c54, 0x6c5b, 0x9bc4]);
    for (o, x) in [(0x9bc4, 180), (0x6c5b, 280), (0x6c54, 380)] {
        assert_eq!(s.r.icon_bars.rects.get(&o).map(|r| (r.1[0], r.1[1])), Some((x, 340)), "{o:#x}");
    }
    host::motion(&mut s, Some((20, 200)));
    let fb = s.render();
    let lit = (340..400).flat_map(|y| (280..360).map(move |x| (x, y))).filter(|&(x, y)| {
        let o = (y * 640 + x) * 3;
        fb[o..o + 3] != [0, 0, 0]
    });
    assert!(lit.count() > 3000, "the middle icon drawn");
    host::motion(&mut s, Some((320, 370)));
    let l = host::button(&mut s, Button::Left, Some((320, 370)));
    host::release(&mut s, Button::Left, Some((320, 370)));
    assert!(l.as_deref().unwrap_or("").contains("0x6c5b"), "{l:?}");
    front::settle_timers(&mut s, 100, 900_000);
    assert_eq!(s.r.w.vars.get(0x14b5), Some(1), "C133: the middle EmotionIcon's reaction");
    // ablation: H1900 off → the earlier gate drops the click
    let Some(mut s) = cook_at_the_door(Rules { h1900_video_click_hit_chain: false, ..Rules::all() }) else { return };
    ticks(&mut s, 16_000);
    assert_eq!(emotion_icons(&s).len(), 3);
    host::motion(&mut s, Some((320, 370)));
    let l = host::button(&mut s, Button::Left, Some((320, 370)));
    assert_eq!(l.as_deref(), Some("click ignored: video playing (right click skips)"));
    front::settle_timers(&mut s, 100, 900_000);
    assert_ne!(s.r.w.vars.get(0x14b5), Some(1));
}

/// The shootout (C166 ff.) through the front: the goon movie is the View's background (no video player), so its
/// movie-hotspot target takes the front's click and the gun shot plays the goon's kill movie.
#[test]
fn h1900_shootout_through_the_front() {
    let Some(mut s) = session("fo-174-end.x", Rules::all()) else { return };
    s.r.w.live_rand = Some(1);
    host::motion(&mut s, Some((20, 440)));
    ticks(&mut s, 300);
    let r = s.r.icon_bars.rects.get(&0x4433).expect("the gun in the bar").1;
    let (gx, gy) = ((r[0] as i32 + r[2] as i32) / 2, (r[1] as i32 + r[3] as i32) / 2);
    host::motion(&mut s, Some((gx, gy)));
    host::button(&mut s, Button::Left, Some((gx, gy)));
    host::release(&mut s, Button::Left, Some((gx, gy)));
    ticks(&mut s, 300);
    assert_eq!(s.r.w.dev.held, Some(7), "the gun held");
    host::motion(&mut s, Some((570, 210)));
    host::button(&mut s, Button::Left, Some((570, 210)));
    host::release(&mut s, Button::Left, Some((570, 210)));
    let mut hit = None;
    for _ in 0..60 {
        s.tick(50);
        if let Some(c) = s.r.clickables().into_iter().find(|c| moviehot::is_movie_hot(c.owner)) {
            ticks(&mut s, 200);
            let (x, y) = ((c.rect[0] as i32 + c.rect[2] as i32) / 2, (c.rect[1] as i32 + c.rect[3] as i32) / 2);
            host::motion(&mut s, Some((x, y)));
            hit = host::button(&mut s, Button::Left, Some((x, y)));
            host::release(&mut s, Button::Left, Some((x, y)));
            break;
        }
    }
    assert!(hit.as_deref().is_some_and(|l| l.starts_with("click ")), "{hit:?}");
    ticks(&mut s, 500);
    assert!(s.r.plays.iter().any(|p| [0x52ee, 0x52f0, 0x52f2].contains(&p.1)), "a kill movie: {:x?}", s.r.plays);
}
