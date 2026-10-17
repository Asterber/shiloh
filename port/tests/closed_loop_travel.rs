//! Closed loop on the bot traces fo-026..fo-029 (every hypothesis on, clicks by owner / position from the trace, settle
//! like the bot): transitions, posts, executed actions and the lifecycle order vs the original. Regression guards for
//! H124 (Deactivation = own dispatch: the background's Deactivation posts run before the View's), H125 (SwapDisc
//! travel) and H123 (one asset object per AssetRef).
mod common;
use std::path::PathBuf;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::replay::{lcs, Ev};
use xf::world::run::Runner;
use xf::world::{Msg, Rules};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub struct Cl {
    pub transitions: (usize, usize, usize),
    pub posts: (usize, usize, usize),
    pub execs: (usize, usize, usize),
    pub exec_equal_prefix: usize,
    pub failed: Vec<u32>,
    pub core_transitions: Vec<(u32, u32)>,
    pub core_execs: Vec<u32>,
    pub trace_execs: Vec<u32>,
    pub regs: Vec<u32>,
    pub lifecycle: Vec<(u32, u8)>,
}


/// Closed loop of `name` from `save`; None without game data.
pub fn closed_loop(save: &str, name: &str, rules: Rules) -> Option<Cl> {
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
    let mut failed = Vec::new();
    for st in xf::front::trace_route_with(&trace, true) {
        match st {
            xf::front::Step::User(o) => {
                if !xf::front::user_click(&mut s, o) {
                    failed.push(o);
                }
                xf::front::settle(&mut s, 100, 900_000);
            }
            xf::front::Step::WaitTimers => xf::front::settle_timers(&mut s, 100, 900_000),
            _ => unreachable!(),
        }
    }
    let tt: Vec<(u32, u32)> = trace.iter().filter_map(|e| if let Ev::Transition { view, kind } = e.ev { Some((view, kind)) } else { None }).collect();
    let tp: Vec<Msg> = trace.iter().filter_map(|e| if let Ev::Post { msg, .. } = e.ev { Some(msg) } else { None }).collect();
    let te: Vec<u32> = trace.iter().filter_map(|e| if let Ev::Exec { action, .. } = e.ev { Some(action) } else { None }).filter(|&a| a != 0).collect();
    let ce: Vec<u32> = s.r.execs.iter().copied().filter(|&a| a != 0).collect();
    let prefix = ce.iter().zip(&te).take_while(|(a, b)| a == b).count();
    Some(Cl {
        transitions: (lcs(&s.r.transitions, &tt).len(), tt.len(), s.r.transitions.len()),
        posts: (lcs(&s.r.posts, &tp).len(), tp.len(), s.r.posts.len()),
        execs: (lcs(&ce, &te).len(), te.len(), ce.len()),
        exec_equal_prefix: prefix,
        failed,
        core_transitions: s.r.transitions.clone(),
        core_execs: ce,
        trace_execs: te,
        regs: s.r.regs.clone(),
        lifecycle: s.r.lifecycle.clone(),
    })
}

/// `run` occurs in `v` as a contiguous slice.
pub fn contains_run(v: &[u32], run: &[u32]) -> bool {
    run.is_empty() || v.windows(run.len()).any(|w| w == run)
}

/// H124 (fo-029, post-hoc): at each exit of View 0x2a24 the background 0x84b4's Deactivation posts 0x145ea after the
/// View's Deactivation posted 0x8815, and 0x145ea runs first (157.82 / 162.43, 333.64 / 335.08, 406.05 / 407.51).
#[test]
fn fo029_closed_loop_exec_order_h121() {
    let Some(r) = closed_loop("fo-028-end", "fo-029", xf::front::rules_hyp()) else { return };
    eprintln!(
        "fo-029: transitions {:?} posts {:?} execs {:?} equal prefix {} failed {:x?}",
        r.transitions, r.posts, r.execs, r.exec_equal_prefix, r.failed
    );
    assert_eq!(r.transitions.0, r.transitions.1);
    assert_eq!(r.transitions.1, r.transitions.2);
    // the executed actions of the original between the Load screen and the save screen, in its order
    assert!(contains_run(&r.trace_execs, &r.core_execs), "core execs are not a contiguous run of the trace's");
    let off = closed_loop("fo-028-end", "fo-029", Rules { h124_deact_own_dispatch: false, ..xf::front::rules_hyp() }).unwrap();
    assert!(!contains_run(&off.trace_execs, &off.core_execs), "ablation: H124 off must reorder 0x8815 / 0x145ea");
}

/// Deactivation order of `owners` in a lifecycle list, per occurrence: the owners' Deacts in order.
fn deacts(l: &[(u32, u8)], owners: &[u32]) -> Vec<u32> {
    l.iter().filter(|&&(o, e)| e == 9 && owners.contains(&o)).map(|&(o, _)| o).collect()
}

