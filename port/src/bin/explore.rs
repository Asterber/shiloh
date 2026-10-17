//! explore — pick the bot's next click on the ORIGINAL from the port's runtime state (for a private research driver).
//!
//!   explore (--load SAVE.x | --new-game) --steps "p:X,Y;o:OWNER;h:X,Y,OWNER;..." [--uneval T.jsonl]... [--visited 2b3,2c9,...] [--memo FILE]
//!
//! The core (every hypothesis on, `front::rules_hyp`) loads SAVE (H65), then replays the steps already made on the
//! original: `p:X,Y` = hover + click at a screen point (topmost clickable, A4), `o:OWNER` = click an owner without a
//! region (dialogue EmotionIcon), `h:X,Y,OWNER` = a click at X,Y whose target OWNER is pre-registered by hand (a hit
//! order the core does not model, e.g. H121), `x:X,Y` = as `p:` but pre-registered by hand (the bot clicks it even when
//! the core offers no clickable there, e.g. a point expected to hit nothing), `m:X,Y` = pointer moved there, no click
//! (H170 hover), `r:X,Y` = right click (H172). After the load and after every step the session settles until nothing is queued or
//! playing and no one-shot timer runs (H59) — the bot waits the same way on the original.
//!
//! Candidates = what the RUNTIME offers now (C1–C3, run.rs `clickables`): active nav-hotspots of the current View and
//! active owners with a region, at a point where the owner is topmost; plus active EmotionIcons (no region; position
//! assumed from fo-017/fo-019: icon k of the creation order at client (220 + 100·k, 370) — hypothesis). Each candidate is
//! simulated (the whole prefix is rebuilt, then the click, then the settle) to learn what the core predicts: new
//! transitions (View, kind), posts, settle time. No static graph search: the only edges known are those simulated from
//! a runtime state, kept in the memo file (View → owner → result) across calls.
//!
//! Policy (first match): (1) a candidate whose simulated click enters a View not in `--visited` nor visited in this run;
//! (2) a candidate of this View not clicked yet in this run whose click posts something or makes a transition;
//! (3) the first step of the shortest path over memo edges to a View that still has an untried productive candidate or
//! one leading to an unvisited View; (4) nothing — prints "choice": null.
//!
//! Output: one JSON object on stdout (view, transitions so far, candidates with predictions, choice).
use serde_json::{json, Value as J};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use xf::front::{self, Session};
use xf::hdb::Hdb;
use xf::replay::parse_trace;
use xf::world::action::{be32, Action, ProcCall};
use xf::world::class;
use xf::world::run::Runner;

#[derive(Clone, Debug)]
enum St {
    Pt(i32, i32),
    Owner(u32),
    /// `h:X,Y,OWNER` — a click at a screen point whose target is given by hand (a pre-registered hit-order hypothesis
    /// the core does not have, e.g. H121); the core hovers and clicks OWNER, the bot clicks X,Y on the original
    Hand(i32, i32, u32),
    /// `m:X,Y` — pointer moved to X,Y, no click (hover effects only, e.g. H170's inventory panel)
    Move(i32, i32),
    /// `r:X,Y` — right click at X,Y (H172: Double Click event 1 on the hit owner)
    Right(i32, i32),
}

fn parse_steps(s: &str) -> Vec<St> {
    s.split(';')
        .filter(|x| !x.trim().is_empty())
        .map(|x| {
            let x = x.trim();
            if let Some(p) = x.strip_prefix("m:").or_else(|| x.strip_prefix("r:")) {
                let (a, b) = p.split_once(',').expect("m:X,Y / r:X,Y");
                let (a, b) = (a.parse().unwrap(), b.parse().unwrap());
                if x.starts_with("m:") { St::Move(a, b) } else { St::Right(a, b) }
            } else if let Some(p) = x.strip_prefix("p:").or_else(|| x.strip_prefix("x:")) {
                let (a, b) = p.split_once(',').expect("p:X,Y");
                St::Pt(a.parse().unwrap(), b.parse().unwrap())
            } else if let Some(h) = x.strip_prefix("h:") {
                let v: Vec<&str> = h.split(',').collect();
                assert!(v.len() == 3, "h:X,Y,OWNER");
                St::Hand(v[0].parse().unwrap(), v[1].parse().unwrap(), u32::from_str_radix(v[2].trim_start_matches("0x"), 16).unwrap())
            } else if let Some(o) = x.strip_prefix("o:") {
                St::Owner(u32::from_str_radix(o.trim_start_matches("0x"), 16).unwrap())
            } else {
                panic!("step {x}")
            }
        })
        .collect()
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).unwrap_or_else(|_| panic!("hex {s}"))
}

