//! Hit test vs the original: closed loop on the bot traces (every hypothesis on, loaded saves); before each traced user
//! click the core's topmost clickable at the traced button-down point (wnd_mouse WM_LBUTTONDOWN lParam) must be the
//! owner the original fired Mouse Click on. Regression guard for H121 (InterfaceItem hit order = layout from the most
//! recently shown, smallest key f4c & 0xff, ties in container order — FUN_00439330) and H130 (the hit chain:
//! InterfaceItems, then in the View band Explorables, then nav-hotspots, earliest activated first); the hover side (H134)
//! checks the traced Roll Into / Roll Out Of at the move points.
mod common;
use std::path::PathBuf;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::replay::{parse_trace, Ev};
use xf::world::run::Runner;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Button-down points of a trace: (t, x, y) from wnd_mouse with msg 0x201 (eax), lParam in esi.
fn presses(text: &str) -> Vec<(f64, i32, i32)> {
    mouse_points(text, 0x201)
}

/// (t, x, y) of the wnd_mouse records with message `msg` (eax; 0x200 move, 0x201 button down), lParam in esi.
fn mouse_points(text: &str, msg: u64) -> Vec<(f64, i32, i32)> {
    let mut v = Vec::new();
    for l in text.lines() {
        if !l.contains("\"wnd_mouse\"") {
            continue;
        }
        let Ok(j) = serde_json::from_str::<serde_json::Value>(l) else { continue };
        let reg = |r: &str| j["regs"][r].as_str().and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok());
        if reg("eax") != Some(msg) {
            continue;
        }
        let Some(lp) = reg("esi") else { continue };
        v.push((j["t"].as_f64().unwrap_or(0.0), (lp & 0xffff) as i16 as i32, (lp >> 16) as i16 as i32));
    }
    v
}

/// (clicked owner, press point) per traced user click; the press is the last button-down within 2 s before the fire.
fn clicks(name: &str) -> Vec<(u32, Option<(i32, i32)>)> {
    let text = common::trace_text(&format!("{name}.jsonl")).expect("the caller has the trace");
    let p = presses(&text);
    let trace = parse_trace(&text).unwrap();
    let mut owner_of = std::collections::HashMap::new();
    let mut v = Vec::new();
    for e in &trace {
        match e.ev {
            Ev::Reg { scope, owner, .. } => {
                owner_of.insert(scope, owner);
            }
            Ev::Fire { scope, event: 0, .. } => {
                if let Some(&o) = owner_of.get(&scope) {
                    let pt = p.iter().rev().find(|q| q.0 < e.t && e.t - q.0 < 2.0).map(|q| (q.1, q.2));
                    v.push((o, pt));
                }
            }
            _ => {}
        }
    }
    v
}

/// Hover events of a trace: (t, owner, event 4 Roll Into / 5 Roll Out Of, last move point within 2 s before),
/// grouped by the user click they precede (index into `clicks`; hovers after the last click are dropped).
fn hovers(name: &str) -> Vec<Vec<(f64, u32, u8, (i32, i32))>> {
    let text = common::trace_text(&format!("{name}.jsonl")).expect("the caller has the trace");
    let mv = mouse_points(&text, 0x200);
    let trace = parse_trace(&text).unwrap();
    let mut owner_of = std::collections::HashMap::new();
    let (mut groups, mut cur) = (Vec::new(), Vec::new());
    for e in &trace {
        match e.ev {
            Ev::Reg { scope, owner, .. } => {
                owner_of.insert(scope, owner);
            }
            Ev::Fire { scope, event, .. } if owner_of.contains_key(&scope) => {
                let o = owner_of[&scope];
                if event == 0 {
                    groups.push(std::mem::take(&mut cur));
                } else if event == 4 || event == 5 {
                    if let Some(q) = mv.iter().rev().find(|q| q.0 < e.t && e.t - q.0 < 2.0) {
                        cur.push((e.t, o, event, (q.1, q.2)));
                    }
                }
            }
            _ => {}
        }
    }
    groups
}

/// Closed loop of `name` from `save`, hover side: before each traced user click, every traced Roll Into of an owner the
/// core offers must be the core's topmost clickable at the move point, every Roll Out Of must not be.
/// (checked, mismatches (owner, event, x, y, core's hit)); None without game data.
fn check_hover(save: &str, name: &str, rules: Rules) -> Option<(usize, Vec<(u32, u8, i32, i32, Option<u32>)>)> {
    let d = root().join("data/InstalledGame");
    let save = root().join(format!("data/bot/{save}.x"));
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let trace = common::trace(&format!("{name}.jsonl"))?;
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.uneval = Runner::uneval_from_trace(&trace, &h);
    s.r.load_game();
    xf::front::settle(&mut s, 100, 900_000);
    let mut groups = hovers(name).into_iter();
    let (mut n, mut bad) = (0, Vec::new());
    for st in xf::front::trace_route_with(&trace, true) {
        match st {
            xf::front::Step::User(o) => {
                for (_, ho, ev, (x, y)) in groups.next().unwrap_or_default() {
                    if !s.r.clickables().iter().any(|c| c.owner == ho) {
                        continue;
                    }
                    n += 1;
                    let got = s.r.hit(x, y).map(|c| c.owner);
                    if (ev == 4) != (got == Some(ho)) {
                        bad.push((ho, ev, x, y, got));
                    }
                }
                xf::front::user_click(&mut s, o);
                xf::front::settle(&mut s, 100, 900_000);
            }
            xf::front::Step::WaitTimers => xf::front::settle_timers(&mut s, 100, 900_000),
            _ => unreachable!(),
        }
    }
    Some((n, bad))
}

