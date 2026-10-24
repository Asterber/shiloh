//! Issue Asterber/shiloh#2 point 5 (a Preference changed in Options not applied): C311 (`traces/fo-266.jsonl`,
//! `traces/fo-267.jsonl`) — the in-game menu's Return applies the Preferences (FUN_00452b20: the AI icon refreshed,
//! the captions' bars re-placed, the inventory row shown with 'V' / hidden with 'H'), and the Options' "Return to main
//! menu" (ReturnMainMenu) writes them to the host's (the registry). Skipped without the game data.
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

const AI_ICON: u32 = 0xdb79;
const VAR_AI: u32 = 0x369;
const VAR_INVENTORY: u32 = 0x3d7;
const VAR_CC: u32 = 0xa0d;
/// Esc, Options, a tab — the toggle follows
const OPTIONS_PLAY: &str = "k:27;p:555,285;p:550,118";
const OPTIONS_GRAPHICS: &str = "k:27;p:555,285;p:480,87";
const AI_TOGGLE: &str = "p:60,175";
const INVENTORY_TOGGLE: &str = "p:360,221";
const CC_TOGGLE: &str = "p:60,419";
/// "Return to main menu", then the menu's Return
const BACK: &str = "p:510,460;p:555,240";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn session(rules: Rules) -> Option<Session<'static>> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-056-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        eprintln!("data missing: skipped");
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, rules);
    s.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    Some(s)
}

/// Run `steps` (`;`-separated, several strings joined), settled after each; the actions posted meanwhile.
fn run(s: &mut Session, steps: &[&str]) -> Vec<u32> {
    let p0 = s.r.posts.len();
    for i in script::parse(&steps.join(";")).unwrap() {
        script::apply(s, &i);
        front::settle_timers(s, 100, 900_000);
    }
    s.r.posts[p0..].iter().map(|m| m.action).collect()
}

fn placed(s: &Session, o: u32) -> bool {
    s.r.icon_bars.placed.contains(&o)
}

/// The active inventory objects (the bar's row).
fn inventory(s: &Session) -> Vec<u32> {
    s.r.active.iter().copied().filter(|&o| s.r.w.h.has(xf::world::class::INVENTORY, o)).collect()
}

/// fo-266 k1–k7: Artificial Intuition off in Options → the icon is gone at the menu's Return (ai_refresh, obj_remove;
/// shot k06), the click at its place hits nothing.
#[test]
fn c311_ai_off_in_options_takes_the_icon_out_at_the_return() {
    let Some(mut s) = session(Rules::all()) else { return };
    assert!(placed(&s, AI_ICON));
    run(&mut s, &[OPTIONS_PLAY, AI_TOGGLE]);
    assert_eq!(s.r.w.vars.get(VAR_AI), Some(0));
    run(&mut s, &[BACK]);
    assert!(!s.r.menu_open);
    assert!(!placed(&s, AI_ICON), "the icon is taken out at the Return");
    assert!(!run(&mut s, &["p:562,63"]).contains(&0x10e72), "the click hits nothing");
}

/// fo-266 k8–k18: icon out by Alt+A, then Artificial Intuition on in Options → the icon is back at the Return
/// (bar_place 118.629, shot k17) and its click posts the hint (act_post 0x10e72).
#[test]
fn c311_ai_on_in_options_places_the_icon_at_the_return() {
    let Some(mut s) = session(Rules::all()) else { return };
    run(&mut s, &["a:A"]);
    assert!(!placed(&s, AI_ICON));
    run(&mut s, &[OPTIONS_PLAY, AI_TOGGLE, BACK]);
    assert_eq!(s.r.w.vars.get(VAR_AI), Some(1));
    assert!(placed(&s, AI_ICON), "the icon is back at the Return");
    assert!(run(&mut s, &["p:562,63"]).contains(&0x10e72), "C69's hint");
}

/// fo-267 k5 / k8 / k10 / k15 / k20: Inventory 'V' in Options → the row is shown at the Return with the pointer away;
/// back to 'H' → hidden again.
#[test]
fn c311_inventory_row_follows_the_pref_at_the_return() {
    let Some(mut s) = session(Rules::all()) else { return };
    let inv = inventory(&s);
    assert!(!inv.is_empty());
    run(&mut s, &["m:300,200"]);
    assert!(inv.iter().all(|&o| !placed(&s, o)), "'H': the row hidden");
    run(&mut s, &[OPTIONS_GRAPHICS, INVENTORY_TOGGLE, BACK, "m:300,200"]);
    assert_eq!(s.r.w.vars.get(VAR_INVENTORY), Some(b'V' as i32));
    assert!(inv.iter().all(|&o| placed(&s, o)), "'V': the row shown at the Return");
    run(&mut s, &[OPTIONS_GRAPHICS, INVENTORY_TOGGLE, BACK, "m:300,200"]);
    assert_eq!(s.r.w.vars.get(VAR_INVENTORY), Some(b'H' as i32));
    assert!(inv.iter().all(|&o| !placed(&s, o)), "'H' again: hidden at the Return");
}

/// fo-267 k6 / k8: Closed Captioning on in Options → the bars are re-placed at the Return (FUN_004529b0 →
/// FUN_004684b0): the 'V' inventory row lower (the shots: rows 426–464 with CC on, 420–459 off).
#[test]
fn c311_captions_replace_the_bars_at_the_return() {
    let Some(mut s) = session(Rules::all()) else { return };
    run(&mut s, &[OPTIONS_GRAPHICS, INVENTORY_TOGGLE, BACK]);
    let first = inventory(&s)[0];
    let y = |s: &Session| s.r.icon_bars.rects.get(&first).map(|r| r.1[1]).unwrap();
    let y_off = y(&s);
    run(&mut s, &[OPTIONS_GRAPHICS, CC_TOGGLE, BACK]);
    assert_eq!(s.r.w.vars.get(VAR_CC), Some(1));
    assert!(y(&s) > y_off, "CC on: the row lower ({} → {})", y_off, y(&s));
}

/// fo-266 `prefs_to_reg` (ReturnMainMenu 0xc67c): leaving Options writes the
/// Preferences to the host's — kept by the front (a restart has them) and taken by the next Load.
#[test]
fn c311_return_to_main_menu_writes_the_host_prefs() {
    let Some(mut s) = session(Rules::all()) else { return };
    assert!(s.host_prefs.contains(&(VAR_AI, 1)));
    run(&mut s, &[OPTIONS_PLAY, AI_TOGGLE]);
    assert!(s.host_prefs.contains(&(VAR_AI, 1)), "not before the Options' return");
    run(&mut s, &["p:510,460"]);
    assert!(s.host_prefs.contains(&(VAR_AI, 0)), "written by ReturnMainMenu");
    assert!(std::mem::take(&mut s.prefs_saved), "the front keeps them");
}

/// Rule off: the port before — the icon stays after the Return, nothing written to the host's.
#[test]
fn c311_rule_off_icon_stays() {
    let Some(mut s) = session(Rules { c311_menu_return_prefs: false, ..Rules::all() }) else { return };
    run(&mut s, &[OPTIONS_PLAY, AI_TOGGLE, BACK]);
    assert_eq!(s.r.w.vars.get(VAR_AI), Some(0));
    assert!(placed(&s, AI_ICON));
    assert!(s.host_prefs.contains(&(VAR_AI, 1)) && !s.prefs_saved);
}
