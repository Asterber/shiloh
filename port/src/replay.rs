//! Trace replay: drive the [`World`] with the inputs recorded in a trace of the original
//! (`fo-*.jsonl`, private research inputs) and compare what it decides with what the original did.
//!
//! Lock-step design (what is INPUT and what is PREDICTED):
//! - inputs: every `trig_register` (owner only — the TL is computed by the world, C20, and compared;
//!   if the world has no rule for the owner's class the trace TL is used and counted), every
//!   `fire_post` on a registered owner (event + owner; action wrappers are skipped), nav clicks
//!   (`nav_exec` + the clicked VCNav), and the *moments* the original dispatched an action message
//!   (`action_cond`): the scheduler timing (H47) is not modelled, so the trace says *when* the next
//!   action runs, the world says *which* one (C24).
//! - predicted: posts (action, trigger) (C19, C25), dispatch order (C24), condition values (C21/C22),
//!   executed actions (id, type), transitions (C3, C6, C18, C25), nav-hotspots (C15, C16),
//!   registrations the world itself causes (C20/C23/H48/H50) and lifecycle events (C23).
//! - if a condition is not evaluable (operand kind 1/4) the trace value is used (counted).
//! - if the trace dispatches an action that is not at the head of the world's queue, the world follows
//!   the trace (counted as a dispatch mismatch).

use crate::hdb::Hdb;
use crate::world::{class, Msg, Out, PostSrc, Rules, World};
use serde_json::Value as J;
use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub enum Ev {
    Reg { scope: u64, owner: u32, tl: u32 },
    Fire { scope: u64, event: u8, ret: u32 },
    Post { msg_ptr: u64, msg: Msg },
    Cond { action: u32, value: bool },
    Exec { action: u32, typ: u8 },
    Transition { view: u32, kind: u32 },
    NavClick { vcnav: u32 },
    Hotspot { vcnav: u32 },
    /// C30 restore gate of an Activation (FUN_00440f80; group `load`): flag, mode (0 when the flag is 0); `mode` None =
    /// inferred in a trace without the `load` group (H64 / C30: an Activation command dispatched on the object fires
    /// event 8 iff the gate passes; no fire_post on that object before the next dispatch = gated)
    Gate { scope: u64, flag: bool, mode: Option<u32> },
    /// H67: the Activation command of a media player is dispatched — schedule input, like the action dispatches
    PlayerRun,
    /// H68: a state command on an EmotionIcon is dispatched; scope = the icon object
    StateRun { scope: u64 },
    /// H70: the main menu's "Load" posted its type-5 message: show layout 0x142ca; H140: the Esc menu's "Save"
    /// (FUN_00515b50) posted it: show layout 0x14269. `from_menu` = the screen was opened from the menu (load screen:
    /// always; save screen: iff the menu object was removed just before, i.e. the Esc menu's mode was not 0 / 3 / 4)
    MenuLayout { layout: u32, from_menu: bool },
    /// H71: entry of the game-start function FUN_0048cd70 (its block reset)
    GameStart,
    /// H74: the Activation command of a CharView media wrapper is dispatched; scope = the wrapper
    CharViewRun { scope: u64 },
    /// H103: a View's state command is dispatched — the entry's transition effect (FUN_00464330) pauses and resumes the
    /// timers there
    ViewStateRun,
    /// H126: the Activation command of a dialogue box (VCConversationList object) is dispatched
    DboxRun,
    /// H160: a state command of the dialogue box is dispatched (its value is not traced)
    DboxState,
    /// H651: a Conversation's cmd 0xd (FUN_0046b4c0) is dispatched — the end of a played line's sequence
    /// (`World::dbox_seq_end`), schedule input
    ConvEnd,
    /// H144: a trig_register repeating the previous one exactly (scope, owner, TL, return address) with no traced event in
    /// between — a tracer double hit, not a second registration (dropped, counted)
    TracerDouble { owner: u32 },
    /// H145: a media end — FUN_0048b8a0 removed a player and stopped it (the block-state record on that player; f28 =
    /// its AssetRef)
    MediaEnd { asset: u32 },
    /// H896: a key event the window procedure hands on (key_in from WM_KEYDOWN with the VK in args[1], from WM_CHAR with
    /// the character in args[2]) — the code the next event-3 fire's KeyPress reads
    Key { code: u32 },
    /// H1201: FUN_00429f00 entered (held_drop) — the held object's removal, owned by the runner's selection (e.g. a
    /// Disable of the selected VCInventory, fo-168 215.641); input here, as the replay has no runner
    HeldDrop,
}

#[derive(Clone, Debug)]
pub struct TraceEv {
    pub t: f64,
    pub ev: Ev,
}

