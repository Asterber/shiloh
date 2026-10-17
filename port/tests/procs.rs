//! Type-6 procedures decoded statically (H175–H198, `world/procs.rs`; NOT traced except EmailInit post-hoc in fo-036):
//! each test runs real VCActions of XFiles.hdb through `World::execute` and checks the hypothesis' effect, and that the
//! procedure is logged as unimplemented with its rule off.
use std::path::PathBuf;
use xf::hdb::Hdb;
use xf::world::{Msg, Out, PostSrc, Rules, World};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn data(gam: &str) -> Option<(Hdb, Hdb)> {
    let d = root().join("data/InstalledGame");
    let g = root().join(gam);
    if !d.join("XFiles.hdb").exists() || !g.exists() {
        return None;
    }
    Some((Hdb::open(d.join("XFiles.hdb")).unwrap(), Hdb::open(g).unwrap()))
}

fn run(w: &mut World, action: u32) -> Vec<Out> {
    let n = w.out.len();
    w.execute(Msg { action, trigger: 0 });
    w.out[n..].to_vec()
}

fn unimplemented(o: &[Out]) -> bool {
    o.iter().any(|x| matches!(x, Out::Unimplemented { .. }))
}

/// H177: PageTurnSound (0x140b4) posts action list 0x16934 with trigger 0x1692f; off → unimplemented.
#[test]
fn h177_page_turn_sound() {
    let Some((h, g)) = data("data/InstalledGame/XFiles.gam") else { return };
    let mut w = World::new(&h, &g, Rules::all());
    let o = run(&mut w, 0x140b4);
    let posts: Vec<(u32, u32)> = o
        .iter()
        .filter_map(|x| match x {
            Out::Post { msg, src: PostSrc::Proc { .. } } => Some((msg.action, msg.trigger)),
            _ => None,
        })
        .collect();
    let al = h.list_of(0x42, 0x16934);
    assert!(!al.is_empty());
    assert_eq!(posts, al.iter().map(|&a| (a, 0x1692f)).collect::<Vec<_>>());
    let mut w = World::new(&h, &g, Rules { h177_page_turn_sound: false, ..Rules::all() });
    assert!(unimplemented(&run(&mut w, 0x140b4)));
}

/// H185: SetShanksVar(0xf49) (0x1087a) with RegCurrNode bound (0xa916 → var 0x4c3) and the node = 2:
/// 0xf49 := (0x834 == 0 ∨ 0x835 == 0) — 1 in the new game's gam; node 3 → 0.
#[test]
fn h185_set_shanks_var() {
    let Some((h, g)) = data("data/InstalledGame/XFiles.gam") else { return };
    let mut w = World::new(&h, &g, Rules::all());
    run(&mut w, 0xa916); // RegCurrNode(0x4c3)
    w.vars.set_native(0x4c3, Some(2), 0);
    run(&mut w, 0x1087a);
    assert_eq!(w.vars.get(0xf49), Some(1));
    w.vars.set_native(0x4c3, Some(3), 0);
    run(&mut w, 0x1087a);
    assert_eq!(w.vars.get(0xf49), Some(0));
}

/// H189: PrefVolume (0xc449 = up) from level 0: the bar items 0xeb5c off / 0xeb5e on (engine type-3 posts),
/// var 0x3d3 := 11 (type 1).
#[test]
fn h189_volume_bar() {
    let Some((h, g)) = data("data/InstalledGame/XFiles.gam") else { return };
    let mut w = World::new(&h, &g, Rules::all());
    let o = run(&mut w, 0xc449);
    assert_eq!(o.iter().filter(|x| matches!(x, Out::Post { src: PostSrc::Engine, .. })).count(), 2);
    assert_eq!(w.vars.get(0x3d3), Some(11));
}

