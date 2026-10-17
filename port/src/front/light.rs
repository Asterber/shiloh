//! The View tint (SetLBoxTint), the night-vision goggles and the flashlight beam (H1040–H1044).
//!
//! Decode (static, XFiles.exe; addresses and object layout in RE_NOTES H1040–H1044, H1772, H1773):
//! - The renderer holds a small set of tint slots and the slot in use. SetLBoxTint(r, g, b) (FUN_00517ea0) fills
//!   slot 0, ClearLBoxTint empties it. The tint multiplies the View's movies per channel by colour / 255 —
//!   **confirmed post-hoc** (C-BX1): `traces/fo-132.jsonl.shots/k05.png` (SetLBoxTint(50, 50, 50) at 69.87) = the
//!   port's untinted View 0x1994 × 0.195 (rms 0.3 per channel).
//! - Night vision (held type 4): its state change switches the renderer to slot 1 = RGB(0, 150, 100) (a static
//!   initialiser) and adds the scene's media to the tinted list; its Activation shows AssetRef **0x87bf** (fixed at
//!   startup) = x.pff #243 "nightvisiongogmask", 600×241, blue = transparent, over the View.
//!   **Confirmed post-hoc** (C-BX2) by `traces/fo-080.jsonl.shots/k02.png` vs `k01.png`: View × (0, 150, 100) / 255
//!   (fit G 0.584, B 0.389, R 0) and the HUD picture.
//! - Flashlight (held type 3): its Activation shows AssetRef **0x87b9** (fixed at startup) = x.pff #241
//!   "flashlightmask", 243×240 grey, centred on the pointer and re-placed on every pointer move. Size by
//!   Pref_cFlashlightBeam (var 0x3de) and SetFLightProps' c1: c1 ≠ 'O': 'M' × 0.72, 'S' × 0.45, else (W) unscaled;
//!   c1 = 'O': 'M' / 'W' × 0.6, 'S' × 0.45; an odd width gets +1 in both. Brightness index by
//!   Pref_cFlashlightBrightness (var 0x3dd) and c2: grey 0x50 / 0x80 / 0xa0 / 200 / 0xdc. Light mode (n ≠ 2): the beam
//!   is blended over the View — **confirmed post-hoc** (C-BX3) by `traces/fo-079.jsonl.shots/k03.png` vs
//!   `fo-080 k01.png` (no SetFLightProps in that session → index 1 = 0x80, beam 'W' unscaled): out = dst +
//!   0.124·(mask − dst), top-left = pointer − (121, 120), clipped to the View band, over the dialogue box too (rms 2.9).
//!   HYPOTHESIS H1043 (dark mode, n = 2, not traced): no blend filter is attached in dark mode; the beam shows the
//!   untinted View through the mask (out = tinted + mask'·(untinted − tinted)).
//! - Binoculars (held type 1, a subclass of the flashlight; H1772, static, NOT traced): AssetRef **0x87c1** (fixed at
//!   startup) = x.pff #244 "binocularmask", 600×275 grey. What it does depends on the View's binocular kind at the
//!   selection (OnBinocClick, FUN_0051bc70's kind): only kind 0 (a View without a binocular special) builds the mask
//!   (the flashlight's placement with the band rect (20, 90, 620, 330), so centred on the pointer like C-BX3's beam)
//!   with a blend filter (mode 2 — not decoded; the port multiplies the band by mask / 255 under the mask, the rest of
//!   the band unchanged); kind 1 hides it, kind 2 (the railyard pair, H1420: the movie shows the binocular view
//!   itself, C230's shot) builds none.
//! - Camera (held type 2, the goggles' base class; H1773): when the View has no camera special at the selection its
//!   Activation shows AssetRef **0x87bd** = x.pff #242 "moveableviewfinder", 320×241, blue transparent, and builds
//!   Camera.amv 0x87bb / DeadShtr.amv 0xdf10; every pointer move draws the exposures left, 24 − the photos, in a 25×25
//!   box at (3, 5) of the viewfinder. **Confirmed post-hoc** by `traces/fo-127.jsonl.shots/k14.png` (during the NSA
//!   video 0x5bbe, 115.145–119.925, pointer at the camera icon (211, 440)): the viewfinder's white frame at (51, 320) =
//!   pointer − (160, 120), clipped to the band, the counter red (255, 0, 0) at (+4, +8) — the viewfinder lies over a
//!   playing video.
//! - Neither tool class overrides the inventory objects' hit test: hotspots are not gated by the beam or the goggles
//!   (H1044, static).

