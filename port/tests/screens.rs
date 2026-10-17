//! The screens after the phone and the computer: the darkened scene under a full-screen layout (H620), the AI
//! icon (H621–H623) and the PDA note pad (H624). Frames rendered headless at the states of the bot's screenshots of the
//! original; sample pixels measured on the PNGs (`traces/fo-0NN.jsonl.shots/kNN.png`). Skipped without the game data.
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Load `save`, run `steps` (front::script syntax), settle after each, then `f` on the session.
fn session_at<T>(save: &str, steps: &str, rules: Rules, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, rules);
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
        s.render();
    }
    Some(f(&mut s))
}

fn frame_at(save: &str, steps: &str, rules: Rules) -> Option<Vec<u8>> {
    session_at(save, steps, rules, |s| s.render())
}

fn px(fb: &[u8], x: usize, y: usize) -> [u8; 3] {
    let o = (y * front::SCREEN_W + x) * 3;
    [fb[o], fb[o + 1], fb[o + 2]]
}

fn near(p: [u8; 3], c: [u8; 3], tol: u8) -> bool {
    p.iter().zip(c).all(|(a, b)| a.abs_diff(b) <= tol)
}

/// fo-057 k13: the PDA's Everett map (layout 0x84dc) over View 0x2bb — the View band and the AI icon darkened (H620),
/// the PDA itself not.
const FO057_K13: (&str, &str) = (
    "fo-056-end.x",
    "p:308,201;h:302,387,da72;p:302,333;p:591,210;p:570,210;p:283,211;p:570,210;p:309,209;p:318,210;m:20,440;p:71,440;\
     p:226,278;p:278,192",
);

#[test]
fn h620_pda_map_darkens_the_scene_fo057_k13() {
    let Some(fb) = frame_at(FO057_K13.0, FO057_K13.1, front::rules_hyp()) else { return };
    for (x, y, c) in [
        (40, 120, [11, 11, 10]),
        (100, 250, [12, 14, 12]),
        (560, 200, [12, 11, 8]),
        (600, 310, [14, 12, 8]),
        (563, 63, [27, 53, 54]),
        (300, 200, [197, 221, 191]),
        (250, 372, [43, 80, 38]),
    ] {
        assert!(near(px(&fb, x, y), c, 4), "({x},{y}) {:?} vs {c:?}", px(&fb, x, y));
    }
    // ablation: the View band at full brightness
    let off = Rules { h620_dim_under_layout: false, ..front::rules_hyp() };
    let Some(fb) = frame_at(FO057_K13.0, FO057_K13.1, off) else { return };
    assert!(!near(px(&fb, 40, 120), [11, 11, 10], 4));
}

/// fo-076 k05: the photo layout 0x1095d over View 0x4d5 (stereo clicked twice) — the scene darkened (H620).
#[test]
fn h620_photo_darkens_the_scene_fo076_k05() {
    let steps = "p:42,209;p:265,179;p:520,229;p:320,240;p:520,229";
    let Some(fb) = frame_at("fo-026-end.x", steps, front::rules_hyp()) else { return };
    for (x, y, c) in [
        (30, 300, [7, 7, 6]),
        (100, 300, [11, 8, 7]),
        (160, 280, [10, 7, 4]),
        (520, 200, [21, 16, 11]),
        (560, 150, [28, 24, 15]),
        (600, 300, [11, 9, 7]),
        (563, 63, [27, 53, 54]),
    ] {
        assert!(near(px(&fb, x, y), c, 4), "({x},{y}) {:?} vs {c:?}", px(&fb, x, y));
    }
}

/// H620's darkening (measured on fo-076 k04 → k05): 32 → 8, 48 → 11, 64 → 15, 80 → 19, 128 → 30, 160 → 38, 200 → 47,
/// 248 → 58.
#[test]
fn h620_dim_function() {
    for (v, d) in [(8, 2), (16, 4), (24, 6), (32, 8), (48, 11), (64, 15), (80, 19), (128, 30), (160, 38), (200, 47), (248, 58)] {
        assert_eq!(front::pda::dim(v), d, "{v}");
    }
}

/// H621: the AI icon lit (fo-063 k01: list 0xfa7f of fo-060-end.x, a condition holds) and dimmed (fo-064 k12: after
/// ClearAIList at the travel to Comity); ablation: always lit.
#[test]
fn h621_ai_icon_lit_and_dimmed() {
    let Some(fb) = frame_at("fo-060-end.x", "m:20,440", front::rules_hyp()) else { return };
    for (x, y, c) in [(563, 63, [115, 227, 230]), (555, 60, [39, 62, 97]), (570, 66, [40, 73, 116])] {
        assert!(near(px(&fb, x, y), c, 4), "lit ({x},{y}) {:?} vs {c:?}", px(&fb, x, y));
    }
    let steps = "p:320,210;m:20,440;p:21,440;p:239,322;p:350,267;p:70,210;m:20,440;p:71,440;p:142,269;p:278,192;p:326,348;\
                 m:20,440";
    let Some((fb, ai)) = session_at("fo-060-end.x", steps, front::rules_hyp(), |s| (s.render(), s.r.w.ai.clone())) else {
        return;
    };
    assert_eq!((ai.list, ai.trigger), (0, 0));
    for (x, y, c) in [(563, 63, [27, 53, 54]), (555, 60, [9, 15, 23]), (570, 66, [9, 17, 27])] {
        assert!(near(px(&fb, x, y), c, 4), "dim ({x},{y}) {:?} vs {c:?}", px(&fb, x, y));
    }
    let off = Rules { h621_ai_icon: false, ..front::rules_hyp() };
    let Some(fb) = frame_at("fo-060-end.x", steps, off) else { return };
    assert!(near(px(&fb, 563, 63), [115, 227, 230], 4));
}

