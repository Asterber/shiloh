//! Front-end acceptance (headless, no window): New Game + the user route of fo-009 through the closed-loop core
//! (`world::run::Runner`) and the front-end hit-testing (`front::Session`). Expected: the 9 transitions of
//! `traces/fo-009.md` (0x2b3 → 0x2b4 → 0x3fe → 0x3fd → 0x4be → 0x4cc → 0x14983 → 0x4cc → 0x4c6) with their kinds.
//!
//! Route: `front::route_fo009`. Inputs besides clicks (all from traces, see RE_NOTES §1b "Closed loop"):
//! - values of the 15 conditions the core cannot evaluate (system values, operand kind 1) — from fo-009;
//! - the dialogue icon VCEmotionIcon 0x4700 (TL 0x6fd9) the user clicked first: registered by the procedure
//!   PreloadEmIcon, not implemented in the core; it has no region, so it is clicked by owner, not by position;
//! - the drawer's InterfaceItems of View 0x4cc: registered by type-5 action 0xe4e5 (IFaceLayout 0xe4b9), a case the
//!   core does not implement — owners and TriggerLists as traced (fo-009 330.31–330.69), deactivated
//!   again when 0x4cc is left (type-5 hide 0xe4e9, also not implemented).
//!
//! With every hypothesis on (`front::rules_hyp`: H58 type 5, H59 timer, H60 PreloadEmIcon) the icon, the drawer items
//! and Timer Expiration come from the core (`fo009_route_core_only`, `traced_sessions_closed_loop_hyp`).
mod common;
use std::path::PathBuf;
use xf::front::{smoke, Session};
use xf::hdb::Hdb;
use xf::replay::{lcs, Ev};
use xf::world::run::Runner;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn fo009_route_closed_loop() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let Some(trace) = common::trace("fo-009.jsonl") else { return };
    let mut s = Session::new(&h, &g, &d);
    s.r.uneval = Runner::uneval_from_trace(&trace, &h);
    let route = xf::front::route_fo009();
    let res = smoke(&mut s, &route, None);
    assert!(res.failed.is_empty(), "{:?}", res.failed);
    let want: Vec<(u32, u32)> = trace
        .iter()
        .filter_map(|e| match e.ev {
            Ev::Transition { view, kind } => Some((view, kind)),
            _ => None,
        })
        .collect();
    assert_eq!(want.len(), 9);
    assert_eq!(res.transitions, want);
    // the whole order of posts and executed actions equals the original's (no trace timing used)
    let tp: Vec<_> = trace.iter().filter_map(|e| if let Ev::Post { msg, .. } = e.ev { Some(msg) } else { None }).collect();
    let te: Vec<_> = trace.iter().filter_map(|e| if let Ev::Exec { action, .. } = e.ev { Some(action) } else { None }).collect();
    eprintln!(
        "closed loop vs fo-009: posts {}/{} (core {}), execs {}/{} (core {})",
        lcs(&s.r.posts, &tp).len(), tp.len(), s.r.posts.len(), lcs(&s.r.execs, &te).len(), te.len(), s.r.execs.len()
    );
    assert_eq!(s.r.posts, tp);
    assert_eq!(s.r.execs, te);
}

