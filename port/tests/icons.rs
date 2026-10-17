//! Icon objects of the type-3 Enable / Disable (H203–H206; static decode of FUN_004201a0's icon cases, RE_NOTES H203–H207)
//! in the closed loop. Post-hoc only: no trace enables an Action / Evidence / Idea icon; the traced EmotionIcon Enables
//! (0xda6e / 0xda6f / 0xda70, fo-005..fo-011) are of preloaded icons and show no event (H204 predicts none).
mod common;
use std::path::PathBuf;
use xf::front::{click_point, smoke, Session, Step};
use xf::hdb::Hdb;
use xf::replay::Ev;
use xf::world::run::Runner;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// H204 (with H150): in the new-game sessions fo-005..fo-009 the preloaded EmotionIcons get state 1 from their Enable
/// (0xda6e..0xda70) and are placed in bar 3 in that order — (180,340), (280,340), (380,340), 80×60 —, so the traced
/// dialogue-icon click is a click on screen. Ablation: without H204 they are never placed in a new game (activated while
/// disabled), the click can only go by owner.
#[test]
fn h204_new_game_icons_placed_by_enable() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut checked = Vec::new();
    for name in ["fo-005", "fo-006", "fo-007", "fo-008", "fo-009"] {
        let Some(trace) = common::trace(&format!("{name}.jsonl")) else { return };
        assert!(trace.iter().any(|e| matches!(e.ev, Ev::Exec { action: 0xda6e, .. })), "{name}");
        let route = xf::front::trace_route_with(&trace, true);
        let Some(k) = route.iter().position(|st| matches!(st, Step::User(o) if [0x46fc, 0x4700, 0x6fde].contains(o))) else {
            continue; // no icon click in this session
        };
        let Step::User(icon) = route[k] else { unreachable!() };
        checked.push(name);
        for on in [true, false] {
            let rules = Rules { h204_emotion_icon_type3: on, ..xf::front::rules_hyp() };
            let mut s = Session::with_rules(&h, &g, &d, rules);
            s.r.uneval = Runner::uneval_from_trace(&trace, &h);
            let res = smoke(&mut s, &route[..k], None);
            assert!(res.failed.is_empty(), "{name}: {:?}", res.failed);
            if on {
                let want = [(0x46fc, 180), (0x4700, 280), (0x6fde, 380)];
                for (o, x) in want {
                    assert_eq!(s.r.icon_bars.rects.get(&o).map(|r| r.1), Some([x, 340, x + 80, 400]), "{name} {o:#x}");
                }
                assert!(click_point(&s, icon).is_some(), "{name}: {icon:#x} not on screen");
            } else {
                assert!(click_point(&s, icon).is_none(), "{name}: ablation");
            }
        }
    }
    eprintln!("icon clicks checked: {checked:?}");
    assert!(checked.len() >= 3);
}
