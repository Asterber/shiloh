//! H731–H769 (static decodes / post-hoc trace reads, NOT pre-registered): closed-loop tests.
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// H732: fo-060-end.x (VCGameState +0x1a8 = VCIFaceLayout 0x14968) — a Load shows that layout with its enabled item
/// 0x1496e (fo-061 / fo-063 / fo-064: REG 0x14968 TL 0 after the start View, REG 0x1496e); the layout
/// takes part in the hit test as a SetIFLAsView layout. Ablation: rule off → not shown, H67 logs it.
#[test]
fn h732_load_shows_saved_layout() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-060-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    for on in [true, false] {
        let mut s = Session::with_rules(&h, &g, &d, Rules { h732_load_layout: on, ..Rules::all() });
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        let shown = s.r.w.layouts().iter().find(|l| l.0 == 0x14968).map(|l| l.1.iter().map(|x| x.0).collect::<Vec<_>>());
        let logged = s.r.log.iter().any(|l| l.contains("H67: Load shows the saved VCIFaceLayout 0x14968"));
        if on {
            assert!(shown.as_ref().is_some_and(|items| items.contains(&0x1496e)), "{shown:x?}");
            assert!(!logged);
        } else {
            assert_eq!(shown, None);
            assert!(logged);
        }
    }
}

/// H733: AssetRef 0x5b8a's event-9 trigger 0xb352 runs FadeToBlack (0x11097) then NukeAllIcons (0x1155d), …: the commands
/// behind FadeToBlack wait until the game clock moved on by 2000 ms (fo-082: proc6 64.076 → the action's obj_remove
/// 66.111). Ablation: rule off → they run at once.
#[test]
fn h733_fade_to_black_stalls() {
    use xf::world::run::Runner;
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    for on in [true, false] {
        let mut r = Runner::new(&h, &g, Rules { h733_fade_stall: on, ..Rules::all() });
        r.advance(1000);
        r.external_register(0x5b8a, 0xb353);
        r.external_fire(0x5b8a, 9);
        r.run(10_000);
        assert!(r.execs.contains(&0x11097));
        assert_eq!(r.execs.contains(&0x1155d), !on, "rule {on}");
        if on {
            r.advance(2999);
            r.run(10_000);
            assert!(!r.execs.contains(&0x1155d));
            r.advance(3000);
            r.run(10_000);
            assert!(r.execs.contains(&0x1155d));
            let i = r.execs.iter().position(|&a| a == 0x11097).unwrap();
            assert_eq!(r.execs[i + 1], 0x1155d);
        }
    }
}

/// H734: ActionIcon 0x6028's Activation (trigger 0xf5af, AL 0xf937) runs SelectInv(0x4433 gun) (0xf938): the gun is
/// clicked as by the user (H152: selected, Mouse Click → OnGunClick → held object 7), the panel shown. Save: the first bot
/// save whose inventory holds the gun. Ablation: rule off → nothing selected.
#[test]
fn h734_select_inv_clicks_the_gun() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let mut saves: Vec<PathBuf> = std::fs::read_dir(root().join("data/bot"))
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "x")).collect())
        .unwrap_or_default();
    saves.sort();
    for save in saves.iter().rev() {
        let g = Hdb::open(save).unwrap();
        let mut s = Session::with_rules(&h, &g, &d, Rules::all());
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        if !s.r.w.dev.inv_active.contains(&0x4433) || s.r.w.dev.held.is_some() {
            continue;
        }
        for on in [true, false] {
            let mut s = Session::with_rules(&h, &g, &d, Rules { h734_select_inv: on, ..Rules::all() });
            s.r.load_game();
            settle_timers(&mut s, 100, 900_000);
            s.r.external_register(0x6028, 0xf5b0);
            s.r.external_fire(0x6028, 0);
            for _ in 0..20 {
                s.tick(100);
            }
            assert!(s.r.execs.contains(&0xf938), "{}", save.display());
            if on {
                assert_eq!(s.r.inv_selected, Some(0x4433), "{}", save.display());
                assert_eq!(s.r.w.dev.held, Some(7));
            } else {
                assert_eq!(s.r.inv_selected, None);
            }
        }
        return;
    }
    eprintln!("h734: no bot save with the gun");
}

/// H736: with the flashlight (0x441e) clicked (held value 8 in RegCurrInvVar's variable), ViewPoint 0x1846's event-8
/// trigger 0xfdff runs SetFLightProps (0xfe03): the held flashlight is selected again (H734's worker) — its click action
/// OnFLightClick (0x7ce4) runs once more per SetFLightProps. Ablation: rule off → not again, H93 logs.
#[test]
fn h736_flight_props_reselect() {
    const FLIGHT_CLICK: u32 = 0x7ce4;
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let mut saves: Vec<PathBuf> = std::fs::read_dir(root().join("data/bot"))
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "x")).collect())
        .unwrap_or_default();
    saves.sort();
    for save in saves.iter().rev() {
        let g = Hdb::open(save).unwrap();
        let mut s = Session::with_rules(&h, &g, &d, Rules::all());
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        if !s.r.w.dev.inv_active.contains(&0x441e) || s.r.w.dev.held.is_some() {
            continue;
        }
        let mut runs = Vec::new();
        for on in [true, false] {
            let mut s = Session::with_rules(&h, &g, &d, Rules { h736_flight_reselect: on, ..Rules::all() });
            s.r.load_game();
            settle_timers(&mut s, 100, 900_000);
            s.r.click(0x441e);
            settle_timers(&mut s, 100, 900_000);
            assert_eq!(s.r.inv_selected, Some(0x441e), "{}", save.display());
            let n0 = s.r.execs.iter().filter(|&&a| a == FLIGHT_CLICK).count();
            s.r.external_register(0x1846, 0xfe00);
            s.r.external_fire(0x1846, 8);
            settle_timers(&mut s, 100, 900_000);
            assert!(s.r.execs.contains(&0xfe03));
            let props = s.r.execs.iter().filter(|&&a| a == 0xfe03).count();
            assert!(props >= 1);
            // one re-selection (click action) per SetFLightProps, right behind it (C24 insert point)
            runs.push((s.r.execs.iter().filter(|&&a| a == FLIGHT_CLICK).count() - n0) as f64 / props as f64);
            if std::env::var_os("H736_DEBUG").is_some() {
                eprintln!("{on}: {:x?}", &s.r.execs[s.r.execs.len().saturating_sub(30)..]);
                eprintln!("{:?}", &s.r.log[s.r.log.len().saturating_sub(15)..]);
            }
            assert_eq!(s.r.inv_selected, Some(0x441e));
        }
        assert_eq!(runs, vec![1.0, 0.0], "{}", save.display());
        return;
    }
    eprintln!("h736: no bot save with the flashlight");
}
