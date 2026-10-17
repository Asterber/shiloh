//! e2e — replay the bot's whole playthrough route (recorded on the ORIGINAL) on the headless core and report the first
//! divergence per segment and overall (see `xf::e2e`). Needs the private research inputs (the bot route and the
//! original's saves), which are not in this repository.
//!
//!   e2e [--route ROUTE.json] [--only fo-054,fo-056] [--main] [--chained] [--continuous] [--pairs] [--details] [--unimpl] [--no-wallclock] [--settle] [--json OUT.json] [--baseline-out BASELINE.json]
//!
//! --main: only the main story chain; --chained: the main chain in one session, the port's own save → Load at every
//! segment border as the bot did on the original (H1580); --continuous: the main chain in one session without any save /
//! Load (the port's whole runtime state crosses the borders); --pairs: each border of the main chain alone, the earlier
//! segment from the original's save, the next one in the same session without a Load (H1590);
//! --details: every per-step difference (primary marked `*`) and every end-save difference; --no-wallclock: do not run
//! the core's clock on to the original's step length; --settle: settle after each step before running the clock on
//! (xfplan's timing; default: the core runs exactly the original's step length); --baseline-out: write the regression baseline of `tests/e2e.rs`
//! (isolated mode, every segment).
use std::path::PathBuf;
use xf::e2e::{load_route, report_details, report_json, report_text, repo, Ctx};

fn main() {
    let mut wallclock = true;
    let mut timed = true;
    let mut baseline: Option<PathBuf> = None;
    let mut route_path = repo().join("tools/emu/e2e-route.json");
    let (mut only, mut main_only, mut chained, mut details, mut json) = (Vec::<String>::new(), false, false, false, None::<PathBuf>);
    let mut unimpl = false;
    let mut continuous = false;
    let mut pairs = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--route" => route_path = it.next().expect("--route PATH").into(),
            "--only" => only = it.next().expect("--only LIST").split(',').map(String::from).collect(),
            "--main" => main_only = true,
            "--chained" => chained = true,
            "--continuous" => (chained, continuous) = (true, true),
            "--pairs" => (chained, continuous, pairs) = (true, true, true),
            "--details" => details = true,
            "--unimpl" => unimpl = true,
            "--no-wallclock" => wallclock = false,
            "--settle" => timed = false,
            "--json" => json = Some(it.next().expect("--json PATH").into()),
            "--baseline-out" => baseline = Some(PathBuf::from(it.next().expect("--baseline-out PATH"))),
            _ => {
                eprintln!("usage: e2e [--route PATH] [--only fo-NNN,..] [--main] [--chained] [--continuous] [--pairs] [--details] [--json OUT]");
                std::process::exit(2);
            }
        }
    }
    let route = load_route(&route_path).unwrap_or_else(|e| {
        eprintln!("e2e: {e} (the route is a private research input, not in this repository)");
        std::process::exit(2)
    });
    let Some(mut ctx) = Ctx::open() else {
        eprintln!("e2e: data/InstalledGame/XFiles.hdb missing");
        std::process::exit(2)
    };
    ctx.wallclock = wallclock;
    ctx.timed = timed;
    let res = if pairs {
        ctx.run_pairs(&route)
    } else if chained {
        ctx.run_chained(&route, !continuous)
    } else {
        route
            .segments
            .iter()
            .filter(|s| (only.is_empty() || only.contains(&s.name)) && (!main_only || s.main))
            .map(|s| ctx.run_segment(s))
            .collect()
    };
    print!("{}", report_text(&route, &res, chained, continuous));
    if unimpl {
        // what the core logged as unimplemented along the route, by reason
        let mut by: std::collections::BTreeMap<String, (usize, Vec<String>)> = Default::default();
        for r in &res {
            for l in &r.unimpl {
                let what = l.splitn(2, ": ").nth(1).unwrap_or(l);
                let key: String = what.chars().take_while(|c| *c != '[').collect();
                let e = by.entry(key).or_default();
                e.0 += 1;
                if !e.1.contains(&r.name) {
                    e.1.push(r.name.clone());
                }
            }
        }
        let mut v: Vec<_> = by.into_iter().collect();
        v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        println!("\nunimplemented along the route ({} reasons):", v.len());
        for (k, (n, segs)) in v {
            println!("{n:5}  {k}  [{}]", segs.join(","));
        }
    }
    if details {
        print!("\n{}", report_details(&res));
    }
    if let Some(p) = baseline {
        std::fs::write(&p, xf::e2e::baseline_json(&res)).expect("write baseline");
    }
    if let Some(p) = json {
        std::fs::write(&p, serde_json::to_string_pretty(&report_json(&res)).unwrap()).expect("write json");
    }
}
