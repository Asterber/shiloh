//! H261 (H208's cause): periodic timers in the replay, Timer command only on live subscribers (closed loop).
mod common;
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::replay::replay;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// fo-055 (`traces/fo-055.md`, save fo-054-end.x, View 0x379): CharView 0x579c's Activation trigger 0xb6d4 runs 0x14559
/// (if 0xad1 == 0x25e: timer 5, 2000 ms, periodic); every Timer fire on 0x579c is explained with H261 (b), only the first
/// without; nothing extra either way.
#[test]
fn h261_periodic_timer_replay_fo055() {
    let data = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-054-end.x");
    if !data.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let hdb = Hdb::open(data.join("XFiles.hdb")).unwrap();
    let gam = Hdb::open(&save).unwrap();
    let Some(trace) = common::trace("fo-055.jsonl") else { return };
    let r = replay(&hdb, &gam, &trace, Rules::all());
    assert_eq!((r.timers_ok, r.timers_trace, r.timer_bad.len()), (23, 23, 0));
    assert_eq!((r.cond_ok, r.cond_unknown), (323, 0));
    assert_eq!((r.posts.matched, r.posts.extra.len(), r.execs.extra.len(), r.lifecycle.extra.len()), (321, 0, 0, 0));
    let off = replay(&hdb, &gam, &trace, Rules { h261_timer_periodic_alive: false, ..Rules::all() });
    assert_eq!((off.timers_ok, off.cond_unknown), (1, 44));
    assert_eq!((off.posts.matched, off.posts.extra.len()), (321, 0));
}

/// fo-054's first 10 steps (traced, `traces/fo-054.md` legs 1–2) end in Shanks's office 0x379. On the original only
/// trigger 0xb6d7 (CharView 0x579c, event 7) posts after t 100 (11004 posts, `traces/fo-054.jsonl.gz`); View 0x4d5's
/// Timer trigger 0xb6a2 (subscribed to timer 5 too, fired once at 52.27 while 0x4d5 was current) does not. H261 (a):
/// the closed loop fires 0xb6d7's list and not 0xb6a2's; without H261 it fires both.
#[test]
fn h261_timer_live_subscribers_fo054() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-026-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let steps = [(42, 209), (265, 179), (556, 281), (460, 183), (70, 210), (475, 210), (512, 209), (299, 209), (70, 210), (305, 210)];
    for (rules, b6a2) in [(Rules::all(), false), (Rules { h261_timer_periodic_alive: false, ..Rules::all() }, true)] {
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        let mut p0 = 0;
        for (x, y) in steps {
            p0 = s.r.posts.len();
            s.hover_at(x, y);
            assert!(s.click_at(x, y).is_some());
            settle_timers(&mut s, 100, 900_000);
        }
        assert_eq!(s.r.w.view, Some(0x379));
        let last = &s.r.posts[p0..];
        assert!(last.iter().filter(|m| m.trigger == 0xb6d7).count() >= 12 * 10);
        assert_eq!(last.iter().any(|m| m.trigger == 0xb6a2), b6a2);
    }
}

/// fo-090 k17 (`traces/fo-090.md`, AG2's note): at the warehouse arrival (View 0x659e) the original posts 0x13bda,
/// 0x8e77, 0x8e78, 0x8e7a, 0x8e7c, 0x8e7e, 0x8e80, 0x8e82 — trigger 0x8e73 (event 7, Timer; subscribed 207.777) every
/// 45 s from 252.98 (`traces/fo-090.jsonl`). The prediction settled only (no 45 s of clock), so it missed them; the core
/// running its clock from the end save posts them as a periodic timer.
#[test]
fn fo090_warehouse_timer_8e73_periodic() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-090-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let p0 = s.r.posts.len();
    for _ in 0..1000 {
        s.tick(100);
    }
    let n = s.r.posts[p0..].iter().filter(|m| m.action == 0x13bda).count();
    let trig: Vec<u32> = s.r.posts[p0..].iter().filter(|m| m.action == 0x8e77).map(|m| m.trigger).collect();
    assert!(n >= 2, "0x13bda posted {n} times in 100 s");
    assert!(trig.iter().all(|&t| t == 0x8e73));
}

