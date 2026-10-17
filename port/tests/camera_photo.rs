//! The NSA sedan photo (C108) and the warehouse music on travel (H1060), driven like a human through the input path
//! the SDL window and the web worker share (`front::host::motion` / `button` / `release`; the web's xf_motion /
//! xf_button / xf_release are thin wrappers): pointer moves, separate button down / up, the clock in 20-ms frames.
//! From `data/bot/fo-126-end.x` (dock, Skinner enabled at the warehouse front) along fo-127's walk (`traces/fo-127.md`).
mod common;
use std::path::PathBuf;
use xf::front::{host, settle_timers, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

const SEDAN_VIDEO: u32 = 0x5bb6;
const NSA_VIDEO: u32 = 0x5bbe;
/// "Warehouse Exterior.Music" (XS/52337.mus), started by the car Explorable's click list (0x16079)
const WAREHOUSE_MUSIC: u32 = 0xcc71;
const CAMERA: (i32, i32) = (211, 440);

fn wait(s: &mut Session, ms: u64) {
    for _ in 0..ms / 20 {
        s.tick(20);
    }
}

fn wait_video(s: &mut Session) {
    for _ in 0..5000 {
        if s.r.video_playing().is_none() {
            break;
        }
        s.tick(20);
    }
}

/// Pointer glides to (x, y) in 16 ms frames (window / browser mouse-move events).
fn glide(s: &mut Session, from: (i32, i32), to: (i32, i32), frames: i32) {
    for k in 1..=frames {
        let x = from.0 + (to.0 - from.0) * k / frames;
        let y = from.1 + (to.1 - from.1) * k / frames;
        host::motion(s, Some((x, y)));
        s.tick(16);
    }
}

/// A human click: the pointer is there, button down, ~80 ms, button up.
fn click(s: &mut Session, x: i32, y: i32) -> Option<String> {
    host::motion(s, Some((x, y)));
    s.tick(16);
    let l = host::button(s, host::Button::Left, Some((x, y)));
    wait(s, 80);
    host::release(s, host::Button::Left, Some((x, y)));
    l
}

/// The session as the window / web start it (system-value conditions from fo-009, live RandGen, the game's cursor),
/// loaded from fo-126-end.x and walked (fo-127's 12 clicks, each after the playing video ended + 1 s) to the warehouse
/// front 0xc3b (`at_front`: Skinner's video 0x4f7f has just started); `at_sedan`: then the sedan (Explorable 0x7e5f)
/// enters View 0xc25. None without the game data.
fn at_front(h: &'static Hdb, g: &'static Hdb, rules: Rules) -> Option<Session<'static>> {
    let d = root().join("data/InstalledGame");
    let mut s = Session::with_rules(h, g, &d, rules);
    let t = common::trace("fo-009.jsonl")?;
    s.r.uneval = xf::world::run::Runner::uneval_from_trace(&t, h);
    s.r.w.live_rand = Some(0);
    s.game_cursor = true;
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    let walk = [
        (70, 210),
        (285, 211),
        (70, 210),
        (70, 210),
        (494, 210),
        (519, 177),
        (70, 210),
        (357, 210),
        (328, 210),
        (570, 210),
        (332, 210),
    ];
    for (x, y) in walk {
        wait_video(&mut s);
        wait(&mut s, 1000);
        click(&mut s, x, y);
    }
    Some(s)
}

fn at_sedan(h: &'static Hdb, g: &'static Hdb, rules: Rules) -> Option<Session<'static>> {
    let mut s = at_front(h, g, rules)?;
    wait_video(&mut s); // Skinner's video 0x4f7f at the front enables the sedan Explorable
    wait(&mut s, 1500);
    let hit = click(&mut s, 475, 210);
    assert_eq!(hit.as_deref(), Some("click 0x7e5f (class 0x37)"), "the sedan Explorable");
    assert_eq!(s.r.w.view, Some(0xc25));
    Some(s)
}

fn open() -> Option<(&'static Hdb, &'static Hdb)> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-126-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    common::trace_path("fo-009.jsonl")?; // at_front's system-value conditions
    let h: &'static Hdb = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).ok()?));
    let g: &'static Hdb = Box::leak(Box::new(Hdb::open(&save).ok()?));
    Some((h, g))
}

