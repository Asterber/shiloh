//! H1280–H1283 (static decode) and H1322 (C166–C171): movie hotspots — the `.HOT` files of the game data, and the
//! Smolnikoff shootout played through them (fo-174-end.x: the raid started, Cook covering): the entrance → DoSmolShootout
//! picks a goon View with the live generator (H550 / H730), the goon movie's hotspot is a click target in its window,
//! with the gun held the click plays the goon's kill movie (SmolGoonShot1), its end the next round.
mod common;
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::media::hot::{self, Hotspot};
use xf::media::MediaLib;
use xf::world::moviehot::{is_movie_hot, MOVIE_HOT_TAG};
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// H1280: the real files — every `.HOT` of XV parses to 8 + 20 · count bytes; the goon movies' records.
#[test]
fn h1280_hot_files_of_the_game() {
    let xv = root().join("data/InstalledGame/XV");
    let Ok(rd) = std::fs::read_dir(&xv) else { return };
    let mut files = 0;
    let mut ids = std::collections::BTreeMap::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("hot")) != Some(true) {
            continue;
        }
        let b = std::fs::read(&p).unwrap();
        let v = hot::parse(&b).unwrap_or_else(|| panic!("{} does not parse", p.display()));
        assert_eq!(b.len(), 8 + 20 * v.len(), "{}", p.display());
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize, v.len());
        for h in &v {
            assert!(h.rect[0] < h.rect[2] && h.rect[1] < h.rect[3] && h.t0 < h.t1, "{}: {h:?}", p.display());
            *ids.entry(h.id).or_insert(0) += 1;
        }
        files += 1;
    }
    assert_eq!(files, 680);
    for g in [221, 222, 223] {
        assert_eq!(ids.get(&g), Some(&3), "goon id {g}");
    }
    assert_eq!(ids.get(&600), Some(&1215));
    let lib = MediaLib::new(root().join("data/InstalledGame"));
    assert_eq!(
        lib.movie_hotspots("XV/47749.xmv"),
        vec![Hotspot { id: 223, rect: [406, 105, 447, 152], t0: 733, t1: 2133 }]
    );
    assert_eq!(lib.movie_hotspots("XV/47793.xmv")[0].id, 222);
    assert_eq!(lib.movie_hotspots("XV/19672.xmv").len(), 9); // nine id-600 records
    assert!(lib.movie_hotspots("XV/21230.xmv").is_empty()); // the kill movie has none
}

/// fo-174-end.x with the live generator seeded `seed`.
fn session<'a>(h: &'a Hdb, g: &'a Hdb, rules: Rules, seed: u32) -> Session<'a> {
    let mut s = Session::with_rules(h, g, &root().join("data/InstalledGame"), rules);
    s.r.w.live_rand = Some(seed);
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    s
}

/// Select the gun: show the panel, click the gun's bar slot.
fn select_gun(s: &mut Session) {
    s.hover_at(20, 440);
    for _ in 0..3 {
        s.tick(100);
    }
    let r = s.r.icon_bars.rects.get(&0x4433).expect("the gun in the bar").1;
    let (x, y) = ((r[0] as i32 + r[2] as i32) / 2, (r[1] as i32 + r[3] as i32) / 2);
    s.hover_at(x, y);
    s.click_at(x, y);
    for _ in 0..3 {
        s.tick(100);
    }
    assert_eq!(s.r.w.dev.held, Some(7), "the gun held");
    s.hover_at(320, 200);
}

/// Wait (50 ms ticks, up to `ms`) for a movie-hotspot target; click its centre once it has been up for `delay` ms.
/// Returns the clicked target's id.
fn shoot(s: &mut Session, ms: u64, delay: u64) -> Option<u32> {
    shoot_at(s, ms, delay).map(|x| x.0)
}