/// H621: a Load takes the AI list from VCGameState f54 / f58 (fo-056-end.x: 0xe324 / 0xe31d) and a save writes it back.
#[test]
fn h621_ai_list_from_and_to_the_save() {
    let Some((ai, st)) = session_at("fo-056-end.x", "", front::rules_hyp(), |s| {
        let base = xf::save::SaveState::default();
        (s.r.w.ai.clone(), s.r.w.save_state(&base))
    }) else {
        return;
    };
    assert_eq!((ai.list, ai.trigger), (0xe324, 0xe31d));
    assert_eq!((st.extra.f4c[2], st.extra.f4c[3]), (0xe324, 0xe31d));
}

/// H623: a click on the AI icon (not glowing) posts the list's first action whose condition holds — fo-056-end.x
/// list 0xe324: 0x10e72 (bAIFieldOffice_CaseFiles2Cook = bFALSE) shows the hint card layout 0x10e17 — and advances
/// the list's position; with H623 off the point (562,63) hits nothing.
#[test]
fn h623_ai_click_posts_the_next_hint() {
    let Some((cursor, shown)) = session_at("fo-056-end.x", "", front::rules_hyp(), |s| {
        s.hover_at(562, 63);
        assert!(s.click_at(562, 63).is_some_and(|c| c.owner == 0xdb79));
        for _ in 0..5 {
            s.tick(20);
        }
        (s.r.w.ai.cursor, s.r.w.layouts().iter().any(|l| l.0 == 0x10e17))
    }) else {
        return;
    };
    assert_eq!(cursor, 1);
    assert!(shown);
    let off = Rules { h623_ai_click: false, ..front::rules_hyp() };
    let Some(hit) = session_at("fo-056-end.x", "", off, |s| s.r.hit(562, 63).map(|c| c.owner)) else { return };
    assert_eq!(hit, None);
}

/// H622: SetAIGlow(View) — the icon animates (frames of XG/56700.pff change over time) and a click sets that View.
#[test]
fn h622_glow_spins_and_click_travels() {
    let Some((a, b, view)) = session_at("fo-056-end.x", "", front::rules_hyp(), |s| {
        s.r.w.ai.glow = true;
        s.r.w.ai.target = 0x30b; // a View of the Field Office (fo-057's route passes it)
        let a: Vec<u8> = s.render()[(48 * 640 + 530) * 3..(78 * 640 + 594) * 3].to_vec();
        let mut b = a.clone();
        for _ in 0..40 {
            s.tick(25);
            b = s.render()[(48 * 640 + 530) * 3..(78 * 640 + 594) * 3].to_vec();
            if b != a {
                break;
            }
        }
        s.hover_at(562, 63);
        s.click_at(562, 63);
        front::settle_timers(s, 100, 60_000);
        (a, b, s.r.w.view)
    }) else {
        return;
    };
    assert_ne!(a, b, "no animation frame change within 1 s");
    assert_eq!(view, Some(0x30b));
}

/// H624: the note pad lists the day's notes (fo-067-end.x: 0xf709, 0xf70b, 0x163aa on day 1) — text drawn in the panel
/// (213,90)–(411,344); ablation: an empty panel.
#[test]
fn h624_note_pad_shows_the_days_notes() {
    let steps = "m:20,440;p:20,440;p:281,399";
    let count = |fb: &[u8]| {
        (92..340).flat_map(|y| (215..409).map(move |x| (x, y))).filter(|&(x, y)| px(fb, x, y) == [0, 0, 0]).count()
    };
    let Some((fb, shown, pad)) =
        session_at("fo-067-end.x", steps, front::rules_hyp(), |s| (s.render(), s.r.w.note_assets(), s.r.w.notes_ui.clone()))
    else {
        return;
    };
    assert_eq!(shown, [0xf709, 0xf70b, 0x163aa]);
    assert_eq!((pad.day, pad.days, pad.open), (0, 1, true));
    let on = count(&fb);
    let off = Rules { h624_note_pad: false, ..front::rules_hyp() };
    let Some(fb) = frame_at("fo-067-end.x", steps, off) else { return };
    assert!(on > count(&fb) + 500, "{on} vs {}", count(&fb));
}