/// Settle like the bot waits on the original: nothing queued or playing, no one-shot timer. Returns game ms spent.
fn settle(s: &mut Session) -> u64 {
    let t0 = s.now;
    front::settle_timers(s, 100, 900_000);
    s.now - t0
}

fn apply(s: &mut Session, st: &St) -> bool {
    match *st {
        St::Pt(x, y) => {
            s.hover_at(x, y);
            s.click_at(x, y).is_some()
        }
        St::Owner(o) => s.r.click(o),
        St::Hand(x, y, o) => {
            s.mouse = Some((x, y));
            s.r.hover(Some(o));
            s.r.click(o)
        }
        St::Move(x, y) => {
            s.hover_at(x, y);
            true
        }
        St::Right(x, y) => {
            s.hover_at(x, y);
            s.r.right_click(x, y).is_some()
        }
    }
}

struct Ctx<'a> {
    h: &'a Hdb,
    gam: &'a Hdb,
    data: PathBuf,
    uneval: HashMap<u32, bool>,
    /// `Rules::h89_queue_model` for this context (`--h89`, or the second simulation of `--h89cmp`)
    h89: bool,
    /// append the step's executed actions (bit 31 set) to `posted` (for the H89 / H76 order comparison)
    h89cmp_execs: bool,
    /// `--new-game`: start as the menu's New Game does (`Runner::new_game` on XFiles.gam) instead of loading a save
    new_game: bool,
}

impl<'a> Ctx<'a> {
    /// Load + the steps; returns the session and, per step, (view before, transitions made, posts, settle ms, ok).
    fn build(&self, steps: &[St]) -> (Session<'a>, Vec<(Option<u32>, Vec<(u32, u32)>, usize, u64, bool)>) {
        let (s, per, _, _) = self.build_k1(steps);
        (s, per)
    }

    /// as `build`, plus per step the conditions the core decided in every trigger firing that has a condition with an
    /// operand of kind 1 (trigger-local, H73): (action, trigger, value); `disc` = the kind-1 conditions whose value
    /// with every local read as 0 (the "always 0" alternative to H73; kind-0 side read after the step) differs
    #[allow(clippy::type_complexity)]
    fn build_k1(
        &self,
        steps: &[St],
    ) -> (
        Session<'a>,
        Vec<(Option<u32>, Vec<(u32, u32)>, usize, u64, bool)>,
        (Vec<Vec<(u32, u32, Option<bool>)>>, Vec<Vec<u32>>, Vec<Vec<(u32, u32, bool)>>),
        Vec<Vec<String>>,
    ) {
        let mut rules = front::rules_hyp();
        rules.h89_queue_model = self.h89;
        let mut s = Session::with_rules(self.h, self.gam, &self.data, rules);
        s.r.uneval = self.uneval.clone();
        if self.new_game {
            s.r.new_game();
        } else {
            s.load_game();
        }
        settle(&mut s);
        let mut per = Vec::new();
        let mut k1 = Vec::new();
        let mut posted = Vec::new();
        let mut disc = Vec::new();
        let mut evs = Vec::new();
        for st in steps {
            let (v0, t0, p0, c0) = (s.r.w.view, s.r.transitions.len(), s.r.posts.len(), s.r.conds.len());
            let (e0, r0, l0) = (s.r.execs.len(), s.r.regs.len(), s.r.lifecycle.len());
            let ok = apply(&mut s, st);
            let ms = settle(&mut s);
            per.push((v0, s.r.transitions[t0..].to_vec(), s.r.posts.len() - p0, ms, ok));
            // every condition of a trigger firing that has a kind-1 condition (the writes of the locals are conditioned
            // by the other ones)
            let is_k1 = |a: u32| {
                Action::load(self.h, a).and_then(|x| x.condition()).map_or(false, |c| c.lhs_kind == 1 || c.rhs_kind == 1)
            };
            let cs = &s.r.conds[c0..];
            let trigs: HashSet<u32> = cs.iter().filter(|c| is_k1(c.0)).map(|c| c.1).collect();
            k1.push(cs.iter().copied().filter(|c| trigs.contains(&c.1)).collect());
            disc.push(
                cs.iter()
                    .filter(|c| is_k1(c.0))
                    .filter_map(|c| {
                        let o = Action::load(self.h, c.0)?.condition()?;
                        let z = s.r.w.vars.condition_in(&o, Some(u32::MAX)); // no locals stored for this id → all 0
                        (z.is_some() && c.2.is_some() && z != c.2).then_some((c.0, c.1, c.2.unwrap()))
                    })
                    .collect(),
            );
            posted.push(s.r.posts[p0..].iter().map(|m| m.action).collect::<Vec<u32>>());
            if self.h89cmp_execs {
                posted.last_mut().unwrap().extend(s.r.execs[e0..].iter().map(|&a| a | 0x8000_0000));
            }
            let mut ev = step_events(self.h, &s, e0, r0, c0, p0);
            ev.extend(s.r.lifecycle[l0..].iter().map(|&(o, e)| format!("{}:{o:x}", if e == xf::world::event::ACTIVATION { "act" } else { "deact" })));
            evs.push(ev);
        }
        (s, per, (k1, posted, disc), evs)
    }
}