/// [`shoot`], also returning the clicked point.
fn shoot_at(s: &mut Session, ms: u64, delay: u64) -> Option<(u32, i32, i32)> {
    let mut seen: Option<u64> = None;
    for _ in 0..ms / 50 {
        s.tick(50);
        let c = s.r.clickables().into_iter().find(|c| is_movie_hot(c.owner));
        match (c, seen) {
            (Some(_), None) => seen = Some(s.now),
            (Some(c), Some(t)) if s.now >= t + delay => {
                let (x, y) = ((c.rect[0] as i32 + c.rect[2] as i32) / 2, (c.rect[1] as i32 + c.rect[3] as i32) / 2);
                s.hover_at(x, y);
                let hit = s.click_at(x, y).expect("a hit");
                assert_eq!(hit.owner, c.owner);
                return Some((c.owner - MOVIE_HOT_TAG, x, y));
            }
            _ => {}
        }
    }
    None
}

fn view(s: &Session) -> Option<u32> {
    s.r.w.view
}

/// H1281 + H1282 + H730 + H1322: with the gun held, three goons shot in turn — each round a goon View of H730's tables,
/// its target's click plays the goon's kill movie, SmolGoonShot1 sets its variable (0xe9b..0xe9d), the kill movie's end
/// runs DoSmolShootout; after the third goon View 0x1458 (first floor clear). Several seeds (the generator picks).
#[test]
fn h1282_shootout_with_the_gun() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-174-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    // H730: the goon Views DoSmolShootout picks from, as the exe lists them (`Hdb::exe`)
    let goon_views: Vec<u32> = h.exe.smol_pick_views.iter().flatten().copied().collect();
    assert_eq!(goon_views.len(), 9);
    let mut firsts = std::collections::HashSet::new();
    for seed in [1u32, 77, 4242, 99991] {
        let mut s = session(&h, &g, Rules::all(), seed);
        assert_eq!(view(&s), Some(0x664a));
        select_gun(&mut s);
        s.hover_at(570, 210);
        s.click_at(570, 210); // the entrance (H1245's risk: the held gun on a nav-hotspot)
        let mut shot = Vec::new();
        for round in 0..3 {
            let life0 = s.r.lifecycle.len();
            let (id, x, y) = shoot_at(&mut s, 3000, 150).unwrap_or_else(|| {
                panic!(
                    "seed {seed} round {round}: no target (View {:x?}) started {:x?} hot {:x?} media_log {:x?} {:?}",
                    view(&s),
                    s.r.started(),
                    s.r.movie_hotspots.iter().filter(|x| !x.1.is_empty()).collect::<Vec<_>>(),
                    &s.r.media_log[s.r.media_log.len().saturating_sub(8)..],
                    &s.r.log[s.r.log.len().saturating_sub(12)..]
                )
            });
            let v = s.r.transitions.iter().rev().map(|t| t.0).find(|v| goon_views.contains(v)).unwrap();
            if round == 0 {
                firsts.insert(v);
            }
            assert!((221..=223).contains(&id), "{id}");
            assert!(!shot.contains(&id), "seed {seed}: goon {id} shot twice");
            shot.push(id);
            let kill = [0x52ee, 0x52f0, 0x52f2][id as usize - 221];
            // H1322 (C166 / C168): at the button-down the goon View and its movie are deactivated (Timers 10 / 11 stop),
            // the target goes with the movie: a second click in the window hits nothing, no second kill movie
            s.tick(0);
            assert!(s.r.lifecycle[life0..].contains(&(v, 9)), "seed {seed}: no Deactivation of {v:#x} at the hit");
            assert!(!s.r.clickables().iter().any(|c| is_movie_hot(c.owner)), "seed {seed}: the target stayed");
            s.tick(50);
            s.click_at(x, y);
            for _ in 0..4 {
                s.tick(50);
            }
            assert!(!s.r.timers.iter().any(|t| t.id == 11), "seed {seed}: Timer 11 still armed after the hit");
            // the kill movie plays, its end (DoSmolShootout) leaves the goon View
            for _ in 0..60 {
                if view(&s).is_some_and(|x| x != v) {
                    break;
                }
                s.tick(50);
            }
            assert_eq!(s.r.plays.iter().filter(|p| p.1 == kill).count(), 1, "seed {seed}: kill movie {kill:#x} played once");
            assert!(view(&s).is_some_and(|x| x != v), "seed {seed}: no View after the goon View {v:#x}");
        }
        for _ in 0..10 {
            s.tick(50);
        }
        for v in [0xe9b, 0xe9c, 0xe9d] {
            assert_eq!(s.r.w.vars.get(v), Some(1), "seed {seed}: {v:#x}");
        }
        assert!(s.r.transitions.iter().any(|t| t.0 == 0x1458), "seed {seed}: {:x?}", s.r.transitions);
    }
    // the generator picks the first goon View (one of the three first-round Views)
    assert!(firsts.iter().all(|v| [0xf330, 0xf336, 0xf33c].contains(v)), "{firsts:x?}");
}

