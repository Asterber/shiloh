//! The held gun (value 9) and the tool selections (H450–H453): a View shot (Location video + the View's Mouse
//! Click), the gun on a CharView with / without a gun reaction (the shot sound 0x167f4 comes from the HDB trigger; the
//! code-side shot is empty in this build), the binoculars / flashlight / night-vision comment 2 s after the selection
//! (fo-073 k1) and a playing video swallowing a click in the movie area (fo-073 k2).
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[derive(Debug, Default)]
struct Step {
    hit: Option<u32>,
    posts: Vec<u32>,
    regs: Vec<u32>,
    plays: Vec<u32>,
    /// (runner clock, AssetRef) of the players registered in this step
    play_times: Vec<(u64, u32)>,
    /// H821: gun shots started in this step (runner clock, AssetRef)
    shots: Vec<(u64, u32)>,
    /// after the step: the menu shows / a game runs (H823)
    menu_open: bool,
    game_running: bool,
}

/// `m:X,Y` move, `p:X,Y` move + click (then settle with timers + 1.2 s, as tests/held.rs), `q:X,Y` move + click with no
/// settle, `w:MS` the clock runs MS ms (100 ms ticks).
fn run(save: &str, steps: &str, rules: Rules) -> Option<Vec<Step>> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let mut out = Vec::new();
    for st in steps.split(';') {
        let (p0, r0, pl0, sh0) = (s.r.posts.len(), s.r.regs.len(), s.r.plays.len(), s.r.shots.len());
        let (kind, xy) = st.split_at(2);
        let v: Vec<i32> = xy.split(',').map(|x| x.parse().unwrap()).collect();
        let hit = match kind {
            "m:" => s.hover_at(v[0], v[1]),
            "w:" => {
                for _ in 0..v[0] / 100 {
                    s.tick(100);
                }
                None
            }
            _ => {
                s.hover_at(v[0], v[1]);
                s.click_at(v[0], v[1])
            }
        };
        if kind == "p:" || kind == "m:" {
            settle_timers(&mut s, 100, 900_000);
            for _ in 0..12 {
                s.tick(100);
            }
        } else if kind == "q:" {
            s.tick(0);
        }
        out.push(Step {
            hit: hit.map(|c| c.owner),
            posts: s.r.posts[p0..].iter().map(|m| m.action).collect(),
            regs: s.r.regs[r0..].to_vec(),
            plays: s.r.plays[pl0..].iter().map(|p| p.1).collect(),
            play_times: s.r.plays[pl0..].iter().map(|p| (p.0, p.1)).collect(),
            shots: s.r.shots[sh0..].to_vec(),
            menu_open: s.r.menu_open,
            game_running: s.r.game_running,
        });
    }
    Some(out)
}

/// fo-064-end.x (Comity Inn lobby View 0x1994, Location 0x197b): the gun 0x4433 at (336..366) of the bar, the clerk
/// 0x5867 around (300,200), nothing at (420,150)
const GUN: &str = "m:20,440;p:351,440";
/// the fo-065 walk from fo-064-end.x to View 0x1995 (Skinner 0x58cb at (341,219))
const TO_SKINNER: &str = "p:570,210;p:70,210;p:70,210;p:570,210;p:157,275;p:320,210;p:246,309;p:570,210;p:570,210;p:70,210";