/// Every traced session (fo-005..fo-009) replayed through the closed loop from its user inputs only (clicks, Timer
/// Expiration, registrations of objects the core cannot create, unevaluable conditions — see `front::trace_route`):
/// transitions, posts and executed actions are identical to the original's, in order.
#[test]
fn traced_sessions_closed_loop() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    for name in ["fo-005", "fo-006", "fo-007", "fo-008", "fo-009"] {
        let Some(trace) = common::trace(&format!("{name}.jsonl")) else { return };
        let mut s = Session::new(&h, &g, &d);
        s.r.uneval = Runner::uneval_from_trace(&trace, &h);
        let res = smoke(&mut s, &xf::front::trace_route(&trace), None);
        assert!(res.failed.is_empty(), "{name}: {:?}", res.failed);
        let tt: Vec<_> = trace.iter().filter_map(|e| if let Ev::Transition { view, kind } = e.ev { Some((view, kind)) } else { None }).collect();
        let tp: Vec<_> = trace.iter().filter_map(|e| if let Ev::Post { msg, .. } = e.ev { Some(msg) } else { None }).collect();
        let te: Vec<_> = trace.iter().filter_map(|e| if let Ev::Exec { action, .. } = e.ev { Some(action) } else { None }).collect();
        assert_eq!(res.transitions, tt, "{name}");
        assert_eq!(s.r.posts, tp, "{name}");
        assert_eq!(s.r.execs, te, "{name}");
    }
}

/// With every hypothesis on (H58 type 5 / IFaceLayout, H59 timer, H60 PreloadEmIcon): the fo-009 route needs no
/// registration from the trace — the dialogue icon 0x4700, the drawer's InterfaceItems and their assets are created by
/// the core; inputs are only the user's clicks and the unevaluable conditions. Same 9 transitions, same posts and
/// executed actions as the original, in order.
#[test]
fn fo009_route_core_only() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let Some(trace) = common::trace("fo-009.jsonl") else { return };
    let mut s = Session::with_rules(&h, &g, &d, xf::front::rules_hyp());
    s.r.uneval = Runner::uneval_from_trace(&trace, &h);
    let res = smoke(&mut s, &xf::front::route_fo009_core(), None);
    assert!(res.failed.is_empty(), "{:?}", res.failed);
    assert!(s.r.externals.is_empty(), "external registrations: {:x?}", s.r.externals);
    let tt: Vec<_> = trace.iter().filter_map(|e| if let Ev::Transition { view, kind } = e.ev { Some((view, kind)) } else { None }).collect();
    let tp: Vec<_> = trace.iter().filter_map(|e| if let Ev::Post { msg, .. } = e.ev { Some(msg) } else { None }).collect();
    let te: Vec<_> = trace.iter().filter_map(|e| if let Ev::Exec { action, .. } = e.ev { Some(action) } else { None }).collect();
    assert_eq!(res.transitions, tt);
    assert_eq!(tt.len(), 9);
    assert_eq!(s.r.posts, tp);
    assert_eq!(s.r.execs, te);
}

/// Every traced session through the closed loop with every hypothesis on: Timer Expiration (fo-007/fo-008) comes from
/// the core's timer (the route only says the user waited), and no registration is taken from the trace — the clicked
/// VCExplorables (fo-005..fo-008 0xac5d) are registered by the core (H55).
#[test]
fn traced_sessions_closed_loop_hyp() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    // H89: the same with the depth-first queue model instead of H76's hold
    for (name, h89) in ["fo-005", "fo-006", "fo-007", "fo-008", "fo-009"].into_iter().flat_map(|n| [(n, false), (n, true)]) {
        let Some(trace) = common::trace(&format!("{name}.jsonl")) else { return };
        let rules = xf::world::Rules { h89_queue_model: h89, ..xf::front::rules_hyp() };
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.uneval = Runner::uneval_from_trace(&trace, &h);
        let route = xf::front::trace_route_with(&trace, true);
        assert!(!route.iter().any(|st| matches!(st, xf::front::Step::Fire(..))), "{name}");
        let res = smoke(&mut s, &route, None);
        assert!(res.failed.is_empty(), "{name}: {:?}", res.failed);
        // nothing is registered from the trace (H55: Explorables come from the core)
        assert!(s.r.externals.is_empty(), "{name}: external registrations {:x?}", s.r.externals);
        let tt: Vec<_> = trace.iter().filter_map(|e| if let Ev::Transition { view, kind } = e.ev { Some((view, kind)) } else { None }).collect();
        let tp: Vec<_> = trace.iter().filter_map(|e| if let Ev::Post { msg, .. } = e.ev { Some(msg) } else { None }).collect();
        let te: Vec<_> = trace.iter().filter_map(|e| if let Ev::Exec { action, .. } = e.ev { Some(action) } else { None }).collect();
        assert_eq!(res.transitions, tt, "{name} h89={h89}");
        assert_eq!(s.r.posts, tp, "{name} h89={h89}");
        assert_eq!(s.r.execs, te, "{name} h89={h89}");
    }
}

