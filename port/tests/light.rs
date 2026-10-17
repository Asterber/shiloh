//! The View tint, the night-vision goggles and the flashlight beam (H1040–H1043): headless frames of the bot's
//! saves checked against what the original's screenshots show (C-BX1 fo-132 k05, C-BX2 fo-080 k02, C-BX3 fo-079 k03).
use std::path::PathBuf;
use xf::front::light::{beam_level, beam_scale};
use xf::front::{settle_timers, Session, SCREEN_W};
use xf::hdb::Hdb;
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Load `save`, run `steps` (`m:X,Y` move, `p:X,Y` move + click; each settled + 1.2 s) and hand the session to `f`.
fn with_session(save: &str, steps: &str, rules: Rules, f: impl FnOnce(&mut Session)) -> bool {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return false;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    let mut s = Session::with_rules(&h, &g, &d, rules);
    s.r.load_game();
    settle_timers(&mut s, 100, 900_000);
    for st in steps.split(';').filter(|x| !x.is_empty()) {
        let (kind, xy) = st.split_at(2);
        let v: Vec<i32> = xy.split(',').map(|x| x.parse().unwrap()).collect();
        s.hover_at(v[0], v[1]);
        if kind == "p:" {
            s.click_at(v[0], v[1]);
        }
        settle_timers(&mut s, 100, 900_000);
        for _ in 0..12 {
            s.tick(100);
        }
    }
    f(&mut s);
    true
}

fn px(fb: &[u8], x: usize, y: usize) -> [u8; 3] {
    let i = (y * SCREEN_W + x) * 3;
    [fb[i], fb[i + 1], fb[i + 2]]
}

/// Mean of each channel over a rect.
fn mean(fb: &[u8], x1: usize, y1: usize, x2: usize, y2: usize) -> [f64; 3] {
    let mut s = [0f64; 3];
    for y in y1..y2 {
        for x in x1..x2 {
            let p = px(fb, x, y);
            for i in 0..3 {
                s[i] += p[i] as f64;
            }
        }
    }
    let n = ((x2 - x1) * (y2 - y1)) as f64;
    s.map(|v| v / n)
}

/// C-BX2: the night-vision goggles (0x5545, held type 4) in Cook's office (fo-056-end.x, View 0x319): the View band ×
/// (0, 150, 100) / 255 and the HUD 0x87bf (its border (0, 62, 34) at the band's corner). Ablation: H1040 off.
#[test]
fn h1041_night_vision_green_view_and_hud() {
    let mut fbs = Vec::new();
    for on in [true, false] {
        let rules = Rules { h1040_lbox_tint: on, ..Rules::all() };
        let ok = with_session("fo-056-end.x", "m:20,440;p:71,440", rules, |s| {
            assert_eq!(s.r.w.dev.held, Some(4), "night vision held");
            fbs.push(s.render());
        });
        if !ok {
            return;
        }
    }
    let (on, off) = (&fbs[0], &fbs[1]);
    let m_on = mean(on, 70, 130, 180, 290);
    let m_off = mean(off, 70, 130, 180, 290);
    assert_eq!(m_on[0], 0.0, "red removed: {m_on:?}");
    assert!(m_on[1] > m_on[2] && m_on[1] > 5.0, "green dominant: {m_on:?}");
    assert!((m_on[1] - m_off[1] * 150.0 / 255.0).abs() < 1.0, "G × 150/255: {m_on:?} vs {m_off:?}");
    assert!((m_on[2] - m_off[2] * 100.0 / 255.0).abs() < 1.0, "B × 100/255: {m_on:?} vs {m_off:?}");
    assert_eq!(px(on, 22, 92), [0, 62, 34], "HUD border");
    assert!(m_off[0] > 20.0, "ablation: the plain View {m_off:?}");
}

