//! H1000 (bug report "no way up the stairs in View 0x1847"): the warehouse office's hidden stairs (View 0x1847
//! (the stairs view), nav 0x18a6 → 0x1848, VCEnabled record 0x1340) are a dark passage — the nav is enabled only
//! while the flashlight (iCurrInventory 0x15a = Inventory_iFlashlight 0x160 = 8) or the night vision (0x164 = 0xd) is
//! held: at the View's Activation (ActionList 0x159d0: 0x159d2 / 0x159d3 / 0x159d4), or by the item's own click while at
//! 0x1847 (0x159d9 in OnFLightClick's list 0x7ce3, 0x159da in OnNVisionClick's 0x7cf7, cond iCurrViewSpecial 0x421 =
//! 0x1847); the View's Deactivation (0x159d6: 0x159d7 / 0x159d8) disables it again. A player's whole manual route from
//! fo-110-end.x (inside the warehouse): stairs with the night vision → crowbar from the toolbox (0xc05, item 0x4416) →
//! crate at View 0xe8f with the crowbar (0xc1c). Port behaviour from static data; the original's flashlight click
//! evaluating 0x159d9 is traced (`traces/fo-043.jsonl` 202.921 act_post of 0x7ce3, 211.103 action_cond 0x159d9).
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const STAIRS: u32 = 0x18a6;
/// fo-110-end.x View 0x8ed → 0x8fb → 0x8a3 → 0x835 → 0x809 → 0xe81 (the office, next to the stairs)
const TO_OFFICE: &str = "p:48,210;p:320,210;p:320,210;p:552,227;p:352,210";
const ENTER: &str = "p:379,209"; // 0xe81 → 0x1847
const BACK: &str = "p:570,210;p:70,210"; // 0x1847 → 0x1550a → 0x1847 (turn and turn back)
const NIGHT_VISION: &str = "m:20,440;p:71,440";
const LOCKPICK: &str = "m:20,440;p:106,440";

fn session<'a>(h: &'a Hdb, g: &'a Hdb, d: &std::path::Path) -> Session<'a> {
    let mut s = Session::with_rules(h, g, d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    s
}

fn steps(s: &mut Session, steps: &str) {
    for st in steps.split(';').filter(|x| !x.is_empty()) {
        let (kind, xy) = st.split_at(2);
        let v: Vec<i32> = xy.split(',').map(|x| x.parse().unwrap()).collect();
        s.hover_at(v[0], v[1]);
        if kind == "p:" {
            s.click_at(v[0], v[1]);
        }
        settle_timers(s, 100, 900_000);
    }
}

/// Runs `f` on a session loaded from fo-110-end.x (None without the game data).
fn with_save(f: impl FnOnce(&mut Session)) -> Option<()> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-110-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = session(&h, &g, &d);
    f(&mut s);
    Some(())
}

fn stairs_shown(s: &Session) -> bool {
    s.r.w.view == Some(0x1847) && s.r.w.hotspots.contains(&STAIRS)
}

#[test]
fn h1000_stairs_dark_without_a_light() {
    with_save(|s| {
        steps(s, &format!("{TO_OFFICE};{ENTER}"));
        assert_eq!(s.r.w.view, Some(0x1847));
        assert!(!stairs_shown(s), "no light held: the stairs nav stays disabled");
        // a non-light item (a player's lockpick report, withdrawn) does not enable it either
        steps(s, &format!("p:570,210;{LOCKPICK};p:70,210"));
        assert_eq!(s.r.w.view, Some(0x1847));
        assert!(!stairs_shown(s));
    });
}

#[test]
fn h1000_stairs_with_night_vision_on_entry_and_on_click() {
    // held when entering: the Activation enables it
    with_save(|s| {
        steps(s, &format!("{TO_OFFICE};{NIGHT_VISION};{ENTER}"));
        assert!(stairs_shown(s));
        // leaving disables it; coming back with the goggles still held enables it again
        steps(s, BACK);
        assert!(stairs_shown(s));
    });
    // clicked while standing at 0x1847: OnNVisionClick's 0x159da enables it at once
    with_save(|s| {
        steps(s, &format!("{TO_OFFICE};{ENTER}"));
        assert!(!stairs_shown(s));
        steps(s, NIGHT_VISION);
        assert!(stairs_shown(s));
    });
}

/// The player's route: up the stairs with the night vision, the crowbar from the toolbox, the crate opened with it.
#[test]
fn h1000_stairs_crowbar_crate() {
    with_save(|s| {
        steps(s, &format!("{TO_OFFICE};{NIGHT_VISION};{ENTER}"));
        steps(s, "p:272,209"); // up the stairs
        assert_eq!(s.r.w.view, Some(0x1848));
        steps(s, "p:151,210;p:248,211;p:312,297"); // the toolbox, the crowbar
        assert_eq!(s.r.w.vars.get(0xc05), Some(1), "bAIWarehouse_GetCrowbar");
        // back down and round to the crates near the front door (View 0x930 → 0xe8f)
        steps(
            s,
            "p:70,210;p:160,211;p:354,212;p:406,210;p:257,210;p:472,210;p:387,212;p:290,208;p:294,209;p:570,210;\
             p:320,210;p:320,210;p:278,210;p:70,210;p:320,210",
        );
        assert_eq!(s.r.w.view, Some(0x930));
        steps(s, "m:20,440;p:421,440"); // the crowbar (inventory 0x4416, value 6)
        assert_eq!(s.r.w.vars.get(0x15a), Some(6));
        steps(s, "p:70,210;p:320,309;p:251,257;p:326,207"); // → 0xe8f, crowbar on the crate
        assert_eq!(s.r.w.vars.get(0xc1c), Some(1), "bAIWarehouse_CrateOpen");
    });
}
