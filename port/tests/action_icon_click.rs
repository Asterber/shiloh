//! C296 (H151 / H2010; `traces/fo-111.jsonl` 689.199–689.447, `traces/fo-220.jsonl` 57.818–57.847, `traces/fo-243.jsonl`
//! 57.886–57.914): a click on an ActionIcon takes it out (state 0, removal, Deactivation → H207 clears its VCEnabled flag)
//! after its click actions, with no Disable in the data. Report: "after the Tarakan a 'Go to wheel house' hint stays at
//! the bottom right": the Tarakan AI list's ActionIcon 0x12174 (Enable 0x10d2 by 0x12183) jumps to View 0x2db9, which —
//! unlike the wheelhouse Views 0x2dc7 / 0x2db2 / 0x2da9 — does not Disable 0x10d2, so without C296 the icon stayed placed
//! at (560,370) for the rest of the game (and kept jumping back to the Tarakan).
use std::path::PathBuf;
use xf::front::{script, settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn leak(p: PathBuf) -> Option<&'static Hdb> {
    p.exists().then(|| &*Box::leak(Box::new(Hdb::open(&p).unwrap())))
}

const GO_TO_WHEELHOUSE: u32 = 0x12174;

/// fo-140-end.x (the Tarakan, View 0x2f69): the AI icon 8 × (the 8th runs 0x121ac → 0x12183 Enable 0x10d2), then the
/// ActionIcon (560,370). Returns (placed before the click, View after, placed after, VCEnabled flag after).
fn wheelhouse_icon(rule: bool) -> Option<(bool, Option<u32>, bool, bool)> {
    let d = root().join("data/InstalledGame");
    let (h, save) = (leak(d.join("XFiles.hdb"))?, leak(root().join("data/bot/fo-140-end.x"))?);
    let mut s = Session::with_rules(h, save, &d, Rules { c296_action_icon_click_off: rule, ..Rules::all() });
    s.r.w.live_rand = Some(1);
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let run = |s: &mut Session, steps: &str| {
        for i in script::parse(steps).unwrap() {
            script::apply(s, &i);
            settle_timers(s, 100, 900_000);
        }
    };
    run(&mut s, &["p:562,63"; 8].join(";"));
    let before = s.r.icon_bars.placed.contains(&GO_TO_WHEELHOUSE);
    run(&mut s, "p:560,370");
    Some((
        before,
        s.r.w.view,
        s.r.icon_bars.placed.contains(&GO_TO_WHEELHOUSE),
        s.r.w.is_enabled(0x47, GO_TO_WHEELHOUSE),
    ))
}

#[test]
fn c296_action_icon_click_takes_the_icon_out() {
    let Some(on) = wheelhouse_icon(true) else { return };
    assert_eq!(on, (true, Some(0x2db9), false, false), "C296 on: the clicked icon leaves, its flag is cleared (H207)");
    let off = wheelhouse_icon(false).unwrap();
    assert_eq!(off, (true, Some(0x2db9), true, true), "C296 off: the icon stays (the reported bug)");
}
