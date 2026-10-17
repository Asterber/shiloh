//! Game overs (H1700 / H1701 / H1703 on C242–C256, the traces `traces/fo-227.jsonl` … `traces/fo-243.jsonl`):
//! the rewind box only for a loss with var 0xa15 ≠ 0, YES = a Load of the rewind snapshot (the last SaveGameState
//! since the Load, else the loaded save), NO = the main menu, "Return" after a game end = New Game, and the suicide
//! note's freeze.
use std::path::PathBuf;
use xf::front::host::{self, HostReq};
use xf::front::rewind::{NO_RECT, YES_RECT};
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const TICK: u64 = 20;

/// Run the clock on `t` ms (the menu stops the game's clock: stop there).
fn run(s: &mut Session, t: u64) {
    let t0 = s.now;
    while s.now < t0 + t && !s.r.menu_open {
        s.tick(TICK.min(t0 + t - s.now));
    }
}

fn click(s: &mut Session, x: i32, y: i32) {
    s.hover_at(x, y);
    s.click_at(x, y);
}

fn centre(r: [i32; 4]) -> (i32, i32) {
    ((r[0] + r[2]) / 2, (r[1] + r[3]) / 2)
}

/// `f` on a session loaded from the bot's save `save` (skipped without the game data)
fn with_save(save: &str, f: impl FnOnce(&mut Session)) {
    let d = root().join("data/InstalledGame");
    let p = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !p.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&p).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 60_000);
    f(&mut s);
}

/// C247: guard 2 lost by waiting — View 0x725a, Timer 10 → 0x9b9c (the 0xa15 write), the gravestone, EndGame 0x9b7d.
fn guard2_loss(s: &mut Session) {
    assert_eq!(s.r.w.view, Some(0x7259));
    click(s, 570, 210);
    run(s, 2000);
    assert_eq!(s.r.w.view, Some(0x725a));
    let t0 = s.now;
    while !s.r.rewind_box && !s.r.menu_open && s.now - t0 < 60_000 {
        s.tick(100);
    }
}

#[test]
fn c247_the_box_after_a_loss_with_0xa15_and_yes_loads_the_loaded_save() {
    with_save("fo-221-end.x", |s| {
        guard2_loss(s);
        assert!(s.r.rewind_box, "the rewind box (0xa15 ≠ 0)");
        assert!(!s.r.menu_open);
        assert_eq!(s.r.w.vars.get(0xa15), Some(0), "the box clears 0xa15 (H182)");
        // the box is modal: the game's clock stands still, a click outside the buttons does nothing
        let (n, view) = (s.r.execs.len(), s.r.w.view);
        run(s, 5000);
        click(s, 100, 100);
        assert!(s.r.rewind_box);
        assert_eq!((s.r.execs.len(), s.r.w.view), (n, view));
        let fb = s.render();
        // XF_GAMEOVER_DUMP=FILE.ppm: the box frame for a look
        if let Some(p) = std::env::var_os("XF_GAMEOVER_DUMP") {
            xf::front::write_ppm(std::path::Path::new(&p), &fb).unwrap();
        }
        let at =|x: usize, y: usize| fb[(y * 640 + x) * 3..(y * 640 + x) * 3 + 3].to_vec();
        assert_eq!(at(20, 20), vec![0, 0, 0], "the screen around the box is black");
        assert!(at(200, 190).iter().any(|&b| b > 40), "the box movie (XG/54200) is drawn");
        // YES (C247): a Load of the loaded save — its View 0x7259 again, the game runs
        let (x, y) = centre(YES_RECT);
        click(s, x, y);
        settle_timers(s, 100, 60_000);
        assert!(!s.r.rewind_box && !s.r.menu_open && s.r.game_running);
        assert_eq!(s.r.w.view, Some(0x7259));
        let tr: Vec<u32> = s.r.transitions.iter().map(|t| t.0).collect();
        let i = tr.iter().position(|&v| v == 0x725a).expect("the loss's View in the history");
        assert!(tr[i..].contains(&0x7259), "the core's history continues across the rewind: {tr:x?}");
    });
}

#[test]
fn c255_no_goes_to_the_main_menu_and_return_starts_a_new_game() {
    with_save("fo-221-end.x", |s| {
        guard2_loss(s);
        assert!(s.r.rewind_box);
        let (x, y) = centre(NO_RECT);
        click(s, x, y);
        assert!(!s.r.rewind_box);
        assert!(s.r.menu_open, "NO → the main menu");
        assert!(!s.r.game_running, "no game running (no Save item, no return.pic)");
        assert_eq!(host::request(s), None);
        // "Return" (555,240) with no game running = New Game (C243)
        s.hover_at(555, 240);
        s.menu_click(555, 240);
        assert_eq!(host::request(s), Some(HostReq::New));
    });
}