/// After 0xc25's entry, the pointer goes down to the inventory strip (the panel shows), along the bar to the camera and
/// clicks it `at_ms` after the entry; then 15 s pass. Returns the session.
fn camera_at(h: &'static Hdb, g: &'static Hdb, at_ms: u64) -> Option<Session<'static>> {
    let mut s = at_sedan(h, g, Rules::all())?;
    let t0 = s.now;
    glide(&mut s, (475, 210), (20, 440), 40);
    assert!(s.r.inv_panel, "the panel shows when the pointer enters the strip");
    glide(&mut s, (20, 440), CAMERA, 30);
    while s.now - t0 + 16 + 80 < at_ms {
        s.tick(16);
    }
    let l = click(&mut s, CAMERA.0, CAMERA.1);
    assert_eq!(l.as_deref(), Some("click 0x4412 (class 0x4b)"), "the camera at {at_ms} ms");
    wait(&mut s, 15_000);
    Some(s)
}

fn played(s: &Session, a: u32) -> bool {
    s.r.plays.iter().any(|p| p.1 == a)
}

/// C108: the camera clicked inside 0xc25's 10-s Timer window — early (3 s) or as late as the original's run (8.7 s) —
/// plays the sedan video 0x5bb6 and then the NSA video; 0x1459 bN2TookPictureOfNSADudes and 0x11c9 are set, the photo
/// (TakePhoto 0x1372e, the plate) is in the camera.
#[test]
fn c108_camera_photo_human_timing() {
    let Some((h, g)) = open() else { return };
    for at in [3_000, 8_700] {
        let s = camera_at(h, g, at).unwrap();
        assert!(played(&s, SEDAN_VIDEO) && played(&s, NSA_VIDEO), "camera at {at} ms: sedan + NSA videos");
        assert_eq!((s.r.w.vars.get(0x1459), s.r.w.vars.get(0x11c9)), (Some(1), Some(1)), "camera at {at} ms");
        assert_eq!(s.r.w.pc.photos, vec![(0x1372e, false)], "camera at {at} ms: the plate's photo");
    }
}

/// C108: after the 10-s window the NSA video has played (at 10 s) and the camera clicked after it takes no photo (0x421
/// iCurrViewSpecial reset by 0x5bbe's Deactivation).
#[test]
fn c108_camera_after_the_window() {
    let Some((h, g)) = open() else { return };
    let s = camera_at(h, g, 16_000).unwrap();
    assert!(played(&s, NSA_VIDEO) && !played(&s, SEDAN_VIDEO));
    assert_eq!((s.r.w.vars.get(0x1459), s.r.w.vars.get(0x11c9)), (Some(0), Some(1)));
}

/// The cue list the window / web mixer follows (`Session::audio_cues`) holds `a`.
fn sounds(s: &mut Session, a: u32) -> bool {
    s.audio_cues().iter().any(|c| c.asset == a)
}

/// H1060: the warehouse music (a played, non-looping .mus player) still sounds when the PDA travel to the Crime Lab
/// runs; the travel removes it (FUN_00493090 removes every player) — no cue of it at the Crime Lab. Rule off: it kept
/// playing over the Crime Lab (a player report).
#[test]
fn h1060_travel_stops_the_warehouse_music() {
    let Some((h, g)) = open() else { return };
    for on in [true, false] {
        let mut s = at_sedan(h, g, Rules { h1060_travel_stops_played: on, ..Rules::all() }).unwrap();
        wait(&mut s, 11_000); // the NSA video (the window passed)
        wait_video(&mut s);
        assert!(sounds(&mut s, WAREHOUSE_MUSIC), "the music plays at 0xc25");
        glide(&mut s, (320, 200), (20, 440), 20);
        wait(&mut s, 300);
        assert_eq!(click(&mut s, 21, 440).as_deref(), Some("click 0x442b (class 0x4b)"), "PDA");
        wait(&mut s, 1500);
        assert_eq!(click(&mut s, 239, 322).as_deref(), Some("click 0xe566 (class 0x4f)"), "map");
        wait(&mut s, 1500);
        assert_eq!(click(&mut s, 350, 267).as_deref(), Some("click 0xe6fd (class 0x4f)"), "Crime Lab dot");
        wait(&mut s, 1000);
        assert_eq!(s.r.w.view, Some(0x2a24), "the Crime Lab");
        assert_eq!(sounds(&mut s, WAREHOUSE_MUSIC), !on, "rule {on}: the warehouse music at the Crime Lab");
    }
}