/// H88 / H87 (fo-029 Y2, 3/3 exits of 0x2a24): the CharView 0x590c's Deactivation comes before the nav-hotspots' because
/// the background's Deactivation posts 0x145ea = type-3 **Disable** of VCEnabled 0x550 = CharView 0x590c (H87: FUN_0047ff40,
/// obj_remove at 157.91), which runs before the exit's queued removals (H88 order hotspots → CharViews kept).
/// Ablation H87 off: the H88 exit order (hotspots first) shows at the first exit.
#[test]
fn fo029_char_view_exit_order_h87() {
    let Some(r) = closed_loop("fo-028-end", "fo-029", xf::front::rules_hyp()) else { return };
    let d = deacts(&r.lifecycle, &[0x590c, 0x6bda, 0x6bdc]);
    eprintln!("fo-029 Deacts {d:x?}");
    assert_eq!(d, [0x590c, 0x6bda, 0x6bdc].repeat(3));
    let off = closed_loop("fo-028-end", "fo-029", Rules { h87_char_view_enable: false, ..xf::front::rules_hyp() }).unwrap();
    assert_eq!(deacts(&off.lifecycle, &[0x590c, 0x6bda, 0x6bdc])[..3], [0x6bda, 0x6bdc, 0x590c]);
}

/// H125 (C40, fo-027 / fo-028): the PDA map item 0x84f4's SwapDisc(0x6af, 0x2a21, 0x2a23, 0x2a24) — the closed loop
/// follows the travel: transitions = the trace's (Load's 0x2c9, 0x2d7, 0x2d1, 0x2bb, then 0x2a24 kind 0), the new Node /
/// Location / ViewPoint / View registered in the trace's order, the old ones deactivated before the new ones' Activation.
#[test]
fn fo027_fo028_closed_loop_swap_disc() {
    for name in ["fo-027", "fo-028"] {
        let Some(r) = closed_loop("fo-2c9-door", name, xf::front::rules_hyp()) else { return };
        eprintln!(
            "{name}: transitions {:x?} posts {:?} execs {:?} failed {:x?}",
            r.core_transitions, r.posts, r.execs, r.failed
        );
        assert_eq!(r.transitions, (5, 5, 5), "{name}");
        // every executed action of the core is the trace's, in order, except the Location 0x2a21 Timer (id 0x19, 14 s)
        // trigger 0xcc85's actions, which run at another point of the closed loop's clock (0x13bdd, RandGen 0xcc8b; 0xcc8c
        // reads the random value). Not reproduced (outside the core): the Roll Into of the hovered item (0xe701, 0x10220 —
        // the closed loop hovers another point), the menu / save screens
        let m = lcs(&r.core_execs, &r.trace_execs);
        let core_only: Vec<u32> =
            (0..r.core_execs.len()).filter(|i| !m.iter().any(|x| x.0 == *i)).map(|i| r.core_execs[i]).collect();
        assert!(core_only.iter().all(|a| [0x1627f, 0x13bdd, 0xcc8b, 0xcc8c].contains(a)), "{name}: {core_only:x?}");
        let i = r.regs.iter().position(|&o| o == 0x6af).expect("REG Node 0x6af");
        assert_eq!(r.regs[i..i + 4], [0x6af, 0x2a21, 0x2a23, 0x2a24], "{name}");
        let pos = |o: u32, e: u8| r.lifecycle.iter().position(|&x| x == (o, e)).unwrap();
        let order = [(0x2bb, 9), (0x2b2, 9), (0x12f, 9), (0x12d, 9), (0x76bf, 9), (0x6af, 8), (0x2a21, 8), (0x2a23, 8), (0x2a24, 8)];
        let p: Vec<usize> = order.iter().map(|&(o, e)| pos(o, e)).collect();
        assert!(p.windows(2).all(|w| w[0] < w[1]), "{name}: lifecycle order {p:?}");
        let off = closed_loop("fo-2c9-door", name, Rules { h125_swap_disc: false, ..xf::front::rules_hyp() }).unwrap();
        assert_eq!(off.transitions.2, 4, "{name}: ablation — no travel");
    }
}

