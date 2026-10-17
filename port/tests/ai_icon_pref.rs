//! H2083 / C299 (HB3, `traces/fo-250.jsonl`): with Pref_bArtificialIntuition 0x369 off the original takes the AI icon
//! (EvidenceIcon 0xdb79) out — the pref-0 branch of the AI icon builder (FUN_00452780), traced obj_remove — and a click at (562,63) hits
//! nothing; turned on again (Alt+A) the icon is built back and its click posts the hint (C69: 0x10e72). Skipped
//! without the game data.
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

const AI_ICON: u32 = 0xdb79;

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

/// Run `steps`, settled; the actions posted meanwhile.
fn run(s: &mut Session, steps: &str) -> Vec<u32> {
    let p0 = s.r.posts.len();
    for i in script::parse(steps).unwrap() {
        script::apply(s, &i);
        front::settle_timers(s, 100, 900_000);
    }
    s.r.posts[p0..].iter().map(|m| m.action).collect()
}

/// fo-250 k5 → k7 then k1 → k3, from fo-056-end.x (0x369 = 1, View 0x319)
#[test]
fn h2083_ai_icon_out_while_the_pref_is_off() {
    let Some(mut s) = session("fo-056-end.x", Rules::all()) else { return };
    assert!(s.r.icon_bars.placed.contains(&AI_ICON));
    run(&mut s, "a:A");
    assert_eq!(s.r.w.vars.get(0x369), Some(0));
    assert!(!s.r.icon_bars.placed.contains(&AI_ICON), "the icon is taken out (traced obj_remove)");
    assert!(!run(&mut s, "p:562,63").contains(&0x10e72), "the click hits nothing");
    run(&mut s, "a:A");
    assert_eq!(s.r.w.vars.get(0x369), Some(1));
    assert!(s.r.icon_bars.placed.contains(&AI_ICON), "built back");
    assert!(run(&mut s, "p:562,63").contains(&0x10e72), "C69's hint");
}

/// A Load with the pref off places no AI icon (fo-250 k00: the original's Load-time FUN_00452780 builds none).
#[test]
fn h2083_no_ai_icon_after_a_load_with_the_pref_off() {
    let Some(mut s) = session("fo-056-end.x", Rules::all()) else { return };
    run(&mut s, "a:A");
    let save = s.save_bytes().expect("save").0;
    let p = std::env::temp_dir().join(format!("h2083-{}.x", std::process::id()));
    std::fs::write(&p, save).unwrap();
    let d = root().join("data/InstalledGame");
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&p).unwrap()));
    let mut t = Session::with_rules(h, g, &d, Rules::all());
    t.r.load_game();
    front::settle_timers(&mut t, 100, 900_000);
    let _ = std::fs::remove_file(&p);
    assert_eq!(t.r.w.vars.get(0x369), Some(0));
    assert!(!t.r.icon_bars.placed.contains(&AI_ICON));
    assert!(!run(&mut t, "p:562,63").contains(&0x10e72));
}

/// Rule off: the port's old behaviour (the icon ignores the pref).
#[test]
fn h2083_rule_off_icon_ignores_the_pref() {
    let Some(mut s) = session("fo-056-end.x", Rules { h2083_ai_icon_pref: false, ..Rules::all() }) else { return };
    run(&mut s, "a:A");
    assert_eq!(s.r.w.vars.get(0x369), Some(0));
    assert!(s.r.icon_bars.placed.contains(&AI_ICON));
    assert!(run(&mut s, "p:562,63").contains(&0x10e72));
}