fn hexs(v: &J) -> Option<u64> {
    let s = v.as_str()?;
    u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// Parse a JSONL trace into the events the replay uses.
pub fn parse_trace(text: &str) -> Result<Vec<TraceEv>, String> {
    let mut out = Vec::new();
    let mut want_nav = false;
    let mut save_menu_removed = false; // H140: the Esc menu object removed (its mode ∉ {0, 3, 4})
    let mut media_end_player: Option<u64> = None; // H145: player removed at its media end
    // H145: traces without the obj_remove group (fo-008 / fo-009) mark a media end by the player stop alone
    let no_obj_remove = !text.contains("\"fn\": \"obj_remove\"");
    let mut last_reg: Option<(u64, u32, u64, u64)> = None; // H144: (scope, owner, TL, ret) of the last record if a REG
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let r: J = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        let Some(fn_) = r.get("fn").and_then(|v| v.as_str()) else { continue };
        let t = r.get("t").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let g = |k: &str| hexs(&r[k]).unwrap_or(0);
        let id = |k: &str| hexs(&r[k]["id"]).unwrap_or(0) as u32;
        // H144: q_dispatch / wnd_mouse / mark records do not separate a double hit
        let reg_key = (fn_ == "trig_register").then(|| (g("scope"), id("owner"), g("triggerList"), g("ret")));
        if !matches!(fn_, "q_dispatch" | "wnd_mouse" | "mark") {
            let prev = std::mem::replace(&mut last_reg, reg_key);
            if reg_key.is_some() && prev == reg_key {
                out.push(TraceEv { t, ev: Ev::TracerDouble { owner: id("owner") } });
                continue;
            }
        }
        let ev = match fn_ {
            "trig_register" => Ev::Reg { scope: g("scope"), owner: id("owner"), tl: g("triggerList") as u32 },
            "fire_post" => Ev::Fire { scope: g("scope"), event: g("event") as u8, ret: g("ret") as u32 },
            "act_post" => {
                Ev::Post { msg_ptr: g("msg"), msg: Msg { action: g("action") as u32, trigger: g("trigger") as u32 } }
            }
            "action_cond" => Ev::Cond { action: g("action") as u32, value: g("cond_eax") != 0 },
            "action_exec" => Ev::Exec { action: g("action") as u32, typ: g("type") as u8 },
            "transition_new" => {
                let kind = r["args"].get(1).and_then(hexs).unwrap_or(0) as u32;
                Ev::Transition { view: id("arg1_obj"), kind }
            }
            "nav_exec" => {
                want_nav = true;
                continue;
            }
            // the VCNav whose dest the clicked nav-hotspot reads (C2: the read inside nav_exec)
            "nav_dest_view" if want_nav && g("ret") == 0x0046_af14 => {
                want_nav = false;
                Ev::NavClick { vcnav: id("vcnav") }
            }
            "nav_hotspot_new" => Ev::Hotspot { vcnav: id("vcnav") },
            "held_drop" => Ev::HeldDrop,
            "key_in" => {
                let a = |i: usize| r["args"].get(i).and_then(hexs).unwrap_or(0) as u32;
                Ev::Key { code: if a(2) != 0 { a(2) } else { a(1) } }
            }
            // C30: flag 0 → the gate passes (a fire_post follows); flag ≠ 0 → wait for the mode (load_mode)
            "load_flag" if g("val") == 0 => Ev::Gate { scope: hexs(&r["obj"]["ptr"]).unwrap_or(0), flag: false, mode: Some(0) },
            "q_dispatch" if g("msg_vt") == 0x0065_4830 && g("tgt_vt") == 0x0065_4020 => Ev::PlayerRun,
            "q_dispatch" if g("msg_vt") == 0x0065_5e58 && g("tgt_vt") == 0x0065_45e0 => Ev::StateRun { scope: g("msg_w1") },
            // C296: an ActionIcon's state command (its click's state 0 runs `World::icon_off`; others are ignored there)
            "q_dispatch" if g("msg_vt") == 0x0065_5e58 && g("tgt_vt") == 0x0065_5e68 => Ev::StateRun { scope: g("msg_w1") },
            // H144 (c): fo-006's tracer did not record the target vt — a state command on any object is offered to the world
            // (`World::icon_off` acts only on an icon whose H68 state-0 command is pending)
            "q_dispatch" if g("msg_vt") == 0x0065_5e58 && r.get("tgt_vt").is_none() => Ev::StateRun { scope: g("msg_w1") },
            "q_dispatch" if g("msg_vt") == 0x0065_4830 && g("tgt_vt") == 0x0065_4178 => Ev::CharViewRun { scope: g("msg_w1") },
            "q_dispatch" if g("msg_vt") == 0x0065_5e58 && g("tgt_vt") == 0x0065_b988 => Ev::ViewStateRun,
            "q_dispatch" if g("msg_vt") == 0x0065_4830 && g("tgt_vt") == 0x0065_5d98 => Ev::DboxRun,
            "q_dispatch" if g("msg_vt") == 0x0065_5e58 && g("tgt_vt") == 0x0065_5d98 => Ev::DboxState,
            "q_dispatch" if g("msg_vt") == 0x0065_4168 && g("tgt_vt") == 0x0065_86d0 => Ev::ConvEnd,
            "blk_reset" if g("ret") == 0x0048_d169 => Ev::GameStart,
            "q_post" if g("ret") == 0x0051_5d7a => Ev::MenuLayout { layout: 0x142ca, from_menu: true },
            "obj_remove" if g("ret") == 0x0048_b8ba => {
                media_end_player = Some(g("target_ptr"));
                continue;
            }
            "blk_state"
                if g("ret") == 0x0043_a999
                    && (no_obj_remove || (media_end_player.is_some() && hexs(&r["this"]["ptr"]) == media_end_player)) =>
            {
                media_end_player = None;
                Ev::MediaEnd { asset: id("f28") }
            }
            "obj_remove" if g("ret") == 0x0051_5bce => {
                save_menu_removed = true;
                continue;
            }
            "q_post" if g("ret") == 0x0051_5cbf => {
                Ev::MenuLayout { layout: 0x14269, from_menu: std::mem::take(&mut save_menu_removed) }
            }
            "load_mode" => Ev::Gate { scope: hexs(&r["obj"]["ptr"]).unwrap_or(0), flag: true, mode: Some(g("val") as u32) },
            _ => continue,
        };
        out.push(TraceEv { t, ev });
    }
    if !text.contains("\"fn\": \"load_flag\"") {
        out = infer_gates(text, out)?;
    }
    Ok(out)
}

