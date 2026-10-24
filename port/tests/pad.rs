//! H2410 (port feature): the controller makes the mouse's own input. From fo-066-end.x, fo-067's first clicks (the
//! points of `teleport_click.rs`) are made twice: with the mouse (front::host motion / button / release at the
//! clickable's snap point) and with the pad (RB cycles the snap targets of front::pad::targets until the pointer is on
//! that clickable, then A down / up, through front::pad::apply). The pad finds every clicked object among its targets,
//! and both runs give the same posts, executed actions and Views.
use std::path::PathBuf;
use xf::front::host::{self, Button};
use xf::front::pad::{self, Pad, PadCfg, PadIn};
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const STEPS: [(i32, i32); 6] = [(314, 372), (320, 240), (310, 372), (599, 210), (584, 210), (139, 309)];

/// (posts, executed actions, transitions, the owners clicked) of the run
type Run = (Vec<u32>, Vec<u32>, Vec<(u32, u32)>, Vec<u32>);

fn run(h: &Hdb, g: &Hdb, d: &std::path::Path, with_pad: bool) -> Run {
    let mut s = Session::with_rules(h, g, d, xf::front::rules_hyp());
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let mut p = Pad::default();
    let cfg = PadCfg::default();
    let mut clicked = Vec::new();
    for (x, y) in STEPS {
        let owner = s.r.hit(x, y).map(|c| c.owner).expect("a clickable at the step's point");
        let ts = pad::targets(&s);
        let t = *ts
            .iter()
            .find(|t| s.r.hit(t.at.0, t.at.1).is_some_and(|c| c.owner == owner))
            .unwrap_or_else(|| panic!("{owner:#x} at ({x}, {y}) is not a snap target: {ts:?}"));
        if with_pad {
            let mut tries = 0;
            while s.r.hit(p.point().0, p.point().1).map(|c| c.owner) != Some(owner) || !p.driving {
                tries += 1;
                assert!(tries <= ts.len() + 1, "RB never reached {owner:#x}");
                for b in [pad::RB, 0] {
                    let o = p.update(&PadIn { buttons: b, ..Default::default() }, 10, &cfg, &mut || pad::targets(&s));
                    for o in o {
                        pad::apply(&mut s, o, p.point());
                    }
                }
            }
            for b in [pad::A, 0] {
                let o = p.update(&PadIn { buttons: b, ..Default::default() }, 10, &cfg, &mut || pad::targets(&s));
                assert_eq!(o.len(), 1, "{o:?}");
                for o in o {
                    pad::apply(&mut s, o, p.point());
                }
            }
        } else {
            host::motion(&mut s, Some(t.at));
            host::button(&mut s, Button::Left, Some(t.at));
            host::release(&mut s, Button::Left, Some(t.at));
        }
        clicked.push(owner);
        settle_timers(&mut s, 100, 900_000);
    }
    let posts = s.r.posts.iter().map(|m| m.action).collect();
    (posts, s.r.execs.clone(), s.r.transitions.clone(), clicked)
}

#[test]
fn h2410_pad_a_press_is_the_mouse_click() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-066-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        eprintln!("skipping: no game data / save");
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mouse = run(&h, &g, &d, false);
    let with_pad = run(&h, &g, &d, true);
    assert_eq!(mouse.3, with_pad.3);
    assert!(mouse.2.len() >= 3, "the clicks moved through Views: {:x?}", mouse.2);
    assert_eq!(mouse.2, with_pad.2, "transitions");
    assert_eq!(mouse.0, with_pad.0, "posts");
    assert_eq!(mouse.1, with_pad.1, "executed actions");
}