/// H1773 (post-hoc C-OV1, `traces/fo-127.jsonl.shots/k14.png`): with the camera held — re-selected by SelectInv after the
/// sedan video, as on the original (C122) — the viewfinder 0x87bd lies over the playing NSA video, centred on the pointer
/// at the camera icon (211, 440): its white frame from (51, 320), clipped to the band (y < 330), and the red counter of
/// the exposures left (24 − 1 photo) at (+4, +8). The night-vision tint applies over the video frame too (H1041).
#[test]
fn h1773_viewfinder_over_the_nsa_video() {
    let Some((h, g)) = open() else { return };
    let mut s = at_sedan(h, g, Rules::all()).unwrap();
    let t0 = s.now;
    glide(&mut s, (475, 210), (20, 440), 40);
    glide(&mut s, (20, 440), CAMERA, 30);
    while s.now - t0 + 96 < 8_700 {
        s.tick(16);
    }
    click(&mut s, CAMERA.0, CAMERA.1);
    for _ in 0..400 {
        if s.r.video_playing() == Some(NSA_VIDEO) {
            break;
        }
        s.tick(20);
    }
    wait(&mut s, 1500);
    assert_eq!(s.r.video_playing(), Some(NSA_VIDEO), "the NSA video plays");
    assert_eq!((s.r.w.dev.held, s.r.w.dev.held_overlay), (Some(2), true), "the camera held again (SelectInv)");
    let fb = s.render();
    let px = |x: usize, y: usize| [fb[(y * 640 + x) * 3], fb[(y * 640 + x) * 3 + 1], fb[(y * 640 + x) * 3 + 2]];
    // k14: white at (51..=71, 320..=323) and (51..=54, 324..=329); the video inside (55, 324)
    for (x, y) in [(51, 320), (60, 321), (71, 323), (51, 329), (54, 326)] {
        assert_eq!(px(x, y), [255, 255, 255], "viewfinder frame at ({x}, {y})");
    }
    assert_ne!(px(60, 325), [255, 255, 255], "the video inside the frame");
    assert_eq!(px(60, 331), [0, 0, 0], "clipped to the band");
    let red = (54..80).flat_map(|x| (324..330).map(move |y| (x, y))).filter(|&(x, y)| {
        let p = px(x, y);
        p[0] > 180 && p[1] < 80 && p[2] < 80
    });
    assert!(red.count() > 0, "the red counter (23)");
    // the goggles' tint over the same video frame (drawing does not depend on a video playing)
    s.r.w.dev.held = Some(4);
    let nv = s.render();
    let r = (100..300).map(|x| nv[(200 * 640 + x) * 3] as u32).sum::<u32>();
    assert_eq!(r, 0, "night vision: R = 0 over the video");
}

/// At the warehouse front during Skinner's video: the pointer into the strip (the panel shows), along the bar, the camera
/// icon clicked (selected: held type 2), the pointer back into the View; then Skinner's video ends.
fn hold_camera_at_front(s: &mut Session) {
    glide(s, (332, 210), (20, 440), 30);
    glide(s, (20, 440), CAMERA, 20);
    click(s, CAMERA.0, CAMERA.1);
    assert_eq!(s.r.w.dev.held, Some(2), "the camera held");
    glide(s, CAMERA, (475, 210), 30);
    assert_eq!(s.r.w.dev.held, Some(2), "still held in the View");
    wait_video(s);
    wait(s, 1500);
}

/// HYPOTHESIS H2170 (reported: "camera selected, walk up to the car, the cutscene starts, no photo"): with the camera
/// held (selected at the front: the held-item flag set), the click on the sedan is the camera's shot (FUN_004f26a0: an exposure
/// used, nothing under the viewfinder reacts to the camera) — no transition to 0xc25, no photo. Dropped on the strip
/// (H361), the sedan click enters 0xc25 and the camera icon clicked inside the 10-s window takes the plate (C108).
#[test]
fn h2170_held_camera_shot_then_the_icon_photo() {
    let Some((h, g)) = open() else { return };
    let mut s = at_front(h, g, Rules::all()).unwrap();
    hold_camera_at_front(&mut s);
    let l = click(&mut s, 475, 210);
    assert_eq!(l.as_deref(), Some("click 0xffffff1c (class 0x0)"), "the held camera takes the click");
    assert_eq!(s.r.w.view, Some(0xc3b), "no transition with the camera held");
    assert_eq!(s.r.w.pc.camera, 1, "one exposure used");
    // C306 (fo-254-end.x: VCPhoto f64 0x82ea, rect 280,0,600,240): the shot is a photo of 0xc3b's background loop
    assert_eq!(s.r.w.pc.photos, vec![(0x82ea, false)], "the shot's photo of the View's loop");
    assert_eq!(s.r.w.pc.photo_frames[0].map(|f| f.1), Some([280, 0, 600, 240]));
    glide(&mut s, (475, 210), (20, 440), 30);
    assert_eq!(s.r.w.dev.held, None, "dropped on entering the strip (H361)");
    glide(&mut s, (20, 440), (475, 210), 30);
    wait_video(&mut s);
    wait(&mut s, 500);
    assert_eq!(click(&mut s, 475, 210).as_deref(), Some("click 0x7e5f (class 0x37)"), "the sedan");
    assert_eq!(s.r.w.view, Some(0xc25));
    glide(&mut s, (475, 210), (20, 440), 40);
    glide(&mut s, (20, 440), CAMERA, 30);
    assert_eq!(click(&mut s, CAMERA.0, CAMERA.1).as_deref(), Some("click 0x4412 (class 0x4b)"));
    wait(&mut s, 15_000);
    assert!(played(&s, SEDAN_VIDEO) && played(&s, NSA_VIDEO));
    assert_eq!(s.r.w.vars.get(0x1459), Some(1));
    assert_eq!(s.r.w.pc.photos, vec![(0x82ea, false), (0x1372e, false)], "the shot's and the plate's photos (fo-254-end.x)");
    assert_eq!(s.r.w.pc.camera, 2, "the shot and the plate");
}

