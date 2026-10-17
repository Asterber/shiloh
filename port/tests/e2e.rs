//! End-to-end regression guard: the bot's whole playthrough route on the ORIGINAL
//! (`tools/emu/e2e-route.json`) replayed on the headless core, every segment from its start save (`xf::e2e`). For every
//! segment of `tools/emu/e2e-baseline.json` the steps reproduced before the first divergence must not drop, and an end
//! state that equalled the original's end save must still equal it. Skips without `data/` (game files, bot saves) and
//! without the route and baselines, which are private research inputs (not in this repository).
//!
//! Refresh the baseline after a core change that moves a divergence later (never to hide a regression):
//!   cargo run --release --no-default-features --bin e2e -- --baseline-out tools/emu/e2e-baseline.json
use serde_json::Value as J;
use xf::e2e::{load_route, repo, Ctx};

#[test]
fn e2e_route_prefix_does_not_regress() {
    let root = repo();
    let Ok(route) = load_route(&root.join("tools/emu/e2e-route.json")) else {
        eprintln!("skipping: tools/emu/e2e-route.json not present (the bot route is not in the public tree)");
        return;
    };
    let Some(ctx) = Ctx::open() else { return };
    let base: J = serde_json::from_str(&std::fs::read_to_string(root.join("tools/emu/e2e-baseline.json")).unwrap()).unwrap();
    let mut bad = Vec::new();
    let mut checked = 0;
    for seg in &route.segments {
        let Some(b) = base.get(&seg.name) else { continue };
        let r = ctx.run_segment(seg);
        if r.skipped.is_some() {
            continue; // a save / trace missing on this machine
        }
        checked += 1;
        let want = b["prefix_ok"].as_u64().unwrap() as usize;
        if r.prefix_ok() < want {
            let d = r.first_div().map(|s| format!("{s:?}")).unwrap_or_default();
            bad.push(format!("{}: {} steps before the first divergence, baseline {want}; {d}", seg.name, r.prefix_ok()));
        }
        if b["end_equal"] == J::Bool(true) && !r.end.as_ref().is_some_and(|e| e.is_empty()) {
            bad.push(format!("{}: end state no longer equals the original's end save: {:?}", seg.name, r.end));
        }
    }
    assert!(bad.is_empty(), "e2e regressions ({checked} segments checked):\n{}", bad.join("\n"));
}

/// H1580 (`--chained`): the main chain in ONE session from the first segment's start save — every border a port save →
/// in-game Load of that save (as the bot did on the original), never a save of the original. The leading segments that
/// pass (no divergence, end state = the original's end save) must not drop below `tools/emu/e2e-chained-baseline.json`,
/// nor the steps in that prefix. Refresh from `e2e --main --chained` (the leading PASS rows and their steps).
#[test]
fn e2e_chained_prefix_does_not_regress() {
    let root = repo();
    let Ok(route) = load_route(&root.join("tools/emu/e2e-route.json")) else {
        eprintln!("skipping: tools/emu/e2e-route.json not present (the bot route is not in the public tree)");
        return;
    };
    let Some(ctx) = Ctx::open() else { return };
    let base: J =
        serde_json::from_str(&std::fs::read_to_string(root.join("tools/emu/e2e-chained-baseline.json")).unwrap()).unwrap();
    let res = ctx.run_chained(&route, true);
    if res.is_empty() || res.iter().any(|r| r.skipped.is_some()) {
        return; // a save / trace missing on this machine
    }
    let lead: Vec<_> = res.iter().take_while(|r| r.pass()).collect();
    let steps: usize = lead.iter().map(|r| r.n_steps()).sum();
    let (want_segs, want_steps) = (base["segments"].as_u64().unwrap() as usize, base["steps"].as_u64().unwrap() as usize);
    let first = res.iter().find(|r| !r.pass()).map(|r| {
        let d = r.first_div().map(|s| format!("{s:?}")).unwrap_or_else(|| format!("end {:?}", r.end));
        format!("{}: {d}", r.name)
    });
    assert!(
        lead.len() >= want_segs && steps >= want_steps,
        "chained prefix {} segments / {steps} steps, baseline {want_segs} / {want_steps}; first failing {first:?}",
        lead.len()
    );
}