/// C30 without the `load` group (fo-012): every Activation command dispatched on an object (q_dispatch, msg_w1 = the
/// object; action messages, players, CharView wrappers and dialogue boxes excluded by their target vt) becomes
/// a gate hit — passed iff a fire_post event 8 on that object follows before the next q_dispatch (C30 (3): 132/132 in
/// fo-013), its restore mode unknown. Inserted where the trace's `load_flag` would be (at the dispatch).
fn infer_gates(text: &str, evs: Vec<TraceEv>) -> Result<Vec<TraceEv>, String> {
    let mut recs: Vec<(f64, String, u64, u64, u64)> = Vec::new(); // (t, fn, msg_w1 / scope, event, tgt_vt)
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let r: J = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let fn_ = r.get("fn").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let t = r.get("t").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let g = |k: &str| hexs(&r[k]).unwrap_or(0);
        match fn_.as_str() {
            "q_dispatch" if g("msg_vt") == 0x0065_4830 => recs.push((t, fn_, g("msg_w1"), 0, g("tgt_vt"))),
            "q_dispatch" => recs.push((t, fn_, 0, 0, 0)),
            "fire_post" => recs.push((t, fn_, g("scope"), g("event"), 0)),
            _ => {}
        }
    }
    let mut gates: Vec<TraceEv> = Vec::new();
    let mut active: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for (i, (t, fn_, scope, ev, tgt)) in recs.iter().enumerate() {
        if fn_ == "fire_post" {
            match ev {
                8 => active.insert(*scope),
                9 => active.remove(scope),
                _ => false,
            };
            continue;
        }
        // action message, media player, CharView wrapper, dialogue box: their command 0 is not the Activation gate; an object
        // already active only gets its command re-posted (H69 / H123: no Activation)
        if *scope == 0 || [0x0065_3438, 0x0065_4020, 0x0065_4178, 0x0065_5d98].contains(tgt) || active.contains(scope) {
            continue;
        }
        let fired = recs[i + 1..]
            .iter()
            .take_while(|x| x.1 != "q_dispatch")
            .any(|x| x.1 == "fire_post" && x.2 == *scope && x.3 == 8);
        gates.push(TraceEv { t: *t, ev: Ev::Gate { scope: *scope, flag: !fired, mode: None } });
    }
    // merge by time (stable: a gate goes before the trace events of the same instant)
    let mut out = Vec::with_capacity(evs.len() + gates.len());
    let mut it = gates.into_iter().peekable();
    for e in evs {
        while it.peek().is_some_and(|g| g.t <= e.t) {
            out.push(it.next().unwrap());
        }
        out.push(e);
    }
    out.extend(it);
    Ok(out)
}

