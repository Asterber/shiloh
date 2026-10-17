//! End-to-end regression harness: replay the bot's whole playthrough route recorded on the ORIGINAL
//! (`tools/emu/e2e-route.json`, built from the traces; a private research input, not in this repository) on the
//! headless core and report, per segment and per step, the first point where the port diverges from the original.
//!
//! The core is driven by the clicks only (as the window / `xfplan` drive it — NOT by the trace's events as
//! [`crate::replay`] does): Load of the segment's start save (H65), settle (nothing queued or playing, no one-shot timer),
//! the core's clock run on to the original's Load length, then per step the click / key / move and the core's clock run
//! for exactly as long as the original's step lasted (click mark → next click mark, wall clock): periodic timers fire as
//! often, and a click the bot made while a video still played comes as early in the core (`--settle`: settle after
//! each step first, xfplan's timing). Every hypothesis on (`Rules::all()`, the
//! window's rule set). The one trace-derived input is the value of conditions the core cannot evaluate (operand kind 1/4,
//! `Runner::uneval_from_trace` over fo-009 and the segment's own trace) — exactly what the bot's planner used.
//!
//! Per step the core's outputs are compared with the original's trace events between this step's click mark and the next
//! one (the bot writes `bot: step K: View V click OWNER …` into the trace just before clicking):
//! - primary (a difference = divergence): transitions (View sequence), posts (multiset of action ids, as the bot's own check), executed actions (multiset);
//! - secondary (reported, counted, not a divergence): the click target (the core's hit now vs the owner the run-time
//!   core hit, logged by the bot — not an observation of the original), repeats of an id both sides posted (periodic
//!   timers), registrations and Activation / Deactivation of the classes the core
//!   models (View / ViewPoint / VCNav / InterfaceItem / IFaceLayout / EmotionIcon / Inventory / Explorable / Cursor /
//!   Conversation rows) — the original registers many objects the core does not model (Node / Location / AssetRef,
//!   CharView media), those are left out.
//!
//! After the last step the core's state (Node, View, every VCVariable, active Inventory objects) is compared with the
//! original's end save (loaded into a fresh core the same way). `--chained` runs the main chain in one session (the port's
//! own state carried across segments instead of the original's save).
use crate::front::{self, Session};
use crate::hdb::Hdb;
use crate::replay::{parse_trace, Ev, TraceEv};
use crate::world::{class, devices, devices::MenuChoice, run::Runner, Rules};
use serde_json::{json, Value as J};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// H604: IFL_scPhoneNumber (string variable; the core keeps the text in the phone's native buffer, H166)
const VAR_PHONE_NUMBER: u32 = 0xa3e;

/// Variables that change with time / navigation only (evaluate.py NOISE)
pub const NOISE: [u32; 5] = [0x421, 0x108c, 0xb6b, 0x1134, 0x38f];

#[derive(Clone, Debug)]
pub struct StepSpec {
    pub step: String,
    pub view: String,
    pub owner: String,
    pub cls: String,
    /// shoot_run.py runs: the text of the bot's mark written just before this step's click (its window's
    /// start) — runs without `bot: step K: View …` click marks
    pub mark: Option<String>,
    /// H1521: a click of one xfcap `seq` (several steps share its mark): its window starts at the n-th button-down after
    /// the mark in the trace
    pub down: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub name: String,
    pub save: String,
    pub end: Option<String>,
    pub trace: String,
    pub steps: Vec<StepSpec>,
    pub parent: Option<String>,
    pub main: bool,
}