/// H1282 (C165): without the gun the click inside the target does nothing — no kill movie; the target swallows it
/// (H1281: the View behind gets no Mouse Click). Ablation: H1282 off → the click shoots; H1281 off → no target.
#[test]
fn h1282_no_gun_no_shot_and_ablations() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-174-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let kills = |s: &Session| s.r.plays.iter().filter(|p| [0x52ee, 0x52f0, 0x52f2].contains(&p.1)).count();

    let mut s = session(&h, &g, Rules::all(), 5);
    s.hover_at(570, 210);
    s.click_at(570, 210);
    let p0 = s.r.posts.len();
    assert!(shoot(&mut s, 3000, 150).is_some(), "the target exists without the gun");
    for _ in 0..10 {
        s.tick(50);
    }
    assert_eq!(kills(&s), 0);
    assert!(s.r.log.iter().any(|l| l.contains("without the gun")));
    // nothing posted by the click itself (the goon movie's timers may run later)
    assert!(s.r.posts[p0..].iter().all(|m| m.action != 0xf181));

    let mut s = session(&h, &g, Rules { h1282_movie_hot_gun: false, ..Rules::all() }, 5);
    s.hover_at(570, 210);
    s.click_at(570, 210);
    assert!(shoot(&mut s, 3000, 150).is_some());
    for _ in 0..4 {
        s.tick(50);
    }
    assert_eq!(kills(&s), 1);

    let mut s = session(&h, &g, Rules { h1281_movie_hotspots: false, ..Rules::all() }, 5);
    s.hover_at(570, 210);
    s.click_at(570, 210);
    assert!(shoot(&mut s, 3000, 150).is_none());
}

/// H1320: the e2e input — DoSmolShootout's picks and SmIsWillShot1's outcomes read from traces of the original (C166 /
/// C167: fo-177 lost round 2 to SmIsWillShot1, fo-178 won all three rounds).
#[test]
fn h1320_picks_from_trace() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let ev = |n: &str| {
        let t = common::trace_text(&format!("{n}.jsonl"))?;
        Some(xf::replay::parse_trace(&t[..t.rfind('\n').unwrap() + 1]).unwrap())
    };
    let (Some(fo178), Some(fo177)) = (ev("fo-178"), ev("fo-177")) else { return };
    let (p, w) = xf::world::shootout::picks_from_trace(&fo178, &h);
    assert_eq!(p, [0xf33c, 0xf330, 0xf336, 0x1458]);
    assert!(w.is_empty());
    let (p, w) = xf::world::shootout::picks_from_trace(&fo177, &h);
    assert_eq!(p, [0xf336, 0xf33c]);
    assert_eq!(w, [true]);
}