/// Pictures of the drawer in View 0x4cc (H58 items + their first enabled AssetRef, from the core with every hypothesis
/// on): the front-end draws the 5 active x.pff pictures at their InterfaceItems' region top-left (H62).
#[test]
fn fo009_drawer_pictures() {
    use xf::front::{settle, user_click};
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let Some(trace) = common::trace("fo-009.jsonl") else { return };
    let mut s = Session::with_rules(&h, &g, &d, xf::front::rules_hyp());
    s.r.uneval = Runner::uneval_from_trace(&trace, &h);
    s.r.new_game();
    settle(&mut s, 100, 600_000);
    for o in [0x4700, 0x2b9, 0x552, 0x404, 0x521, 0x4cd] {
        assert!(user_click(&mut s, o), "{o:#x}");
        settle(&mut s, 100, 600_000);
    }
    assert_eq!(s.r.w.view, Some(0x4cc));
    let mut p = s.region_pictures();
    p.sort();
    assert_eq!(p, vec![(95, 201, 0xe4b2), (165, 230, 0xe4b1), (234, 221, 0xe4b0), (282, 214, 0xe4ae), (368, 227, 0xe4af)]);
    let fb = s.render();
    assert_ne!(fb[(230 * 640 + 300) * 3..][..3], [0, 0, 255]); // key colour not drawn
}

/// Sound cues: after New Game the intro 0xfba5 (XV/64421.xmv, QDMC sound — not decoded → silent), then FO.amv 0x76bf
/// (type 7 0x16767, whose condition reads trigger locals — H73; fo-009 REG 0xfba5 110.97, 0x76bf 131.31, 0x4d64 151.27),
/// then SC03C 0x4d64 (XV/19812.xmv, IMA4 stereo 22050 Hz): the front-end plays 0x4d64's track from its start, following
/// the media clock; the View background 0x6453 (no sound track) adds nothing. A3 (H280 off): 0x4d64 waits at the head of
/// the player until 0x76bf's first play ends, then 0x76bf loops (H100). H280: the sound 0x76bf does not hold the player —
/// 0x4d64 plays as soon as it is queued, alongside 0x76bf's first play.
#[test]
fn new_game_sound_cues() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    for h280 in [false, true] {
        let mut rules = xf::front::rules_hyp();
        rules.h280_sound_no_hold = h280;
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.new_game();
        s.tick(0);
        assert_eq!(s.r.playing(), Some(0xfba5));
        // the intro's QDMC sound sounds only with an xf-prep cache (user cache dir), so it may or may not be a cue
        assert!(s.audio_cues().iter().all(|c| c.asset == 0xfba5), "{:?}", s.audio_cues());
        while s.r.playing() == Some(0xfba5) {
            s.tick(100);
        }
        assert_eq!(s.r.started().first(), Some(&0x76bf));
        let t0 = s.now;
        while s.r.playing() != Some(0x4d64) && s.now - t0 < 60_000 {
            s.tick(100);
        }
        assert_eq!(s.r.playing(), Some(0x4d64));
        // FO.amv is 15.0 s long: A3 starts SC03C only after it, H280 within the queue's drain
        assert_eq!(s.now - t0 >= 15_000, !h280, "SC03C started {} ms after the intro", s.now - t0);
        s.tick(1000);
        let c = s.audio_cues();
        let cue = |a: u32| c.iter().find(|x| x.asset == a).unwrap_or_else(|| panic!("no cue {a:#x} in {c:?}"));
        assert_eq!(c.len(), 2, "{c:?}");
        let sc = cue(0x4d64);
        assert_eq!((sc.pcm.rate, sc.pcm.channels, sc.looped), (22050, 2, false));
        assert!((22050..=22050 + 2205).contains(&sc.pos), "{}", sc.pos);
        // H100: FO.amv 0x76bf (type-7 0x16767, mode 2) is a looping player — past its media end it sounds on, looped;
        // with H280 it is still in its first play (a started player, not looped yet)
        assert_eq!(cue(0x76bf).looped, !h280);
    }
}