pub struct Route {
    pub chain: Vec<String>,
    pub segments: Vec<Segment>,
}

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub fn load_route(path: &Path) -> Result<Route, String> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let j: J = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    let s = |v: &J| v.as_str().unwrap_or("").to_string();
    let segments = j["segments"]
        .as_array()
        .ok_or("no segments")?
        .iter()
        .map(|g| Segment {
            name: s(&g["name"]),
            save: s(&g["save"]),
            end: g["end"].as_str().map(String::from),
            trace: s(&g["trace"]),
            steps: g["steps"]
                .as_array()
                .map(|v| {
                    v.iter()
                        .map(|x| StepSpec {
                            step: s(&x["step"]),
                            view: s(&x["view"]),
                            owner: s(&x["owner"]),
                            cls: s(&x["cls"]),
                            mark: x["mark"].as_str().map(String::from),
                            down: x["down"].as_u64().map(|n| n as usize),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            parent: g["parent"].as_str().map(String::from),
            main: g["main"].as_bool().unwrap_or(false),
        })
        .collect();
    let chain = j["chain"].as_array().map(|v| v.iter().map(s).collect()).unwrap_or_default();
    Ok(Route { chain, segments })
}

/// A trace's text; `.jsonl.gz` (large traces) through `gzip -dc`
pub fn read_trace(path: &Path) -> Option<String> {
    if let Ok(t) = std::fs::read_to_string(path) {
        return Some(t);
    }
    let gz = PathBuf::from(format!("{}.gz", path.display()));
    let out = std::process::Command::new("gzip").arg("-dc").arg(&gz).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---- steps ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum St {
    Pt(i32, i32),
    Owner(u32),
    Hand(i32, i32, u32),
    Move(i32, i32),
    Right(i32, i32),
    Key(u32),
    Type(String),
    /// `n:X,Y` (explore.py / xfplan): a left click as `p:`; the bot's prediction settled it only until
    /// nothing is queued and no picture medium plays (one-shot timers and sounds not waited for — timed windows such as
    /// Link 2's 15-s Timer). On the original it is a plain click.
    Quick(i32, i32),
    /// `q:X,Y` (xfplan): as `m:`, settled like `n:`
    QuickMove(i32, i32),
}

fn hexu(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

fn parse_step(x: &str) -> Option<St> {
    let x = x.trim();
    let xy = |p: &str| -> Option<(i32, i32)> {
        let (a, b) = p.split_once(',')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    };
    let (k, rest) = x.split_at(2.min(x.len()));
    Some(match k {
        "k:" => St::Key(if let Some(h) = rest.strip_prefix("0x") { u32::from_str_radix(h, 16).ok()? } else { rest.parse().ok()? }),
        "t:" => St::Type(rest.to_string()),
        "m:" => xy(rest).map(|(a, b)| St::Move(a, b))?,
        "r:" => xy(rest).map(|(a, b)| St::Right(a, b))?,
        "p:" | "x:" => xy(rest).map(|(a, b)| St::Pt(a, b))?,
        "n:" => xy(rest).map(|(a, b)| St::Quick(a, b))?,
        "q:" => xy(rest).map(|(a, b)| St::QuickMove(a, b))?,
        "o:" => St::Owner(hexu(rest)?),
        "h:" => {
            let v: Vec<&str> = rest.split(',').collect();
            St::Hand(v.first()?.parse().ok()?, v.get(1)?.parse().ok()?, hexu(v.get(2)?)?)
        }
        _ => return None,
    })
}

/// H842: bot runs whose typed letters reached the game as Cyrillic characters: xfcap posted WM_KEYDOWN of the letter's
/// VK and the game's TranslateMessage under the bottle's layout 0x419 made the WM_CHAR (R30 / H660: VK 'S' → 0xfb 'ы',
/// `traces/bf1-d/bf1-d.jsonl`); FUN_00486e60 drops them (shots fo-070 / fo-087 k23: the password field empty). Fixed in
/// xfcap from fo-092 on (C72). These are the original's inputs, so the core gets the same characters.
const H842_CYRILLIC_TYPING: &[&str] = &["fo-070", "fo-087"];

/// H842: the cp1251 character TranslateMessage gives a letter key under the Russian layout 0x419 (ЙЦУКЕН); other
/// characters unchanged
fn ru_layout_char(c: char) -> u32 {
    const QWERTY: &str = "qwertyuiopasdfghjklzxcvbnm";
    const RU: [u8; 26] = [
        0xe9, 0xf6, 0xf3, 0xea, 0xe5, 0xed, 0xe3, 0xf8, 0xf9, 0xe7, 0xf4, 0xfb, 0xe2, 0xe0, 0xef, 0xf0, 0xee, 0xeb, 0xe4,
        0xff, 0xf7, 0xf1, 0xec, 0xe8, 0xf2, 0xfc,
    ];
    match QWERTY.find(c.to_ascii_lowercase()) {
        Some(i) => RU[i] as u32,
        None => c as u32,
    }
}

fn settle(s: &mut Session) {
    front::settle_timers(s, 100, 900_000);
}

/// xfplan's quick settle after a `n:` / `q:` step: only until nothing is queued and no picture medium plays (≤ 60 s);
/// sounds (AssetRefs under "Sounds": background music, atmosphere) and one-shot timers are not waited for
fn settle_quick(s: &mut Session) {
    let t0 = s.now;
    s.tick(0);
    let h = s.r.w.h;
    while (s.r.pending() > 0 || s.r.started().iter().any(|&a| !is_sound(h, a))) && s.now - t0 < 60_000 {
        s.tick(100);
    }
}

/// An AssetRef under "Sounds" (background music, atmosphere)
fn is_sound(h: &Hdb, a: u32) -> bool {
    h.read(class::ASSET_REF, a)
        .and_then(|r| r.blob("asset"))
        .and_then(|b| h.blob_bytes(b))
        .map_or(false, |b| String::from_utf8_lossy(b).contains("Sounds"))
}

/// The settle after a Load: as [`settle`], but a sound playing (a restored player, H1601 — fo-109's 0x6f60 resumes at
/// 13 s of 40 s and ends 27.3 s later, after the first click) is not waited for: the Load window then runs the
/// original's wall-clock length, so the sound ends in the same window as on the original
fn settle_load(s: &mut Session) {
    let t0 = s.now;
    s.tick(0);
    let h = s.r.w.h;
    let busy = |s: &Session| {
        !s.r.menu_open && (s.r.pending() > 0 || s.r.started().iter().any(|&a| !is_sound(h, a)) || s.r.one_shot_timers() > 0)
    };
    while busy(s) && s.now - t0 < 900_000 {
        s.tick(100);
    }
}

impl St {
    fn quick(&self) -> bool {
        matches!(self, St::Quick(..) | St::QuickMove(..))
    }
}

/// Apply one step; returns the owner the core's click hit (None: nothing hit / no click)
fn apply(s: &mut Session, st: &St) -> Option<u32> {
    match *st {
        St::Key(k) => {
            s.r.key_press(k);
            None
        }
        St::Type(ref t) => {
            for c in t.chars() {
                // H867: '-' / '.' / ',' are two key events (VK_OEM_* keydown, then the WM_CHAR)
                for k in crate::front::host::key_events_of_char(c) {
                    s.r.key_press(k);
                }
                settle(s);
            }
            None
        }
        St::Pt(x, y) | St::Quick(x, y) => {
            s.hover_at(x, y);
            s.click_at(x, y).map(|c| c.owner)
        }
        St::Owner(o) => s.r.click(o).then_some(o),
        St::Hand(x, y, o) => {
            // H1406: the hand-registered owner is no clickable of the core's at all and the point hits no HDB owner (a
            // movie hotspot of the playing answer, or nothing): the original's click went there too (fo-156 k10: the
            // box hidden while the answer 0x5ab5 played, view_reshow 182.57 after the click 179.93 — no event 0; k11)
            let engine = |c: u32| {
                crate::world::moviehot::is_movie_hot(c)
                    || [devices::PANEL_OWNER, devices::MENU_OWNER, devices::GUN_OWNER, devices::VIDEO_OWNER].contains(&c)
            };
            if !s.r.clickables().iter().any(|c| c.owner == o) && s.r.hit(x, y).map_or(true, |c| engine(c.owner)) {
                s.hover_at(x, y);
                return s.click_at(x, y).map(|c| c.owner);
            }
            s.mouse = Some((x, y));
            s.r.hover(Some(o));
            s.r.click(o).then_some(o)
        }
        St::Move(x, y) | St::QuickMove(x, y) => {
            s.hover_at(x, y);
            None
        }
        // H1404: the bot moves the pointer there first (as explore's `r:`): the hover leaves the panel strip (H361's
        // entry test — fo-148 k14 r:300,200 after k13's bar click, then k15 m:20,440 shows the panel again)
        St::Right(x, y) => {
            s.hover_at(x, y);
            s.r.right_click(x, y).map(|c| c.owner)
        }
    }
}

// ---- per-step observations -----------------------------------------------------------------------------------------

/// What happened in one step window (core or original)
#[derive(Clone, Debug, Default)]
pub struct Obs {
    pub trans: Vec<(u32, u32)>,
    pub posts: Vec<u32>,
    pub execs: Vec<u32>,
    pub regs: Vec<u32>,
    pub life: Vec<(u32, u8)>,
    /// H600: actions posted / executed on the original by a Roll Out Of (event 5) on a runtime object that got no Roll
    /// Into since its registration — the engine's hover pointer dangling into a reused heap block (allocator-dependent,
    /// not modelled): left out of the primary comparison on both sides
    pub stale: Vec<u32>,
    /// H840: the Roll Into actions the original did NOT post because the pointer moved onto a runtime object that took
    /// the freed block of the hovered one (the engine sees "same object" and fires Mouse Move, event 2, instead of
    /// Roll Into) — allocator-dependent, not modelled: one occurrence of each is dropped from the core's posts / execs
    pub stale_into: Vec<u32>,
}

/// classes whose registrations / Activations the core models (secondary comparison)
fn modelled(h: &Hdb, o: u32) -> bool {
    o == 0
        || [
            class::VIEW,
            class::VIEWPOINT,
            class::NAV,
            class::INTERFACE_ITEM,
            class::IFACE_LAYOUT,
            class::EMOTION_ICON,
            class::INVENTORY,
            class::EXPLORABLE,
            class::CURSOR,
            class::CONVERSATION,
        ]
        .iter()
        .any(|&c| h.has(c, o))
}

/// C107: how long the step's left button was held on the original (first 0x201 → the next 0x202 in [from, to)), s.
fn button_hold(text: &str, from: f64, to: f64) -> Option<f64> {
    let mut down = None;
    for line in text.lines() {
        if !line.contains("\"wnd_mouse\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<J>(line) else { continue };
        let t = r["t"].as_f64().unwrap_or(0.0);
        if t < from {
            continue;
        }
        if t >= to {
            return None;
        }
        let msg = r["regs"]["eax"].as_str().and_then(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).ok());
        match (msg, down) {
            (Some(0x201), None) => down = Some(t),
            (Some(0x202), Some(d)) => return Some(t - d),
            _ => {}
        }
    }
    None
}

/// H1402: the time of the first pointer input (wnd_mouse 0x200 / 0x201 / 0x204) in [from, to) of the trace.
fn input_after(text: &str, from: f64, to: f64) -> Option<f64> {
    for line in text.lines() {
        if !line.contains("\"wnd_mouse\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<J>(line) else { continue };
        let t = r["t"].as_f64().unwrap_or(0.0);
        if t < from {
            continue;
        }
        if t >= to {
            return None;
        }
        let msg = r["regs"]["eax"].as_str().and_then(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).ok());
        if matches!(msg, Some(0x200 | 0x201 | 0x204)) {
            return Some(t);
        }
    }
    None
}

/// H861: the time of the first button-down (wnd_mouse message 0x201 / 0x204) in [from, to) of the trace.
fn button_down_after(text: &str, from: f64, to: f64) -> Option<f64> {
    for line in text.lines() {
        if !line.contains("\"wnd_mouse\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<J>(line) else { continue };
        let t = r["t"].as_f64().unwrap_or(0.0);
        if t < from {
            continue;
        }
        if t >= to {
            return None;
        }
        let msg = r["regs"]["eax"].as_str().and_then(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).ok());
        if matches!(msg, Some(0x201 | 0x204)) {
            return Some(t);
        }
    }
    None
}

/// H1521: the times of every left button-down (wnd_mouse 0x201) in [from, to) of the trace.
fn button_downs_after(text: &str, from: f64, to: f64) -> Vec<f64> {
    let mut v = Vec::new();
    for line in text.lines() {
        if !line.contains("\"wnd_mouse\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<J>(line) else { continue };
        let t = r["t"].as_f64().unwrap_or(0.0);
        if t < from {
            continue;
        }
        if t >= to {
            break;
        }
        if r["regs"]["eax"].as_str() == Some("0x00000201") {
            v.push(t);
        }
    }
    v
}

/// H1321: the time of the last pointer move (wnd_mouse 0x200) in [from, to) of the trace.
fn mouse_move_before(text: &str, from: f64, to: f64) -> Option<f64> {
    let mut last = None;
    for line in text.lines() {
        if !line.contains("\"wnd_mouse\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<J>(line) else { continue };
        let t = r["t"].as_f64().unwrap_or(0.0);
        if t < from {
            continue;
        }
        if t >= to {
            break;
        }
        if r["regs"]["eax"].as_str() == Some("0x00000200") {
            last = Some(t);
        }
    }
    last
}

/// H1180: the Load's restored Timers (VCGameState l154, H67) are armed on the original by their queued type-2 message
/// (its timer_arm record, matched below) 0.13–0.63 s of wall clock after the game-start function (fo-161: id 2 at +0.126, first
/// expiry +2.128 = arm + 2000 ms; fo-109..fo-166 the same), not at the game start as in the core. Per restored timer id:
/// the arm's offset after the game start (ms), from the first such arm within 2 s of the game start before the first click.
fn load_timer_arms(text: &str, evs: &[TraceEv]) -> HashMap<u8, u64> {
    let mut out = HashMap::new();
    let Some(gs) = evs.iter().find(|e| matches!(e.ev, Ev::GameStart)).map(|e| e.t) else { return out };
    for line in text.lines() {
        if line.contains("\"mark\"") && line.contains("bot: step 0:") {
            break;
        }
        if !line.contains("\"timer_arm\"") || !line.contains("\"ret\": \"0x00428891\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<J>(line) else { continue };
        let t = r["t"].as_f64().unwrap_or(0.0);
        if t < gs {
            continue;
        }
        if t > gs + 2.0 {
            break;
        }
        let id = r["timer"]["dw"][2].as_str().and_then(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).ok());
        if let Some(id) = id.filter(|&i| i < 0x100) {
            out.entry(id as u8).or_insert(((t - gs) * 1000.0).round() as u64);
        }
    }
    out
}

/// H1180: shift the core's timers restored by a Load (armed at the core's game start) by the original's arm delay
fn delay_load_timers(s: &mut Session, arms: &HashMap<u8, u64>) {
    // the restored timers start when the Load's queued type-2 actions run (H59)
    s.tick(0);
    for t in s.r.timers.iter_mut() {
        if let Some(&off) = arms.get(&t.id) {
            t.due += off;
        }
    }
}

/// Split a trace into windows: [0] = before the first click (Load), [k+1] = step k. Returns also the click marks' times.
/// `bounds[i]..bounds[i + 1]` = window i
fn trace_windows(h: &Hdb, text: &str, evs: &[TraceEv], steps: &[StepSpec]) -> (Vec<Obs>, Vec<f64>, Vec<f64>) {
    let nsteps = steps.len();
    let mut click_t: Vec<Option<f64>> = vec![None; nsteps];
    let mut end_t = f64::INFINITY;
    for line in text.lines() {
        if !line.contains("\"mark\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<J>(line) else { continue };
        if r["ev"] != "mark" {
            continue;
        }
        let t = r["t"].as_f64().unwrap_or(0.0);
        let txt = r["text"].as_str().unwrap_or("");
        // shoot_run.py: the step's own mark text, in step order
        if let Some(k) = (0..nsteps).find(|&k| click_t[k].is_none() && steps[k].mark.as_deref() == Some(txt)) {
            if (0..k).all(|j| click_t[j].is_some()) {
                click_t[k] = Some(t);
                // H1521: the other clicks of the same `seq` share the mark
                let mut j = k + 1;
                while steps[k].down.is_some() && j < nsteps && steps[j].down.is_some() && steps[j].mark.as_deref() == Some(txt) {
                    click_t[j] = Some(t);
                    j += 1;
                }
            }
            continue;
        }
        if let Some(rest) = txt.strip_prefix("bot: step ") {
            if let Some((k, tail)) = rest.split_once(':') {
                if tail.starts_with(" View ") {
                    if let Ok(k) = k.parse::<usize>() {
                        if k < nsteps && click_t[k].is_none() {
                            click_t[k] = Some(t);
                        }
                    }
                }
            }
        } else if (txt.starts_with("bot: save:") || txt == "bot: stop") && end_t.is_infinite() && click_t.iter().all(|c| c.is_some()) {
            end_t = t;
        }
    }
    let mut ts: Vec<f64> = click_t.iter().map(|c| c.unwrap_or(f64::INFINITY)).collect();
    // H1321: a shoot_run.py round mark is written when the bot sees the goon View (tens of ms after the transition, in the
    // middle of the View's entry); its click follows after a computed wait. A click step of such a run starts at its
    // click instead: the pointer's move just before the button-down (≤ 0.2 s), else the button-down
    // H1521: a `seq` click starts at its own button-down (the n-th after the shared mark, before the run's end)
    for k in 0..nsteps {
        if let (Some(n), true) = (steps[k].down, ts[k].is_finite()) {
            ts[k] = button_downs_after(text, ts[k], end_t).get(n).copied().unwrap_or(f64::INFINITY);
        }
    }
    for k in 0..nsteps {
        let clickish = matches!(parse_step(&steps[k].step), Some(St::Pt(..) | St::Quick(..)));
        if steps[k].mark.is_none() || steps[k].down.is_some() || !clickish || !ts[k].is_finite() {
            continue;
        }
        let next = ts.get(k + 1).copied().unwrap_or(end_t);
        if let Some(down) = button_down_after(text, ts[k], next) {
            ts[k] = mouse_move_before(text, ts[k], down).filter(|&m| down - m <= 0.2).unwrap_or(down);
        }
    }
    // H1402: a pointer step starts when the original reads its input (first wnd_mouse 0x200 / 0x201 / 0x204 after the
    // mark), not at the bot's mark: under the old winedbg tracer (~0.3 s per breakpoint) the engine still works through
    // the previous step's chain for tens of seconds after the next mark (fo-041 k2's Deactivations up to 444.9, the k3
    // move read at 453.3, mark 409.4)
    // only the winedbg traces: the in-process tracer keeps up, and its bot marks lead the pointer by ~0.5 s (H1120)
    let input_start = !text.lines().next().is_some_and(|l| l.contains("\"tracer\": \"inproc\""));
    if input_start {
        for k in 0..nsteps {
            let pointer = matches!(
                parse_step(&steps[k].step),
                Some(St::Pt(..) | St::Quick(..) | St::Move(..) | St::QuickMove(..) | St::Right(..) | St::Hand(..))
            );
            if steps[k].mark.is_some() || !pointer || !ts[k].is_finite() {
                continue;
            }
            let next = ts.get(k + 1).copied().unwrap_or(end_t);
            if let Some(t) = input_after(text, ts[k], next) {
                ts[k] = t;
            }
        }
    }
    // the Load window starts at the game-start function (H71, after the main menu and the load screen, which the core's
    // `load_game` does not run); a trace without it: from the start
    let first = ts.first().copied().unwrap_or(end_t);
    let start = evs.iter().filter(|e| matches!(e.ev, Ev::GameStart) && e.t < first).map(|e| e.t).last().unwrap_or(f64::NEG_INFINITY);
    let mut bounds = vec![start];
    bounds.extend(&ts);
    bounds.push(end_t);
    let mut win = vec![Obs::default(); nsteps + 1];
    let idx = |t: f64| -> Option<usize> { (0..=nsteps).find(|&i| t >= bounds[i] && t < bounds[i + 1]) };
    let mut ptrs: HashMap<u64, u32> = HashMap::new();
    // H600: per runtime object (scope): its TriggerList and whether it got a Roll Into since its registration
    let mut rolled: HashMap<u64, (u32, bool)> = HashMap::new();
    let mut stale_trig: Vec<u32> = Vec::new();
    let mut stale_act: Vec<u32> = Vec::new();
    for e in evs {
        if let Ev::Reg { scope, owner, tl } = e.ev {
            ptrs.insert(scope, owner);
            rolled.insert(scope, (tl, false));
        }
        match e.ev {
            Ev::Fire { scope, event: 4, .. } => {
                if let Some(r) = rolled.get_mut(&scope) {
                    r.1 = true;
                }
            }
            Ev::Fire { scope, event: 5, .. } => {
                if let Some(r) = rolled.get_mut(&scope) {
                    if !r.1 {
                        stale_trig = h
                            .list_of(class::TRIGGER_LIST, r.0)
                            .into_iter()
                            .filter(|&t| h.read(class::TRIGGER, t).is_some_and(|x| x.u32("fe8") == 5))
                            .collect();
                    }
                    r.1 = false;
                }
            }
            Ev::Fire { scope, event: 2, .. } => {
                stale_trig.clear();
                // H840: a Mouse Move on a runtime object that got no Roll Into since its registration = the hover
                // pointer still holds the freed block this object now occupies: its Roll Into was not fired
                if let Some(r) = rolled.get_mut(&scope) {
                    if !r.1 {
                        r.1 = true;
                        if let Some(i) = idx(e.t) {
                            for t in h.list_of(class::TRIGGER_LIST, r.0) {
                                if let Some(x) = h.read(class::TRIGGER, t).filter(|x| x.u32("fe8") == 4) {
                                    win[i].stale_into.extend(h.list_of(class::ACTION_LIST, x.u32("fe4")));
                                }
                            }
                        }
                    }
                }
            }
            Ev::Fire { .. } => stale_trig.clear(),
            _ => {}
        }
        let Some(i) = idx(e.t) else { continue };
        let w = &mut win[i];
        match &e.ev {
            Ev::Transition { view, kind } => w.trans.push((*view, *kind)),
            Ev::Post { msg, .. } if msg.action != 0 && stale_trig.contains(&msg.trigger) => {
                stale_act.push(msg.action);
                w.stale.push(msg.action);
            }
            Ev::Post { msg, .. } if msg.action != 0 => w.posts.push(msg.action),
            Ev::Exec { action, .. } if stale_act.contains(action) => {
                stale_act.retain(|a| a != action);
            }
            Ev::Exec { action, .. } => w.execs.push(*action),
            Ev::Reg { owner, .. } if modelled(h, *owner) => w.regs.push(*owner),
            Ev::Fire { scope, event, .. } if *event == 8 || *event == 9 => {
                if let Some(&o) = ptrs.get(scope) {
                    if modelled(h, o) {
                        w.life.push((o, *event));
                    }
                }
            }
            Ev::Gate { scope, flag: true, .. } => {
                if let Some(&o) = ptrs.get(scope) {
                    if modelled(h, o) {
                        w.life.push((o, 8));
                    }
                }
            }
            _ => {}
        }
    }
    (win, ts, bounds)
}

fn core_obs(h: &Hdb, r: &Runner, m: &Marks) -> Obs {
    Obs {
        trans: r.transitions[m.t..].to_vec(),
        posts: r.posts[m.p..].iter().map(|x| x.action).filter(|&a| a != 0).collect(),
        execs: r.execs[m.e..].to_vec(),
        regs: r.regs[m.r..].iter().copied().filter(|&o| modelled(h, o)).collect(),
        life: r.lifecycle[m.l..].iter().copied().filter(|x| modelled(h, x.0)).collect(),
        stale: Vec::new(),
        stale_into: Vec::new(),
    }
}

#[derive(Clone, Copy, Default)]
struct Marks {
    t: usize,
    p: usize,
    e: usize,
    r: usize,
    l: usize,
}

fn marks(r: &Runner) -> Marks {
    Marks { t: r.transitions.len(), p: r.posts.len(), e: r.execs.len(), r: r.regs.len(), l: r.lifecycle.len() }
}

/// multiset difference: (only in a, only in b)
fn mdiff<T: Ord + Clone>(a: &[T], b: &[T]) -> (Vec<T>, Vec<T>) {
    let mut ca: BTreeMap<T, i32> = BTreeMap::new();
    for x in a {
        *ca.entry(x.clone()).or_default() += 1;
    }
    for x in b {
        *ca.entry(x.clone()).or_default() -= 1;
    }
    let (mut oa, mut ob) = (Vec::new(), Vec::new());
    for (k, n) in ca {
        for _ in 0..n.max(0) {
            oa.push(k.clone());
        }
        for _ in 0..(-n).max(0) {
            ob.push(k.clone());
        }
    }
    (oa, ob)
}

fn hl(v: &[u32]) -> String {
    let mut s: Vec<String> = v.iter().take(12).map(|x| format!("{x:x}")).collect();
    if v.len() > 12 {
        s.push(format!("…+{}", v.len() - 12));
    }
    format!("[{}]", s.join(","))
}

fn hlife(v: &[(u32, u8)]) -> String {
    let mut s: Vec<String> =
        v.iter().take(10).map(|(o, e)| format!("{}{o:x}", if *e == 8 { "Act " } else { "Deact " })).collect();
    if v.len() > 10 {
        s.push(format!("…+{}", v.len() - 10));
    }
    format!("[{}]", s.join(","))
}

/// The bot's live verdict on step `k` from its marks in the trace (see [`StepRes::bot_flag`])
fn bot_flags(text: &str, k: usize) -> Option<bool> {
    let (mut checked, mut flagged) = (false, false);
    let differ = format!("bot: step {k}: posts differ");
    let ok = format!("bot: step {k}: posts as predicted");
    let mm = format!("MISMATCH at step {k}:");
    let lock = format!("LOCK-STEP MISMATCH after {} steps", k + 1);
    for line in text.lines().filter(|l| l.contains("\"mark\"") && l.contains("bot: ")) {
        if line.contains("posts as predicted") || line.contains("posts differ") {
            checked = true;
        }
        if line.contains(&differ) || line.contains(&mm) || line.contains(&lock) {
            flagged = true;
        }
        if line.contains(&ok) {
            checked = true;
        }
    }
    if flagged {
        Some(true)
    } else if checked {
        Some(false)
    } else {
        None
    }
}

/// One divergence in one step
#[derive(Clone, Debug)]
pub struct Diff {
    pub cat: &'static str,
    pub primary: bool,
    pub expected: String,
    pub got: String,
}

#[derive(Clone, Debug)]
pub struct StepRes {
    /// the bot's live check at run time (its marks): Some(true) = it flagged this step (posts differ / MISMATCH /
    /// LOCK-STEP MISMATCH), Some(false) = it checked and found the step as predicted, None = the run had no such check
    pub bot_flag: Option<bool>,
    /// -1 = the Load window (before the first click)
    pub index: i64,
    pub step: String,
    pub view: String,
    pub t: f64,
    pub diffs: Vec<Diff>,
}

impl StepRes {
    pub fn diverges(&self) -> bool {
        self.diffs.iter().any(|d| d.primary)
    }
}

/// the log lines of Out::Unimplemented / UnknownType (`Runner` logs them as "action 0x…: what"; the H182 /
/// H191 / H403 outcome lines are not gaps)
pub fn unimpl_lines(log: &[String]) -> Vec<String> {
    log.iter()
        .filter(|l| l.starts_with("action 0x") && !l.contains(": H182 game end") && !l.contains(": H191 menu object") && !l.contains(": H733 main loop busy"))
        .cloned()
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct SegRes {
    pub name: String,
    pub main: bool,
    pub skipped: Option<String>,
    pub steps: Vec<StepRes>,
    /// end-state differences vs the original's end save (None = no end save)
    pub end: Option<Vec<String>>,
    pub end_noise: Vec<String>,
    pub views: Vec<u32>,
    /// the runner's "action …: <what>" lines of Out::Unimplemented / UnknownType logged during the segment
    pub unimpl: Vec<String>,
    /// `--chained` (H1580): the port's save at the segment's end vs the original's end save, field by field
    /// ([`save_diff`]; not a divergence — what the next segment's Load starts from)
    pub save: Vec<String>,
}

/// H1580: the fields of the port's save that differ from the original's save of the same point (values unknown to the
/// port — `unknown` variables, kept from the previous save — and the timers' remaining time / players' positions, which
/// depend on the bot's wall-clock waits, are left out)
pub fn save_diff(port: &crate::save::SaveState, orig: &crate::save::SaveState, unknown: &[u32]) -> Vec<String> {
    let mut d = Vec::new();
    let hx = |v: &[u32]| v.iter().map(|x| format!("{x:x}")).collect::<Vec<_>>().join(",");
    let po: BTreeMap<_, _> = port.enabled.iter().copied().collect();
    let oo: BTreeMap<_, _> = orig.enabled.iter().copied().collect();
    for (k, v) in &oo {
        if po.get(k) != Some(v) {
            d.push(format!("enabled ({:x},{:x}): original {v} port {:?}", k.0, k.1, po.get(k)));
        }
    }
    let strs: Vec<u32> = orig.strings.iter().map(|s| s.0).collect();
    let pv: BTreeMap<u32, i32> = port.vars.iter().map(|v| (v.0, v.1)).collect();
    for &(id, v, _) in &orig.vars {
        if unknown.contains(&id) || strs.contains(&id) || NOISE.contains(&id) {
            continue;
        }
        if pv.get(&id) != Some(&v) {
            d.push(format!("var {id:x}: original {v} port {:?}", pv.get(&id)));
        }
    }
    let mut lists: Vec<(&str, Vec<u32>, Vec<u32>)> = vec![
        ("start", port.start.to_vec(), orig.start.to_vec()),
        ("inventories", port.inventories.clone(), orig.inventories.clone()),
        // [AssetRef, flag] (H1600: the flag = looping)
        ("players", port.players.iter().flat_map(|p| [p[0], p[1]]).collect(), orig.players.iter().flat_map(|p| [p[0], p[1]]).collect()),
        ("evidence", port.evidence.iter().flatten().copied().collect(), orig.evidence.iter().flatten().copied().collect()),
        ("kit", port.kit.clone(), orig.kit.clone()),
        ("timers", port.timers.iter().flat_map(|t| [t[0], t[2]]).collect(), orig.timers.iter().flat_map(|t| [t[0], t[2]]).collect()),
        ("char_view", vec![port.char_view], vec![orig.char_view]),
        ("conv_history", port.conv_history.clone(), orig.conv_history.clone()),
        ("notes", port.notes.clone(), orig.notes.clone()),
        ("inbox", port.inbox.clone(), orig.inbox.clone()),
        ("read", port.read.clone(), orig.read.clone()),
        ("pending", port.pending.clone(), orig.pending.clone()),
        ("photos", port.photos.iter().map(|p| p.picture()).collect(), orig.photos.iter().map(|p| p.picture()).collect()),
    ];
    lists.push(("gs f4c", port.extra.f4c.to_vec(), orig.extra.f4c.to_vec()));
    lists.push(("gs f198 f1a0 f1a8", vec![port.extra.f198, port.extra.f1a0, port.extra.f1a8], vec![orig.extra.f198, orig.extra.f1a0, orig.extra.f1a8]));
    for (name, p, o) in lists {
        if p != o {
            d.push(format!("{name}: original [{}] port [{}]", hx(&o), hx(&p)));
        }
    }
    if port.extra.b190 != orig.extra.b190 || port.extra.b19c != orig.extra.b19c || port.extra.mode != orig.extra.mode {
        d.push(format!(
            "gs b190 b19c mode: original {:?} {} {} port {:?} {} {}",
            orig.extra.b190, orig.extra.b19c, orig.extra.mode, port.extra.b190, port.extra.b19c, port.extra.mode
        ));
    }
    d
}

impl SegRes {
    /// number of steps (the Load window counts as step -1) before the first primary divergence
    pub fn first_div(&self) -> Option<&StepRes> {
        self.steps.iter().find(|s| s.diverges())
    }
    /// steps of the route (Load excluded) reproduced before the first divergence
    pub fn prefix_ok(&self) -> usize {
        match self.first_div() {
            Some(s) if s.index < 0 => 0,
            Some(s) => s.index as usize,
            None => self.steps.iter().filter(|s| s.index >= 0).count(),
        }
    }
    pub fn n_steps(&self) -> usize {
        self.steps.iter().filter(|s| s.index >= 0).count()
    }
    pub fn first_secondary(&self) -> Option<&StepRes> {
        self.steps.iter().find(|s| s.diffs.iter().any(|d| !d.primary))
    }
    pub fn pass(&self) -> bool {
        self.skipped.is_none() && self.first_div().is_none() && self.end.as_ref().map_or(true, |e| e.is_empty())
    }
}

fn compare(core: &Obs, orig: &Obs) -> Vec<Diff> {
    let mut d = Vec::new();
    // H600: the original's stale Roll Out posts are allocator-dependent — drop one occurrence of each from the core too
    let mut core = core.clone();
    for a in &orig.stale {
        for v in [&mut core.posts, &mut core.execs] {
            if let Some(i) = v.iter().position(|x| x == a) {
                v.remove(i);
            }
        }
    }
    let mut dropped = Vec::new();
    for a in &orig.stale_into {
        let mut hit = false;
        for v in [&mut core.posts, &mut core.execs] {
            if let Some(i) = v.iter().position(|x| x == a) {
                v.remove(i);
                hit = true;
            }
        }
        if hit {
            dropped.push(*a);
        }
    }
    let core = &core;
    if !dropped.is_empty() {
        d.push(Diff {
            cat: "stale roll-into (H840)",
            primary: false,
            expected: "Mouse Move on a runtime object in the hovered one's freed block: no Roll Into".into(),
            got: format!("posted {} (Roll Into, not modelled: allocator-dependent)", hl(&dropped)),
        });
    }
    if !orig.stale.is_empty() {
        d.push(Diff {
            cat: "stale roll-out (H600)",
            primary: false,
            expected: format!("posted {} (Roll Out Of on a reused runtime object)", hl(&orig.stale)),
            got: "not modelled (allocator-dependent)".into(),
        });
    }
    let cv: Vec<u32> = core.trans.iter().map(|x| x.0).collect();
    let ov: Vec<u32> = orig.trans.iter().map(|x| x.0).collect();
    if cv != ov {
        d.push(Diff {
            cat: "transitions",
            primary: true,
            expected: orig.trans.iter().map(|(v, k)| format!("{v:x}/{k}")).collect::<Vec<_>>().join(","),
            got: core.trans.iter().map(|(v, k)| format!("{v:x}/{k}")).collect::<Vec<_>>().join(","),
        });
    }
    for (cat, c, o) in [("posts", &core.posts, &orig.posts), ("execs", &core.execs, &orig.execs)] {
        let (only_c, only_o) = mdiff(c, o);
        // an id both sides have, with a different count (periodic Timer repeats over the original's longer wall-clock
        // window, H261): secondary; an id only one side has: primary
        let (rep_c, new_c): (Vec<u32>, Vec<u32>) = only_c.iter().partition(|x| o.contains(x));
        let (rep_o, new_o): (Vec<u32>, Vec<u32>) = only_o.iter().partition(|x| c.contains(x));
        if !new_c.is_empty() || !new_o.is_empty() {
            d.push(Diff {
                cat,
                primary: true,
                expected: format!("{} total, only original {}", o.len(), hl(&new_o)),
                got: format!("{} total, only port {}", c.len(), hl(&new_c)),
            });
        }
        if !rep_c.is_empty() || !rep_o.is_empty() {
            d.push(Diff {
                cat: if cat == "posts" { "posts (repeats)" } else { "execs (repeats)" },
                primary: false,
                expected: format!("more on the original {}", hl(&rep_o)),
                got: format!("more on the port {}", hl(&rep_c)),
            });
        }
    }
    let (only_c, only_o) = mdiff(&core.regs, &orig.regs);
    if !only_c.is_empty() || !only_o.is_empty() {
        d.push(Diff { cat: "regs", primary: false, expected: format!("only original {}", hl(&only_o)), got: format!("only port {}", hl(&only_c)) });
    }
    let (only_c, only_o) = mdiff(&core.life, &orig.life);
    if !only_c.is_empty() || !only_o.is_empty() {
        d.push(Diff {
            cat: "act/deact",
            primary: false,
            expected: format!("only original {}", hlife(&only_o)),
            got: format!("only port {}", hlife(&only_c)),
        });
    }
    d
}

// ---- state ---------------------------------------------------------------------------------------------------------

pub struct State {
    pub node: Option<u32>,
    pub view: Option<u32>,
    pub vars: Vec<(u32, Option<i32>)>,
    pub inv: Vec<u32>,
    pub held: bool,
    /// H604: the texts of the string variables the core knows (the phone number 0xa3e = the native buffer, H166)
    pub texts: Vec<(u32, String)>,
}

pub fn state_of(s: &Session, var_ids: &[u32]) -> State {
    let w = &s.r.w;
    let mut inv: Vec<u32> = s.r.active.iter().copied().filter(|&o| w.h.has(class::INVENTORY, o)).collect();
    inv.sort();
    inv.dedup();
    let mut texts = vec![(VAR_PHONE_NUMBER, w.dev.number.clone())];
    // H650: the string variables whose text the core knows (SetLanguage's ` scGameLanguage`)
    if w.rules.h650_string_compare {
        texts.extend(w.vars.changed_texts().into_iter().map(|(v, t)| (v, t.iter().map(|&c| c as char).collect())));
    }
    State { node: w.node, view: w.view, vars: var_ids.iter().map(|&v| (v, w.vars.get(v))).collect(), inv, held: w.dev.held.is_some(), texts }
}

pub fn var_ids(gam: &Hdb) -> Vec<u32> {
    gam.class(0x53).map(|c| c.objects.iter().map(|o| o.0).collect()).unwrap_or_default()
}

fn var_name(gam: &Hdb, v: u32) -> String {
    gam.read(0x53, v)
        .and_then(|r| r.blob("name"))
        .and_then(|b| gam.blob_bytes(b))
        .map(|b| b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect::<String>().trim().to_string())
        .unwrap_or_default()
}

/// (differences, noise: time / navigation variables and variables whose core value is unknown): original end state vs core
fn state_diff(gam: &Hdb, orig: &State, core: &State) -> (Vec<String>, Vec<String>) {
    let (mut d, mut noise) = (Vec::new(), Vec::new());
    let o: HashMap<u32, Option<i32>> = orig.vars.iter().copied().collect();
    // H604 / H475: a string variable's value is the objId of its VCString — the original gives every new text a new
    // objId, the core keeps texts natively: compared by text where the core knows it, else listed with the noise
    let texts = crate::save::SaveState::from_gam(gam).strings;
    for &(k, v) in &core.vars {
        let ov = o.get(&k).copied().flatten();
        if ov == v {
            continue;
        }
        if let Some((_, t)) = texts.iter().find(|x| x.0 == k) {
            let ot: String = t.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
            let name = var_name(gam, k);
            match core.texts.iter().find(|x| x.0 == k) {
                Some((_, ct)) if *ct == ot => {}
                Some((_, ct)) => d.push(format!("string var 0x{k:x} {name}: original {ot:?} port {ct:?}")),
                None => noise.push(format!("string var 0x{k:x} {name}: original {ot:?} (strings not modelled)")),
            }
            continue;
        }
        // iCurrInventory: the save's Esc menu drops the held object (C57) after the core's state (evaluate.py)
        if k == 0x15a && core.held && ov == Some(0) {
            continue;
        }
        let line = format!("var 0x{k:x} {}: original {} port {}", var_name(gam, k), fmt_opt(ov), fmt_opt(v));
        // the core's value unknown (a RandGen / unevaluable write): not a prediction, listed with the noise
        if NOISE.contains(&k) || v.is_none() {
            noise.push(line)
        } else {
            d.push(line)
        }
    }
    let ho = |v: Option<u32>| v.map_or("-".into(), |v| format!("{v:x}"));
    if orig.node != core.node {
        d.push(format!("node: original {} port {}", ho(orig.node), ho(core.node)));
    }
    if orig.view != core.view {
        d.push(format!("view: original {} port {}", ho(orig.view), ho(core.view)));
    }
    if orig.inv != core.inv {
        let (oc, oo) = mdiff(&core.inv, &orig.inv);
        d.push(format!("inventory: only original {} only port {}", hl(&oo), hl(&oc)));
    }
    (d, noise)
}

fn fmt_opt(v: Option<i32>) -> String {
    v.map_or("unknown".into(), |v| v.to_string())
}

// ---- runner --------------------------------------------------------------------------------------------------------

pub struct Ctx {
    /// run the core's clock on to the original's step length (default on)
    pub wallclock: bool,
    /// no settle after a step: the core's clock runs exactly the original's step length (default on; off = settle, then
    /// run on to the step length — `xfplan`'s timing, which delivers a click only after every media ended)
    pub timed: bool,
    pub hdb: Hdb,
    pub data: PathBuf,
    pub uneval_base: HashMap<u32, bool>,
}

impl Ctx {
    pub fn open() -> Option<Ctx> {
        // XF_DATA: a folder, or disc images / several sources as a path list (crate::vfs)
        let src = crate::vfs::env_sources("XF_DATA");
        let data = if src.is_empty() {
            repo().join("data/InstalledGame")
        } else {
            crate::vfs::data_root(&src, &mut |l| eprintln!("e2e: {l}")).map_err(|e| eprintln!("e2e: {e}")).ok()?
        };
        let hdb = Hdb::open(data.join("XFiles.hdb")).ok()?;
        let mut uneval_base = HashMap::new();
        if let Some(t) = read_trace(&repo().join("traces/fo-009.jsonl")) {
            if let Ok(tr) = parse_trace(&t) {
                uneval_base = Runner::uneval_from_trace(&tr, &hdb);
            }
        }
        Some(Ctx { wallclock: true, timed: true, hdb, data, uneval_base })
    }

    fn session<'a>(&'a self, gam: &'a Hdb, uneval: HashMap<u32, bool>) -> Session<'a> {
        let mut s = Session::with_rules(&self.hdb, gam, &self.data, Rules::all());
        s.r.uneval = uneval;
        s.load_game();
        settle_load(&mut s);
        s
    }

    /// Run one segment from its start save (isolated)
    pub fn run_segment(&self, seg: &Segment) -> SegRes {
        let mut res = SegRes { name: seg.name.clone(), main: seg.main, ..Default::default() };
        let root = repo();
        let Ok(gam) = Hdb::open(root.join(&seg.save)) else {
            res.skipped = Some(format!("start save {} missing", seg.save));
            return res;
        };
        let Some((text, evs)) = self.trace(seg) else {
            res.skipped = Some(format!("trace {} missing / unreadable", seg.trace));
            return res;
        };
        let mut uneval = self.uneval_base.clone();
        uneval.extend(Runner::uneval_from_trace(&evs, &self.hdb));
        uneval.extend(Runner::uneval_random_from_trace(&evs, &self.hdb)); // H603
        let mut s = Session::with_rules(&self.hdb, &gam, &self.data, Rules::all());
        s.r.uneval = uneval;
        s.r.uneval_seq = Runner::uneval_random_seq_from_trace(&evs, &self.hdb); // H841
        (s.r.w.smol_trace_picks, s.r.w.smol_trace_will) = crate::world::shootout::picks_from_trace(&evs, &self.hdb); // H1320
        let m0 = marks(&s.r);
        s.load_game();
        delay_load_timers(&mut s, &load_timer_arms(&text, &evs)); // H1180
        settle_load(&mut s);
        self.compare_steps(seg, &mut s, Some(m0), &text, &evs, &mut res);
        // XF_E2E_LOG=1: the core's and the session's logs of the segment (debugging)
        if std::env::var_os("XF_E2E_LOG").is_some() {
            for l in s.r.log.iter().chain(&s.log) {
                eprintln!("  {}: {l}", seg.name);
            }
            for (t, a, st) in &s.r.media_log {
                eprintln!("  {}: media {t} {a:#x} {st}", seg.name);
            }
        }
        let ids = var_ids(&gam);
        self.end_check(seg, &s, &ids, &mut res);
        res.unimpl = unimpl_lines(&s.r.log);
        res
    }

    fn trace(&self, seg: &Segment) -> Option<(String, Vec<TraceEv>)> {
        let text = read_trace(&repo().join(&seg.trace))?;
        let cut = &text[..text.rfind('\n').map(|i| i + 1).unwrap_or(0)];
        let evs = parse_trace(cut).ok()?;
        Some((text, evs))
    }

    fn compare_steps(&self, seg: &Segment, s: &mut Session, load: Option<Marks>, text: &str, evs: &[TraceEv], res: &mut SegRes) {
        let (win, ts, bounds) = trace_windows(&self.hdb, text, evs, &seg.steps);
        if let Some(m0) = load {
            // the Load window's wall-clock length (game start → first click), as for the steps
            let dur = bounds[1] - bounds[0];
            if self.wallclock && dur.is_finite() {
                let want = (dur * 1000.0) as u64;
                let t0 = s.now;
                // H404: the menu stops the game's clock — the step's wall time cannot be run on while it shows
                while s.now - t0 < want && !s.r.menu_open {
                    s.tick(100);
                }
            }
            let c = core_obs(&self.hdb, &s.r, &m0);
            res.steps.push(StepRes { bot_flag: None, index: -1, step: "load".into(), view: String::new(), t: 0.0, diffs: compare(&c, &win[0]) });
        }
        for (k, sp) in seg.steps.iter().enumerate() {
            let view = s.r.w.view.map_or("-".into(), |v| format!("{v:x}"));
            let Some(st) = parse_step(&sp.step) else {
                res.steps.push(StepRes {
                    bot_flag: None,
                    index: k as i64,
                    step: sp.step.clone(),
                    view,
                    t: ts[k],
                    diffs: vec![Diff { cat: "step", primary: true, expected: sp.step.clone(), got: "unparsable step".into() }],
                });
                break;
            };
            let m = marks(&s.r);
            let mut diffs = Vec::new();
            if sp.view != view && !sp.view.is_empty() {
                diffs.push(Diff { cat: "view before click", primary: false, expected: sp.view.clone(), got: view.clone() });
            }
            let t0 = s.now;
            // H1120: the bot's mark precedes the click's button-down by ~0.5 s (varying); the original's step window
            // starts at the mark, its click at the button-down — the core's clock runs on to the button-down first, so
            // a timer armed by the click is as late as on the original (fo-155 k4: Cook's 7.5-s Timer 0x126a3 armed at
            // the button-down 80.845, mark 80.304; its third expiry 103.462 lands after k5's mark 103.203)
            let mut lag = 0u64;
            if self.timed && matches!(st, St::Pt(..) | St::Quick(..) | St::Right(..) | St::Hand(..)) && bounds[k + 2].is_finite() {
                if let Some(down) = button_down_after(text, bounds[k + 1], bounds[k + 2]) {
                    lag = ((down - bounds[k + 1]).max(0.0) * 1000.0) as u64;
                    let want = t0 + lag;
                    while s.now < want && !s.r.menu_open {
                        s.tick((want - s.now).min(100));
                    }
                }
            }
            // H1403: a pointer step whose input never reached the game's window proc (no wnd_mouse 0x200 / 0x201 / 0x204
            // in its window: fo-020 k16, the window was deactivated, WM_IME_SETCONTEXT 561.02) did not happen on the
            // original — the core does not make it either
            // H1702: a shoot_run.py `--after-end` click (the "Try again?" box, the main menu after a game end) goes to a
            // modal loop of its own (FUN_004d7250: `skipped wnd_mouse`, `traces/fo-229.jsonl` 75.87–76.40) — never lost
            let after_end = sp.mark.as_deref().is_some_and(|m| m.starts_with("bot: after-end "));
            let lost = !after_end
                && matches!(st, St::Pt(..) | St::Quick(..) | St::Move(..) | St::QuickMove(..) | St::Right(..) | St::Hand(..))
                && bounds[k + 1].is_finite()
                && bounds[k + 2].is_finite()
                && input_after(text, bounds[k + 1], bounds[k + 2]).is_none();
            if lost {
                diffs.push(Diff {
                    cat: "input lost",
                    primary: false,
                    expected: "the original's window proc got no pointer message in this step".into(),
                    got: "step not made on the core".into(),
                });
            }
            let hit = match st {
                _ if lost => None,
                St::Type(ref t) if H842_CYRILLIC_TYPING.contains(&seg.name.as_str()) => {
                    for c in t.chars() {
                        s.r.key_press(ru_layout_char(c));
                        settle(s);
                    }
                    None
                }
                // C107: the left button held as long as on the original — a dragged IdeaIcon's answer starts at the
                // button-up (fo-124 64.024 down → 64.180 up)
                St::Pt(x, y) | St::Quick(x, y) if self.timed && bounds[k + 2].is_finite() => {
                    let hold = button_hold(text, bounds[k + 1], bounds[k + 2]);
                    s.hover_at(x, y);
                    let hit = s.press_at(x, y).map(|c| c.owner);
                    if let Some(h) = hold {
                        s.tick((h * 1000.0) as u64);
                    }
                    s.release_at(Some((x, y)));
                    hit
                }
                _ => apply(s, &st),
            };
            // H1702: the host's part of a menu choice — New (C243: "Return" after a game end) starts a new game from
            // XFiles.gam in the same session (the core's histories continue, as `Session::rewind_yes`)
            if let Some(crate::front::host::HostReq::New) = crate::front::host::request(s) {
                match Hdb::open(self.data.join("XFiles.gam")) {
                    Ok(g) => {
                        let g: &Hdb = Box::leak(Box::new(g));
                        let hist = s.r.take_history();
                        s.reload(g, false);
                        s.r.continue_history(hist);
                    }
                    Err(e) => diffs.push(Diff { cat: "step", primary: true, expected: "XFiles.gam".into(), got: e.to_string() }),
                }
            }
            let mut dur = bounds[k + 2] - bounds[k + 1];
            // H861: the last step ends at the save / stop mark, not at a click: the core's clock runs from the click mark,
            // the original's step from its button-down (the bot's mark comes ~0.5 s before it) — the last step is run for
            // (end − button-down), as every earlier step's length is click-to-click (H1120: superseded in timed mode — the
            // core's click itself waits for the button-down, so the step runs mark → end mark)
            if lag == 0 && k + 1 == seg.steps.len() && matches!(st, St::Pt(..) | St::Quick(..) | St::Right(..)) && dur.is_finite() {
                if let Some(down) = button_down_after(text, bounds[k + 1], bounds[k + 2]) {
                    dur = bounds[k + 2] - down;
                }
            }
            // `timed`: the core runs exactly as long as the original's step (no settle — a click that came while a
            // video still played on the original comes as early in the core)
            if !(self.timed && dur.is_finite()) {
                if st.quick() {
                    settle_quick(s);
                } else {
                    settle(s);
                }
            }
            // the original's step lasted (click mark → next click mark) in wall-clock time; periodic timers (H261) fire
            // during it — the core's clock is run on to the same length (ms) unless `wallclock` is off
            if (self.wallclock || self.timed) && dur.is_finite() {
                s.tick(0);
                let want = t0 + (dur * 1000.0) as u64;
                // H1521: to the ms — a 100-ms tick's overshoot adds up over a `seq`'s 0.12-s steps (fo-211: +0.4 s
                // after ten clicks, 0x1fca's Activation three windows early)
                while s.now < want && !s.r.menu_open {
                    s.tick((want - s.now).min(100));
                }
            }
            // the click target: the bot logged the owner it clicked (`-` / a key string for hand-registered steps)
            if let Some(want) = hexu(&sp.owner).filter(|_| matches!(st, St::Pt(..) | St::Quick(..) | St::Right(..))) {
                if hit != Some(want) {
                    // the logged owner is the run-time core's hit, not an observation of the original: secondary (the
                    // original's observable effects — transitions, posts, executions — decide)
                    diffs.push(Diff {
                        cat: "click target",
                        primary: false,
                        expected: format!("{want:x}"),
                        got: hit.map_or("nothing".into(), |o| format!("{o:x}")),
                    });
                }
            }
            let c = core_obs(&self.hdb, &s.r, &m);
            diffs.extend(compare(&c, &win[k + 1]));
            res.steps.push(StepRes { bot_flag: bot_flags(text, k), index: k as i64, step: sp.step.clone(), view, t: ts[k], diffs });
        }
        res.views = s.r.transitions.iter().map(|x| x.0).collect();
    }

    fn end_check(&self, seg: &Segment, s: &Session, ids: &[u32], res: &mut SegRes) {
        let Some(end) = &seg.end else { return };
        let Ok(eg) = Hdb::open(repo().join(end)) else { return };
        let se = self.session(&eg, self.uneval_base.clone());
        let ids_end = var_ids(&eg);
        let ids: Vec<u32> = if ids_end.is_empty() { ids.to_vec() } else { ids_end };
        let mut orig = state_of(&se, &ids);
        // H1102 / H1121: the variables as the original WROTE them — the Load of the end save runs the game-start procedures
        // (H80 RegViewState: cViewState 0x5d7 := 0, trigger 0x66d6 at every Load, fo-157 / fo-158), so the loaded
        // session's value is not the saved one (fo-158-end.x holds 'w', fo-131-end.x 'a')
        let file: HashMap<u32, i32> = crate::save::SaveState::from_gam(&eg).vars.iter().map(|v| (v.0, v.1)).collect();
        for (k, v) in orig.vars.iter_mut() {
            if let Some(&f) = file.get(k) {
                *v = Some(f);
            }
        }
        let (d, noise) = state_diff(&eg, &orig, &state_of(s, &ids));
        res.end = Some(d);
        res.end_noise = noise;
    }

    /// Harness inputs of one segment taken from its own trace (as `run_segment`): unevaluable conditions (fo-009 + the
    /// segment's trace, H603), RandGen sequences (H841), the shootout picks (H1320)
    fn seg_inputs(&self, s: &mut Session, evs: &[TraceEv]) {
        let mut uneval = self.uneval_base.clone();
        uneval.extend(Runner::uneval_from_trace(evs, &self.hdb));
        uneval.extend(Runner::uneval_random_from_trace(evs, &self.hdb)); // H603
        s.r.uneval = uneval;
        s.r.uneval_seq = Runner::uneval_random_seq_from_trace(evs, &self.hdb); // H841
        (s.r.w.smol_trace_picks, s.r.w.smol_trace_will) = crate::world::shootout::picks_from_trace(evs, &self.hdb); // H1320
    }

    /// The main chain in ONE session from the first segment's start save. `load` (`--chained`, H1580): at every segment
    /// border the session does what the bot did on the original — the save (Esc menu → Save screen, H602) written by the
    /// port's own writer from the port's own state (`Session::save_bytes`, H400 / H405), then the in-game Load of that
    /// file (`Session::reload`, H403 → `Runner::load_game`, H65 / H67 / H80 / H1180): nothing the save does not hold
    /// crosses the border (open layouts, timer phases, players, hover). Each segment's harness inputs come from its own
    /// trace as in the isolated run, and its Load window is compared too. `load` off (`--continuous`): no save / Load,
    /// the port's whole runtime state crosses the borders (a player who never reloads; a segment's first steps may then
    /// rely on state the original's Load cleared — the bot's route, not the port, makes that difference).
    pub fn run_chained(&self, route: &Route, load: bool) -> Vec<SegRes> {
        let segs: Vec<&Segment> =
            route.chain.iter().filter_map(|n| route.segments.iter().find(|s| &s.name == n)).collect();
        self.run_segs(&segs, load)
    }

    /// H1590 (`--pairs`): every border of the main chain on its own — the earlier segment from the original's start save
    /// (as isolated), then the next one in the same session WITHOUT the save / Load. A pair whose second segment
    /// diverges while it passes from the original's save = a border where the Load hides something (the route relies
    /// on what the Load cleared, or the port's runtime state differs from the original's where the save does not
    /// carry it). Result: the second segment of each pair.
    pub fn run_pairs(&self, route: &Route) -> Vec<SegRes> {
        let segs: Vec<&Segment> =
            route.chain.iter().filter_map(|n| route.segments.iter().find(|s| &s.name == n)).collect();
        segs.windows(2).filter_map(|w| self.run_segs(w, false).into_iter().nth(1)).collect()
    }

    fn run_segs(&self, segs: &[&Segment], load: bool) -> Vec<SegRes> {
        let mut out = Vec::new();
        let Some(first) = segs.first() else { return out };
        let Ok(gam) = Hdb::open(repo().join(&first.save)) else { return out };
        let traces: Vec<Option<(String, Vec<TraceEv>)>> = segs.iter().map(|s| self.trace(s)).collect();
        let mut s = Session::with_rules(&self.hdb, &gam, &self.data, Rules::all());
        if load {
            if let Some((_, e)) = &traces[0] {
                self.seg_inputs(&mut s, e);
            }
        } else {
            // one session without a Load: every segment's inputs, in chain order
            let mut uneval = self.uneval_base.clone();
            let mut seq: HashMap<u32, std::collections::VecDeque<bool>> = HashMap::new();
            let (mut picks, mut will) = (std::collections::VecDeque::new(), std::collections::VecDeque::new());
            for (_, e) in traces.iter().flatten() {
                let (p, w) = crate::world::shootout::picks_from_trace(e, &self.hdb); // H1320, in chain order
                picks.extend(p);
                will.extend(w);
                uneval.extend(Runner::uneval_from_trace(e, &self.hdb));
                uneval.extend(Runner::uneval_random_from_trace(e, &self.hdb)); // H603
                for (a, q) in Runner::uneval_random_seq_from_trace(e, &self.hdb) {
                    seq.entry(a).or_default().extend(q); // H841, in chain order
                }
            }
            s.r.uneval = uneval;
            s.r.uneval_seq = seq;
            (s.r.w.smol_trace_picks, s.r.w.smol_trace_will) = (picks, will);
        }
        let mut m0 = Some(marks(&s.r));
        s.load_game();
        if let Some((text, evs)) = &traces[0] {
            delay_load_timers(&mut s, &load_timer_arms(text, evs)); // H1180
        }
        settle_load(&mut s);
        let ids = var_ids(&gam);
        for (i, seg) in segs.iter().enumerate() {
            let mut res = SegRes { name: seg.name.clone(), main: true, ..Default::default() };
            let Some((text, evs)) = &traces[i] else {
                res.skipped = Some("trace missing".into());
                out.push(res);
                break;
            };
            if i > 0 && load {
                // H1580: the save the bot made at the previous segment's end, then a Load of it in the same session
                let bytes = match s.save_bytes() {
                    Ok((b, _)) => b,
                    Err(e) => {
                        res.skipped = Some(format!("port save failed: {e}"));
                        out.push(res);
                        break;
                    }
                };
                // the session borrows its gam for its whole life: the chain's port saves live as long (a few 100 KB each)
                let saved: &Hdb = Box::leak(Box::new(Hdb::from_bytes(bytes)));
                // variables the port does not know (a RandGen write, H603): the save keeps an older value for them; the
                // harness keeps them unknown across the Load (the original's save holds its own random draw)
                let unknown: Vec<(u32, u8)> = s.r.w.vars.stored().into_iter().filter(|v| v.1.is_none()).map(|v| (v.0, v.2)).collect();
                if let Some(prev) = segs[i - 1].end.as_ref().and_then(|e| Hdb::open(repo().join(e)).ok()) {
                    let uk: Vec<u32> = unknown.iter().map(|u| u.0).collect();
                    let d = save_diff(&crate::save::SaveState::from_gam(saved), &crate::save::SaveState::from_gam(&prev), &uk);
                    if let Some(r) = out.last_mut() {
                        r.save = d;
                    }
                }
                s.reload_core(saved);
                for &(v, t) in &unknown {
                    s.r.w.vars.set_native(v, None, t);
                }
                self.seg_inputs(&mut s, evs);
                m0 = Some(marks(&s.r));
                s.load_game();
                delay_load_timers(&mut s, &load_timer_arms(text, evs)); // H1180
                settle_load(&mut s);
            } else if i > 0 {
                // H862: this segment starts from a Load of the previous one's save: a periodic timer is saved as
                // [id][period] (VCGameState l154; fo-109-end.x: timer 0x19, 45000 ms, periodic) and re-armed by the Load
                // with its full period (H67) — its phase does not carry over; then the Load window (game start → first
                // click) runs on the clock as in an isolated run
                s.r.w.load_reg_writes(); // H1124
                let now = s.r.now;
                for t in s.r.timers.iter_mut() {
                    if let Some(p) = t.period {
                        t.due = now + p;
                    }
                }
                delay_load_timers(&mut s, &load_timer_arms(text, evs)); // H1180
                let (_, _, bounds) = trace_windows(&self.hdb, text, evs, &seg.steps);
                let dur = bounds[1] - bounds[0];
                if self.wallclock && dur.is_finite() {
                    let t0 = s.now;
                    while s.now - t0 < (dur * 1000.0) as u64 && !s.r.menu_open {
                        s.tick(100);
                    }
                }
            }
            self.compare_steps(seg, &mut s, m0.take(), text, evs, &mut res);
            // the bot's live verdicts were made from the original's saves, not from the port's carried state
            res.steps.iter_mut().for_each(|st| st.bot_flag = None);
            self.end_check(seg, &s, &ids, &mut res);
            if load {
                res.unimpl = unimpl_lines(&s.r.log); // the core since this segment's Load
            }
            out.push(res);
            // H602: the bot's save after the last step (Esc menu → Save screen) moved the pointer off the world: the
            // hovered object got its Roll Out Of there, before the save was written (fo-065 208.291: 0x5902 → 0x11321's
            // posts) — the isolated runs start after it, the one session must run it too (outside every step window)
            if text.contains("\"bot: save:") {
                s.r.open_menu();
                // the pointer went to the menu's buttons: it left the inventory strip too (a later point in the strip
                // enters it from outside, H361 — fo-128 k0 / k1 after fo-127's camera click at (211,440))
                s.r.pointer_off();
                if load {
                    // H1583: the menu's queued work runs before the save is written (the clock stops, the queue does
                    // not, H404): the held object's removal at the Esc (fo-198 154.897 held_drop → the binoculars'
                    // Deactivation → OnBinoc mode 0: background 0x1ec7, nav-hotspots 0x6e60 / 0x6e62 by 154.903 — all
                    // in fo-198-end.x, written 189.75); the Load replaces the session
                    s.tick(0);
                    continue;
                }
                s.r.menu_choose(MenuChoice::Resume);
                settle(&mut s);
            }
        }
        out
    }
}

// ---- report --------------------------------------------------------------------------------------------------------

pub fn report_text(route: &Route, res: &[SegRes], chained: bool, continuous: bool) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    writeln!(
        s,
        "e2e ({}; Rules::all(); route tools/emu/e2e-route.json, main chain {})",
        if continuous {
            "CONTINUOUS: one session over the main chain, no save / Load"
        } else if chained {
            "CHAINED: one session over the main chain, the port's save → Load at each border"
        } else { "isolated: each segment from the original's start save" },
        route.chain.join(" > ")
    )
    .unwrap();
    writeln!(s, "{:<7} {:<5} {:>9} {:<10} {:<9} {:<26} first divergence", "seg", "chain", "ok/steps", "result", "end save", "start save").unwrap();
    for r in res {
        let seg = route.segments.iter().find(|x| x.name == r.name);
        let start = seg.map_or(String::new(), |x| x.save.trim_start_matches("data/bot/").to_string());
        if let Some(why) = &r.skipped {
            writeln!(s, "{:<7} {:<5} {:>9} {:<10} {:<9} {:<26} {why}", r.name, if r.main { "main" } else { "" }, "-", "SKIPPED", "-", start).unwrap();
            continue;
        }
        let end = match &r.end {
            None => "-".to_string(),
            Some(d) if d.is_empty() => "equal".to_string(),
            Some(d) => format!("{} diff", d.len()),
        };
        let fd = r.first_div().map_or(String::new(), |st| {
            let d = st.diffs.iter().find(|d| d.primary).unwrap();
            format!(
                "{}step {} ({} in View {}, t={:.1}): {}: original {} | port {}",
                match st.bot_flag {
                    Some(true) => "[bot flagged] ",
                    Some(false) => "[NEW: bot saw it as predicted] ",
                    None => "",
                },
                if st.index < 0 { "load".into() } else { st.index.to_string() },
                st.step,
                st.view,
                st.t,
                d.cat,
                d.expected,
                d.got
            )
        });
        writeln!(
            s,
            "{:<7} {:<5} {:>9} {:<10} {:<9} {:<26} {}",
            r.name,
            if r.main { "main" } else { "" },
            format!("{}/{}", r.prefix_ok(), r.n_steps()),
            if r.pass() { "PASS" } else if r.first_div().is_some() { "DIVERGE" } else { "END-DIFF" },
            end,
            start,
            fd
        )
        .unwrap();
    }
    let main: Vec<&SegRes> = res.iter().filter(|r| r.main).collect();
    let first_main = main.iter().find(|r| !r.pass());
    writeln!(
        s,
        "\nmain chain: {} of {} segments pass{}",
        main.iter().filter(|r| r.pass()).count(),
        main.len(),
        first_main.map_or(String::new(), |r| format!("; overall first divergence in {} (trace traces/{}.jsonl)", r.name, r.name))
    )
    .unwrap();
    let tot: usize = res.iter().map(|r| r.n_steps()).sum();
    let ok: usize = res.iter().map(|r| r.prefix_ok()).sum();
    let clean: usize = res.iter().map(|r| r.steps.iter().filter(|s| s.index >= 0 && !s.diverges()).count()).sum();
    writeln!(s, "steps: {ok} of {tot} reproduced before each segment's first divergence; {clean} of {tot} steps match individually").unwrap();
    // story checkpoints (rows of the private checkpoint list, when present): is the run that produced the checkpoint
    // save covered, does the port's end state equal it, and is the run itself free of divergences
    if let Ok(cp) = std::fs::read_to_string(repo().join("tools/emu/CHECKPOINTS.md")) {
        let rows: Vec<String> = cp
            .lines()
            .filter_map(|l| l.strip_prefix("| fo-").and_then(|r| r.split_once("-end.x")).map(|(n, _)| format!("fo-{n}")))
            .collect();
        let cell = |n: &str| -> String {
            match res.iter().find(|r| r.name == n) {
                None => "not run".into(),
                Some(r) if r.skipped.is_some() => "skipped".into(),
                Some(r) => format!(
                    "{}{}",
                    match &r.end {
                        Some(e) if e.is_empty() => "end equal",
                        Some(_) => "END DIFFERS",
                        None => "no end save",
                    },
                    if r.first_div().is_some() { format!(", diverges at step {}", r.first_div().unwrap().index) } else { String::new() }
                ),
            }
        };
        let eq = rows.iter().filter(|n| res.iter().any(|r| &r.name == *n && r.end.as_ref().is_some_and(|e| e.is_empty()))).count();
        writeln!(s, "checkpoints (CHECKPOINTS.md): {eq} of {} end saves reproduced by the port", rows.len()).unwrap();
        for n in &rows {
            writeln!(s, "  {n}-end.x: {}", cell(n)).unwrap();
        }
    }
    s
}

pub fn report_details(res: &[SegRes]) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    for r in res {
        if r.skipped.is_some() {
            continue;
        }
        writeln!(s, "== {} (traces/{}.jsonl)", r.name, r.name).unwrap();
        for st in &r.steps {
            for d in &st.diffs {
                writeln!(
                    s,
                    "  {:>4} {:<14} {:<5} {}{:<18} original {} | port {}",
                    if st.index < 0 { "load".into() } else { st.index.to_string() },
                    st.step,
                    st.view,
                    if d.primary { "* " } else { "  " },
                    d.cat,
                    d.expected,
                    d.got
                )
                .unwrap();
            }
        }
        if let Some(e) = &r.end {
            for l in e {
                writeln!(s, "  end  {l}").unwrap();
            }
            for l in &r.end_noise {
                writeln!(s, "  end  (noise) {l}").unwrap();
            }
        }
        for l in &r.save {
            writeln!(s, "  save {l}").unwrap();
        }
    }
    s
}

pub fn report_json(res: &[SegRes]) -> J {
    J::Array(
        res.iter()
            .map(|r| {
                json!({
                    "name": r.name, "main": r.main, "skipped": r.skipped, "pass": r.pass(),
                    "steps": r.n_steps(), "prefix_ok": r.prefix_ok(),
                    "first_divergence": r.first_div().map(|st| json!({"bot_flag": st.bot_flag, "index": st.index, "step": st.step, "view": st.view, "t": st.t,
                        "diffs": st.diffs.iter().filter(|d| d.primary).map(|d| json!({"cat": d.cat, "original": d.expected, "port": d.got})).collect::<Vec<_>>()})),
                    "first_secondary": r.first_secondary().map(|st| st.index),
                    "end": r.end, "end_noise": r.end_noise,
                })
            })
            .collect(),
    )
}

/// Baseline for the regression test (`tests/e2e.rs`): per segment the steps reproduced before the first divergence and
/// whether the end state equals the original's end save
pub fn baseline_json(res: &[SegRes]) -> String {
    let mut out = String::from("{\n");
    let rows: Vec<String> = res
        .iter()
        .filter(|r| r.skipped.is_none())
        .map(|r| {
            format!(
                "  \"{}\": {{\"prefix_ok\": {}, \"steps\": {}, \"end_equal\": {}}}",
                r.name,
                r.prefix_ok(),
                r.n_steps(),
                r.end.as_ref().map_or("null".to_string(), |e| e.is_empty().to_string())
            )
        })
        .collect();
    out.push_str(&rows.join(",\n"));
    out.push_str("\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// explore.py's `n:` / xfplan's `q:` steps replay (fo-158 / fo-159 were "unparsable step")
    #[test]
    fn quick_steps_parse() {
        assert!(matches!(parse_step("n:341,206"), Some(St::Quick(341, 206))));
        assert!(matches!(parse_step("q:20,440"), Some(St::QuickMove(20, 440))));
        assert!(parse_step("n:341").is_none());
        assert!(parse_step("z:1,2").is_none());
    }
}
