//! H263 (H210): a CharView wrapper's player ends at its media end in the closed loop (post-hoc on fo-056 / fo-057).
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Runs `steps` (`p:` = hover + click, `h:` = hover + click on a hand-aimed owner) from `save`; returns the posts
/// (action, trigger) of the last step.
fn last_posts(save: &str, steps: &[(i32, i32, Option<u32>)], rules: Rules) -> Option<Vec<(u32, u32)>> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let mut p0 = 0;
    for &(x, y, owner) in steps {
        p0 = s.r.posts.len();
        match owner {
            Some(o) => {
                s.mouse = Some((x, y));
                s.r.hover(Some(o));
                s.r.click(o);
            }
            None if (x, y) == (20, 440) => {
                s.hover_at(x, y); // fo-057 k9 `m:20,440`
            }
            None => {
                s.hover_at(x, y);
                s.click_at(x, y);
            }
        }
        settle_timers(&mut s, 100, 900_000);
    }
    Some(s.r.posts[p0..].iter().map(|m| (m.action, m.trigger)).collect())
}

/// fo-057 k13 (`traces/fo-057.md`): the SwapDisc travel to Node 0x6af / View 0x1994; CharView 0x5867's player 0x4e0f
/// (XV/19983.xmv, 1166 ms) is registered at 136.80 and removed at its media end 138.06 → Deactivation → trigger 0x7a93
/// (event 9) posts 0x7a96 — 78 posts on the original. Without H263 the core never ends it (77 posts, H210).
#[test]
fn h263_fo057_arrival_video_ends() {
    let steps = [
        (308, 201, None),
        (302, 387, Some(0xda72)),
        (302, 333, None),
        (591, 210, None),
        (570, 210, None),
        (283, 211, None),
        (570, 210, None),
        (309, 209, None),
        (318, 210, None),
        (20, 440, None),
        (71, 440, None),
        (226, 278, None),
        (278, 192, None),
        (326, 348, None),
    ];
    let Some(on) = last_posts("fo-056-end.x", &steps, Rules::all()) else { return };
    assert_eq!(on.len(), 78);
    assert!(on.contains(&(0x7a96, 0x7a93)));
    let off = last_posts("fo-056-end.x", &steps, Rules { h263_char_view_player_end: false, ..Rules::all() }).unwrap();
    assert_eq!(off.len(), 77);
    assert!(!off.iter().any(|p| p.0 == 0x7a96));
}

/// fo-056 k18 (`traces/fo-056.md`, from fo-054-end.x): entering View 0x319 plays 0x4d38 (XV/19768.xmv, 5028 ms; REG
/// 198.5, media end 203.62 → Deactivation → trigger 0xe336 posts 0xe339, 0xf1de) not through a CharView wrapper (0x55e9's f50
/// = [0x4d22, 0x4e89, 0x4e8b]; the core already ends it) — 33 posts predicted and traced; H263 changes nothing there (regression guard).
#[test]
fn h263_fo056_charview_player() {
    let pts = [
        (105, 190, None), (117, 336, Some(0x57a3)), (505, 187, None), (499, 350, Some(0x5686)), (536, 92, None), (570, 210, None),
        (287, 209, None), (220, 370, None), (314, 209, None), (70, 210, None), (470, 209, None), (215, 212, None), (424, 286, None),
        (2, 2, None), (460, 183, None), (70, 210, None), (475, 210, None), (193, 210, None), (322, 210, None),
    ];
    let Some(on) = last_posts("fo-054-end.x", &pts, Rules::all()) else { return };
    let off = last_posts("fo-054-end.x", &pts, Rules { h263_char_view_player_end: false, ..Rules::all() }).unwrap();
    assert_eq!(on, off);
    assert_eq!(on.len(), 33);
    assert!(on.contains(&(0xe339, 0xe336)) && on.contains(&(0xf1de, 0xe336)));
}