/// Closed loop of `name` from `save`: (clicks checked, mismatches (owner, x, y, core's hit)); None without game data.
fn check(save: &str, name: &str, rules: Rules) -> Option<(usize, Vec<(u32, i32, i32, Option<u32>)>)> {
    let d = root().join("data/InstalledGame");
    let save = root().join(format!("data/bot/{save}.x"));
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let trace = common::trace(&format!("{name}.jsonl"))?;
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.uneval = Runner::uneval_from_trace(&trace, &h);
    s.r.load_game();
    xf::front::settle(&mut s, 100, 900_000);
    let mut pts = clicks(name).into_iter();
    let (mut n, mut bad) = (0, Vec::new());
    for st in xf::front::trace_route_with(&trace, true) {
        match st {
            xf::front::Step::User(o) => {
                let offered = s.r.clickables().iter().any(|c| c.owner == o); // not: the menu's load screen (load_game)
                if let Some((_, Some((x, y)))) = pts.find(|c| c.0 == o).filter(|_| offered) {
                    n += 1;
                    let got = s.r.hit(x, y).map(|c| c.owner);
                    if got != Some(o) {
                        bad.push((o, x, y, got));
                    }
                }
                xf::front::user_click(&mut s, o);
                xf::front::settle(&mut s, 100, 900_000);
            }
            xf::front::Step::WaitTimers => xf::front::settle_timers(&mut s, 100, 900_000),
            _ => unreachable!(),
        }
    }
    Some((n, bad))
}

const RUNS: [(&str, &str); 8] = [
    ("fo-2c9-door", "fo-022"),
    ("fo-2c9-door", "fo-023"),
    ("fo-2c9-door", "fo-025"),
    ("fo-2c9-door", "fo-026"),
    ("fo-2c9-door", "fo-027"),
    ("fo-2c9-door", "fo-028"),
    ("fo-028-end", "fo-029"),
    ("fo-029-end", "fo-030"),
];

/// H121: every traced click point hits the owner the original clicked (fo-030 321.0: (286,179) → IItem 0xe92e, key 0x64,
/// over 0x8500, key 0x66, registered earlier; fo-027 / fo-028 (350,267) → 0xe6fd over 0x84f4). Ablation (H77's "earliest
/// registered on top"): those clicks go to 0x8500 / 0x84f4.
#[test]
fn h121_traced_click_points() {
    for (save, name) in RUNS {
        let Some((n, bad)) = check(save, name, Rules::all()) else { return };
        eprintln!("{name}: {n} clicks checked, mismatches {bad:x?}");
        assert!(n > 0, "{name}");
        assert!(bad.is_empty(), "{name}: {bad:x?}");
        let off = check(save, name, Rules { h121_iitem_key_order: false, h130_hit_chain: false, ..Rules::all() }).unwrap();
        eprintln!("{name} without H121: mismatches {:x?}", off.1);
    }
    let h77 = Rules { h121_iitem_key_order: false, h130_hit_chain: false, ..Rules::all() };
    let (_, bad) = check("fo-029-end", "fo-030", h77).unwrap();
    assert_eq!(bad, vec![(0xe92e, 286, 179, Some(0x8500))]);
    let (_, bad) = check("fo-2c9-door", "fo-027", h77).unwrap();
    assert_eq!(bad, vec![(0xe6fd, 350, 267, Some(0x84f4))]);
}

/// H130 hover side (the mouse-move dispatch, q_dispatch cases 5 / 8: the same hit chain (FUN_00519e40), then Roll Out Of
/// the previous object and Roll Into the new one): every traced Roll Into / Roll Out Of at the last mouse
/// move point, for owners the core offers.
#[test]
fn h130_traced_hover_points() {
    for (save, name) in RUNS {
        let Some((n, bad)) = check_hover(save, name, Rules::all()) else { return };
        eprintln!("{name}: {n} hover events checked, mismatches {bad:x?}");
        assert!(n > 0, "{name}");
        assert!(bad.is_empty(), "{name}: {bad:x?}");
        let off = check_hover(save, name, Rules { h130_hit_chain: false, ..Rules::all() }).unwrap();
        eprintln!("{name} without H130: mismatches {:x?}", off.1);
    }
}

/// H150 (static: the icon-bar layout, FUN_004373e0, and the bar configs; post-hoc): every traced click on a dialogue icon
/// (EmotionIcon bar 3: start (180,340), fixed, right, spacing 20; icons 80×60 → (180..260), (280..360), (380..460) ×
/// 340..400) hits the icon the original fired Mouse Click on: fo-012 / fo-013 / fo-017 (220,370) → 0x46fc, fo-019
/// (420,370) → 0x6fde. Without H150 the icons are not offered (0 checked).
#[test]
fn h150_traced_icon_clicks() {
    for name in ["fo-012", "fo-013", "fo-017", "fo-019"] {
        let Some((n, bad)) = check("fo-2b3-user", name, Rules::all()) else { return };
        eprintln!("{name}: {n} clicks checked, mismatches {bad:x?}");
        assert_eq!(n, 1, "{name}");
        assert!(bad.is_empty(), "{name}: {bad:x?}");
        let off = check("fo-2b3-user", name, Rules { h150_icon_bars: false, ..Rules::all() }).unwrap();
        assert_eq!(off.0, 0, "{name} without H150");
    }
}
