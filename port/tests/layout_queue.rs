//! H1500 (H1462, `traces/fo-206.jsonl` 65.991–69.721): a layout's items are activated one by one from the
//! queue, and InitEmailRead's Disable of IN BOX 0x116cc (posted by 0xed62's Activation, before 0x116cc's own Activation)
//! finds no active item — IN BOX stays in the e-mail read view 0xed5d. Route = fo-206's first steps from fo-203-end.x.
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

const READ_VIEW: u32 = 0xed5d;
const IN_BOX: u32 = 0x116cc;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// (IN BOX in the shown read view's items, IN BOX active in the runner)
fn read_view(rules: Rules) -> Option<(bool, bool)> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-203-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, rules);
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    for i in script::parse("m:20,440;p:21,440;p:350,399;p:323,331").unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
    }
    let l = s.r.w.layouts().iter().find(|l| l.0 == READ_VIEW).expect("e-mail read view shown");
    Some((l.1.iter().any(|x| x.0 == IN_BOX), s.r.active.contains(&IN_BOX)))
}

#[test]
fn h1500_email_read_view_keeps_in_box() {
    let Some(on) = read_view(front::rules_hyp()) else { return };
    assert_eq!(on, (true, true));
    let off = read_view(Rules { h1500_iitem_disable_needs_active: false, ..front::rules_hyp() }).unwrap();
    // ablation: the Disable takes the item out of the layout (not drawn, not clickable); its Activation, still
    // queued behind the Disable's Deactivation, runs afterwards
    assert_eq!(off, (false, true));
}
