//! Bug report: "after loading a save, the emotion icons of an earlier choice show at the bottom".
//! Cause: the port kept an EmotionIcon's VCEnabled flag at 1 after the icon click switched every icon off (H68), so a
//! save written later held the flag and the Load (H64 / C30: the save's VCEnabled builds every enabled EmotionIcon)
//! placed the icons again. The original clears it: fo-056 enables 0x6c58 / 0x6c47 / 0x6c5e (actions 0x77d0..0x77d2,
//! `traces/fo-056.jsonl` 99.44–99.57), the bot clicks one (k7, (220,370) in View 0x39d), no action ever Disables them
//! (XFiles.hdb has none), and the original's end save `data/bot/fo-056-end.x` has all three flags at 0 (H207 / H980).
use std::path::PathBuf;
use xf::front::{script, settle, settle_timers, Session};
use xf::hdb::Hdb;
use xf::save::SaveState;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const ICONS: [u32; 3] = [0x6c58, 0x6c47, 0x6c5e];

fn flag(st: &SaveState, id: u32) -> u8 {
    st.enabled.iter().find(|((c, o), _)| *c == 0x48 && *o == id).map(|e| e.1).unwrap_or(0)
}

/// fo-056's first 8 steps from the original's fo-054-end.x: the icons are enabled, one is clicked; the save the port
/// writes afterwards has their flags at 0 as the original's fo-056-end.x, and a Load of it places no EmotionIcon.
#[test]
fn h980_clicked_emotion_icons_not_saved_enabled() {
    let d = root().join("data/InstalledGame");
    let start = root().join("data/bot/fo-054-end.x");
    let orig_end = root().join("data/bot/fo-056-end.x");
    if !d.join("XFiles.hdb").exists() || !start.exists() || !orig_end.exists() {
        eprintln!("data missing: skipped");
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&start).unwrap();
    let orig = SaveState::from_gam(&Hdb::open(&orig_end).unwrap());
    for id in ICONS {
        assert_eq!(flag(&orig, id), 0, "original's fo-056-end.x {id:#x}");
    }
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let steps =
        script::parse("p:105,190;h:117,336,57a3;p:505,187;h:499,350,5686;p:536,92;p:570,210;p:287,209").unwrap();
    for st in &steps {
        script::apply(&mut s, st);
        settle_timers(&mut s, 100, 900_000);
    }
    assert_eq!(s.r.w.view, Some(0x39d), "{:?}", s.log);
    let placed: Vec<u32> = ICONS.iter().copied().filter(|i| s.r.icon_bars.placed.contains(i)).collect();
    assert_eq!(placed.len(), 3, "the three icons are shown before the click");
    script::apply(&mut s, &script::Input::Click(220, 370));
    settle_timers(&mut s, 100, 900_000);
    assert!(ICONS.iter().all(|i| !s.r.icon_bars.placed.contains(i)), "icons off after the click (H68)");
    let st = s.save_state(&SaveState::from_gam(&g));
    for id in ICONS {
        assert_eq!(flag(&st, id), 0, "port save {id:#x}");
    }
}

/// A save holding an EmotionIcon flag 1 (the original's fo-2b3-icons.x: the first choice pending) still shows its icons
/// after a Load (H64): the fix must not drop legitimately enabled icons.
#[test]
fn h980_pending_choice_icons_still_load() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2b3-icons.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle(&mut s, 100, 10_000); // before 0x2b3's 30-s Timer disables them (H59 / H61)
    for i in [0x46fc, 0x4700, 0x6fde] {
        assert!(s.r.icon_bars.placed.contains(&i), "{i:#x}");
    }
}