/// H320: a View background is not a media player (0 of 507 traced background registrations get a player state switch),
/// so it makes no sound. New Game, the user waits in 0x2b3: the Timer's video 0x4d6c (XV/19820.xmv) plays, then the
/// View's background becomes 0x112fe — a 67 ms range of the same file (7800..7867), which has a sound track. Rule off
/// (the port before): the background's 67 ms of sound loops for as long as the View is shown; on: no cue.
#[test]
fn h320_background_no_sound() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    for h320 in [false, true] {
        let mut rules = xf::front::rules_hyp();
        rules.h320_background_silent = h320;
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.new_game();
        while s.r.background != Some(0x112fe) && s.now < 300_000 {
            s.tick(100);
        }
        assert_eq!(s.r.background, Some(0x112fe), "View {:x?}", s.r.w.view);
        assert_eq!(s.r.w.view, Some(0x2b3));
        s.tick(1000);
        let c = s.audio_cues();
        let bg: Vec<_> = c.iter().filter(|x| x.asset == 0x112fe).collect();
        let assets: Vec<u32> = c.iter().map(|x| x.asset).collect();
        if h320 {
            assert!(bg.is_empty(), "{assets:x?}");
        } else {
            assert_eq!(bg.len(), 1, "{assets:x?}");
            // 7800..7867 ms of a 22050 Hz track (< 100 ms), looped
            let (start, end) = (bg[0].start, bg[0].end);
            assert!(bg[0].looped && end > start && end - start < 2205, "{start}..{end}");
        }
    }
}

/// H520 (player report "the Comity TV has no sound"; the bot's screen-change marks in fo-059 / fo-065 / fo-067 /
/// fo-068 show the TV close-up 0xfcc3 still changing 16–18 s after entry, i.e. the 6 s background movie loops): a View
/// background that is a whole movie file (empty range, not a still) plays its sound track looped with its picture.
/// Closed loop from `data/bot/fo-067-end.x` along fo-068 k0..k8 into Scully's room 0x19ff → the TV 0xfcc6 → View 0xfcc3,
/// background 0xfcbd (XV/64701.xmv, 6 s cinepak + IMA4). It is not a media player (H320: no FIFO entry), so the right
/// click has no video to skip there (`video_playing` None): the user leaves the TV with a left click (0xfcc9).
/// Ranged backgrounds (0x112fe, `h320_background_no_sound`) stay silent.
#[test]
fn h520_tv_background_sound() {
    use xf::front::script;
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-067-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    for h520 in [false, true] {
        let mut rules = xf::front::rules_hyp();
        rules.h520_whole_movie_background_sound = h520;
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.load_game();
        xf::front::settle_timers(&mut s, 100, 900_000);
        let steps = "p:64,210;p:275,295;p:52,210;p:158,271;p:106,282;p:214,201;p:570,270;p:2,92;p:411,256";
        for i in script::parse(steps).unwrap() {
            script::apply(&mut s, &i);
            xf::front::settle_timers(&mut s, 100, 900_000);
        }
        assert_eq!(s.r.w.view, Some(0xfcc3));
        assert_eq!(s.r.background, Some(0xfcbd));
        assert_eq!(s.r.video_playing(), None, "the TV is a background, not a player");
        s.tick(7000);
        let c = s.audio_cues();
        let tv: Vec<_> = c.iter().filter(|x| x.asset == 0xfcbd).collect();
        if h520 {
            assert_eq!(tv.len(), 1, "{:x?}", c.iter().map(|x| x.asset).collect::<Vec<_>>());
            // the whole 6 s IMA4 track at 22050 Hz, looped (7 s after entry it is in its second pass)
            assert!(tv[0].looped && tv[0].start == 0 && tv[0].end > 22050 * 5, "{}..{}", tv[0].start, tv[0].end);
        } else {
            assert!(tv.is_empty());
        }
    }
}