/// Longest common subsequence: matched index pairs (a_i, b_j).
pub fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
    let w = m + 1;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] =
                if a[i] == b[j] { dp[(i + 1) * w + j + 1] + 1 } else { dp[(i + 1) * w + j].max(dp[i * w + j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Sequence comparison: `core` predicted vs `trace` observed.
#[derive(Clone, Debug, Default)]
pub struct SeqCmp {
    pub matched: usize,
    pub core: usize,
    pub trace: usize,
    /// (time, item) predicted by the core but absent from the trace
    pub extra: Vec<(f64, String)>,
    /// (time, item) in the trace but not predicted by the core
    pub missing: Vec<(f64, String)>,
    /// per-owner categories only: matched pairs whose trace event comes *before* the trace event during which the core
    /// produced it (the effect precedes the core's cause — a match by order only, not a causal prediction)
    pub early: Vec<(f64, String)>,
}

fn compare<T: PartialEq + std::fmt::Debug>(
    core: &[(f64, T)],
    trace: &[(f64, T)],
    fmt: impl Fn(&T) -> String,
) -> SeqCmp {
    let a: Vec<&T> = core.iter().map(|x| &x.1).collect();
    let b: Vec<&T> = trace.iter().map(|x| &x.1).collect();
    let m = lcs(&a, &b);
    let (mut ca, mut cb) = (vec![false; a.len()], vec![false; b.len()]);
    for &(i, j) in &m {
        ca[i] = true;
        cb[j] = true;
    }
    SeqCmp {
        matched: m.len(),
        core: a.len(),
        trace: b.len(),
        extra: core.iter().zip(&ca).filter(|(_, &k)| !k).map(|(x, _)| (x.0, fmt(&x.1))).collect(),
        missing: trace.iter().zip(&cb).filter(|(_, &k)| !k).map(|(x, _)| (x.0, fmt(&x.1))).collect(),
        early: m
            .iter()
            .filter(|&&(i, j)| trace[j].0 < core[i].0)
            .map(|&(i, j)| (trace[j].0, format!("{} (core cause at {:.3})", fmt(&trace[j].1), core[i].0)))
            .collect(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    /// trig_register: TL computed by the world (C20) == trace TL
    pub reg_tl_ok: usize,
    pub reg_tl_bad: Vec<(f64, String)>,
    /// owners whose TL rule is unknown (trace TL used)
    pub reg_tl_unknown: Vec<(f64, String)>,
    pub reg_total: usize,
    /// registrations predicted by the world vs all trace registrations
    pub regs: SeqCmp,
    pub posts: SeqCmp,
    /// dispatch points: head of the world's queue == action the original dispatched
    pub dispatch_ok: usize,
    pub dispatch_total: usize,
    pub dispatch_bad: Vec<(f64, String)>,
    pub cond_ok: usize,
    pub cond_bad: Vec<(f64, String)>,
    pub cond_unknown: usize,
    pub cond_total: usize,
    pub execs: SeqCmp,
    pub transitions: SeqCmp,
    pub hotspots: SeqCmp,
    /// C23 lifecycle events predicted vs trace Activation/Deactivation of View/ViewPoint/VCNav owners
    pub lifecycle: SeqCmp,
    /// Act/Deact of Node/Location/AssetRef owners predicted by the core (start node, played assets, View
    /// backgrounds) vs the trace's Act/Deact of those classes (the trace has many more: not-modelled objects);
    /// compared per owner
    pub lifecycle_other: SeqCmp,
    /// Act/Deact of VCIFaceLayout / VCInterfaceItem / VCEmotionIcon / VCInventory owners (H58, H60, H66) predicted vs trace, per owner
    pub lifecycle_iface: SeqCmp,
    /// Act/Deact of VCExplorable owners (H55) predicted vs trace, per owner
    pub lifecycle_expl: SeqCmp,
    /// H69: Act/Deact of VCCursor owners and the null owner
    pub lifecycle_cursor: SeqCmp,
    /// Act/Deact of owners in no other category (VCTitle, EvidenceIcon, …)
    pub lifecycle_misc: SeqCmp,
    /// H58: global order of the Deactivations of InterfaceItems and of the assets the core created for them (type-5 hide:
    /// item, its asset, next item, … in item order) — core vs trace, same owner set
    pub hide_order: SeqCmp,
    /// H55: registrations of VCExplorables (trace: every trig_register of a class-0x37 owner) predicted vs trace, in order
    pub regs_expl: SeqCmp,
    /// H59: trace Timer (7) fires explained by a core timer (started by a type-2 action, subscribed owner, fired no
    /// earlier than its delay): (matched, trace fires, core expiries due before the trace end)
    pub timers_ok: usize,
    pub timers_trace: usize,
    pub timers_core: usize,
    pub timer_bad: Vec<(f64, String)>,
    /// world log lines for unimplemented / unknown cases
    pub unimplemented: Vec<(f64, String)>,
    /// H65 / C30: trace gate hits on known owners, those where the core's restore window agrees, disagreements, and
    /// window ends taken from the trace (the core has no rule for when FUN_00493090 closes it)
    pub gate_total: usize,
    pub gate_ok: usize,
    pub gate_bad: Vec<(f64, String)>,
    pub restore_end_from_trace: usize,
    pub unmapped_fires: usize,
    pub queue_left: Vec<String>,
    /// H144: trig_register tracer double hits dropped from the inputs (time, owner)
    pub tracer_doubles: Vec<(f64, String)>,
    /// H145: media ends (trace input) that deactivated an AssetRef the core played
    pub media_ends_used: usize,
    /// H160: the dialogue box's state commands (core: `Out::DboxState`; trace: their q_dispatch records) —
    /// compared as a sequence of commands, the state values are not in the traces
    pub dbox_states: SeqCmp,
}

fn h(x: u32) -> String {
    format!("{x:#x}")
}

/// Run the lock-step replay of one trace.
pub fn replay(hdb: &Hdb, gam: &Hdb, trace: &[TraceEv], rules: Rules) -> Report {
    let mut w = World::new(hdb, gam, rules);
    if rules.h69_startup {
        w.startup(); // H69: application start, before every trace's menu
    }
    w.load_saved_start(); // H65: a save as the gam → its start goes to SetStartView (None for XFiles.gam)
    let mut rep = Report::default();
    #[derive(Clone, Copy)]
    enum Ptr {
        Owner(u32),
        ActionMsg,
    }
    let mut ptrs: HashMap<u64, Ptr> = HashMap::new();
    let mut times: Vec<f64> = Vec::new(); // time of each world output
    let (mut t_regs, mut t_posts, mut t_execs, mut t_trans, mut t_hs, mut t_life) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let lifecycle_class = |o: u32| [class::VIEW, class::VIEWPOINT, class::NAV].iter().any(|&c| hdb.has(c, o));
    let other_class = |o: u32| [0x28, class::LOCATION, class::ASSET_REF].iter().any(|&c| hdb.has(c, o));
    let mut t_other = Vec::new();
    let iface_class = |o: u32| [class::IFACE_LAYOUT, class::INTERFACE_ITEM, class::EMOTION_ICON, class::INVENTORY].iter().any(|&c| hdb.has(c, o));
    let mut t_iface = Vec::new();
    let mut t_deacts: Vec<(f64, u32)> = Vec::new(); // every Deactivation on a known owner (H58 hide order)
    let expl_class = |o: u32| hdb.has(class::EXPLORABLE, o);
    let (mut t_expl, mut t_regs_expl) = (Vec::new(), Vec::new());
    let cursor_class = |o: u32| o == 0 || hdb.has(class::CURSOR, o);
    let mut t_cursor = Vec::new();
    let mut t_misc = Vec::new();
    let mut t_dbox_states: Vec<(f64, &str)> = Vec::new();
    let any_class = |o: u32| {
        lifecycle_class(o) || other_class(o) || iface_class(o) || expl_class(o) || cursor_class(o)
    };
    // H59: running core timers: (id, start time, ms); H103 can hold a second (re-armed) copy of an id
    let mut timers: Vec<(u8, f64, u32)> = Vec::new();
    // H261 (b): ids of the running timers that are periodic (re-armed at expiry)
    let mut periodic: Vec<u8> = Vec::new();
    // H59: one expiry fires Timer on every subscriber (FUN_0041d430 visits them): (id, owners already fired)
    let mut expired: Option<(u8, Vec<u32>)> = None;
    let mut last_kind = 0u32; // H103: kind of the core's last transition
    let mut seen_out = 0usize;
    let mut t_end = 0.0;
    // H651: line ends the core queued (run at the trace's Conversation cmd 0xd; at once in a trace without that event)
    let has_conv_end = trace.iter().any(|e| e.ev == Ev::ConvEnd);
    let mut seq_end: Vec<u32> = Vec::new();
    let mut show: Vec<u32> = Vec::new(); // H922

    for te in trace {
        let t = te.t;
        t_end = t;
        match &te.ev {
            Ev::Reg { scope, owner, tl } => {
                ptrs.insert(*scope, Ptr::Owner(*owner));
                rep.reg_total += 1;
                t_regs.push((t, *owner));
                if expl_class(*owner) {
                    t_regs_expl.push((t, *owner));
                }
                match w.register(*owner) {
                    Some(x) if x == *tl => rep.reg_tl_ok += 1,
                    Some(x) => {
                        rep.reg_tl_bad.push((t, format!("owner {} world TL {} trace TL {}", h(*owner), h(x), h(*tl))))
                    }
                    None => {
                        rep.reg_tl_unknown.push((t, format!("owner {} trace TL {} (no TL rule)", h(*owner), h(*tl))));
                        w.register_tl(*owner, *tl);
                    }
                }
            }
            Ev::Post { msg_ptr, msg } => {
                ptrs.insert(*msg_ptr, Ptr::ActionMsg);
                t_posts.push((t, *msg));
            }
            Ev::Fire { scope, event, .. } => match ptrs.get(scope) {
                Some(Ptr::Owner(o)) => {
                    if (*event == 8 || *event == 9) && lifecycle_class(*o) {
                        t_life.push((t, (*o, *event)));
                    }
                    if (*event == 8 || *event == 9) && other_class(*o) {
                        t_other.push((t, (*o, *event)));
                    }
                    if (*event == 8 || *event == 9) && iface_class(*o) {
                        t_iface.push((t, (*o, *event)));
                    }
                    if *event == 9 {
                        t_deacts.push((t, *o));
                    }
                    if (*event == 8 || *event == 9) && expl_class(*o) {
                        t_expl.push((t, (*o, *event)));
                    }
                    if (*event == 8 || *event == 9) && cursor_class(*o) {
                        t_cursor.push((t, (*o, *event)));
                    }
                    if (*event == 8 || *event == 9) && !any_class(*o) {
                        t_misc.push((t, (*o, *event)));
                    }
                    if *event == crate::world::event::TIMER && rules.h59_timer {
                        rep.timers_trace += 1;
                        // a subscribed timer that is due (earliest start first), else the earliest subscribed one
                        // H261 (b): 5 ms tolerance — the periodic mm timer fires a few ms early or late (fo-054 459.229)
                        let tol = if rules.h261_timer_periodic_alive { 0.005 } else { 0.0 };
                        let due = |x: &(u8, f64, u32)| t - x.1 + tol >= x.2 as f64 / 1000.0;
                        let mut subs: Vec<usize> =
                            (0..timers.len()).filter(|&i| w.timer_subscribers(timers[i].0).contains(o)).collect();
                        subs.sort_by(|&a, &b| (!due(&timers[a])).cmp(&!due(&timers[b])).then(timers[a].1.total_cmp(&timers[b].1)));
                        let hit = subs.first().map(|&i| (i, timers[i]));
                        // a further subscriber of the expiry just matched (same id, owner not fired yet)
                        let same = expired.as_mut().filter(|(id, done)| !done.contains(o) && w.timer_subscribers(*id).contains(o));
                        if let (None, Some((id, done))) = (hit, same) {
                            done.push(*o);
                            w.timer_ctx_next(*id);
                            rep.timers_ok += 1;
                        } else {
                            if let Some((_, (id, _, _))) = hit {
                                w.timer_ctx_next(id); // H90: the fire's event context = the core's expired id
                            }
                            match hit {
                                Some((i, (id, t0, ms))) if t - t0 + tol >= ms as f64 / 1000.0 => {
                                    rep.timers_ok += 1;
                                    if rules.h261_timer_periodic_alive && periodic.contains(&id) {
                                        timers[i].1 = t.min(t0 + ms as f64 / 1000.0); // H261 (b): periodic — next expiry one period after this one (an early fire moves the schedule)
                                    } else {
                                        timers.remove(i);
                                    }
                                    expired = Some((id, vec![*o]));
                                }
                                Some((_, (id, t0, ms))) => rep.timer_bad.push((
                                    t,
                                    format!("Timer on {}: core timer {id} started {t0:.2} fires {:.2}", h(*o), t0 + ms as f64 / 1000.0),
                                )),
                                None => rep.timer_bad.push((t, format!("Timer on {}: no core timer subscribed", h(*o)))),
                            }
                        }
                    }
                    w.fire(*o, *event);
                    // H1408: a Mouse Click on a CharView also runs the native click (H155: the dialogue box / refusal
                    // player), as `Runner`'s Click command does — the trace's event 0 is the input (the user's click, or
                    // H601's queued mouse command after a CharTeleport, fo-162 54.556)
                    if *event == crate::world::event::MOUSE_CLICK && rules.h155_char_view_click && rules.h1408_replay_char_view_click {
                        w.char_view_clicked(*o);
                    }
                }
                Some(Ptr::ActionMsg) => {} // activation/deactivation wrapper around an action run (C23)
                None => rep.unmapped_fires += 1,
            },
            Ev::Cond { action, value } => {
                rep.dispatch_total += 1;
                let head = w.pending().first().copied();
                let msg = if head.map(|m| m.action) == Some(*action) {
                    rep.dispatch_ok += 1;
                    w.next().unwrap()
                } else {
                    let pos = w.pending().iter().position(|m| m.action == *action);
                    rep.dispatch_bad.push((
                        t,
                        format!(
                            "trace runs {} ; world head {} ; {}",
                            h(*action),
                            head.map_or("<empty>".into(), |m| h(m.action)),
                            match pos {
                                Some(p) => format!("found at queue position {p}"),
                                None => "not in world queue".into(),
                            }
                        ),
                    ));
                    // follow the trace
                    w.take(*action).unwrap_or_else(|| {
                        w.begin_dispatch();
                        Msg { action: *action, trigger: 0 }
                    })
                };
                rep.cond_total += 1;
                let decided = match w.condition(&msg) {
                    Some(v) if v == *value => {
                        rep.cond_ok += 1;
                        v
                    }
                    Some(v) => {
                        rep.cond_bad.push((t, format!("action {} world {} trace {}", h(msg.action), v, value)));
                        *value
                    }
                    None => {
                        rep.cond_unknown += 1;
                        *value
                    }
                };
                if decided {
                    w.execute(msg);
                }
            }
            Ev::Exec { action, typ } => t_execs.push((t, (*action, *typ))),
            Ev::Transition { view, kind } => t_trans.push((t, (*view, *kind))),
            Ev::NavClick { vcnav } => w.click_nav(*vcnav),
            Ev::Gate { scope, flag, mode } => {
                if let Some(Ptr::Owner(o)) = ptrs.get(scope) {
                    let o = *o;
                    rep.gate_total += 1;
                    if !*flag && w.restoring {
                        // input: the window closed before this Activation (when FUN_00493090 closes it is not modelled
                        // here — the replay has no command queue; the runner approximates it)
                        rep.restore_end_from_trace += 1;
                        w.end_restore();
                    }
                    if *flag && mode.is_some_and(|m| m != w.restore_mode as u32) {
                        rep.gate_bad.push((t, format!("Act {}: trace mode {mode:?}, save mode {}", h(o), w.restore_mode)));
                    } else if *flag == w.restoring {
                        rep.gate_ok += 1;
                    } else {
                        rep.gate_bad.push((t, format!("Act {}: trace flag {flag}, core restoring {}", h(o), w.restoring)));
                    }
                    if *flag {
                        // a gated Activation has no fire_post in the trace; it is still the object's Activation
                        if lifecycle_class(o) {
                            t_life.push((t, (o, 8)));
                        }
                        if other_class(o) {
                            t_other.push((t, (o, 8)));
                        }
                        if iface_class(o) {
                            t_iface.push((t, (o, 8)));
                        }
                        if expl_class(o) {
                            t_expl.push((t, (o, 8)));
                        }
                        if cursor_class(o) {
                            t_cursor.push((t, (o, 8)));
                        }
                        if !any_class(o) {
                            t_misc.push((t, (o, 8)));
                        }
                        w.fire(o, 8);
                    }
                }
            }
            Ev::Hotspot { vcnav } => t_hs.push((t, *vcnav)),
            Ev::Key { code } => {
                if rules.h896_replay_keys {
                    w.dev.key = Some(*code); // H896: input, as `Runner::key_press`
                }
            }
            Ev::PlayerRun => {
                w.run_player();
            }
            // H1201 / H362: the held object's Deactivation resets RegCurrInvVar's variable before the queued posts run
            // (fo-168: 0xa070's condition 0x15a = 0x3ee false at 215.646)
            Ev::HeldDrop => {
                w.held_drop(); // H362 / H1420
            }
            Ev::GameStart => {
                // H895: FUN_0048cd70 also runs for the game-end object (mode 1) — the same blk_reset, then no game start
                if rules.h895_game_end_no_start && std::mem::take(&mut w.px.game_end_object) {
                } else if rules.h71_game_start {
                    w.game_start();
                }
            }
            Ev::MenuLayout { layout, from_menu } => {
                if (*layout == 0x142ca && rules.h70_menu_layout) || (*layout == 0x14269 && rules.h140_save_screen_open) {
                    w.menu_screen(*layout, *from_menu);
                }
            }
            Ev::StateRun { scope } => {
                if let Some(Ptr::Owner(o)) = ptrs.get(scope) {
                    w.icon_off(*o);
                }
            }
            // H103: a transition of kind 1..4 runs the entry effect inside the View's state command: timers
            // paused (killed, remaining time kept only if > 0) and resumed (every timer with no native handle re-armed).
            // A one-shot timer whose native expiry already happened (its id queued for the timer drain, not yet drained)
            // is re-armed with its full delay → a second expiry. Trace-timed: due by the trace clock = expired before
            // the pause.
            Ev::ViewStateRun => {
                if rules.h103_timer_pause_resume && (1..=4).contains(&last_kind) {
                    let rearm: Vec<(u8, f64, u32)> =
                        timers.iter().filter(|x| t - x.1 >= x.2 as f64 / 1000.0).map(|x| (x.0, t, x.2)).collect();
                    timers.extend(rearm);
                }
            }
            Ev::DboxRun => {
                w.dbox_run();
            }
            Ev::DboxState => {
                t_dbox_states.push((t, "box state cmd"));
                // H922: the box's queued state-1 command runs at the trace's dispatch of a box state command
                for l in std::mem::take(&mut show) {
                    w.dbox_show_cmd(l);
                }
            }
            Ev::ConvEnd => {
                for c in std::mem::take(&mut seq_end) {
                    w.dbox_seq_end(c);
                }
            }
            Ev::MediaEnd { asset } => {
                if w.media_end(*asset) {
                    rep.media_ends_used += 1;
                }
            }
            Ev::TracerDouble { owner } => rep.tracer_doubles.push((t, format!("REG {} repeated", h(*owner)))),
            Ev::CharViewRun { scope } => {
                if let Some(Ptr::Owner(o)) = ptrs.get(scope) {
                    w.char_view_player(*o);
                }
            }
        }
        times.resize(w.out.len(), t);
        for o in &w.out[seen_out..] {
            if let Out::Timer { id, start } = o {
                timers.retain(|x| x.0 != *id);
                periodic.retain(|x| x != id);
                if let Some((ms, per)) = start {
                    timers.push((*id, t, *ms));
                    if *per {
                        periodic.push(*id);
                    }
                }
            }
            if let Out::Transition { kind, .. } = o {
                last_kind = *kind;
            }
            if let Out::DboxSeqEndPost { conv } = o {
                seq_end.push(*conv);
            }
            if let Out::DboxShowPost { list } = o {
                show.push(*list);
            }
        }
        seen_out = w.out.len();
        if !has_conv_end && !seq_end.is_empty() {
            for c in std::mem::take(&mut seq_end) {
                w.dbox_seq_end(c);
            }
            times.resize(w.out.len(), t);
            seen_out = w.out.len();
        }
    }
    // timers still running whose delay passed before the trace ended: predicted expiries the trace does not have
    for (id, t0, ms) in &timers {
        if t0 + *ms as f64 / 1000.0 <= t_end {
            rep.timer_bad.push((*t0, format!("core timer {id} started {t0:.2} ({ms} ms) never fired in the trace")));
        }
    }
    rep.timers_core = rep.timers_ok + rep.timer_bad.iter().filter(|x| x.1.starts_with("core timer")).count();

    let (mut c_regs, mut c_posts, mut c_execs, mut c_trans, mut c_hs, mut c_life) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut c_other = Vec::new();
    let mut c_iface = Vec::new();
    let (mut c_expl, mut c_regs_expl) = (Vec::new(), Vec::new());
    let mut c_cursor = Vec::new();
    let mut c_misc = Vec::new();
    let mut c_dbox_states: Vec<(f64, &str)> = Vec::new();
    // H58: InterfaceItems and their assets (owner set of the hide-order check)
    let mut item_set: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for o in &w.out {
        if let Out::Reg { owner, cause, .. } = o {
            match cause {
                crate::world::RegCause::IfaceItem { .. } => {
                    item_set.insert(*owner);
                }
                crate::world::RegCause::ObjectAsset { of } if hdb.has(class::INTERFACE_ITEM, *of) => {
                    item_set.insert(*owner);
                }
                _ => {}
            }
        }
    }
    let mut c_hide = Vec::new();
    for (o, &t) in w.out.iter().zip(&times) {
        if let Out::Fire { owner, event: 9 } = o {
            if item_set.contains(owner) {
                c_hide.push((t, *owner));
            }
        }
    }
    let t_hide: Vec<(f64, u32)> = t_deacts.iter().filter(|x| item_set.contains(&x.1)).copied().collect();
    rep.hide_order = compare(&c_hide, &t_hide, |o| format!("Deact {}", h(*o)));
    for (o, &t) in w.out.iter().zip(&times) {
        match o {
            Out::Reg { owner, .. } => {
                c_regs.push((t, *owner));
                if expl_class(*owner) {
                    c_regs_expl.push((t, *owner));
                }
            }
            // H67: a restored timer message is posted by FUN_00477940, not by the trigger path (no act_post hit)
            Out::Post { src: PostSrc::Restore | PostSrc::Engine, .. } => {}
            Out::Post { msg, .. } => c_posts.push((t, *msg)),
            Out::Exec { action, typ } => c_execs.push((t, (*action, *typ))),
            Out::Transition { view, kind, .. } => c_trans.push((t, (*view, *kind))),
            Out::Hotspot { vcnav, .. } => c_hs.push((t, *vcnav)),
            Out::Fire { owner, event } if lifecycle_class(*owner) => c_life.push((t, (*owner, *event))),
            Out::Fire { owner, event } if iface_class(*owner) => c_iface.push((t, (*owner, *event))),
            Out::Fire { owner, event } if expl_class(*owner) => c_expl.push((t, (*owner, *event))),
            Out::Fire { owner, event } if cursor_class(*owner) => c_cursor.push((t, (*owner, *event))),
            Out::Fire { owner, event } if other_class(*owner) => c_other.push((t, (*owner, *event))),
            Out::Fire { owner, event } => c_misc.push((t, (*owner, *event))),
            Out::DboxState { .. } => c_dbox_states.push((t, "box state cmd")),
            // H100 mode 3: a stop of a player the core knows ends it like a media end → its Deactivation
            Out::PlayerStop { asset, known: true } if other_class(*asset) => c_other.push((t, (*asset, 9))),
            Out::Unimplemented { action, what } => {
                rep.unimplemented.push((t, format!("action {}: {what}", h(*action))))
            }
            Out::UnknownType { action, typ } => {
                rep.unimplemented.push((t, format!("action {}: unknown type {typ:#x}", h(*action))))
            }
            // H73: a write of a trigger-local is modelled
            Out::SetVar { action, result }
                if !matches!(result, crate::world::vars::SetResult::Ok { .. } | crate::world::vars::SetResult::Local { .. }) =>
            {
                rep.unimplemented.push((t, format!("action {}: type 0 not modelled: {result:?}", h(*action))))
            }
            _ => {}
        }
    }
    let ev = |e: u8| match e {
        8 => "Act".to_string(),
        9 => "Deact".to_string(),
        e => format!("ev{e}"),
    };
    rep.regs = compare(&c_regs, &t_regs, |o| h(*o));
    rep.posts = compare(&c_posts, &t_posts, |m| format!("{} trig {}", h(m.action), h(m.trigger)));
    rep.execs = compare(&c_execs, &t_execs, |(a, ty)| format!("{} type {ty:#x}", h(*a)));
    rep.transitions = compare(&c_trans, &t_trans, |(v, k)| format!("view {} kind {k}", h(*v)));
    rep.hotspots = compare(&c_hs, &t_hs, |n| h(*n));
    rep.lifecycle = compare(&c_life, &t_life, |(o, e)| format!("{} {}", ev(*e), h(*o)));
    // per owner (the core emits a transition's lifecycle at transition_new, the original runs it as queued steps:
    // the relative order of different owners' events is not a prediction, each owner's own sequence is)
    let per_owner = |core: &[(f64, (u32, u8))], trace: &[(f64, (u32, u8))]| {
        let mut owners: Vec<u32> = core.iter().chain(trace).map(|x| x.1 .0).collect();
        owners.sort_unstable();
        owners.dedup();
        let mut lo = SeqCmp::default();
        for o in owners {
            let pick = |v: &[(f64, (u32, u8))]| v.iter().filter(|x| x.1 .0 == o).cloned().collect::<Vec<_>>();
            let c = compare(&pick(core), &pick(trace), |(o, e)| format!("{} {}", ev(*e), h(*o)));
            lo.matched += c.matched;
            lo.core += c.core;
            lo.trace += c.trace;
            lo.extra.extend(c.extra);
            lo.missing.extend(c.missing);
            lo.early.extend(c.early);
        }
        lo.early.sort_by(|a, b| a.0.total_cmp(&b.0));
        lo
    };
    rep.lifecycle_other = per_owner(&c_other, &t_other);
    rep.lifecycle_iface = per_owner(&c_iface, &t_iface);
    rep.lifecycle_expl = per_owner(&c_expl, &t_expl);
    rep.lifecycle_cursor = per_owner(&c_cursor, &t_cursor);
    rep.lifecycle_misc = per_owner(&c_misc, &t_misc);
    rep.regs_expl = compare(&c_regs_expl, &t_regs_expl, |o| h(*o));
    rep.dbox_states = compare(&c_dbox_states, &t_dbox_states, |x| x.to_string());
    rep.queue_left = w.pending().iter().map(|m| format!("{} trig {}", h(m.action), h(m.trigger))).collect();
    rep
}

impl Report {
    /// Short table: one line per category.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        let seq = |s: &mut String, name: &str, c: &SeqCmp| {
            let early = if c.early.is_empty() { String::new() } else { format!(", {} matched before the core's cause", c.early.len()) };
            writeln!(
                s,
                "{name:<28} {:>4}/{:<4} trace items reproduced ({} predicted, {} extra{early})",
                c.matched,
                c.trace,
                c.core,
                c.core - c.matched
            )
            .unwrap()
        };
        writeln!(
            s,
            "{:<28} {:>4}/{:<4} (TL rule unknown: {}, wrong: {})",
            "trig_register TL (C20)",
            self.reg_tl_ok,
            self.reg_total,
            self.reg_tl_unknown.len(),
            self.reg_tl_bad.len()
        )
        .unwrap();
        seq(&mut s, "trig_register predicted", &self.regs);
        seq(&mut s, "act_post (action,trigger)", &self.posts);
        writeln!(s, "{:<28} {:>4}/{:<4}", "dispatch order (C24)", self.dispatch_ok, self.dispatch_total).unwrap();
        writeln!(
            s,
            "{:<28} {:>4}/{:<4} evaluable (not evaluable: {}, total {})",
            "conditions (C21/C22)",
            self.cond_ok,
            self.cond_total - self.cond_unknown,
            self.cond_unknown,
            self.cond_total
        )
        .unwrap();
        seq(&mut s, "action_exec (id,type)", &self.execs);
        seq(&mut s, "transition_new (view,kind)", &self.transitions);
        seq(&mut s, "nav_hotspot_new", &self.hotspots);
        seq(&mut s, "lifecycle events (C23)", &self.lifecycle);
        seq(&mut s, "Act/Deact Node/Loc/AssetRef", &self.lifecycle_other);
        seq(&mut s, "Act/Deact IFL/IItem/EmIc/Inv", &self.lifecycle_iface);
        if self.hide_order.trace + self.hide_order.core > 0 {
            seq(&mut s, "Deact order IItem+asset(H58)", &self.hide_order);
        }
        seq(&mut s, "REG Explorable (H55)", &self.regs_expl);
        seq(&mut s, "Act/Deact Explorable (H55)", &self.lifecycle_expl);
        seq(&mut s, "Act/Deact Cursor (H69)", &self.lifecycle_cursor);
        seq(&mut s, "Act/Deact other classes", &self.lifecycle_misc);
        if self.dbox_states.trace + self.dbox_states.core > 0 {
            seq(&mut s, "dialogue box state cmds", &self.dbox_states);
        }
        if self.timers_trace + self.timers_core > 0 {
            writeln!(
                s,
                "{:<28} {:>4}/{:<4} trace Timer fires explained ({} core expiries due, {} bad)",
                "timers (H59)",
                self.timers_ok,
                self.timers_trace,
                self.timers_core,
                self.timer_bad.len()
            )
            .unwrap();
        }
        if self.gate_total > 0 {
            writeln!(
                s,
                "{:<28} {:>4}/{:<4} gate hits agree with the core's restore window ({} window ends from the trace)",
                "restore gate (C30/H65)",
                self.gate_ok,
                self.gate_total,
                self.restore_end_from_trace
            )
            .unwrap();
        }
        if self.media_ends_used > 0 {
            writeln!(s, "{:<28} {:>4}       media ends taken from the trace (H145)", "media ends (input)", self.media_ends_used).unwrap();
        }
        writeln!(s, "unimplemented/unknown cases  {}", self.unimplemented.len()).unwrap();
        s
    }

    /// Every discrepancy, by category.
    pub fn details(&self) -> String {
        let mut s = String::new();
        let list = |s: &mut String, title: &str, v: &[(f64, String)]| {
            if !v.is_empty() {
                writeln!(s, "-- {title} ({})", v.len()).unwrap();
                for (t, x) in v {
                    writeln!(s, "  {t:9.3} {x}").unwrap();
                }
            }
        };
        list(&mut s, "trig_register TL wrong", &self.reg_tl_bad);
        list(&mut s, "trig_register TL rule unknown", &self.reg_tl_unknown);
        list(&mut s, "registrations predicted, not in trace", &self.regs.extra);
        list(&mut s, "registrations in trace, not predicted", &self.regs.missing);
        list(&mut s, "posts predicted, not in trace", &self.posts.extra);
        list(&mut s, "posts in trace, not predicted", &self.posts.missing);
        list(&mut s, "dispatch order mismatches", &self.dispatch_bad);
        list(&mut s, "condition mismatches", &self.cond_bad);
        list(&mut s, "execs predicted, not in trace", &self.execs.extra);
        list(&mut s, "execs in trace, not predicted", &self.execs.missing);
        list(&mut s, "transitions predicted, not in trace", &self.transitions.extra);
        list(&mut s, "transitions in trace, not predicted", &self.transitions.missing);
        list(&mut s, "hotspots predicted, not in trace", &self.hotspots.extra);
        list(&mut s, "hotspots in trace, not predicted", &self.hotspots.missing);
        list(&mut s, "lifecycle events predicted, not in trace", &self.lifecycle.extra);
        list(&mut s, "lifecycle events in trace (View/VP/VCNav), not predicted", &self.lifecycle.missing);
        list(&mut s, "Act/Deact Node/Loc/AssetRef in trace, not predicted", &self.lifecycle_other.missing);
        list(&mut s, "Act/Deact Node/Loc/AssetRef predicted, not in trace", &self.lifecycle_other.extra);
        list(&mut s, "Act/Deact IFL/IItem/EmIc/Inv predicted, not in trace", &self.lifecycle_iface.extra);
        list(&mut s, "Act/Deact IFL/IItem/EmIc/Inv in trace, not predicted", &self.lifecycle_iface.missing);
        list(&mut s, "Act/Deact Node/Loc/AssetRef matched before the core's cause", &self.lifecycle_other.early);
        list(&mut s, "Act/Deact IFL/IItem/EmIc/Inv matched before the core's cause", &self.lifecycle_iface.early);
        list(&mut s, "Act/Deact Explorable matched before the core's cause", &self.lifecycle_expl.early);
        list(&mut s, "H58 hide order: Deacts predicted, not in trace order", &self.hide_order.extra);
        list(&mut s, "H58 hide order: Deacts in trace, not predicted in this order", &self.hide_order.missing);
        list(&mut s, "Explorable registrations predicted, not in trace", &self.regs_expl.extra);
        list(&mut s, "Explorable registrations in trace, not predicted", &self.regs_expl.missing);
        list(&mut s, "Act/Deact Explorable predicted, not in trace", &self.lifecycle_expl.extra);
        list(&mut s, "Act/Deact Explorable in trace, not predicted", &self.lifecycle_expl.missing);
        list(&mut s, "Act/Deact Cursor predicted, not in trace", &self.lifecycle_cursor.extra);
        list(&mut s, "Act/Deact Cursor in trace, not predicted", &self.lifecycle_cursor.missing);
        list(&mut s, "Act/Deact other classes predicted, not in trace", &self.lifecycle_misc.extra);
        list(&mut s, "Act/Deact other classes in trace, not predicted", &self.lifecycle_misc.missing);
        list(&mut s, "dialogue box state commands predicted, not in trace", &self.dbox_states.extra);
        list(&mut s, "dialogue box state commands in trace, not predicted", &self.dbox_states.missing);
        list(&mut s, "timers (H59)", &self.timer_bad);
        list(&mut s, "restore gate (C30/H65)", &self.gate_bad);
        list(&mut s, "unimplemented / unknown (world log)", &self.unimplemented);
        list(&mut s, "tracer double hits dropped (H144)", &self.tracer_doubles);
        if !self.queue_left.is_empty() {
            writeln!(
                s,
                "-- left in world queue at trace end ({}): {}",
                self.queue_left.len(),
                self.queue_left.join(", ")
            )
            .unwrap();
        }
        writeln!(s, "-- fires on unmapped scopes (ignored): {}", self.unmapped_fires).unwrap();
        s
    }
}