use super::{Session, SCREEN_W, VIEW_X, VIEW_Y};
use crate::media::FRAME_H;
use crate::media::FRAME_W;

/// AssetRef of the flashlight's beam mask (fixed at startup, C-BX3)
pub const FLASHLIGHT_MASK: u32 = 0x87b9;
/// AssetRef of the night-vision goggles' HUD (fixed at startup, C-BX2)
pub const NIGHT_VISION_HUD: u32 = 0x87bf;
/// the renderer's tint slot 1 used while the goggles are held (C-BX2)
pub const NIGHT_VISION_TINT: [u8; 3] = [0, 150, 100];
/// held-object types (H171)
pub const HELD_FLASHLIGHT: u8 = 3;
pub const HELD_NIGHT_VISION: u8 = 4;
pub const HELD_BINOCULARS: u8 = 1;
pub const HELD_CAMERA: u8 = 2;
/// AssetRef of the binoculars' mask (H1772) and of the camera's viewfinder (H1773)
pub const BINOCULAR_MASK: u32 = 0x87c1;
pub const VIEWFINDER: u32 = 0x87bd;
/// the camera holds 24 photos (the viewfinder's counter shows 24 − taken)
pub const CAMERA_EXPOSURES: u32 = 24;
/// the counter's place in the viewfinder (H1773) and its colour (fo-127 k14)
pub const COUNTER_AT: (i32, i32) = (3, 5);
pub const COUNTER_RGB: [u8; 3] = [255, 0, 0];
/// Pref_cFlashlightBrightness (M / B / D) and Pref_cFlashlightBeam (M / W / S)
pub const VAR_FLASH_BRIGHTNESS: u32 = 0x3dd;
pub const VAR_FLASH_BEAM: u32 = 0x3de;
/// light-mode blend weight measured at grey 0x80 (C-BX3); other greys scale linearly (H1042)
const BLEND_AT_0X80: f32 = 0.124;

/// The View band's lighting for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// per-channel multiplier of the View's content (colour / 255)
    pub tint: Option<[u8; 3]>,
    pub night_vision: bool,
    /// the flashlight beam at the pointer: (pointer, scale, dark mode, brightness index 0..4)
    pub beam: Option<Beam>,
    /// H1772: the binoculars' mask centred on this point
    pub binoc: Option<(i32, i32)>,
    /// H1773: the camera's viewfinder centred on this point, with the exposures left
    pub viewfinder: Option<((i32, i32), u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub at: (i32, i32),
    pub scale: f32,
    pub dark: bool,
    pub level: usize,
}

/// The beam's scale by Pref_cFlashlightBeam and c1 ('O') (FUN_004d8300).
pub fn beam_scale(beam_pref: u8, c1_o: bool) -> f32 {
    match (c1_o, beam_pref) {
        (false, b'M') => 0.72,
        (false, b'S') | (true, b'S') => 0.45,
        (true, b'M') | (true, b'W') => 0.6,
        _ => 1.0,
    }
}

/// The brightness index (0..4) by SetFLightProps' c2 and Pref_cFlashlightBrightness (FUN_004d7e80).
pub fn beam_level(c2: u8, bright_pref: u8) -> usize {
    match (c2, bright_pref) {
        (0, b'B') => 0,
        (0, b'D') => 2,
        (0, _) => 1,
        (1, b'B') => 1,
        (1, b'D') => 3,
        (1, _) => 2,
        (2, b'B') => 2,
        (2, b'D') => 4,
        (2, _) => 3,
        _ => 2,
    }
}

/// The light-mode grey of each brightness index (a grey colour v, v, v per index).
pub const LEVEL_GREY: [u8; 5] = [0x50, 0x80, 0xa0, 200, 0xdc];

