//! C107: a plain click on an IdeaIcon of the shown dialogue box plays its idea's answer — button-down = the
//! icon's Mouse Click + the box's state 0, button-up on the icon = the IdeaMap's Conversation registered and activated
//! (`traces/fo-124.jsonl` 63.96–64.37: Skinner 0x584e at the warehouse 0x904, jar icon 0x5583 at (35,55) → Conversation
//! 0x5977, trigger 0x8881 posts 0x8884 / 0x8885). H921 (port choice): a release elsewhere plays nothing and re-shows the box.
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const SKINNER: (i32, i32) = (210, 218);
const JAR: (i32, i32) = (35, 55);

fn posts_since(s: &Session, p0: usize) -> Vec<u32> {
    s.r.posts[p0..].iter().map(|m| m.action).collect()
}

fn with_box(f: impl FnOnce(&mut Session)) {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-123-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    s.hover_at(SKINNER.0, SKINNER.1);
    s.click_at(SKINNER.0, SKINNER.1);
    settle_timers(&mut s, 100, 900_000);
    assert!(s.r.w.dbox_layout().is_some(), "Skinner's box is shown");
    f(&mut s);
}

#[test]
fn c107_plain_click_plays_the_answer() {
    with_box(|s| {
        let p0 = s.r.posts.len();
        s.hover_at(JAR.0, JAR.1);
        s.click_at(JAR.0, JAR.1);
        s.tick(0);
        let p = posts_since(s, p0);
        assert!(p.contains(&0x8884) && p.contains(&0x8885), "{p:x?}");
        // the answer's end re-shows the box with the same icon (R38)
        settle_timers(s, 100, 900_000);
        assert!(s.r.w.dbox_layout().is_some());
        assert!(s.r.hit(JAR.0, JAR.1).is_some_and(|c| c.owner == 0x5583));
    });
}

#[test]
fn c107_answer_waits_for_the_button_up() {
    with_box(|s| {
        let p0 = s.r.posts.len();
        s.hover_at(JAR.0, JAR.1);
        s.press_at(JAR.0, JAR.1);
        s.tick(150);
        assert!(!posts_since(s, p0).contains(&0x8884), "no answer before the button-up");
        assert_eq!(s.r.w.idea_drag, Some(0x5583));
        s.release_at(Some(JAR));
        s.tick(0);
        assert!(posts_since(s, p0).contains(&0x8884));
    });
}

#[test]
fn h921_release_elsewhere_reshows_the_box() {
    with_box(|s| {
        let p0 = s.r.posts.len();
        s.hover_at(JAR.0, JAR.1);
        s.press_at(JAR.0, JAR.1);
        s.tick(100);
        s.release_at(Some((600, 470)));
        s.tick(500);
        assert!(!posts_since(s, p0).contains(&0x8884));
        assert!(s.r.w.dbox_layout().is_some(), "the box is shown again");
        assert!(s.r.hit(JAR.0, JAR.1).is_some_and(|c| c.owner == 0x5583));
    });
}

/// The window / web front's path: host::button (down) then host::release (up) on the icon.
#[test]
fn c107_host_button_and_release() {
    use xf::front::host::{button, release, Button};
    with_box(|s| {
        let p0 = s.r.posts.len();
        xf::front::host::motion(s, Some(JAR));
        button(s, Button::Left, Some(JAR));
        s.tick(100);
        assert!(!posts_since(s, p0).contains(&0x8884));
        assert!(release(s, Button::Left, Some(JAR)).is_some());
        s.tick(0);
        assert!(posts_since(s, p0).contains(&0x8884));
    });
}

/// Bug report (H1023): the IdeaIcons in the top band are drawn through their pictures' alpha plane, without
/// the white background — as the original's `traces/fo-123.jsonl.burst/k15-0.png` (jar x.pff 174 at (20,36),
/// cigarette 188 at (57,40), bag 172 at (104,32), on black): every pixel = α 255 the picture, α 0 black, else
/// src·α >> 8 (4156 of 4156 pixels of the three icons, measured on the PNG).
#[test]
fn h1023_idea_icons_alpha() {
    with_box(|s| {
        let fb = s.render();
        if let Some(d) = std::env::var_os("XF_DOC_DUMP") {
            xf::front::write_ppm(&PathBuf::from(d).join("idea-icons.ppm"), &fb).unwrap();
        }
        let px = |x: usize, y: usize| -> [u8; 3] { fb[(y * xf::front::SCREEN_W + x) * 3..][..3].try_into().unwrap() };
        let white = (20..80).flat_map(|y| (10..150).map(move |x| (x, y))).filter(|&(x, y)| px(x, y) == [255, 255, 255]).count();
        assert_eq!(white, 0, "white pixels in the IdeaIcon band");
        for (asset, x0, y0) in [(0x4c60u32, 20usize, 36usize), (0x4c80, 57, 40), (0x4c5c, 104, 32)] {
            let p = s.picture(asset).cloned().expect("IdeaIcon picture");
            let a = p.alpha.as_ref().expect("alpha plane");
            let mut same = 0;
            for y in 0..p.height {
                for x in 0..p.width {
                    let k = y * p.width + x;
                    let want = [0, 1, 2].map(|c| ((p.rgb[k * 3 + c] as u32 * a[k] as u32) >> 8) as u8);
                    let want = if a[k] == 255 { [p.rgb[k * 3], p.rgb[k * 3 + 1], p.rgb[k * 3 + 2]] } else { want };
                    same += (px(x0 + x, y0 + y) == want) as usize;
                }
            }
            assert_eq!(same, p.width * p.height, "IdeaIcon {asset:#x} at ({x0},{y0})");
        }
    });
}