/// H2170 off (the port before): the held camera is ignored by the hit test — the sedan click walks into 0xc25 with the
/// camera held, a click on the car there does nothing and the window passes without a photo (a player report).
#[test]
fn h2170_off_held_camera_walks_in_without_photo() {
    let Some((h, g)) = open() else { return };
    let mut s = at_front(h, g, Rules { h2170_held_camera_click: false, ..Rules::all() }).unwrap();
    hold_camera_at_front(&mut s);
    assert_eq!(click(&mut s, 475, 210).as_deref(), Some("click 0x7e5f (class 0x37)"), "the sedan, camera held");
    assert_eq!((s.r.w.view, s.r.w.dev.held), (Some(0xc25), Some(2)));
    wait(&mut s, 2000);
    click(&mut s, 475, 210);
    wait(&mut s, 15_000);
    assert!(played(&s, NSA_VIDEO) && !played(&s, SEDAN_VIDEO));
    assert_eq!(s.r.w.vars.get(0x1459), Some(0));
    assert!(s.r.w.pc.photos.is_empty());
}

/// From the entry into 0xc25 (`at_sedan`): wait for the Timer's car-leaving video 0x5bbe (+10.1 s on the original,
/// fo-259) and `into` ms more, the pointer down to the strip, along the bar, the camera icon clicked. Returns the session
/// (the camera now selected).
fn icon_during_car_video(h: &'static Hdb, g: &'static Hdb, rules: Rules, into: u64) -> Session<'static> {
    let mut s = at_sedan(h, g, rules).unwrap();
    glide(&mut s, (475, 210), (20, 440), 40);
    glide(&mut s, (20, 440), CAMERA, 30);
    for _ in 0..1000 {
        if s.r.video_playing() == Some(NSA_VIDEO) {
            break;
        }
        s.tick(16);
    }
    assert_eq!(s.r.video_playing(), Some(NSA_VIDEO), "the car-leaving video");
    wait(&mut s, into);
    let l = click(&mut s, CAMERA.0, CAMERA.1);
    assert_eq!(l.as_deref(), Some("click 0x4412 (class 0x4b) while video 0x5bbe plays"));
    s
}