/// What the core did in one step, for route search (fo-025): executed actions of the types the open hypotheses are
/// about (type 5 layouts H58, type 6 procedures H96/H97, type 1/7 plays with mode H100, type 3 Enables H87, type 2
/// timers), registrations, Activations / Deactivations (`act:` / `deact:`, in order, at the end), kind-4 conditions (H90) and the triggers that posted.
fn step_events(h: &Hdb, s: &Session, e0: usize, r0: usize, c0: usize, p0: usize) -> Vec<String> {
    let mut ev = Vec::new();
    for &a in &s.r.execs[e0..] {
        let Some(x) = Action::load(h, a) else { continue };
        let b = x.body();
        let hexb = |b: &[u8]| b.iter().map(|c| format!("{c:02x}")).collect::<String>();
        match x.kind() {
            5 => ev.push(format!("t5:{a:x}:{:x}:{}", be32(b, 0).unwrap_or(0), b.get(4).copied().unwrap_or(0))),
            6 => {
                if let Some(p) = ProcCall::parse(b) {
                    let args: Vec<String> = p.args.iter().map(|v| format!("{v:x}")).collect();
                    ev.push(format!("proc:{a:x}:{}({})", p.name, args.join(",")));
                }
            }
            1 | 7 => ev.push(format!("play{}:{a:x}:{:x}:{}", x.kind(), be32(b, 0).unwrap_or(0), b.get(4).copied().unwrap_or(1))),
            2 => ev.push(format!("t2:{a:x}:{}", hexb(b))),
            3 => ev.push(format!("t3:{a:x}:{}", hexb(b))),
            _ => {}
        }
    }
    for &o in &s.r.regs[r0..] {
        ev.push(format!("reg:{o:x}"));
    }
    for c in &s.r.conds[c0..] {
        let k4 = Action::load(h, c.0).and_then(|x| x.condition()).map_or(false, |o| o.lhs_kind == 4 || o.rhs_kind == 4);
        if k4 {
            ev.push(format!("k4:{:x}@{:x}={:?}", c.0, c.1, c.2));
        }
    }
    let mut trigs: Vec<u32> = Vec::new();
    for m in &s.r.posts[p0..] {
        if !trigs.contains(&m.trigger) {
            trigs.push(m.trigger);
        }
    }
    ev.extend(trigs.iter().map(|t| format!("trig:{t:x}")));
    ev
}

fn candidates(s: &Session) -> Vec<(u32, u32, u32, [i16; 4], St)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for c in s.r.clickables() {
        if !seen.insert(c.owner) {
            continue;
        }
        if let Some((x, y)) = front::click_point(s, c.owner) {
            // aim at the middle of the part of the rect where the owner is topmost: scan the centre row first
            let [x1, y1, x2, y2] = c.rect.map(|v| v as i32);
            let (cx, cy) = ((x1 + x2) / 2, (y1 + y2) / 2);
            let pt = if s.r.hit(cx, cy).map(|h| h.owner) == Some(c.owner) { (cx, cy) } else { (x, y) };
            out.push((c.owner, c.cls, c.kind, c.rect, St::Pt(pt.0, pt.1)));
        }
    }
    let h = s.r.w.h;
    let icons: Vec<u32> = s.r.regs.iter().copied().filter(|&o| h.has(class::EMOTION_ICON, o)).fold(Vec::new(), |mut v, o| {
        if !v.contains(&o) {
            v.push(o);
        }
        v
    });
    for (k, &o) in icons.iter().enumerate() {
        if s.r.active.contains(&o) {
            out.push((o, class::EMOTION_ICON, 0, [0; 4], St::Owner(o)));
            let _ = k;
        }
    }
    out
}

