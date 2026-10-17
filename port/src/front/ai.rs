//! Display of the AI icon (EvidenceIcon 0xdb79, bar 6) — H621 (dimmed while no hint applies) and H622 (the spin).
//! Display only: the state is the core's (`world::ai`).
use super::{blit, pda, Session};
use crate::media::pict::Picture;
use crate::world::ai::AI_ICON;

/// HYPOTHESIS H622 (static, the original's icon glow switch FUN_0043bcb0: an icon without a roll-over picture whose
/// shown asset is a kind-2 animation is set to loop and started; switched off it stops and goes back to frame 0):
/// while SetAIGlow ≥ 1 the icon plays the 8 entries of
/// XG/56700.pff (0xdd7c's range 0..7) in a loop with the PFF header's durations (as the cursors, H330); otherwise
/// frame 0. No screenshot shows a glowing icon (every traced shot matches entry 0 exactly or dimmed).
pub fn frame(s: &mut Session, asset: u32) -> Option<Picture> {
    let c = s.clip(asset)?;
    if c.ext() != "pff" {
        return None;
    }
    let pff = s.pffs.entry(c.file.clone()).or_insert_with(|| crate::media::pict::Pff::open(&c.file).ok()).as_ref()?;
    let (a, b) = (c.start as usize, (c.end.max(c.start) as usize).min(pff.len().saturating_sub(1)));
    let i = if s.r.w.ai.glow && s.r.w.rules.h621_ai_icon {
        let d = pff.durations();
        let durs: Vec<u32> = (a..=b).map(|i| d.get(i).copied().unwrap_or(0)).collect();
        a + super::cursor::frame_at(&durs, s.now)
    } else {
        a
    };
    pff.picture(i).ok()
}

/// Draw the AI icon at `(x, y)` (its bar rect's top-left); false = not the AI icon / rule off (the caller draws it).
/// HYPOTHESIS H621 (display): while `World::ai_lit` is false the icon is drawn darkened as H620's layer (each channel
/// (15·v + 32) >> 6) — 47 of the 48 screenshots of fo-055..fo-080 match (bright = entry 0 pixel for pixel, dim = the
/// same factor; the one miss, fo-060 k12, is the core's H212 divergence: it opened the PDA there). Under a dimming
/// layout (H620) the icon is drawn plain — the layer darkens it once.
pub fn draw(s: &mut Session, fb: &mut [u8], owner: u32, x: i32, y: i32) -> bool {
    if owner != AI_ICON || !s.r.w.rules.h621_ai_icon {
        return false;
    }
    let Some(asset) = s.r.w.icon_picture(owner) else { return false };
    let Some(mut p) = frame(s, asset) else { return false };
    if !s.r.w.ai_lit() && pda::dim_layout(s).is_none() {
        dim_keep_key(&mut p);
    }
    blit(fb, x, y, &p);
    true
}

/// Darken a picture's pixels except the transparent key colour (H63).
fn dim_keep_key(p: &mut Picture) {
    for px in p.rgb.chunks_exact_mut(3) {
        if px != super::KEY {
            for v in px.iter_mut() {
                *v = pda::dim(*v);
            }
        }
    }
}