impl Session<'_> {
    /// What the View band shows this frame (rule H1040 on; no lighting otherwise).
    pub fn lighting(&self) -> Lighting {
        let w = &self.r.w;
        if !w.rules.h1040_lbox_tint {
            return Lighting { tint: None, night_vision: false, beam: None, binoc: None, viewfinder: None };
        }
        let held = w.dev.held;
        let night_vision = held == Some(HELD_NIGHT_VISION);
        let tint = if night_vision { Some(NIGHT_VISION_TINT) } else { w.dev.lbox_tint };
        let beam = (held == Some(HELD_FLASHLIGHT)).then_some(()).and(self.mouse).map(|at| {
            let [n2, c1_o, c2] = w.dev.flight;
            let pref = |v: u32, d: u8| w.vars.get(v).map_or(d, |x| x as u8);
            Beam {
                at,
                scale: beam_scale(pref(VAR_FLASH_BEAM, b'M'), c1_o != 0),
                dark: n2 != 0,
                level: beam_level(c2, pref(VAR_FLASH_BRIGHTNESS, b'M')),
            }
        });
        let overlay = |t: u8| (held == Some(t) && w.dev.held_overlay).then_some(()).and(self.mouse);
        let binoc = overlay(HELD_BINOCULARS);
        let viewfinder = overlay(HELD_CAMERA).map(|at| (at, CAMERA_EXPOSURES.saturating_sub(w.pc.camera)));
        Lighting { tint, night_vision, beam, binoc, viewfinder }
    }
}

/// The View band (x1, y1, x2, y2), exclusive.
const BAND: (usize, usize, usize, usize) = (VIEW_X, VIEW_Y, VIEW_X + FRAME_W, VIEW_Y + FRAME_H);

/// Copy of the View band (for the dark-mode beam).
pub fn band_copy(fb: &[u8]) -> Vec<u8> {
    let (x1, y1, x2, y2) = BAND;
    let mut out = Vec::with_capacity((x2 - x1) * (y2 - y1) * 3);
    for y in y1..y2 {
        out.extend_from_slice(&fb[(y * SCREEN_W + x1) * 3..(y * SCREEN_W + x2) * 3]);
    }
    out
}

/// Multiply the View band by `c` / 255 per channel (H1040).
pub fn tint_band(fb: &mut [u8], c: [u8; 3]) {
    let (x1, y1, x2, y2) = BAND;
    for y in y1..y2 {
        for px in fb[(y * SCREEN_W + x1) * 3..(y * SCREEN_W + x2) * 3].chunks_exact_mut(3) {
            for i in 0..3 {
                px[i] = ((px[i] as u32 * c[i] as u32 + 127) / 255) as u8;
            }
        }
    }
}

/// The mask scaled (nearest neighbour; the engine's scaler is not decoded): (width, height, grey).
pub fn scaled_mask(p: &crate::media::pict::Picture, scale: f32) -> (usize, usize, Vec<u8>) {
    if scale >= 1.0 {
        return (p.width, p.height, p.rgb.chunks_exact(3).map(|c| c[0]).collect());
    }
    let mut w = (p.width as f32 * scale) as usize;
    let mut h = (p.height as f32 * scale) as usize;
    if w & 1 != 0 {
        w += 1;
        h += 1;
    }
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let sy = (y * p.height / h).min(p.height - 1);
        for x in 0..w {
            let sx = (x * p.width / w).min(p.width - 1);
            out.push(p.rgb[(sy * p.width + sx) * 3]);
        }
    }
    (w, h, out)
}