/// H193: IsoChamberStat (0xb848): lights off (0x2dc ≠ 1) → a4 (0x93a) = a7 (0x93b) = 1, the others 0.
#[test]
fn h193_iso_chamber() {
    let Some((h, g)) = data("data/InstalledGame/XFiles.gam") else { return };
    let mut w = World::new(&h, &g, Rules::all());
    w.vars.set_native(0x2dc, Some(0), 2);
    run(&mut w, 0xb848);
    let v: Vec<Option<i32>> = [0x93a, 0x938, 0x939, 0x93b, 0x93c, 0x93d, 0x93e, 0x93f].iter().map(|&x| w.vars.get(x)).collect();
    assert_eq!(v, [1, 0, 0, 1, 0, 0, 0, 0].map(Some).to_vec());
}

/// H193 / C215: with the lights on, the doors pick the two flags (both open s3 s5, east s2 s7, west s3 s6, closed s2 s8;
/// fo-215: closed → 0x938 and 0x93f).
#[test]
fn h193_iso_chamber_doors() {
    use xf::world::action::{Action, ProcCall};
    let Some((h, g)) = data("data/InstalledGame/XFiles.gam") else { return };
    let call = ProcCall::parse(Action::load(&h, 0xb848).unwrap().body()).unwrap();
    let (east, west, lights) = (call.args[0], call.args[1], call.args[2]);
    let flags = &call.args[3..11];
    for (e, w, on) in [(1, 1, [2, 4]), (1, 0, [1, 6]), (0, 1, [2, 5]), (0, 0, [1, 7])] {
        let mut w_ = World::new(&h, &g, Rules::all());
        for (v, x) in [(east, e), (west, w), (lights, 1)] {
            w_.vars.set_native(v, Some(x), 2);
        }
        run(&mut w_, 0xb848);
        let got: Vec<Option<i32>> = flags.iter().map(|&f| w_.vars.get(f)).collect();
        let want: Vec<Option<i32>> = (0..8).map(|i| Some(on.contains(&i) as i32)).collect();
        assert_eq!(got, want, "east {e} west {w}");
    }
}

/// H186: a save's VCGameState lists seed the e-mail lists (fo-033-end.x: inbox [0xfc11, 0xfc0d, 0xfc12], none read) —
/// EmailInit('b') (0x1514a) then posts list 0x1512c with trigger 0x1512a (fo-036 623.25, post-hoc).
#[test]
fn h186_email_init_posts_with_unread_mail() {
    let Some((h, g)) = data("data/bot/fo-033-end.x") else { return };
    let mut w = World::new(&h, &g, Rules::all());
    assert_eq!(w.px.inbox, vec![0xfc11, 0xfc0d, 0xfc12]);
    let o = run(&mut w, 0x1514a);
    assert!(o.iter().any(|x| matches!(x, Out::Post { msg: Msg { trigger: 0x1512a, .. }, .. })));
    let mut w = World::new(&h, &g, Rules { h186_email: false, ..Rules::all() });
    assert!(unimplemented(&run(&mut w, 0x1514a)));
}

/// H199: type-3 Disable of the enabled PDA 0x442b (action 0xabf4) while it is active: Deactivation of the PDA; with
/// prefs 'V' the other active inventory objects are deactivated and activated again; rule off → logged.
#[test]
fn h199_inventory_disable() {
    let Some((h, g)) = data("data/InstalledGame/XFiles.gam") else { return };
    for (pref, others) in [(b'H', 0), (b'V', 2)] {
        let mut w = World::new(&h, &g, Rules::all());
        w.vars.set_native(0x3d7, Some(pref as i32), 0);
        w.dev.inv_active = vec![0x442f, 0x442b];
        let o = run(&mut w, 0xabf4);
        let fires: Vec<(u32, u8)> = o
            .iter()
            .filter_map(|x| match x {
                Out::Fire { owner, event } => Some((*owner, *event)),
                _ => None,
            })
            .collect();
        assert_eq!(fires[0], (0x442b, 9));
        assert_eq!(fires.len(), 1 + others);
        assert_eq!(w.dev.inv_active, vec![0x442f]);
    }
    let mut w = World::new(&h, &g, Rules { h199_inventory_disable: false, ..Rules::all() });
    assert!(unimplemented(&run(&mut w, 0xabf4)));
}