/// H1322 ablation: without the View's removal at the hit the goon movie and its target stay — a second click inside the
/// window posts the kill movie again (the double hit the original does not have, C168) and the goon View is not
/// deactivated at the hit.
#[test]
fn h1322_off_double_hit() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-174-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = session(&h, &g, Rules { h1322_movie_hot_view_removal: false, ..Rules::all() }, 1);
    select_gun(&mut s);
    s.hover_at(570, 210);
    s.click_at(570, 210);
    let life0 = s.r.lifecycle.len();
    let (_, x, y) = shoot_at(&mut s, 3000, 150).expect("a target");
    let v = view(&s).unwrap();
    s.tick(0);
    assert!(!s.r.lifecycle[life0..].contains(&(v, 9)));
    s.tick(50);
    assert!(s.click_at(x, y).is_some_and(|c| is_movie_hot(c.owner)), "the target stayed");
    for _ in 0..4 {
        s.tick(50);
    }
    let kills = s.r.plays.iter().filter(|p| [0x52ee, 0x52f0, 0x52f2].contains(&p.1)).count();
    assert_eq!(kills, 2);
}

/// H1520: the reactions by hotspot id — the gun gate, the gun's shot before the
/// shoot-anything reactions, the stiletto alternatives (433 / 462 instead of the gun; 461 / 470 after it), the ungated
/// 441 / 451 / 463; the traced reactions name their trace entries.
#[test]
fn h1520_reactions_table() {
    use xf::world::moviehot::{reactions, Then};
    // (shot, kill movie, then) per reaction
    let r = |id, gun, stiletto| -> Vec<(bool, Option<u32>, Then)> {
        reactions(id, gun, stiletto).iter().map(|r| (r.shot, r.hit.kill, r.hit.then)).collect()
    };
    assert_eq!(r(101, true, false), [(false, Some(0x54a0), Then::ReEnter)]);
    assert_eq!(r(104, true, false), [(false, Some(0x549e), Then::ReEnter)]);
    assert_eq!(r(104, false, false), []);
    assert_eq!(r(100, true, false), [(true, None, Then::Stay)]);
    assert_eq!(r(401, true, false), [(false, Some(0x5534), Then::ReEnter)]);
    assert_eq!(r(421, true, false), [(false, Some(0x5530), Then::ReEnter)]);
    assert_eq!(r(211, true, false), [(false, Some(0x52f6), Then::SetView(0x157d))]);
    assert_eq!(r(433, true, false), [(true, Some(0x138da), Then::Stay)]);
    assert_eq!(r(433, false, true), [(false, Some(0xbf75), Then::Stay)]);
    assert_eq!(r(433, true, true), [(false, Some(0xbf75), Then::Stay)]);
    assert_eq!(r(461, true, true), [(true, Some(0x138da), Then::Stay), (false, Some(0x138da), Then::Stay)]);
    assert_eq!(r(461, false, true), [(false, Some(0x138da), Then::Stay)]);
    assert_eq!(r(462, true, false), [(false, Some(0xbf7e), Then::SetView(0xc0de))]);
    assert_eq!(r(462, false, true), [(false, Some(0x5b8a), Then::Stay)]);
    assert_eq!(r(441, false, false), [(false, Some(0x5516), Then::Stay)]);
    assert_eq!(r(500, true, false), [(true, None, Then::Stay)]);
    assert_eq!(r(600, true, false), [(true, Some(0x4cc8), Then::Stay)]);
    assert_eq!(r(666, true, false), [(true, None, Then::ReEnter)]);
    assert_eq!(r(2400, true, true), []);
    let hit = |id| *reactions(id, true, false)[0].hit;
    assert_eq!(hit(201).view_state, Some(b'r'));
    assert_eq!(hit(101).two_men.map(|t| (t.done_view_state, t.done)), Some((Some(b'a'), Then::SetView(0x1fca))));
    assert_eq!(hit(111).two_men.map(|t| (t.done_view_state, t.done)), Some((None, Then::Play(0x9630))));
    assert!(hit(500).freeze && hit(600).freeze && hit(666).layouts.is_some());
    // the reactions that hold on a trace of the original
    let traced: Vec<u32> = [101, 102, 201, 211, 221, 222, 223, 401, 411, 421, 431, 432, 433, 434, 441, 442, 451, 461, 463, 470, 500, 600, 666]
        .into_iter()
        .filter(|&id| !hit(id).evidence.is_empty())
        .collect();
    assert_eq!(traced, [101, 102, 201, 211, 221, 222, 223, 401, 411, 431, 433, 434, 441, 442, 451, 463, 500]);
    assert_eq!(reactions(462, false, true)[0].hit.evidence, "C220");
}

