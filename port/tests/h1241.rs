//! H1241 (C161, `traces/fo-172.jsonl` 121.946–122.028): at the Day-4 arrival at View 0x38f the original runs the
//! background's (0xb511) Enables before the View's own Activation actions (0x13b9c, posted earlier) — the H89 queue.
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// fo-172's walk from fo-169-end.x (PDA → Seattle → Field Office → 0x2c9 → 0x2e0 → 0x390 → 0x38f). At the 0x38f
/// arrival the View's cmd 0 queues its cmd 11 (background) before its Activation actions (0x13b9c); the background
/// 0x9796's Activation (trigger 0xb511: 0xb516 / 0xb517 / 0x13397, the Enables) runs depth-first right behind it, then
/// 0x13b9c's 0x1654a / 0x1654b / 0x13b9f, 0x13398's 0x1339a, 0x13ba0 / 0x13ba1 (the Disables) — the trace's order — so
/// Shanks's CharView 0x131fc ends disabled and (305,185) is the forward nav to 0x379 (kind 3). The H76 runner (H89 off)
/// executes them in post order, ends with 0x131fc enabled and CharTeleports there (kind 0).
#[test]
fn h1241_fo172_background_actions_before_view_actions() {
    assert!(Rules::all().h89_queue_model);
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-169-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    for h89 in [true, false] {
        let rules = Rules { h89_queue_model: h89, ..Rules::all() };
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        s.hover_at(20, 440);
        settle_timers(&mut s, 100, 900_000);
        let mut e0 = 0;
        for (x, y) in [(21, 440), (239, 322), (326, 234), (276, 210), (512, 209), (299, 209), (70, 210)] {
            e0 = s.r.execs.len();
            s.hover_at(x, y);
            assert!(s.click_at(x, y).is_some(), "({x},{y})");
            settle_timers(&mut s, 100, 900_000);
        }
        assert_eq!(s.r.w.view, Some(0x38f));
        let offers = s.r.clickables().iter().any(|c| c.owner == 0x131fc);
        let execs = s.r.execs[e0..e0 + 9].to_vec();
        let t0 = s.r.transitions.len();
        s.hover_at(305, 185);
        s.click_at(305, 185);
        settle_timers(&mut s, 100, 900_000);
        let trans = s.r.transitions[t0..].to_vec();
        if h89 {
            assert_eq!(execs, [0xb516, 0xb517, 0x13397, 0x1654a, 0x1654b, 0x13b9f, 0x1339a, 0x13ba0, 0x13ba1]);
            assert!(!offers, "0x131fc disabled on Day 4 (C161)");
            assert_eq!(trans.first(), Some(&(0x379, 3)), "the forward nav (fo-172 125.694)");
        } else {
            assert_eq!(execs, [0x1654a, 0x1654b, 0x13b9f, 0x1339a, 0x13ba0, 0x13ba1, 0xb516, 0xb517, 0x13397]);
            assert!(offers);
            assert_eq!(trans.first(), Some(&(0x379, 0)));
        }
    }
}