/// H450: a View click with the gun held that hits nothing else is the gun's click — in Comity Inn (0x197b) the video
/// 0x4cc6 is played. H821: the gun's shot 0x87b7 first, played to its end before the video (fo-082: the View's Mouse
/// Click 1.845 s after the button-down). Off: the point hits nothing.
#[test]
fn h450_h821_gun_shot_in_the_view() {
    let steps = format!("{GUN};p:420,150");
    let Some(on) = run("fo-064-end.x", &steps, Rules::all()) else { return };
    assert_eq!(on[1].hit, Some(0x4433));
    assert_eq!(on[2].hit, Some(xf::world::devices::GUN_OWNER));
    assert_eq!(on[2].plays, vec![0x4cc6]);
    assert_eq!(on[2].shots.iter().map(|s| s.1).collect::<Vec<_>>(), vec![0x87b7]);
    let (t_shot, t_video) = (on[2].shots[0].0, on[2].play_times[0].0);
    assert!(t_video >= t_shot + 1500, "the video waits for the shot: shot {t_shot}, video {t_video}");
    // H821 off (H453): no shot, the video at once
    let h453 = run("fo-064-end.x", &steps, Rules { h821_gun_shot_sound: false, ..Rules::all() }).unwrap();
    assert!(h453[2].shots.is_empty());
    assert_eq!(h453[2].plays, vec![0x4cc6]);
    let off = run("fo-064-end.x", &steps, Rules { h450_gun_view_click: false, ..Rules::all() }).unwrap();
    assert_eq!(off[2].hit, None);
    assert!(off[2].plays.is_empty() && off[2].posts.is_empty());
    // nothing held: the point hits nothing either
    let none = run("fo-064-end.x", "m:20,440;p:420,150", Rules::all()).unwrap();
    assert_eq!(none[1].hit, None);
}

/// C71 (fo-091, AX2b) / H820: the gun held, the pointer rests on Skinner 0x58cb → Roll Into → his refusal slot 5 =
/// hover video **0x4e25** (fo-091 104.993, ~1.0 s); the click after it ends → Mouse Click posts 0x748e / 0x7490 /
/// 0x1166b (trigger 0x748b reacts to the gun: 0x748e plays 0x167f4 = "FBI Issue/gun.amv", the gunshot) → at its media
/// end the Deactivation trigger 0x167f5 posts 0x167f8 → the arrest 0x4cc8. A reacting CharView: no code-side shot
/// (FUN_00436080 returns after the reaction test).
#[test]
fn c71_gun_on_skinner_hover_video_then_shot_then_arrest() {
    let steps = format!("{TO_SKINNER};{GUN};m:341,219;p:341,219");
    let Some(on) = run("fo-064-end.x", &steps, Rules::all()) else { return };
    let k = on.len() - 1;
    assert_eq!(on[k - 1].plays, vec![0x4e25], "hover video");
    assert!(on[k - 1].posts.is_empty());
    assert_eq!(on[k].hit, Some(0x58cb));
    assert!(on[k].posts.starts_with(&[0x748e, 0x7490, 0x1166b, 0x167f8]));
    assert_eq!(on[k].plays, vec![0x167f4, 0x4cc8]);
    assert!(on[k].shots.is_empty());
    // H823: 0x4cc8's end posts 0xc0f2 = EndGame(0xa15) → the main menu with no game running (fo-091 k14)
    assert!(on[k].posts.contains(&0xc0f2));
    assert!(on[k].menu_open && !on[k].game_running);
    assert!(!on[k - 1].menu_open && on[k - 1].game_running);
    // H820 off: no hover video
    let off = run("fo-064-end.x", &steps, Rules { h820_char_view_hover_refusal: false, ..Rules::all() }).unwrap();
    assert!(off[k - 1].plays.is_empty());
    assert_eq!(off[k].plays, vec![0x167f4, 0x4cc8]);
}

/// fo-081 (AX1, post-hoc): the pointer moved onto the clerk 0x5867 with the gun held → hover video 0x4e15 (his slot 5;
/// REG 61.62); the button-down 0.13 s later falls inside it → H452: the video takes the click, no Mouse Click, no posts.
#[test]
fn h820_h452_fo081_hover_video_swallows_the_click() {
    // the bot's `x:300,200` (e2e): move, then the button-down — H822: the move's commands run in between
    let steps = format!("{GUN};q:300,200");
    let Some(on) = run("fo-064-end.x", &steps, Rules::all()) else { return };
    assert_eq!(on[2].hit, Some(xf::world::devices::VIDEO_OWNER));
    assert_eq!(on[2].plays, vec![0x4e15]);
    assert!(on[2].posts.is_empty(), "{:x?}", on[2]);
}