/// H890 / C98: the PDA click pauses the timers. fo-118-end.x (boat dock View 0x11fd, Timer trigger 0x16e78 every 2 s,
/// `traces/fo-119.attempt1/fo-119.jsonl`: fires up to the PDA click 255.755, none in the ≥ 60 s the PDA stayed open).
/// The core: 0x16e7a posted while the PDA is closed, never after the click; without H890 it keeps firing.
#[test]
fn h890_pda_pauses_timers_fo119() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-118-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    for on in [true, false] {
        let mut s = Session::with_rules(&h, &g, &d, Rules { h890_timer_pause: on, ..Rules::all() });
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        let p0 = s.r.posts.len();
        for _ in 0..50 {
            s.tick(100);
        }
        assert!(s.r.posts[p0..].iter().any(|m| m.action == 0x16e7a), "timer 0x16e78 fires before the PDA click");
        s.hover_at(20, 440);
        for _ in 0..10 {
            s.tick(100);
        }
        assert_eq!(s.click_at(21, 440).map(|c| c.owner), Some(0x442b));
        for _ in 0..5 {
            s.tick(100); // the click's own commands
        }
        let p1 = s.r.posts.len();
        for _ in 0..200 {
            s.tick(100);
        }
        let n = s.r.posts[p1..].iter().filter(|m| m.action == 0x16e7a).count();
        if on {
            assert_eq!(n, 0, "no Timer fire while the PDA is open");
            assert!(s.r.w.dev.pda_paused);
        } else {
            assert!(n >= 9, "without H890 the timer keeps firing ({n})");
        }
    }
}

/// C191 / H1203, the player option "long timers" (`xf --long-timers`, `Rules::set_long_timers`): fo-171's route from
/// fo-167-end.x (k0–k3 to the office 0x1b9a, k4 the log manifest → 0x1b41, which arms the bomb's one-shot Timer 7 =
/// 0x1d4c0 = 120 s, `traces/fo-171.jsonl` 110.945). `wine` (the default, as the bottle): Timer 7 stays handle-less, no
/// game over in 200 s (fo-171: none in 258 s). `nt`: it expires 120 s after the arm → DoActionList(0x10bb0) (action
/// 0xa086) → NukeAllIcons, EndGame 0x10bc7: the game ends (with auto-rewind on: H182's rewind box).
#[test]
fn c191_long_timers_option_bomb() {
    use xf::world::LongTimers;
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-167-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    assert_eq!(Rules::all().long_timers(), LongTimers::Wine, "the default is the bottle's behaviour");
    for opt in [LongTimers::Wine, LongTimers::Nt] {
        let mut rules = Rules::all();
        rules.set_long_timers(opt);
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        for (x, y) in [(281, 211), (570, 210), (320, 301), (164, 190)] {
            s.hover_at(x, y);
            assert!(s.click_at(x, y).is_some(), "{opt:?} ({x},{y})");
            settle_timers(&mut s, 100, 900_000);
        }
        assert_eq!(s.r.w.view, Some(0x1b9a));
        // the manifest: no settle (it would run the timer) — the clock runs until Timer 7 is armed
        s.hover_at(322, 206);
        assert!(s.click_at(322, 206).is_some());
        let t0 = s.now;
        while !s.r.timers.iter().any(|t| t.id == 7) && s.now - t0 < 120_000 {
            s.tick(100);
        }
        let armed = s.now;
        let t7 = *s.r.timers.iter().find(|t| t.id == 7).expect("Timer 7 armed by the manifest");
        assert_eq!(t7.period, None);
        let mut fired = None;
        while s.now - armed < 200_000 && fired.is_none() {
            let p0 = s.r.posts.len();
            s.tick(100);
            if s.r.posts[p0..].iter().any(|m| m.action == 0xa086) {
                fired = Some(s.now - armed);
            }
        }
        match opt {
            LongTimers::Wine => {
                assert_eq!(t7.paused, Some(0x1d4c0), "no native handle");
                assert_eq!(fired, None, "no bomb on the bottle");
                assert!(s.r.game_running && !s.r.menu_open);
                assert!(!s.r.log.iter().any(|l| l.contains("H182 game end")));
                assert!(s.r.timers.iter().any(|t| t.id == 7 && t.paused == Some(0x1d4c0)), "still pending");
            }
            LongTimers::Nt => {
                assert_eq!(t7.paused, None);
                let at = fired.expect("the bomb goes off with the NT option");
                assert!((119_800..=120_300).contains(&at), "expired {at} ms after the arm");
                settle_timers(&mut s, 100, 60_000);
                // EndGame 0x10bc7 (the bomb's DoActionList 0x10bb0) with auto-rewind on: the rewind box (H182)
                assert!(s.r.posts.iter().any(|m| m.action == 0x10bc7) && s.r.posts.iter().any(|m| m.action == 0x1573b));
                assert!(s.r.log.iter().any(|l| l.starts_with("action 0x10bc7: H182 game end")), "the game is over");
            }
        }
    }
}