#[test]
#[ignore]
fn dbg_execs() {
    let name = std::env::var("XF_DBG").unwrap_or("fo-027".into());
    let save = std::env::var("XF_DBG_SAVE").unwrap_or("fo-2c9-door".into());
    let r = closed_loop(&save, &name, xf::front::rules_hyp()).unwrap();
    let start = r.trace_execs.iter().position(|&a| Some(&a) == r.core_execs.first()).unwrap();
    let t = &r.trace_execs[start..];
    let n = r.core_execs.iter().zip(t).take_while(|(a, b)| a == b).count();
    eprintln!("start {start} equal {n} of core {} trace {}", r.core_execs.len(), r.trace_execs.len());
    eprintln!("core  {:x?}", &r.core_execs[n.saturating_sub(3)..(n + 15).min(r.core_execs.len())]);
    eprintln!("trace {:x?}", &t[n.saturating_sub(3)..(n + 15).min(t.len())]);
    let m = lcs(&r.core_execs, &r.trace_execs);
    let ci: std::collections::HashSet<usize> = m.iter().map(|x| x.0).collect();
    let ti: std::collections::HashSet<usize> = m.iter().map(|x| x.1).collect();
    let cu: Vec<String> = (0..r.core_execs.len()).filter(|i| !ci.contains(i)).map(|i| format!("{i}:{:x}", r.core_execs[i])).collect();
    let tu: Vec<String> = (0..r.trace_execs.len()).filter(|i| !ti.contains(i)).map(|i| format!("{i}:{:x}", r.trace_execs[i])).collect();
    eprintln!("core only {cu:?}\ntrace only {tu:?}");
}

/// H126 (fo-029): the Load of fo-028-end.x (VCGameState +0x1a4 = CharView 0x590c) rebuilds the dialogue box: REG 0x590d,
/// 0x590e, 0x5914, 0x591a, then each Conversation's asset 0x4503 / 0x4505 / 0x4507; leaving 0x2a24 (0x590c Deact) removes
/// them (Deact 0x590e, 0x4503, 0x5914, 0x4505, 0x591a, 0x4507 — fo-029 159.89–161.51).
#[test]
fn fo029_closed_loop_dialog_box_h126() {
    let Some(r) = closed_loop("fo-028-end", "fo-029", xf::front::rules_hyp()) else { return };
    let ids = [0x590d, 0x590e, 0x5914, 0x591a, 0x4503, 0x4505, 0x4507];
    let regs: Vec<u32> = r.regs.iter().copied().filter(|o| ids.contains(o)).collect();
    assert_eq!(regs[..7], ids);
    let d = deacts(&r.lifecycle, &[0x590c, 0x590e, 0x5914, 0x591a, 0x4503, 0x4505, 0x4507]);
    assert_eq!(d[..7], [0x590c, 0x590e, 0x4503, 0x5914, 0x4505, 0x591a, 0x4507]);
}

/// H121 + H122 (fo-030, post-hoc): the PDA map click (286,179) reaches IItem 0xe92e (H121, `tests/hit_test.rs`) and the
/// whole route runs in the core: transitions 0x6bf0, 0x2a2c, 0xfd1 (SwapDisc, H125), the original's posts and executed
/// actions; H122: the Disables 0xe6bd / 0xe6c4 / 0x1107d deactivate 0x855f / 0x84f6 / 0x84f2 at once, so they are
/// deactivated before the hide of their layout (0x84d8 hide by 0x8a4e's 0x1174c-chain).
#[test]
fn fo030_closed_loop_h121_h122() {
    let Some(r) = closed_loop("fo-029-end", "fo-030", xf::front::rules_hyp()) else { return };
    eprintln!("fo-030: transitions {:?} posts {:?} execs {:?} prefix {} failed {:x?}", r.transitions, r.posts, r.execs, r.exec_equal_prefix, r.failed);
    let off = closed_loop("fo-029-end", "fo-030", Rules { h121_iitem_key_order: false, h122_iitem_disable: false, h130_hit_chain: false, ..xf::front::rules_hyp() }).unwrap();
    eprintln!("off: transitions {:?} posts {:?} execs {:?} prefix {} failed {:x?}", off.transitions, off.posts, off.execs, off.exec_equal_prefix, off.failed);
    let d = deacts(&r.lifecycle, &[0x855f, 0x84f6, 0x84f2]);
    let d_off = deacts(&off.lifecycle, &[0x855f, 0x84f6, 0x84f2]);
    eprintln!("deacts {d:x?} off {d_off:x?}");
    assert_eq!(r.transitions, (3, 3, 3));
    // the load-screen clicks are not replayed (load_game), hence posts / execs < trace; 0 extra on both
    assert_eq!((r.posts.0, r.posts.2, r.execs.0, r.execs.2), (128, 128, 99, 99));
    assert_eq!((off.posts.0, off.execs.0), (126, 97));
    assert!(r.core_execs.contains(&0xe93f)); // 0xe92e's SwapDisc (without H121 only by the owner fallback: 0xe92e fully hidden)
    assert_eq!(d, vec![0x855f, 0x84f6, 0x84f2]); // trace order 210.06, 283.53, 294.86
    assert_eq!(d_off, vec![0x855f, 0x84f2, 0x84f6]); // at the hides, container order
}
