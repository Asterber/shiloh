//! H2100 (H2084; `traces/fo-250.jsonl`, `traces/fo-252.jsonl`): a Load takes the host's Preferences (the original's
//! registry / prefs object) into the RegPreferences variables over the save's own values — fo-056-end.x (0x369 = 1)
//! loaded with the registry's Artificial Intuition 0 builds no AI icon; a Save writes the running game's Preferences
//! back to the host's. Skipped without the game data.
use std::path::PathBuf;
use xf::front::settings::{parse_host_prefs, Settings, HOST_PREFS};
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

const AI_ICON: u32 = 0xdb79;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn leak(p: PathBuf) -> Option<&'static Hdb> {
    p.exists().then(|| &*Box::leak(Box::new(Hdb::open(&p).unwrap())))
}

/// A Load of `save` (a gam) with the host's Artificial Intuition `host`.
fn load(save: &'static Hdb, host: i32, rules: Rules) -> Option<Session<'static>> {
    let d = root().join("data/InstalledGame");
    let h = leak(d.join("XFiles.hdb"))?;
    let mut s = Session::with_rules(h, save, &d, rules);
    s.host_prefs = vec![(0x369, host)];
    s.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    Some(s)
}

fn run(s: &mut Session, steps: &str) -> Vec<u32> {
    let p0 = s.r.posts.len();
    for i in script::parse(steps).unwrap() {
        script::apply(s, &i);
        front::settle_timers(s, 100, 900_000);
    }
    s.r.posts[p0..].iter().map(|m| m.action).collect()
}

/// fo-056-end.x with its 0x369 set to `v` (a port save of it after Alt+A when 0)
fn fo056(v: i32) -> Option<&'static Hdb> {
    let g = leak(root().join("data/bot/fo-056-end.x"))?;
    assert_eq!(g.read(0x53, 0x369).map(|r| r.u32("f38")), Some(1), "fo-056-end.x has 0x369 = 1");
    if v == 1 {
        return Some(g);
    }
    let mut s = load(g, 1, Rules::all())?;
    run(&mut s, "a:A");
    assert_eq!(s.r.w.vars.get(0x369), Some(0));
    Some(Box::leak(Box::new(Hdb::from_bytes(s.save_bytes().expect("save").0))))
}

/// fo-250 k00 / k7: the save's 0x369 = 1, the registry's 0 → no AI icon, the (562,63) click hits nothing
#[test]
fn h2100_load_with_the_host_pref_off_has_no_ai_icon() {
    let Some(g) = fo056(1) else { return };
    let Some(mut s) = load(g, 0, Rules::all()) else { return };
    assert_eq!(s.r.w.vars.get(0x369), Some(0), "the host's value over the save's");
    assert!(!s.r.icon_bars.placed.contains(&AI_ICON));
    assert!(!run(&mut s, "p:562,63").contains(&0x10e72));
    // fo-250 k1 / fo-252 k1: the first Alt+A turns it on (pref_ai_set value 1)
    run(&mut s, "a:A");
    assert_eq!(s.r.w.vars.get(0x369), Some(1));
    assert!(s.r.icon_bars.placed.contains(&AI_ICON));
}

/// The reverse: a save with 0x369 = 0 loaded with the host's pref on → the icon, C69's hint
#[test]
fn h2100_load_with_the_host_pref_on_has_the_ai_icon() {
    let Some(g) = fo056(0) else { return };
    let Some(mut s) = load(g, 1, Rules::all()) else { return };
    assert_eq!(s.r.w.vars.get(0x369), Some(1));
    assert!(s.r.icon_bars.placed.contains(&AI_ICON));
    assert!(run(&mut s, "p:562,63").contains(&0x10e72));
}

/// Rule off: a Load keeps the save's values (H1990)
#[test]
fn h2100_rule_off_load_keeps_the_saves_value() {
    let Some(g) = fo056(1) else { return };
    let Some(s) = load(g, 0, Rules { h2100_load_host_prefs: false, ..Rules::all() }) else { return };
    assert_eq!(s.r.w.vars.get(0x369), Some(1));
    assert!(s.r.icon_bars.placed.contains(&AI_ICON));
}

/// H2084 (fo-249 / fo-252): a Save writes the running game's Preferences to the host's (`prefs_saved` for the front)
#[test]
fn h2100_save_writes_the_host_prefs() {
    let Some(g) = fo056(1) else { return };
    let Some(mut s) = load(g, 1, Rules::all()) else { return };
    let dir = std::env::temp_dir().join(format!("h2100-saves-{}", std::process::id()));
    s.set_saves(xf::save::Slots::open(&dir));
    run(&mut s, "a:A");
    assert_eq!(s.host_prefs, vec![(0x369, 1)], "Alt+A alone leaves the host's Preferences (fo-248: registry stays 1)");
    s.r.save_req.push(xf::world::Out::SaveGame { slot: 1, name: "h2100".into() });
    s.tick(0);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(std::mem::take(&mut s.prefs_saved));
    assert!(s.host_prefs.contains(&(0x369, 0)), "{:?}", s.host_prefs);
    assert_eq!(s.host_prefs.len(), 24, "all 24 RegPreferences variables");
}

/// The fronts keep them in the settings (the window's file, the page's localStorage): `host_prefs = var:value …`
#[test]
fn h2100_settings_keep_the_host_prefs() {
    let mut set = Settings::default();
    assert_eq!(set.host_prefs(), None);
    assert!(!set.to_text().contains(HOST_PREFS));
    set.set_host_prefs(&[(0x369, 0), (0x3d5, 72)]);
    assert_eq!(set.get(HOST_PREFS), "369:0 3d5:72");
    let (back, warn) = Settings::parse(&set.to_text());
    assert!(warn.is_empty(), "{warn:?}");
    assert_eq!(back.host_prefs(), Some(vec![(0x369, 0), (0x3d5, 72)]));
    assert!(parse_host_prefs("1234:1").is_err(), "not a RegPreferences variable");
    assert!(set.clone().set(HOST_PREFS, "369=1").is_err());
}
