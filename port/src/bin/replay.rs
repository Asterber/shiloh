//! replay — run the headless core against a trace of the original and print match counts.
//!
//!   replay [--hyp] [--h89 | --h76] [--details] [--load SAVE.x] <trace.jsonl>...
//!
//! Data: data/InstalledGame/XFiles.hdb and XFiles.gam (override with XF_DATA=<dir>).
//! --hyp enables hypothesis rules (world::Rules); default = confirmed rules only.
//! --load: the trace starts with a Load of this save (H65) — it replaces XFiles.gam as the world's gam.
use std::path::PathBuf;
use xf::hdb::Hdb;
use xf::replay::{parse_trace, replay};
use xf::sched_replay::replay_blocks;
use xf::world::Rules;

fn main() {
    let mut rules = Rules::default();
    let mut details = false;
    let mut files = Vec::new();
    let mut load: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--hyp" => rules = Rules::all(),
            "--details" => details = true,
            // H89 queue model (on in --hyp since H1241); --h76 = the H76 runner instead (after --hyp)
            "--h89" => rules.h89_queue_model = true,
            "--h76" => rules.h89_queue_model = false,
            "--load" => load = args.next(),
            // ablation of H1500 (a Disable of a layout item whose Activation is still queued)
            "--no-h1500" => rules.h1500_iitem_disable_needs_active = false,
            _ => files.push(a),
        }
    }
    if files.is_empty() {
        eprintln!("usage: replay [--hyp] [--details] <trace.jsonl>...");
        std::process::exit(2);
    }
    // XF_DATA: a folder, or disc images / several sources as a path list (xf::vfs)
    let mut src = xf::vfs::env_sources("XF_DATA");
    if src.is_empty() {
        src.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame"));
    }
    let dir = xf::vfs::data_root_or_exit("replay", &src);
    let open = |n: &str| {
        Hdb::open(dir.join(n)).unwrap_or_else(|e| {
            eprintln!("replay: {e}");
            std::process::exit(2)
        })
    };
    let hdb = open("XFiles.hdb");
    let gam = match &load {
        Some(p) => Hdb::open(p).unwrap_or_else(|e| {
            eprintln!("replay: {p}: {e}");
            std::process::exit(2)
        }),
        None => open("XFiles.gam"),
    };
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_else(|e| {
            eprintln!("replay: {f}: {e}");
            std::process::exit(2)
        });
        let trace = parse_trace(&text).unwrap_or_else(|e| {
            eprintln!("replay: {f}: {e}");
            std::process::exit(2)
        });
        let rep = replay(&hdb, &gam, &trace, rules);
        println!("== {f} (rules: {})", if rules.h48_play_first_enabled { "confirmed + H48/H53/H55/H58/H59/H60/H61" } else { "confirmed only" });
        print!("{}", rep.summary());
        if details {
            print!("{}", rep.details());
        }
        if text.contains("\"tgt_vt\"") {
            // a table of the research notes (not in this repository); without it only the block replay is left out
            // (one line on stderr)
            let tab = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../generated/reverse/vt_kinds.json");
            match std::fs::read_to_string(&tab).map_err(|e| e.to_string()).and_then(|t| replay_blocks(&text, &t, Some(&hdb))) {
                Ok(b) => {
                    print!("{}", b.summary());
                    if details {
                        b.ops_bad.iter().chain(&b.through_raised).for_each(|l| println!("  {l}"));
                    }
                }
                Err(e) => eprintln!("replay: blocks: {e}"),
            }
        }
    }
}
