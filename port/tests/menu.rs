//! The game's menu and save games in the closed loop (H169, H400–H407; static hypotheses, not traced): Esc / the
//! top-right object open the menu, its items, the save screen writing a `.x` the port reads back, the load screen
//! switching the session, the menu's Quit and New.
use std::path::{Path, PathBuf};
use xf::front::host::{self, HostReq};
use xf::front::{settle, settle_timers, Session};
use xf::hdb::Hdb;
use xf::save::{SaveState, Slots};
use xf::world::devices::MenuChoice;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("xf-menu-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Session from a save of the original, settled; `saves` = the slot directory.
fn with_save(save: &str, saves: &Path, f: impl FnOnce(&mut Session)) {
    with_save_rules(save, saves, Rules::all(), f)
}

/// [`with_save`] with `rules`.
fn with_save_rules(save: &str, saves: &Path, rules: Rules, f: impl FnOnce(&mut Session)) {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        eprintln!("data missing: skipped");
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.set_saves(Slots::open(saves));
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    f(&mut s);
}

/// Order-free comparison (the file lists records in objId order, the session by key).
fn norm(mut st: SaveState) -> SaveState {
    st.enabled.sort();
    st.vars.sort();
    st
}

fn shown(s: &Session, layout: u32) -> bool {
    s.r.w.layouts().iter().any(|l| l.0 == layout)
}

fn click(s: &mut Session, x: i32, y: i32) {
    s.hover_at(x, y);
    s.click_at(x, y);
    settle(s, 100, 60_000);
}

fn key(s: &mut Session, k: u32) {
    host::key(s, k);
    settle(s, 100, 60_000);
}

/// H169 / H404: Esc opens the menu (the held object dropped, the game's clock stopped, the menu screen drawn instead
/// of the View); Esc again = Return; the menu's "Return" item closes it too.
#[test]
fn esc_opens_the_menu() {
    let d = tmp("esc");
    with_save("fo-056-end.x", &d, |s| {
        let game = s.render();
        assert!(!s.r.menu_open);
        key(s, 0x1b);
        assert!(s.r.menu_open);
        let t = s.now;
        s.tick(1000);
        assert_eq!(s.now, t, "the game's clock stops while the menu shows");
        s.tick(6000);
        let menu = s.render();
        assert_ne!(menu, game);
        // the item labels (RIGHT.mov) are drawn in x 472..640: not black after the intro
        let lit = (100..420).any(|y| (480..630).any(|x| menu[(y * 640 + x) * 3 + 2] > 60));
        assert!(lit, "menu labels drawn");
        key(s, 0x1b);
        assert!(!s.r.menu_open, "Esc in the menu = Return");
        key(s, 0x1b);
        click(s, 555, 240); // "Return"
        assert!(!s.r.menu_open);
        s.tick(100);
        assert!(s.now > t, "the clock runs again");
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H823 / C66 / C71: the menu's 4th item reads "Return" (return.pic 0x11cbf over RIGHT.mov at (472,215)–(640,273)) while
/// a game runs (fo-086 k01), and RIGHT.mov's own "Return" after a game end (fo-091 k14) — only that rect changes; the
/// Save item is not clickable without a running game.
#[test]
fn h823_return_label_depends_on_a_running_game() {
    let d = tmp("ret");
    with_save("fo-056-end.x", &d, |s| {
        key(s, 0x1b);
        s.tick(6000);
        let running = s.render();
        s.r.game_running = false;
        let ended = s.render();
        let px = |fb: &[u8], x: usize, y: usize| fb[(y * 640 + x) * 3..(y * 640 + x) * 3 + 3].to_vec();
        let inside = |x: usize, y: usize| (472..640).contains(&x) && (215..273).contains(&y);
        let diff_in = (215..273).any(|y| (472..640).any(|x| px(&running, x, y) != px(&ended, x, y)));
        let diff_out = (0..480).any(|y| (0..640).any(|x| !inside(x, y) && px(&running, x, y) != px(&ended, x, y)));
        assert!(diff_in, "return.pic drawn only while a game runs");
        assert!(!diff_out);
        assert_eq!(s.menu_hover(555, 195), None, "no Save item after a game end");
        s.r.game_running = true;
        assert_eq!(s.menu_hover(555, 195), Some(MenuChoice::Save));
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H169: the top-right engine object (600,10) = Esc.
#[test]
fn top_right_object_opens_the_menu() {
    let d = tmp("top");
    with_save("fo-056-end.x", &d, |s| {
        click(s, 600, 10);
        assert!(s.r.menu_open);
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H403 / H400 / H405: Esc → "Save" → the save screen (0x14269) → a typed name + Enter writes `<NAME>.x` in the
/// slot directory; the file reads back as the session's state (View, variables, flags, inventories), differs from the
/// loaded save only in the fields the session changed, and the menu comes back after the save (H143).
#[test]
fn save_round_trip() {
    let d = tmp("save");
    with_save("fo-056-end.x", &d, |s| {
        key(s, 0x1b);
        click(s, 555, 195);
        assert!(!s.r.menu_open);
        assert!(shown(s, 0x14269), "save screen");
        for c in "AU TEST".chars() {
            key(s, c as u32);
        }
        assert_eq!(s.r.w.save_slot_text(0), "AU TEST");
        key(s, 0x0d);
        let file = d.join("AU TEST.x");
        assert!(file.is_file(), "{:?}", s.log);
        assert_eq!(Slots::open(&d).names[0], "AU TEST");
        assert!(s.r.menu_open, "the menu object after the save (H143)");
        let g = Hdb::open(&file).unwrap();
        let st = SaveState::from_gam(&g);
        assert_eq!(st.start[3], s.r.w.view.unwrap());
        assert_eq!(st.start[3], 0x319);
        let base = Hdb::open(root().join("data/bot/fo-056-end.x")).unwrap();
        let want = s.save_state(&SaveState::from_gam(&base));
        assert_eq!(norm(st.clone()), norm(want));
        assert_eq!(st.inventories, SaveState::from_gam(&base).inventories);
        // only the player position may differ from the original's save loaded unchanged
        let diff = g.d.iter().zip(&base.d).filter(|(a, b)| a != b).count();
        assert!(g.d.len() == base.d.len() && diff <= 8, "{diff} bytes differ");
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H403: an empty name does not save (the original shows a message box and does nothing else).
#[test]
fn save_without_a_name_writes_nothing() {
    let d = tmp("noname");
    with_save("fo-056-end.x", &d, |s| {
        key(s, 0x1b);
        click(s, 555, 195);
        key(s, 0x0d);
        assert!(std::fs::read_dir(&d).unwrap().next().is_none());
        assert!(shown(s, 0x14269), "the screen stays");
        key(s, 0x1b); // Esc on the save screen = cancel (SaveLoadComit(0))
        settle(s, 100, 60_000);
        assert!(!shown(s, 0x14269));
        assert!(s.r.menu_open, "back to the menu");
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H403: the load screen lists the slots; slot + "Load" asks the host to switch; the reloaded session stands in the
/// saved View with the save's variables.
#[test]
fn load_switches_the_session() {
    let d = tmp("load");
    std::fs::copy(root().join("data/bot/fo-056-end.x"), d.join("COOK.x")).ok();
    std::fs::write(d.join("slots.txt"), "COOK\n\n\n").unwrap();
    let target = root().join("data/bot/fo-056-end.x");
    with_save("fo-2c9-door.x", &d, |s| {
        if !target.exists() {
            return;
        }
        assert_eq!(s.r.w.view, Some(0x2c9));
        key(s, 0x1b);
        click(s, 555, 145);
        assert!(shown(s, 0x142ca), "load screen");
        assert_eq!(s.r.w.save_slot_text(0), "COOK");
        click(s, 320, 178);
        click(s, 390, 395);
        let req = host::request(s);
        assert_eq!(req, Some(HostReq::Load(d.join("COOK.x"))));
        let g: &'static Hdb = Box::leak(Box::new(Hdb::open(d.join("COOK.x")).unwrap()));
        s.reload(g, true);
        settle_timers(s, 100, 900_000);
        assert_eq!(s.r.w.view, Some(0x319));
        assert!(!s.r.menu_open);
        assert_eq!(s.r.w.gam.d.len(), g.d.len());
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H404: "Quit" asks the host to quit; "New" to start over (FUN_0042a810(3) / (4); their save question is not
/// modelled).
#[test]
fn quit_and_new_requests() {
    let d = tmp("quit");
    with_save("fo-056-end.x", &d, |s| {
        key(s, 0x1b);
        s.tick(6000);
        click(s, 555, 400);
        assert_eq!(host::request(s), Some(HostReq::Quit));
        assert!(!s.r.menu_open);
        key(s, 0x1b);
        click(s, 570, 101);
        assert_eq!(host::request(s), Some(HostReq::New));
        assert_eq!(host::request(s), None);
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H404: Options (0xc24a) and Help (0xc51c) are the game's own layouts; "Return to main menu" (ReturnMainMenu, H191)
/// brings the menu back.
#[test]
fn options_and_help() {
    let d = tmp("opt");
    with_save("fo-056-end.x", &d, |s| {
        key(s, 0x1b);
        click(s, 555, 285);
        assert!(shown(s, 0xc24a));
        assert!(!s.r.menu_open);
        click(s, 510, 460);
        assert!(s.r.menu_open, "{:?}", s.r.log.iter().rev().take(5).collect::<Vec<_>>());
        s.menu_choose(MenuChoice::Help);
        settle(s, 100, 60_000);
        assert!(shown(s, 0xc51c));
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H401 / H470: a game started from XFiles.gam is saved without any save of the original (built from XFiles.gam,
/// `Rules::h470_save_builder`); with the rule off it needs a template (H401). The result holds the session's View and
/// every variable of the session, and passes the container checks.
#[test]
fn new_game_save_without_a_template() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    for builder in [false, true] {
        let mut rules = Rules::all();
        rules.h470_save_builder = builder;
        let mut s = Session::with_rules(&h, &g, &d, rules);
        s.r.new_game();
        // up to the first View (the intro plays on: skip media)
        for _ in 0..200 {
            settle(&mut s, 100, 600_000);
            if s.r.w.view.is_some() && s.idle() {
                break;
            }
            if let Some(m) = s.r.playing() {
                s.r.media_ended(m);
            }
        }
        if !builder {
            assert!(s.save_bytes().is_err(), "no template → no save");
            continue;
        }
        // H650: SetLanguage 0x10dec at game start wrote the text (AssetRef 0x10dea = XfilesT.dll 1091)
        assert_eq!(s.r.w.vars.text(xf::save::VAR_GAME_LANGUAGE).as_deref(), Some(&b"english"[..]));
        let (bytes, rep) = s.save_bytes().unwrap();
        assert!(rep.missing_vars.is_empty() && rep.missing_enabled.is_empty(), "{rep:?}");
        xf::neodb::Db::new(bytes.clone()).validate().unwrap();
        let st = SaveState::from_gam(&Hdb::from_bytes(bytes));
        assert_eq!(st.start[3], s.r.w.view.unwrap());
        assert!(st.strings.contains(&(xf::save::VAR_GAME_LANGUAGE, b"english\0".to_vec())));
        for (id, v, _) in s.r.w.vars.stored() {
            if let Some(v) = v {
                if !st.strings.iter().any(|x| x.0 == id) {
                    assert!(st.vars.iter().any(|x| x.0 == id && x.1 == v), "var {id:#x}");
                }
            }
        }
    }
}

/// H1680 (port choice): menu → Load while the intro video still plays: the load screen covers the video and takes
/// the left click (its slots / Cancel / Load), instead of H322's "click ignored: video playing".
#[test]
fn h1680_load_screen_takes_clicks_over_a_video() {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let saves = tmp("h1680");
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.set_saves(Slots::open(&saves));
    s.r.new_game();
    for _ in 0..100 {
        if s.r.video_playing().is_some() {
            break;
        }
        s.tick(100);
    }
    assert!(s.r.video_playing().is_some(), "the intro plays");
    host::key(&mut s, 0x1b);
    s.tick(100);
    assert!(host::button(&mut s, host::Button::Left, Some((555, 145))).is_some());
    s.tick(100);
    assert!(shown(&s, 0x142ca), "load screen");
    assert!(s.r.video_playing().is_some(), "the video still plays under it");
    let l = host::button(&mut s, host::Button::Left, Some((320, 178)));
    assert!(!l.as_deref().unwrap_or("").contains("video playing"), "{l:?}");
    let _ = std::fs::remove_dir_all(&saves);
}

/// RMS (dBFS) of a cue's PCM over its remaining range.
fn cue_db(c: &xf::front::Cue) -> f64 {
    let ch = c.pcm.channels.max(1) as usize;
    let v = &c.pcm.samples[c.pos * ch..c.end * ch];
    let rms = (v.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt();
    20.0 * (rms.max(1.0) / 32768.0).log10()
}

/// C290 / H1770: the first menu (cold start): LEFTmain.mov's sound alone, RIGHT.mov / MIDloop.amv from LEFTmain's end;
/// a hover plays RIGHT.mov's item range with its sound (the hover sound), a held hover / no item / a click are silent;
/// TOPro.mov sounds while hovered. The in-game Esc menu: no opening, the same hover sound.
#[test]
fn h1770_menu_hover_sound() {
    use xf::front::menu::{ITEMS, LEFT, MID_SOUND, RIGHT, TOP};
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, Rules::all());
    s.boot(xf::front::intro::Start::Menu);
    let assets = |s: &mut Session| s.audio_cues().iter().map(|c| c.asset).collect::<Vec<_>>();
    s.tick(1000);
    assert_eq!(assets(&mut s), vec![LEFT.0], "LEFTmain alone first");
    s.tick(5000);
    assert_eq!(assets(&mut s), vec![MID_SOUND, RIGHT.0], "RIGHT.mov's opening and MIDloop.amv at LEFTmain's end");
    s.tick(5000);
    assert_eq!(assets(&mut s), vec![MID_SOUND], "idle");
    // hover "New": RIGHT.mov 6200 → 6700 with its sound
    host::motion(&mut s, Some((570, 101)));
    s.tick(50);
    let cues = s.audio_cues();
    let r = cues.iter().find(|c| c.asset == RIGHT.0).expect("the hover sound");
    let ms = |f: usize| f as u64 * 1000 / r.pcm.rate as u64;
    assert_eq!((ms(r.start), ms(r.end)), (ITEMS[0].2, ITEMS[0].3));
    assert!((ITEMS[0].2 + 40..ITEMS[0].2 + 60).contains(&ms(r.pos)));
    assert!(cue_db(r) > -40.0, "audible: {:.1} dBFS", cue_db(r));
    s.tick(600);
    assert_eq!(assets(&mut s), vec![MID_SOUND], "held at the chosen time: silent");
    // another item restarts the sound in its range; leaving the items stops it
    host::motion(&mut s, Some((555, 145)));
    s.tick(20);
    let cues = s.audio_cues();
    let r = cues.iter().find(|c| c.asset == RIGHT.0).expect("Load's hover sound");
    assert_eq!(r.start as u64 * 1000 / r.pcm.rate as u64, ITEMS[1].2);
    host::motion(&mut s, Some((300, 300)));
    s.tick(20);
    assert_eq!(assets(&mut s), vec![MID_SOUND]);
    // TOPro loops its sound while the pointer is in its rect
    host::motion(&mut s, Some((200, 30)));
    s.tick(20);
    assert_eq!(assets(&mut s), vec![MID_SOUND, TOP.0]);
    host::motion(&mut s, Some((300, 300)));
    // a click: the click sound, RIGHT.mov seeks (silent)
    host::motion(&mut s, Some((570, 101)));
    s.tick(20);
    host::button(&mut s, host::Button::Left, Some((570, 101)));
    s.tick(20);
    assert!(!assets(&mut s).contains(&RIGHT.0), "a click is silent but for its click sound");

    // the in-game Esc menu: no opening movies, the same hover sound
    let d2 = tmp("h1770");
    with_save("fo-056-end.x", &d2, |s| {
        key(s, 0x1b);
        s.tick(20);
        assert_eq!(s.audio_cues().iter().map(|c| c.asset).collect::<Vec<_>>(), vec![MID_SOUND], "no opening");
        host::motion(s, Some((555, 145)));
        s.tick(20);
        assert!(s.audio_cues().iter().any(|c| c.asset == RIGHT.0), "the Esc menu's hover sound");
    });
    let _ = std::fs::remove_dir_all(&d2);
}

/// H1735 (H1960): every filled slot of the load screen shows its name — `typed_texts` lists all three slot items.
#[test]
fn h1960_load_screen_lists_every_filled_slot() {
    let d = tmp("h1960");
    for n in ["ONE", "TWO", "THREE"] {
        std::fs::copy(root().join("data/bot/fo-056-end.x"), d.join(format!("{n}.x"))).ok();
    }
    std::fs::write(d.join("slots.txt"), "ONE\nTWO\nTHREE\n").unwrap();
    with_save("fo-2c9-door.x", &d, |s| {
        if !d.join("THREE.x").exists() {
            return;
        }
        key(s, 0x1b);
        click(s, 555, 145);
        assert!(shown(s, 0x142ca), "load screen");
        let t = s.r.w.typed_texts();
        eprintln!("{t:?}");
        for (i, (item, name)) in [(0x1438a, "ONE"), (0x1438b, "TWO"), (0x1438c, "THREE")].into_iter().enumerate() {
            assert!(t.iter().any(|x| x.0 == item && x.1 == name), "row {i}: {t:?}");
        }
        // drawn: white ink (fo-086 k05.png) in every row's rect — black ink vanished on rows 2 / 3 (the H1735 bug)
        let fb = s.render();
        for (i, y0) in [166usize, 224, 282].into_iter().enumerate() {
            let white = (y0..y0 + 24)
                .flat_map(|y| (200..300).map(move |x| (x, y)))
                .filter(|&(x, y)| fb[(y * 640 + x) * 3..][..3] == [255, 255, 255])
                .count();
            assert!(white > 20, "row {i}: {white} white pixels");
        }
    });
    let _ = std::fs::remove_dir_all(&d);
}

/// H2280 (bug report from the web front: "only the first save can be chosen"): with two filled slots, a click on the
/// second slot (through the host's button down / up, as the window and the web front) moves the load screen's
/// selection bar 0x14578 there and "Load" loads that slot's file; Down moves the bar too. Flag off: the bar stays on
/// the first slot although the second is selected (what the player saw).
#[test]
fn h2280_load_screen_bar_follows_the_selected_slot() {
    for on in [true, false] {
        let d = tmp(&format!("h2280-{on}"));
        for n in ["123", "321"] {
            std::fs::copy(root().join("data/bot/fo-056-end.x"), d.join(format!("{n}.x"))).ok();
        }
        std::fs::write(d.join("slots.txt"), "123\n321\n\n").unwrap();
        let dd = d.clone();
        with_save_rules("fo-2c9-door.x", &d, Rules { h2280_save_slot_bar: on, ..Rules::all() }, move |s| {
            if !dd.join("321.x").exists() {
                return;
            }
            // the bar's picture: its top-left (x, y)
            let bar = |s: &mut Session| {
                let list = s.r.w.h.read(xf::world::class::INTERFACE_ITEM, 0x14578).map(|r| r.u32("f44")).unwrap();
                let pics = s.r.w.h.list_of(xf::world::class::ASSET_REF_LIST, list);
                s.region_pictures().into_iter().find(|p| pics.contains(&p.2)).map(|p| (p.0, p.1))
            };
            let hclick = |s: &mut Session, x: i32, y: i32| {
                host::motion(s, Some((x, y)));
                settle(s, 100, 1_000);
                host::button(s, host::Button::Left, Some((x, y)));
                settle(s, 100, 1_000);
                host::release(s, host::Button::Left, Some((x, y)));
                settle(s, 100, 60_000);
            };
            let (first, second) = if on { (164, 222) } else { (165, 165) };
            key(s, 0x1b);
            click(s, 555, 145);
            assert!(shown(s, 0x142ca), "load screen");
            assert_eq!((s.r.w.save_slot, bar(s)), (0, Some((194, first))));
            hclick(s, 250, 236);
            assert_eq!((s.r.w.save_slot, bar(s)), (1, Some((194, second))));
            // the moved bar does not take the clicks: the first slot is selected again, then the second by Down
            hclick(s, 250, 178);
            assert_eq!((s.r.w.save_slot, bar(s)), (0, Some((194, first))));
            key(s, 0x28);
            assert_eq!((s.r.w.save_slot, bar(s)), (1, Some((194, second))));
            hclick(s, 390, 395);
            assert_eq!(host::request(s), Some(HostReq::Load(dd.join("321.x"))));
        });
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// H2280, save screen: a click on the third slot moves the selection bar 0x14572 there; Up moves it to the second.
#[test]
fn h2280_save_screen_bar_follows_the_selected_slot() {
    let d = tmp("h2280-save");
    with_save("fo-056-end.x", &d, |s| {
        let bar = |s: &mut Session| {
            let list = s.r.w.h.read(xf::world::class::INTERFACE_ITEM, 0x14572).map(|r| r.u32("f44")).unwrap();
            let pics = s.r.w.h.list_of(xf::world::class::ASSET_REF_LIST, list);
            s.region_pictures().into_iter().find(|p| pics.contains(&p.2)).map(|p| (p.0, p.1))
        };
        key(s, 0x1b);
        click(s, 555, 195);
        assert!(shown(s, 0x14269), "save screen");
        assert_eq!((s.r.w.save_slot, bar(s)), (0, Some((194, 165))));
        click(s, 250, 294);
        assert_eq!((s.r.w.save_slot, bar(s)), (2, Some((194, 280))));
        key(s, 0x26);
        assert_eq!((s.r.w.save_slot, bar(s)), (1, Some((194, 222))));
    });
    let _ = std::fs::remove_dir_all(&d);
}