#[test]
fn c249_no_box_without_0xa15_the_main_menu_and_return_starts_a_new_game() {
    with_save("fo-223-end.x", |s| {
        assert_eq!(s.r.w.view, Some(0x3a43));
        click(s, 344, 238);
        run(s, 2000);
        assert_eq!(s.r.w.view, Some(0xaa83));
        // Cook's Timer 10 (62 s) → the gravestone, EndGame 0x9b7d with 0xa15 = 0
        let t0 = s.now;
        while !s.r.rewind_box && !s.r.menu_open && s.now - t0 < 120_000 {
            s.tick(100);
        }
        assert!(!s.r.rewind_box, "no box (0xa15 = 0)");
        assert!(s.r.menu_open && !s.r.game_running, "the main menu at once, no game running");
        // Esc with no game running does not leave the menu (port choice)
        host::key(s, 0x1b);
        assert!(s.r.menu_open);
        s.hover_at(555, 240);
        s.menu_click(555, 240);
        assert_eq!(host::request(s), Some(HostReq::New), "\"Return\" after a game end = New Game (C243)");
        let d = root().join("data/InstalledGame");
        let fresh: &'static Hdb = Box::leak(Box::new(Hdb::open(d.join("XFiles.gam")).unwrap()));
        s.reload(fresh, false);
        settle_timers(s, 100, 900_000);
        assert!(s.r.game_running && !s.r.menu_open);
        assert_eq!(s.r.w.view, Some(0x2b3), "the new game's first View");
    });
}

#[test]
fn c245_yes_loads_the_last_save_game_state() {
    // fo-230: from fo-173-end.x (View 0x4bf) the PDA → the Smolnikoff dot → View 0x664a (its arrival SaveGameState),
    // Cook covers, the entrance → Willmore shot, the newspaper, EndGame 0xa011 with the box; YES → View 0x664a
    with_save("fo-173-end.x", |s| {
        assert_eq!(s.r.w.view, Some(0x4bf));
        s.hover_at(20, 440);
        run(s, 1500);
        // the original's step lengths (fo-230: 52.47, 57.72, 62.97, the arrival 63.02, the cover 89.25)
        for (x, y, wait) in [(21, 440, 5250), (239, 322, 5250), (415, 320, 26_000)] {
            click(s, x, y);
            run(s, wait);
        }
        assert_eq!(s.r.w.view, Some(0x664a));
        assert!(s.r.rewind_snap.is_none(), "no SaveGameState before Cook covers");
        // Cook covers (+26 s after the arrival): 0x655b's Activation list 0x14b31 runs SaveGameState 0x14b36
        // (`traces/fo-230.jsonl` 89.5)
        click(s, 236, 209);
        run(s, 11_000);
        let snap = s.r.rewind_snap.as_ref().expect("Cook's cover took the snapshot");
        assert_eq!(snap.start[3], 0x664a, "the snapshot's View");
        // the entrance 11 s later → trigger 0xb80a, Willmore shot
        click(s, 570, 210);
        let t0 = s.now;
        while !s.r.rewind_box && !s.r.menu_open && s.now - t0 < 120_000 {
            s.tick(100);
        }
        assert!(
            s.r.rewind_box,
            "the box after the FBI-shot ending; menu {} views {:x?}",
            s.r.menu_open,
            s.r.transitions.iter().map(|t| t.0).collect::<Vec<_>>()
        );
        let (x, y) = centre(YES_RECT);
        click(s, x, y);
        settle_timers(s, 100, 120_000);
        assert!(s.r.game_running && !s.r.rewind_box);
        assert_eq!(s.r.w.view, Some(0x664a), "YES → the snapshot's View, not the loaded save's 0x4bf");
    });
}

#[test]
fn c251_the_suicide_note_freezes_the_game() {
    // fo-238: from fo-223-end.x with the gun, (344,238) → View 0xaa83, hotspot 500 at +2.52 s → the suicide note
    with_save("fo-223-end.x", |s| {
        s.hover_at(20, 440);
        run(s, 1500);
        click(s, 351, 440);
        run(s, 500);
        click(s, 344, 238);
        let t0 = s.now;
        run(s, 300);
        assert_eq!(s.r.w.view, Some(0xaa83));
        s.hover_at(394, 224);
        while s.now < t0 + 2520 {
            s.tick(20);
        }
        s.click_at(394, 224);
        run(s, 1000);
        assert!(s.r.frozen, "the game frozen (H1703)");
        let shown = |s: &Session| s.r.w.layouts().iter().any(|l| l.0 == 0xf57f || l.0 == 0xf581);
        assert!(shown(s), "a suicide note layout shows");
        // C251: no event for 50 s — nothing runs, a click does nothing, nothing ends
        let (n, tr) = (s.r.execs.len(), s.r.transitions.len());
        run(s, 50_000);
        click(s, 320, 200);
        run(s, 1000);
        assert_eq!((s.r.execs.len(), s.r.transitions.len()), (n, tr));
        assert!(!s.r.menu_open && !s.r.rewind_box && s.r.frozen);
        assert!(shown(s));
        // the menu's Return lifts the freeze (FUN_0046c9b0(app, 0), static, not traced)
        host::key(s, 0x1b);
        assert!(s.r.menu_open);
        host::key(s, 0x1b);
        assert!(!s.r.menu_open && !s.r.frozen);
    });
}

#[test]
fn h1700_rule_off_the_box_is_only_logged() {
    let d = root().join("data/InstalledGame");
    let p = root().join("data/bot/fo-221-end.x");
    if !d.join("XFiles.hdb").exists() || !p.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&p).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules { h1700_rewind_box: false, ..Rules::all() });
    s.r.load_game();
    settle_timers(&mut s, 100, 60_000);
    guard2_loss(&mut s);
    run(&mut s, 30_000);
    assert!(!s.r.rewind_box);
    assert!(s.r.log.iter().any(|l| l.contains("H182 game end (rewind box true)")));
}