/// Wait (50 ms ticks, up to `ms`) for the movie-hotspot target `id`; click its centre once it has been up `delay` ms.
fn shoot_id(s: &mut Session, id: u32, ms: u64, delay: u64) -> bool {
    let mut seen: Option<u64> = None;
    for _ in 0..ms / 50 {
        s.tick(50);
        let c = s.r.clickables().into_iter().find(|c| c.owner == MOVIE_HOT_TAG | id);
        match (c, seen) {
            (Some(_), None) => seen = Some(s.now),
            (Some(c), Some(t)) if s.now >= t + delay => {
                let (x, y) = ((c.rect[0] as i32 + c.rect[2] as i32) / 2, (c.rect[1] as i32 + c.rect[3] as i32) / 2);
                s.hover_at(x, y);
                return s.click_at(x, y).is_some_and(|h| h.owner == c.owner);
            }
            _ => {}
        }
    }
    false
}

/// H1520 / H1483 (C210 / C211): the NSA porch shootout from fo-209-end.x — the gun, the far Explorable (333,210) → View
/// 0x9b42 ('both' movie 0x549c); hit 101 → only 0x54a4 stays enabled ('t'), the kill movie 0x54a0, the View
/// re-entered with 0x54a4 as its background after the kill movie; hit 104 → no entry left: 'a', kill movie 0x549e,
/// Set View 0x1fca, 0x9b42's Deactivation with 'a' (0x6ae bNSAOutside := 0); no 'willmore dead' 0x9676. Ablation: H1520
/// off → the hits do nothing, the View's timer ends the scene at 0x9676.
#[test]
fn h1520_nsa_shootout() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-209-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let start = |rules: Rules| {
        let mut s = Session::with_rules(&h, &g, &root().join("data/InstalledGame"), rules);
        s.r.load_game();
        settle_timers(&mut s, 100, 900_000);
        assert_eq!(view(&s), Some(0x2012));
        select_gun(&mut s);
        s.hover_at(333, 210);
        s.click_at(333, 210);
        s
    };
    let mut s = start(Rules::all());
    assert!(shoot_id(&mut s, 101, 4000, 100), "hotspot 101 hit");
    s.tick(0);
    assert_eq!(s.r.w.vars.get(0x5d7), Some(b't' as i32));
    for _ in 0..40 {
        s.tick(50);
        if s.r.background == Some(0x54a4) {
            break;
        }
    }
    assert_eq!((view(&s), s.r.background), (Some(0x9b42), Some(0x54a4)), "re-entered with the remaining man's movie");
    assert!(shoot_id(&mut s, 104, 1000, 50), "hotspot 104 hit");
    for _ in 0..60 {
        s.tick(50);
    }
    assert_eq!(view(&s), Some(0x1fca));
    assert_eq!((s.r.w.vars.get(0x5d7), s.r.w.vars.get(0x6ae)), (Some(b'a' as i32), Some(0)));
    assert!(s.r.plays.iter().any(|p| p.1 == 0x54a0) && s.r.plays.iter().any(|p| p.1 == 0x549e));
    assert!(!s.r.transitions.iter().any(|t| t.0 == 0x9676));

    let mut s = start(Rules { h1520_movie_hot_all_cases: false, ..Rules::all() });
    assert!(shoot_id(&mut s, 101, 4000, 100));
    settle_timers(&mut s, 100, 30_000);
    assert!(s.r.transitions.iter().any(|t| t.0 == 0x9676), "the scene's timer: 'willmore dead'");
}