/// the current View's nav-hotspots the runtime offers (owner = VCNav), for the bot's wait after a travel (fo-031: the
/// original builds and activates them ~170 s after the click)
fn nav_hotspots(s: &Session) -> Vec<String> {
    let mut v: Vec<String> = s.r.clickables().iter().filter(|c| c.cls == class::NAV).map(|c| format!("{:x}", c.owner)).collect();
    v.dedup();
    v
}

fn icon_point(s: &Session, owner: u32) -> Option<(i32, i32)> {
    let h = s.r.w.h;
    let mut icons: Vec<u32> = Vec::new();
    for &o in &s.r.regs {
        if h.has(class::EMOTION_ICON, o) && !icons.contains(&o) {
            icons.push(o);
        }
    }
    icons.iter().position(|&o| o == owner).map(|k| (220 + 100 * k as i32, 370))
}

fn main() {
    let mut save = None;
    let mut steps = Vec::new();
    let mut unevals = Vec::new();
    let mut visited: HashSet<u32> = HashSet::new();
    let mut memo_path: Option<PathBuf> = None;
    // H1241: the queue model of the rule set (H89 on); `--h76` = the H76 runner
    let (mut h89, mut h89cmp, mut new_game) = (front::rules_hyp().h89_queue_model, false, false);
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut v = || it.next().expect("value");
        match a.as_str() {
            "--load" => save = Some(v()),
            "--steps" => steps = parse_steps(&v()),
            "--uneval" => unevals.push(v()),
            "--visited" => visited.extend(v().split(',').filter(|x| !x.is_empty()).map(hex)),
            "--memo" => memo_path = Some(v().into()),
            "--h89" => h89 = true,
            "--h76" => h89 = false,
            "--h89cmp" => h89cmp = true,
            "--new-game" => new_game = true,
            _ => panic!("unknown argument {a}"),
        }
    }
    // XF_DATA: a folder, or disc images / several sources as a path list (xf::vfs)
    let mut src = xf::vfs::env_sources("XF_DATA");
    if src.is_empty() {
        src.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame"));
    }
    let data = xf::vfs::data_root_or_exit("explore", &src);
    let h = Hdb::open(data.join("XFiles.hdb")).expect("hdb");
    let gam = match (&save, new_game) {
        (Some(p), false) => Hdb::open(Path::new(p)).expect("save"),
        (None, true) => Hdb::open(data.join("XFiles.gam")).expect("XFiles.gam"),
        _ => panic!("give --load SAVE or --new-game"),
    };
    let mut uneval = HashMap::new();
    for f in &unevals {
        let Ok(t) = std::fs::read_to_string(f) else { continue };
        let t = &t[..t.rfind('\n').map(|i| i + 1).unwrap_or(0)]; // a live trace may end with a partial line
        if let Ok(tr) = parse_trace(t) {
            uneval.extend(Runner::uneval_from_trace(&tr, &h)); // later files (the live trace) win
        }
    }
    let ctx = Ctx { h: &h, gam: &gam, data: data.clone(), uneval: uneval.clone(), h89, h89cmp_execs: h89cmp, new_game };
    // the other queue model (H89 vs the H76 runner), only for `--h89cmp`
    let alt = Ctx { h: &h, gam: &gam, data, uneval, h89: !h89, h89cmp_execs: true, new_game };
    let (s, per, (k1, posted, disc), evs) = ctx.build_k1(&steps);
    let view = s.r.w.view;
    let mut run_views: HashSet<u32> = s.r.transitions.iter().map(|t| t.0).collect();
    run_views.extend(view);
    let mut memo: serde_json::Map<String, J> = memo_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    // owners clicked in this run, per View (from the replayed prefix)
    let mut tried: HashSet<(u32, u32)> = HashSet::new();
    {
        let (mut s2, _) = ctx.build(&[]);
        for st in &steps {
            let v = s2.r.w.view.unwrap_or(0);
            let o = match *st {
                St::Pt(x, y) | St::Right(x, y) => s2.r.hit(x, y).map(|c| c.owner),
                St::Move(..) => None,
                St::Owner(o) | St::Hand(_, _, o) => Some(o),
            };
            if let Some(o) = o {
                tried.insert((v, o));
            }
            apply(&mut s2, st);
            settle(&mut s2);
        }
    }
    let mut cands = Vec::new();
    for (owner, cls, kind, rect, st) in candidates(&s) {
        let mut all = steps.clone();
        all.push(st.clone());
        let (s2, per2, (_, posted2, disc2), evs2) = ctx.build_k1(&all);
        // H89 vs H76: the posts + executed actions of this click in the other queue model, if they differ
        let h89d: Option<J> = h89cmp.then(|| {
            let (_, _, (_, posted3, _), _) = alt.build_k1(&all);
            let (a, b) = (posted2.last().cloned().unwrap_or_default(), posted3.last().cloned().unwrap_or_default());
            let f = |v: &[u32]| v.iter().map(|x| if x & 0x8000_0000 != 0 { format!("x{:x}", x & 0x7fff_ffff) } else { format!("{x:x}") }).collect::<Vec<_>>();
            if a == b { J::Null } else { json!({"this": f(&a), "other": f(&b)}) }
        });
        let cev = evs2.last().cloned().unwrap_or_default();
        let (_, trans, posts, ms, ok) = per2.last().cloned().unwrap();
        let cdisc: Vec<String> = disc2.last().map_or(Vec::new(), |d| d.iter().map(|c| format!("{:x}@{:x}={}", c.0, c.1, c.2)).collect());
        let dest = s2.r.w.view;
        let pt = match st {
            St::Pt(x, y) | St::Hand(x, y, _) | St::Move(x, y) | St::Right(x, y) => (x, y),
            St::Owner(o) => icon_point(&s, o).unwrap_or((0, 0)),
        };
        cands.push((owner, cls, kind, rect, st, pt, trans, posts, ms, ok, dest, cdisc, cev, h89d, nav_hotspots(&s2)));
    }
    // memo: this View's simulated edges
    if let Some(v) = view {
        let mut m = serde_json::Map::new();
        for c in &cands {
            m.insert(format!("{:x}", c.0), json!({"dest": c.10.map(|d| format!("{d:x}")), "posts": c.7,
                "trans": c.6.len(), "tried": tried.contains(&(v, c.0))}));
        }
        memo.insert(format!("{v:x}"), J::Object(m));
    }
    let new_view = |d: Option<u32>| d.map_or(false, |d| !visited.contains(&d) && !run_views.contains(&d));
    #[allow(clippy::type_complexity)]
    let productive = |c: &(u32, u32, u32, [i16; 4], St, (i32, i32), Vec<(u32, u32)>, usize, u64, bool, Option<u32>, Vec<String>, Vec<String>, Option<J>, Vec<String>)| {
        c.9 && (c.7 > 0 || !c.6.is_empty())
    };
    let v0 = view.unwrap_or(0);
    let mut choice: Option<(usize, String)> = None;
    if let Some(i) = cands.iter().position(|c| c.9 && !c.6.is_empty() && new_view(c.10) && !tried.contains(&(v0, c.0))) {
        choice = Some((i, "enters a View not visited before".into()));
    }
    if choice.is_none() {
        // prefer nav-hotspots / items that move (transition) over in-place ones
        let mut best: Option<usize> = None;
        for (i, c) in cands.iter().enumerate() {
            if productive(c) && !tried.contains(&(v0, c.0)) {
                if best.map_or(true, |b| cands[b].6.is_empty() && !c.6.is_empty()) {
                    best = Some(i);
                }
            }
        }
        if let Some(i) = best {
            choice = Some((i, "not clicked yet in this View, the core predicts an effect".into()));
        }
    }
    if choice.is_none() {
        // BFS over memo edges (simulated from runtime states) to a View with work left
        let edges = |v: &str| -> Vec<(String, String)> {
            memo.get(v)
                .and_then(|m| m.as_object())
                .map(|m| {
                    m.iter()
                        .filter_map(|(o, r)| r["dest"].as_str().map(|d| (o.clone(), d.to_string())))
                        .filter(|(_, d)| d != v)
                        .collect()
                })
                .unwrap_or_default()
        };
        let work = |v: &str| -> bool {
            memo.get(v).and_then(|m| m.as_object()).map_or(false, |m| {
                m.values().any(|r| {
                    !r["tried"].as_bool().unwrap_or(false) && (r["posts"].as_u64().unwrap_or(0) > 0 || r["trans"].as_u64().unwrap_or(0) > 0)
                        || r["dest"].as_str().map_or(false, |d| !visited.contains(&hex(d)) && !run_views.contains(&hex(d)))
                })
            })
        };
        let start = format!("{v0:x}");
        let mut prev: HashMap<String, (String, String)> = HashMap::new();
        let mut q = VecDeque::from([start.clone()]);
        let mut seen = HashSet::from([start.clone()]);
        let mut goal = None;
        while let Some(v) = q.pop_front() {
            if v != start && work(&v) {
                goal = Some(v);
                break;
            }
            for (o, d) in edges(&v) {
                if seen.insert(d.clone()) {
                    prev.insert(d.clone(), (v.clone(), o));
                    q.push_back(d);
                }
            }
        }
        if let Some(g) = goal {
            let mut cur = g.clone();
            let mut first = None;
            while let Some((p, o)) = prev.get(&cur) {
                first = Some(o.clone());
                if *p == start {
                    break;
                }
                cur = p.clone();
            }
            if let Some(o) = first {
                let o = hex(&o);
                if let Some(i) = cands.iter().position(|c| c.0 == o && c.9) {
                    choice = Some((i, format!("path towards View {g} (memo edges)")));
                }
            }
        }
    }
    if let Some(p) = &memo_path {
        let _ = std::fs::write(p, serde_json::to_string_pretty(&J::Object(memo.clone())).unwrap());
    }
    let cj: Vec<J> = cands
        .iter()
        .map(|c| {
            json!({"owner": format!("{:x}", c.0), "cls": format!("{:x}", c.1), "kind": format!("{:x}", c.2),
                "rect": c.3, "pt": [c.5.0, c.5.1], "by": if matches!(c.4, St::Owner(_)) {"owner"} else {"point"},
                "step": match c.4 { St::Pt(x, y) => format!("p:{x},{y}"), St::Owner(o) => format!("o:{o:x}"),
                    St::Hand(x, y, o) => format!("h:{x},{y},{o:x}"), St::Move(x, y) => format!("m:{x},{y}"),
                    St::Right(x, y) => format!("r:{x},{y}") },
                "trans": c.6.iter().map(|t| format!("{:x}/{}", t.0, t.1)).collect::<Vec<_>>(),
                "posts": c.7, "settle_ms": c.8, "ok": c.9, "dest": c.10.map(|d| format!("{d:x}")),
                "tried": tried.contains(&(v0, c.0)), "k1_disc": c.11, "ev": c.12, "h89_disc": c.13.clone().unwrap_or(J::Null),
                "hot": c.14})
        })
        .collect();
    let out = json!({
        "view": view.map(|v| format!("{v:x}")),
        "hot": nav_hotspots(&s),
        "transitions": s.r.transitions.iter().map(|t| format!("{:x}/{}", t.0, t.1)).collect::<Vec<_>>(),
        "steps": per.iter().map(|p| json!({"view": p.0.map(|v| format!("{v:x}")),
            "trans": p.1.iter().map(|t| format!("{:x}/{}", t.0, t.1)).collect::<Vec<_>>(), "posts": p.2,
            "settle_ms": p.3, "ok": p.4})).collect::<Vec<_>>(),
        "k1_conds": k1.iter().map(|v| v.iter().map(|c| format!("{:x}@{:x}={:?}", c.0, c.1, c.2)).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "k1_disc": disc.iter().map(|v| v.iter().map(|c| format!("{:x}@{:x}={}", c.0, c.1, c.2)).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "ev": evs,
        "posted": posted.iter().map(|v| v.iter().map(|a| format!("{a:x}")).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "candidates": cj,
        "choice": choice.map(|(i, why)| json!({"index": i, "why": why})),
        "unimplemented": s.r.log.iter().filter(|l| l.contains("not implemented") || l.contains("unknown")).cloned().collect::<Vec<_>>(),
    });
    println!("{out}");
}
