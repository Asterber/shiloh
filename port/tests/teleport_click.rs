//! H601: the CharView a CharTeleport targets gets, when its media wrapper is removed, an engine mouse command at
//! the centre of its f2c rect (the wrapper removal, FUN_0047e830). fo-067 k10: CharView 0x58d0 (View 0x3ad7) → CharTeleport
//! 0xcad8 → View 0x19de; the target 0x58e8 (f2c 317,103–422,325, no f50) is clicked at (369, 214) → trigger 0x7493 posts
//! 0x7496, 0x7498, 0x17012, 0x9f0a (`traces/fo-067.jsonl`, `traces/fo-068.jsonl` k18).
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// fo-067's clicks up to k10 from fo-066-end.x; the posts of the last click
fn last_posts(rules: Rules) -> Option<(Vec<u32>, Option<u32>)> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-066-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let steps = [(314, 372), (320, 240), (310, 372), (599, 210), (584, 210), (139, 309), (320, 210), (104, 253), (320, 210), (227, 211), (283, 164)];
    let mut posts = Vec::new();
    for (x, y) in steps {
        let p0 = s.r.posts.len();
        s.hover_at(x, y);
        s.click_at(x, y);
        settle_timers(&mut s, 100, 900_000);
        posts = s.r.posts[p0..].iter().map(|m| m.action).collect();
    }
    Some((posts, s.r.w.view))
}

#[test]
fn h601_teleport_target_gets_a_click() {
    let Some((on, view)) = last_posts(Rules::all()) else { return };
    assert_eq!(view, Some(0x19de));
    for a in [0x7496, 0x7498, 0x17012, 0x9f0a] {
        assert!(on.contains(&a), "{a:#x} missing from {on:x?}");
    }
    let (off, _) = last_posts(Rules { h601_teleport_click: false, ..Rules::all() }).unwrap();
    assert!(!off.contains(&0x7496), "{off:x?}");
}