/// H66 (SetStartView → FUN_00477940), closed loop on fo-010 with every hypothesis on: the core creates VCInventory
/// 0x442b / 0x442f (TL f40) and their activation builds 3 AssetRefs each (FUN_00470b40, as an EmotionIcon); the runner's
/// Activation order of these 8 owners is the trace's (fo-010 166.0–169.2: inventory, its assets, next inventory, …).
#[test]
fn fo010_h66_start_icons_order() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let Some(trace) = common::trace("fo-010.jsonl") else { return };
    let mut s = Session::with_rules(&h, &g, &d, xf::front::rules_hyp());
    s.r.uneval = Runner::uneval_from_trace(&trace, &h);
    let route = xf::front::trace_route_with(&trace, true);
    let res = smoke(&mut s, &route, None);
    assert!(res.failed.is_empty(), "{:?}", res.failed);
    assert!(s.r.externals.is_empty(), "external registrations {:x?}", s.r.externals);
    let want = [0x442b, 0x1be3, 0x1be4, 0xd553, 0x442f, 0x1be5, 0x1be7, 0xd555];
    let got: Vec<u32> = s.r.activations.iter().copied().filter(|o| want.contains(o)).collect();
    assert_eq!(got, want, "{got:x?}");
}

/// C33 (H69 application start, H71 game start) in the runner: New Game registers the 25 startup objects (NavBeep, the
/// cursor table, the null cursor, the cursor pictures) and then 0xdb79, VCGameState 0x14cb, VCTitle 0x12b, 0xdd7c — the
/// first 29 registrations of fo-010, in its order.
#[test]
fn new_game_start_registrations() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let Some(trace) = common::trace("fo-010.jsonl") else { return };
    let want: Vec<u32> = trace.iter().filter_map(|e| match e.ev { Ev::Reg { owner, .. } => Some(owner), _ => None }).take(29).collect();
    let mut r = Runner::new(&h, &g, xf::front::rules_hyp());
    r.new_game();
    r.run(10_000);
    assert_eq!(&r.regs[..29], &want[..], "{:x?}", &r.regs[..29]);
}