/// C-BX3: the flashlight (0x441e, held type 3) in Cook's office, no SetFLightProps in the session (index 1, grey 0x80),
/// beam pref 'W' as the bot's bottle (var 0x3de) → unscaled: inside the circle (radius ≈ 120) around the pointer the
/// View is lighter, outside it unchanged; the beam is clipped to the View band. Ablation: no held object.
#[test]
fn h1042_flashlight_lit_circle_at_the_pointer() {
    with_session("fo-056-end.x", "m:20,440;p:176,440;m:308,201", Rules::all(), |s| {
        assert_eq!(s.r.w.dev.held, Some(3), "flashlight held");
        assert_eq!(s.r.w.vars.get(0x3de), Some(b'W' as i32), "the bottle's Pref_cFlashlightBeam");
        let lit = s.render();
        let held = s.r.w.dev.held.take();
        let plain = s.render();
        s.r.w.dev.held = held;
        // inside: ring at r ≈ 100 and the centre lighter by the blend dst + 0.124·(mask − dst)
        let a = mean(&lit, 300, 95, 316, 105); // r ≈ 100 above the pointer
        let b = mean(&plain, 300, 95, 316, 105);
        assert!(a.iter().zip(&b).all(|(x, y)| x - y > 10.0), "lit ring {a:?} vs {b:?}");
        // outside the circle: unchanged
        assert_eq!(mean(&lit, 470, 100, 600, 320), mean(&plain, 470, 100, 600, 320), "outside the beam");
        // below the View band: unchanged (clip)
        assert_eq!(mean(&lit, 250, 331, 360, 340), mean(&plain, 250, 331, 360, 340), "clipped to the band");
    });
}

/// C-BX1: SetLBoxTint(50, 50, 50) (0x10635, Comity Inn lobby at night: fo-131-end.x → PDA map → View 0x1994) darkens
/// the View band to 50/255; with the flashlight in dark mode (SetFLightProps(2, 'I', 'M') 0x10634 → flight [1, 0, 1])
/// H1043 shows the untinted View through the beam.
#[test]
fn h1040_lbox_tint_and_h1043_dark_beam() {
    with_session("fo-131-end.x", "m:20,440;p:21,440;p:278,192;p:326,348", Rules::all(), |s| {
        assert_eq!(s.r.w.dev.lbox_tint, Some([50, 50, 50]));
        assert_eq!(s.r.w.dev.flight, [1, 0, 1], "SetFLightProps(2, 'I', 'M')");
        let dark = s.render();
        let tint = s.r.w.dev.lbox_tint.take();
        let bright = s.render();
        s.r.w.dev.lbox_tint = tint;
        let (d, b) = (mean(&dark, 30, 100, 610, 320), mean(&bright, 30, 100, 610, 320));
        for i in 0..3 {
            assert!((d[i] - b[i] * 50.0 / 255.0).abs() < 1.0, "× 50/255: {d:?} vs {b:?}");
        }
        // dark-mode beam: the pointer's circle shows the untinted View
        s.r.w.dev.held = Some(3);
        s.hover_at(320, 210);
        let beam = s.render();
        let c = mean(&beam, 300, 190, 340, 230);
        let (cd, cb) = (mean(&dark, 300, 190, 340, 230), mean(&bright, 300, 190, 340, 230));
        assert!(c[0] > cd[0] + (cb[0] - cd[0]) * 0.5, "beam centre {c:?}, dark {cd:?}, bright {cb:?}");
        assert_eq!(mean(&beam, 30, 100, 150, 320), mean(&dark, 30, 100, 150, 320), "outside the beam stays dark");
    });
}

#[test]
fn h1042_beam_tables() {
    assert_eq!(beam_scale(b'W', false), 1.0);
    assert_eq!(beam_scale(b'M', false), 0.72);
    assert_eq!(beam_scale(b'S', false), 0.45);
    assert_eq!(beam_scale(b'M', true), 0.6);
    assert_eq!(beam_scale(b'W', true), 0.6);
    assert_eq!(beam_level(0, b'M'), 1);
    assert_eq!(beam_level(1, b'M'), 2);
    assert_eq!(beam_level(1, b'D'), 3);
    assert_eq!(beam_level(2, b'D'), 4);
    assert_eq!(beam_level(0, b'B'), 0);
}
