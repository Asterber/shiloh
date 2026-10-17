//! H603: RandGen's value is seeded by timeGetTime() at every call — the traced value of a condition read after
//! a traced draw is an input (`Runner::uneval_random_from_trace`), as the unevaluable conditions are.
use std::path::PathBuf;
use xf::hdb::Hdb;
mod common;
use xf::world::run::{random_actions, Runner};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn h603_random_branch_inputs() {
    let d = root().join("data/InstalledGame/XFiles.hdb");
    if !d.exists() {
        return;
    }
    let Some(tr) = common::trace("fo-027.jsonl") else { return };
    let h = Hdb::open(d).unwrap();
    let r = random_actions(&h);
    assert_eq!(r.get(&0xcc8b), Some(&0x5c7)); // RandGen[0x5c7, 2]
    assert_eq!(r.get(&0x133d3), Some(&0x1175)); // the phone's RandGen[0x1175, 3]
    // fo-027: the draw 0xcc8b at 524.02 → 0xcc8c true (524.80); the later false (561.26) follows no draw
    let m = Runner::uneval_random_from_trace(&tr, &h);
    assert_eq!(m.get(&0xcc8c), Some(&true));
}