/// Trace's Activation / Deactivation fires (event 8 / 9) of owners `keep` accepts, in order (owner from the latest
/// registration of the fire's scope, unless an action message took the address since — as `replay`).
fn trace_lifecycle(trace: &[xf::replay::TraceEv], keep: impl Fn(u32) -> bool) -> Vec<(u32, u8)> {
    let mut owner_of = std::collections::HashMap::new();
    let mut out = Vec::new();
    for e in trace {
        match e.ev {
            Ev::Reg { scope, owner, .. } => {
                owner_of.insert(scope, owner);
            }
            // the address now holds an action message (as `replay`: scopes are reused)
            Ev::Post { msg_ptr, .. } => {
                owner_of.remove(&msg_ptr);
            }
            // C30: a gated Activation (restore window) has no fire_post; it is still the Activation
            Ev::Gate { scope, flag: true, .. } => {
                if let Some(&o) = owner_of.get(&scope) {
                    if keep(o) {
                        out.push((o, 8));
                    }
                }
            }
            Ev::Fire { scope, event, .. } if event == 8 || event == 9 => {
                if let Some(&o) = owner_of.get(&scope) {
                    if keep(o) {
                        out.push((o, event));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// H76 in the runner (closed loop, every hypothesis on): the order of View / ViewPoint / VCNav / VCExplorable
/// Activations and Deactivations equals the trace's (fo-005..fo-009); ablation `h76_entry_hotspot_order` off = A2
/// (fo-009 89/95). H89 (depth-first queue model, no hold): the same as H76.
#[test]
fn closed_loop_lifecycle_order_h76() {
    use xf::world::class;
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let keep = |o: u32| [class::VIEW, class::VIEWPOINT, class::NAV, class::EXPLORABLE].iter().any(|&c| h.has(c, o));
    for name in ["fo-005", "fo-006", "fo-007", "fo-008", "fo-009"] {
        let Some(trace) = common::trace(&format!("{name}.jsonl")) else { return };
        let tl = trace_lifecycle(&trace, keep);
        for (h76, h89) in [(true, false), (false, false), (false, true)] {
            let rules = xf::world::Rules { h76_entry_hotspot_order: h76, h89_queue_model: h89, ..xf::front::rules_hyp() };
            let h76 = h76 || h89; // H89 orders the same fires (its own way)
            let mut s = Session::with_rules(&h, &g, &d, rules);
            s.r.uneval = Runner::uneval_from_trace(&trace, &h);
            let res = smoke(&mut s, &xf::front::trace_route_with(&trace, true), None);
            assert!(res.failed.is_empty(), "{name}: {:?}", res.failed);
            let cl: Vec<(u32, u8)> = s.r.lifecycle.iter().copied().filter(|&(o, _)| keep(o)).collect();
            eprintln!("{name} h76={h76} h89={h89}: lifecycle {}/{} (core {}), equal {}", lcs(&cl, &tl).len(), tl.len(), cl.len(), cl == tl);
            let want = if h76 || name != "fo-009" { tl.len() } else { 89 }; // A2: 6 Deacts out of order (0x13baf, 0x4c4..)
            assert_eq!(lcs(&cl, &tl).len(), want, "{name} h76={h76}");
            assert_eq!(cl.len(), tl.len(), "{name} h76={h76}");
        }
    }
}

/// H76 in the runner on the loaded traces (saves fo-2c9-door.x / fo-2b3-user.x, H65 load): clicks by owner/position from the trace, settle like
/// the bot (`explore`); transitions and the View / ViewPoint / VCNav / VCExplorable lifecycle order vs the trace.
#[test]
fn closed_loop_lifecycle_order_h76_load() {
    use xf::world::class;
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let keep = |o: u32| [class::VIEW, class::VIEWPOINT, class::NAV, class::EXPLORABLE].iter().any(|&c| h.has(c, o));
    for (save, name) in [("fo-2c9-door", "fo-020"), ("fo-2c9-door", "fo-021"), ("fo-2c9-door", "fo-023"), ("fo-2b3-user", "fo-013"), ("fo-2b3-user", "fo-014"), ("fo-2b3-user", "fo-019")] {
        let save = root().join(format!("data/bot/{save}.x"));
        if !save.exists() {
            continue;
        }
        let g = Hdb::open(&save).unwrap();
        let Some(trace) = common::trace(&format!("{name}.jsonl")) else { return };
        let tl = trace_lifecycle(&trace, keep);
        let tt: Vec<_> = trace.iter().filter_map(|e| if let Ev::Transition { view, kind } = e.ev { Some((view, kind)) } else { None }).collect();
        // H89 (depth-first queue model, no hold): posts / executed actions / lifecycle as H76's
        let mut h76_run: Option<(Vec<xf::world::Msg>, Vec<u32>)> = None;
        for (h76, h89) in [(true, false), (false, false), (false, true)] {
            let rules = xf::world::Rules { h76_entry_hotspot_order: h76, h89_queue_model: h89, ..xf::front::rules_hyp() };
            let h76 = h76 || h89;
            let mut s = Session::with_rules(&h, &g, &d, rules);
            s.r.uneval = Runner::uneval_from_trace(&trace, &h);
            s.r.load_game();
            // timers run out only where the trace has a Timer fire (Step::WaitTimers, as in `smoke`): fo-013 / fo-019
            // click a dialogue icon before the save's restored timer 0 expires (no Timer in the trace); running it out
            // first fires 0x2b3's Timer trigger (H90: its `[ctx 0 == 0]` actions true) and disables the icons (H61)
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
            let cl: Vec<(u32, u8)> = s.r.lifecycle.iter().copied().filter(|&(o, _)| keep(o)).collect();
            let tp: Vec<_> = trace.iter().filter_map(|e| if let Ev::Post { msg, .. } = e.ev { Some(msg) } else { None }).collect();
            let te: Vec<_> = trace.iter().filter_map(|e| if let Ev::Exec { action, .. } = e.ev { Some(action) } else { None }).collect();
            eprintln!("{name} h89={h89}: posts {}/{} (core {}), execs {}/{} (core {})", lcs(&s.r.posts, &tp).len(), tp.len(),
                s.r.posts.len(), lcs(&s.r.execs, &te).len(), te.len(), s.r.execs.len());
            if !h89 && h76 {
                h76_run = Some((s.r.posts.clone(), s.r.execs.clone()));
            } else if h89 {
                let (p, e) = h76_run.clone().unwrap();
                assert_eq!((&s.r.posts, &s.r.execs), (&p, &e), "{name}: H89 posts / execs differ from H76's");
            }
            eprintln!("{name} h76={h76} h89={h89}: failed {failed:x?} transitions {}/{} lifecycle {}/{} (core {}), equal {}",
                lcs(&s.r.transitions, &tt).len(), tt.len(), lcs(&cl, &tl).len(), tl.len(), cl.len(), cl == tl);
            // the menu's Load screen items, clicked before the save was loaded (the closed loop starts at the load)
            assert_eq!(failed, [0x142d6, 0x142da], "{name}");
            assert_eq!(s.r.transitions, tt, "{name} h76={h76}");
            // A2 (off): fo-021 exits of View 0x3ef deactivate 0x3f4 / 0x3f9 before the Disable-deactivated 0xd242 / 0xd244
            // (fo-023: 397/403, held out — fo-023 was traced after H76 was fitted)
            let want = match (h76, name) {
                (false, "fo-021") => 142,
                (false, "fo-023") => 397,
                _ => tl.len(),
            };
            assert_eq!(lcs(&cl, &tl).len(), want, "{name} h76={h76}");
            assert_eq!(cl.len(), tl.len(), "{name} h76={h76}");
        }
    }
}

/// H100 in the runner (closed loop on fo-023, save fo-2c9-door.x, every hypothesis on): 0xcc41 is played by the periodic
/// Timer's type-0x87 action 0x1238a in mode 2 (looping player). At its media end the player stays (FUN_0048b8a0 removes
/// only non-looping players): no Deactivation, and the later re-runs of 0x1238a create nothing — the 0xcc41
/// lifecycle equals the trace's (one Activation, no Deactivation). Ablation: the runner without the loop rule
/// (`h100_player_mode` off) deactivates it at media end and re-plays it.
#[test]
fn closed_loop_h100_looping_player_stays() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let Some(trace) = common::trace("fo-023.jsonl") else { return };
    let keep = |o: u32| o == 0xcc41;
    let tl = trace_lifecycle(&trace, keep);
    assert_eq!(tl, [(0xcc41, 8)], "trace");
    for h100 in [true, false] {
        let rules = xf::world::Rules { h100_player_mode: h100, ..xf::front::rules_hyp() };
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.uneval = Runner::uneval_from_trace(&trace, &h);
        s.r.load_game();
        xf::front::settle(&mut s, 100, 900_000);
        let mut looped = false;
        for st in xf::front::trace_route_with(&trace, true) {
            match st {
                xf::front::Step::User(o) => {
                    xf::front::user_click(&mut s, o);
                    xf::front::settle(&mut s, 100, 900_000);
                }
                xf::front::Step::WaitTimers => xf::front::settle_timers(&mut s, 100, 900_000),
                _ => unreachable!(),
            }
            looped |= s.r.looping().contains(&0xcc41);
        }
        let cl: Vec<(u32, u8)> = s.r.lifecycle.iter().copied().filter(|&(o, _)| keep(o)).collect();
        eprintln!("h100={h100}: 0xcc41 lifecycle {cl:x?}, looping seen {looped}");
        if h100 {
            assert_eq!(cl, tl, "h100 on");
            assert!(looped, "0xcc41 never reached its media end");
        } else {
            assert_ne!(cl, tl, "h100 off");
            assert!(!looped);
        }
    }
    // New Game (fo-005..fo-009): FO.amv 0x76bf (type-7 0x16767, mode 2) is activated and never deactivated in the trace
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    for name in ["fo-005", "fo-006", "fo-007", "fo-008", "fo-009"] {
        let Some(trace) = common::trace(&format!("{name}.jsonl")) else { return };
        let keep = |o: u32| o == 0x76bf;
        let tl = trace_lifecycle(&trace, keep);
        for h100 in [true, false] {
            let rules = xf::world::Rules { h100_player_mode: h100, ..xf::front::rules_hyp() };
            let mut s = Session::with_rules(&h, &g, &d, rules);
            s.r.uneval = Runner::uneval_from_trace(&trace, &h);
            let res = smoke(&mut s, &xf::front::trace_route_with(&trace, true), None);
            assert!(res.failed.is_empty(), "{name}: {:?}", res.failed);
            let cl: Vec<(u32, u8)> = s.r.lifecycle.iter().copied().filter(|&(o, _)| keep(o)).collect();
            eprintln!("{name} h100={h100}: 0x76bf lifecycle {cl:x?}, trace {tl:x?}");
            assert_eq!(cl == tl, h100, "{name} h100={h100}");
        }
    }
}

/// H100 mode 3 in the runner (closed loop on fo-025, save fo-2c9-door.x, every hypothesis on; post-hoc): 0xcc41's looping
/// player (type-0x87 0x1238a, mode 2) is stopped by type-1 0xd281 (list 0x14acc = [0xcc41], mode 3) — it ends like a media
/// end: removed, Deactivation queued behind the trigger's actions (trace: one Act 214.49, one Deact 885.72). Ablation
/// (`h100_player_mode` off): mode 3 plays the list → the lifecycle differs.
#[test]
fn closed_loop_h100_mode3_stop() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-2c9-door.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let Some(trace) = common::trace("fo-025.jsonl") else { return };
    let keep = |o: u32| o == 0xcc41;
    let tl = trace_lifecycle(&trace, keep);
    assert_eq!(tl, [(0xcc41, 8), (0xcc41, 9)], "trace");
    for h100 in [true, false] {
        let rules = xf::world::Rules { h100_player_mode: h100, ..xf::front::rules_hyp() };
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.uneval = Runner::uneval_from_trace(&trace, &h);
        s.r.load_game();
        xf::front::settle(&mut s, 100, 900_000);
        for st in xf::front::trace_route_with(&trace, true) {
            match st {
                xf::front::Step::User(o) => {
                    xf::front::user_click(&mut s, o);
                    xf::front::settle(&mut s, 100, 900_000);
                }
                xf::front::Step::WaitTimers => xf::front::settle_timers(&mut s, 100, 900_000),
                _ => unreachable!(),
            }
        }
        let cl: Vec<(u32, u8)> = s.r.lifecycle.iter().copied().filter(|&(o, _)| keep(o)).collect();
        eprintln!("h100={h100}: 0xcc41 lifecycle {cl:x?}");
        assert_eq!(cl == tl, h100, "h100={h100}");
        if h100 {
            assert!(!s.r.looping().contains(&0xcc41));
        }
    }
}