/// C306 (fo-259, a player report: "camera in hand during the car leaving"): during the Timer's car video the
/// camera icon takes no photo (TakePhoto cond 0) — it selects the camera; the click on the car is the camera's shot and
/// stores a photo of the video frame (f64 0x5bbe, rect 100,0,420,240 for the pointer at x 260), count 1; 0x1459 stays 0
/// (the story's plate photo is the icon's before the video, C108). The photo's picture = that frame's crop, as the
/// original's thumbnail in fo-259-end.x. Variant fo-260: the shot at (590,110) → rect 280,0,600,240. Rule off: no photo.
#[test]
fn c306_shot_during_the_car_video_is_a_frame_photo() {
    let Some((h, g)) = open() else { return };
    let mut s = icon_during_car_video(h, g, Rules::all(), 1200);
    assert_eq!(s.r.w.dev.held, Some(2), "the camera selected");
    assert!(s.r.w.pc.photos.is_empty() && !played(&s, SEDAN_VIDEO), "no TakePhoto during the car video");
    glide(&mut s, CAMERA, (260, 230), 20);
    let l = click(&mut s, 260, 230);
    assert_eq!(l.as_deref(), Some("click 0xffffff1c (class 0x0) while video 0x5bbe plays"), "the camera's shot");
    assert_eq!((s.r.w.pc.photos.clone(), s.r.w.pc.camera), (vec![(NSA_VIDEO, false)], 1), "a photo of the car video");
    let (f68, rect) = s.r.w.pc.photo_frames[0].unwrap();
    assert_eq!(rect, [100, 0, 420, 240]);
    assert!((900..2400).contains(&f68), "the video's time (1/600 s): {f68}");
    let pic = s.photo_picture(NSA_VIDEO, Some((f68, rect))).expect("the frame's crop");
    assert_eq!((pic.width, pic.height), (320, 240));
    // against the original's thumbnail of its shot (fo-259-end.x, 2.9 s into the video): the same 320×240 crop at 1732
    if let Ok(o) = Hdb::open(root().join("data/bot/fo-259-end.x")) {
        let r = o.read(0x59, o.class(0x59).unwrap().objects[0].0).unwrap();
        let thumb = xf::media::pict::decode(o.blob_bytes(r.blob("data").unwrap()).unwrap()).unwrap();
        let port = xf::media::pict::scale(&s.photo_picture(NSA_VIDEO, Some((1732, rect))).unwrap(), 80, 60);
        let err: u64 = thumb.rgb.iter().zip(&port.rgb).map(|(&a, &b)| (a as i32 - b as i32).unsigned_abs() as u64).sum();
        assert!(err / (80 * 60 * 3) < 12, "the original's thumbnail = the frame's crop (mean error {})", err / 14400);
    }
    wait(&mut s, 15_000);
    assert_eq!(s.r.w.vars.get(0x1459), Some(0), "not the story's plate photo");
    let st = s.save_state(&xf::save::SaveState::from_gam(g));
    let ph = st.photos.iter().find(|p| p.picture() == NSA_VIDEO).expect("saved");
    assert_eq!((ph.f68(), ph.rect(), ph.thumb.is_empty()), (f68, rect, false));

    let mut s = icon_during_car_video(h, g, Rules::all(), 1200);
    glide(&mut s, CAMERA, (590, 110), 20);
    click(&mut s, 590, 110);
    assert_eq!(s.r.w.pc.photo_frames[0].map(|f| f.1), Some([280, 0, 600, 240]), "fo-260's rect");

    let mut s = icon_during_car_video(h, g, Rules { c306_shot_photo: false, ..Rules::all() }, 1200);
    glide(&mut s, CAMERA, (260, 230), 20);
    click(&mut s, 260, 230);
    assert_eq!((s.r.w.pc.photos.len(), s.r.w.pc.camera), (0, 1), "rule off: the shot only counts");
}

/// C306 (`traces/fo-261.jsonl`): the camera icon before the car video takes the plate (C108); the camera re-selected
/// after 0x5bb6 shoots during 0x5bbe — a second photo, of the car video's frame; count 2, 0x1459 = 1.
#[test]
fn c306_plate_then_a_shot_of_the_car_leaving() {
    let Some((h, g)) = open() else { return };
    let mut s = camera_at_no_wait(h, g, 4_800);
    for _ in 0..400 {
        if s.r.video_playing() == Some(NSA_VIDEO) {
            break;
        }
        s.tick(20);
    }
    wait(&mut s, 1500);
    assert_eq!(s.r.w.dev.held, Some(2), "re-selected by SelectInv");
    glide(&mut s, CAMERA, (260, 230), 20);
    click(&mut s, 260, 230);
    assert_eq!(s.r.w.pc.photos, vec![(0x1372e, false), (NSA_VIDEO, false)]);
    assert_eq!(s.r.w.pc.camera, 2);
    wait(&mut s, 15_000);
    assert_eq!(s.r.w.vars.get(0x1459), Some(1));
}

/// `camera_at` without the 15 s after the click.
fn camera_at_no_wait(h: &'static Hdb, g: &'static Hdb, at_ms: u64) -> Session<'static> {
    let mut s = at_sedan(h, g, Rules::all()).unwrap();
    let t0 = s.now;
    glide(&mut s, (475, 210), (20, 440), 40);
    glide(&mut s, (20, 440), CAMERA, 30);
    while s.now - t0 + 16 + 80 < at_ms {
        s.tick(16);
    }
    assert_eq!(click(&mut s, CAMERA.0, CAMERA.1).as_deref(), Some("click 0x4412 (class 0x4b)"));
    s
}