/// Draw the beam over the View band. Light mode: dst + k·(mask − dst), k = 0.124 · grey / 0x80 (C-BX3 / H1042). Dark
/// mode (H1043): tinted + m·(untinted − tinted), m = mask / 255 × the level's factor.
pub fn draw_beam(fb: &mut [u8], b: &Beam, mask: &crate::media::pict::Picture, untinted: Option<&[u8]>) {
    let (mw, mh, m) = scaled_mask(mask, b.scale);
    let (x0, y0) = (b.at.0 - (mw as i32) / 2, b.at.1 - (mh as i32) / 2);
    let (bx1, by1, bx2, by2) = (BAND.0 as i32, BAND.1 as i32, BAND.2 as i32, BAND.3 as i32);
    let k = BLEND_AT_0X80 * LEVEL_GREY[b.level.min(4)] as f32 / 128.0;
    // H1043: dark-mode brightness factors (the original changes the mask per level; not decoded)
    let dark_gain = [0.6f32, 0.8, 1.0, 1.2, 1.4][b.level.min(4)];
    for my in 0..mh {
        let sy = y0 + my as i32;
        if sy < by1 || sy >= by2 {
            continue;
        }
        for mx in 0..mw {
            let sx = x0 + mx as i32;
            if sx < bx1 || sx >= bx2 {
                continue;
            }
            let g = m[my * mw + mx] as f32;
            if g == 0.0 {
                continue;
            }
            let d = (sy as usize * SCREEN_W + sx as usize) * 3;
            match (b.dark, untinted) {
                (true, Some(u)) => {
                    let ui = ((sy - by1) as usize * FRAME_W + (sx - bx1) as usize) * 3;
                    let a = (g / 255.0 * dark_gain).min(1.0);
                    for i in 0..3 {
                        let t = fb[d + i] as f32;
                        fb[d + i] = (t + a * (u[ui + i] as f32 - t)).round().clamp(0.0, 255.0) as u8;
                    }
                }
                (true, None) => {}
                (false, _) => {
                    for i in 0..3 {
                        let t = fb[d + i] as f32;
                        fb[d + i] = (t + k * (g - t)).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
}

/// H1772: the binoculars' mask (grey) centred on `at`, multiplying the band under it by grey / 255 (clipped to the band).
pub fn draw_binoc(fb: &mut [u8], at: (i32, i32), mask: &crate::media::pict::Picture) {
    let (x0, y0) = (at.0 - mask.width as i32 / 2, at.1 - mask.height as i32 / 2);
    let (bx1, by1, bx2, by2) = (BAND.0 as i32, BAND.1 as i32, BAND.2 as i32, BAND.3 as i32);
    for my in 0..mask.height {
        let sy = y0 + my as i32;
        if sy < by1 || sy >= by2 {
            continue;
        }
        for mx in 0..mask.width {
            let sx = x0 + mx as i32;
            if sx < bx1 || sx >= bx2 {
                continue;
            }
            let g = mask.rgb[(my * mask.width + mx) * 3] as u32;
            let d = (sy as usize * SCREEN_W + sx as usize) * 3;
            for px in &mut fb[d..d + 3] {
                *px = ((*px as u32 * g + 127) / 255) as u8;
            }
        }
    }
}

/// H1773: the camera's viewfinder centred on `at` (blue transparent, clipped to the band) and the exposures left.
pub fn draw_viewfinder(fb: &mut [u8], at: (i32, i32), left: u32, vf: &crate::media::pict::Picture) {
    let (x0, y0) = (at.0 - vf.width as i32 / 2, at.1 - vf.height as i32 / 2);
    let (bx1, by1, bx2, by2) = (BAND.0 as i32, BAND.1 as i32, BAND.2 as i32, BAND.3 as i32);
    for py in 0..vf.height {
        let sy = y0 + py as i32;
        if sy < by1 || sy >= by2 {
            continue;
        }
        for px in 0..vf.width {
            let sx = x0 + px as i32;
            if sx < bx1 || sx >= bx2 {
                continue;
            }
            let s = (py * vf.width + px) * 3;
            if vf.rgb[s..s + 3] != super::KEY {
                let d = (sy as usize * SCREEN_W + sx as usize) * 3;
                fb[d..d + 3].copy_from_slice(&vf.rgb[s..s + 3]);
            }
        }
    }
    // the counter (font not decoded: the port's Arial 12), clipped to its 25×25 box and the band
    if let Some(f) = super::text::arial12() {
        let (tx, ty) = (x0 + COUNTER_AT.0, y0 + COUNTER_AT.1);
        if ty >= by1 && tx >= bx1 {
            f.draw_clip_rect(fb, tx, ty, &left.to_string(), COUNTER_RGB, (tx + 25).min(bx2), (ty + 25).min(by2));
        }
    }
}