/// H360 / H821: the gun on a CharView without a gun reaction (the clerk 0x5867), clicked after his hover video →
/// FUN_004cb440(gun, 0): the shot 0x87b7 to its end, then refusal slot 7 = 0x4cc8 (the arrest). Off (H453): 0x4cc8 at once.
#[test]
fn h821_gun_on_char_view_without_reaction_shot_then_arrest() {
    let steps = format!("{GUN};m:300,200;p:300,200");
    let Some(on) = run("fo-064-end.x", &steps, Rules::all()) else { return };
    assert_eq!(on[2].plays, vec![0x4e15]);
    assert_eq!(on[3].hit, Some(0x5867));
    assert_eq!(on[3].plays, vec![0x4cc8]);
    assert_eq!(on[3].shots.iter().map(|s| s.1).collect::<Vec<_>>(), vec![0x87b7]);
    assert!(on[3].play_times[0].0 >= on[3].shots[0].0 + 1500);
    let off = run("fo-064-end.x", &steps, Rules { h821_gun_shot_sound: false, ..Rules::all() }).unwrap();
    assert_eq!(off[3].plays, vec![0x4cc8]);
    assert!(off[3].shots.is_empty());
}

/// fo-073 (`traces/fo-073.jsonl`, from fo-056-end.x): k1 selects the flashlight (value 8) 57.83; H451: timer_arm
/// 57.831, 2.0 s later REG **0x4d52** (Cook 0x55e9's refusal slot 2) 59.833, blk_state 1
/// 59.94; k2 clicks Cook (308,201) at 61.607 while 0x4d52 plays (media end 62.691) → H452: no Mouse Click (0 posts).
/// The bot's timing: k1 → k2 3.8 s. Off (H451 + H452): no comment, Cook's 3 Mouse Click posts (the run-time core's
/// prediction, AR3).
#[test]
fn h451_h452_fo073_flashlight_comment_swallows_the_click() {
    let steps = "m:20,440;q:176,440;w:3800;q:308,201";
    let Some(on) = run("fo-056-end.x", steps, Rules::all()) else { return };
    assert_eq!(on[1].hit, Some(0x441e));
    assert!(on[2].regs.contains(&0x4d52) && on[2].plays.contains(&0x4d52));
    assert_eq!(on[3].hit, Some(xf::world::devices::VIDEO_OWNER));
    assert!(on[3].posts.is_empty());
    let off = run("fo-056-end.x", steps, Rules { h451_tool_comment: false, h452_video_player_hit: false, ..Rules::all() }).unwrap();
    assert!(!off[2].regs.contains(&0x4d52));
    assert_eq!(off[3].hit, Some(0x55e9));
    assert_eq!(off[3].posts, vec![0x769d, 0x7c4e, 0x7c4c]);
    // H451 alone (no H452): the comment plays but the click reaches Cook
    let h451 = run("fo-056-end.x", steps, Rules { h452_video_player_hit: false, ..Rules::all() }).unwrap();
    assert!(h451[2].plays.contains(&0x4d52));
    assert_eq!(h451[3].hit, Some(0x55e9));
}

/// H451: the comment comes 2000 ms after the selection, not before.
#[test]
fn h451_comment_after_2000_ms() {
    let Some(on) = run("fo-056-end.x", "m:20,440;q:176,440;w:1900;w:200", Rules::all()) else { return };
    assert!(!on[2].regs.contains(&0x4d52));
    assert!(on[3].regs.contains(&0x4d52));
}

/// H451 / AX5: the night vision (0x5545, value 0xd) → slot 3 of Cook's refusal list = 0x4d48, 2 s later.
#[test]
fn h451_night_vision_slot_3() {
    let Some(on) = run("fo-056-end.x", "m:20,440;q:71,440;w:2100", Rules::all()) else { return };
    assert_eq!(on[1].hit, Some(0x5545));
    assert!(on[2].regs.contains(&0x4d48));
}
