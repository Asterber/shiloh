//! H730 (`world/shootout.rs`; static decode, NOT traced): the Smolnikoff shootout's native state, run with the
//! real VCActions of XFiles.hdb (InitSmShootout 0x14b2a, DoSmolShootout 0xf185, SmolGoonShot1 0xf181, SmIsWillShot1
//! 0xf1cf, SmGoonShoots 0xb66b) through `World::execute`, the engine messages they post run like the queue does.
use std::path::PathBuf;
use xf::hdb::Hdb;
use xf::world::{Msg, Out, Rules, World};

const PICK_VIEWS: [[u32; 3]; 3] = [[0xf330, 0xf332, 0xf334], [0xf336, 0xf338, 0xf33a], [0xf33c, 0xf33e, 0xf340]];
const VARS: [u32; 3] = [0xe9b, 0xe9c, 0xe9d];

fn data() -> Option<(Hdb, Hdb)> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return None;
    }
    Some((Hdb::open(d.join("XFiles.hdb")).unwrap(), Hdb::open(d.join("XFiles.gam")).unwrap()))
}

/// Run the action, then every engine message it posted (action id 0); the outputs.
fn run(w: &mut World, action: u32) -> Vec<Out> {
    let n = w.out.len();
    w.execute(Msg { action, trigger: 0 });
    let engine = w.out[n..].iter().filter(|o| matches!(o, Out::Post { msg, .. } if msg.action == 0)).count();
    for _ in 0..engine {
        w.execute(Msg { action: 0, trigger: 0 });
    }
    w.out[n..].to_vec()
}

fn views(o: &[Out]) -> Vec<u32> {
    o.iter().filter_map(|x| if let Out::Transition { view, .. } = x { Some(*view) } else { None }).collect()
}

/// Three DoSmolShootout / SmolGoonShot1 rounds pick three different goons, each at its first pick View, the flags reach
/// the bound variables, and the fourth DoSmolShootout goes to 0x1458 (all down). Ablation: no live generator → logged.
#[test]
fn h730_rounds_pick_standing_goons() {
    let Some((h, g)) = data() else { return };
    let mut firsts = std::collections::HashSet::new();
    for seed in 0..12u32 {
        let mut w = World::new(&h, &g, Rules::all());
        w.live_rand = Some(seed * 7919);
        run(&mut w, 0x14b2a);
        assert_eq!(VARS.map(|v| w.vars.get(v)), [Some(0); 3]);
        let mut down = Vec::new();
        for round in 0..3 {
            w.clock += 1000 + seed as u64; // a new draw
            let v = views(&run(&mut w, 0xf185));
            assert_eq!(v.len(), 1, "seed {seed} round {round}: {v:x?}");
            let goon = PICK_VIEWS.iter().position(|p| p[0] == v[0]).expect("a first-pick View");
            assert!(!down.contains(&goon), "seed {seed}: goon {goon} picked again");
            if round == 0 {
                firsts.insert(goon);
            }
            run(&mut w, 0xf181);
            down.push(goon);
            for (i, &var) in VARS.iter().enumerate() {
                assert_eq!(w.vars.get(var), Some(down.contains(&i) as i32));
            }
        }
        assert_eq!(views(&run(&mut w, 0xf185)), vec![0x1458]);
    }
    assert!(firsts.len() >= 2, "the first target varies with the seed: {firsts:?}");

    let mut w = World::new(&h, &g, Rules::all());
    run(&mut w, 0x14b2a);
    let o = run(&mut w, 0xf185);
    assert!(views(&o).is_empty());
    assert!(o.iter().any(|x| matches!(x, Out::Unimplemented { what, .. } if what.contains("H192"))));
}

/// SmIsWillShot1(10) on the first pick, difficulty 'E' (20 %) vs 'H' (60 %) over 60 seeds: a hit stops timer 10 and
/// plays 0x52b0; 'H' hits more often. SmGoonShoots(0x7d3, 0x7d2 = 1, 0xb): no draw, a := 0 and timer 0xb stopped.
#[test]
fn h730_will_and_goon_hits() {
    let Some((h, g)) = data() else { return };
    let mut hits = [0; 2];
    for (k, d) in [b'E', b'H'].into_iter().enumerate() {
        for seed in 0..60u32 {
            let mut w = World::new(&h, &g, Rules::all());
            w.live_rand = Some(seed.wrapping_mul(2654435761));
            w.vars.set_native(0x3df, Some(d as i32), 0);
            run(&mut w, 0x14b2a);
            run(&mut w, 0xf185);
            w.clock += 17;
            let o = run(&mut w, 0xf1cf);
            let stop = o.iter().any(|x| matches!(x, Out::Timer { id: 10, start: None }));
            let play = o.iter().any(|x| matches!(x, Out::Reg { owner: 0x52b0, .. }));
            assert_eq!(stop, play);
            hits[k] += stop as usize;
        }
    }
    assert!(hits[0] < hits[1], "E {} vs H {}", hits[0], hits[1]);
    assert!(hits[0] > 0 && hits[1] < 60);

    let mut w = World::new(&h, &g, Rules::all());
    w.live_rand = Some(1);
    w.vars.set_native(0x7d2, Some(1), 1);
    w.vars.set_native(0x7d3, Some(1), 2);
    let o = run(&mut w, 0xb66b);
    assert!(o.iter().any(|x| matches!(x, Out::Timer { id: 0xb, start: None })));
    assert_eq!(w.vars.get(0x7d3), Some(0));
    // rule off: H192 (count 2 → unknown hit, no timer stop)
    let mut w = World::new(&h, &g, Rules { h730_smol_shootout: false, ..Rules::all() });
    w.live_rand = Some(1);
    w.vars.set_native(0x7d2, Some(2), 1);
    run(&mut w, 0xb66b);
    assert_eq!(w.vars.get(0x7d3), None);
}
