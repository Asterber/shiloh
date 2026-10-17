//! Text drawing for the UI: TrueType fonts hinted for monochrome output by `skrifa`'s TrueType interpreter (as GDI's
//! non-anti-aliased text) and filled by pixel centres (non-zero winding), drawn into the 640×480 RGB24 frame.
//!
//! HYPOTHESIS H301 (fo-055 k01 / k02 screenshots): the original draws the dialogue rows as GDI text without
//! anti-aliasing (every text pixel of k01 is one colour, (205,205,205)) on a transparent background; the cell advance is
//! the font's advance rounded to whole pixels (Courier New 12 px: 7.2 → 7, as H262); the glyphs are the 12-ppem outlines
//! hinted by the font's own TrueType instructions for monochrome output, filled by pixel centres; the baseline is the top
//! of the row's rect + **12** (measured: k01 text pixels match at +12 — 1151 of 1164 identical, the 13 others are
//! dropout pixels of diagonals —, not the font's rounded ascent 10; the 15-px row height (H262) = 12 + descent 3).
//! The glyph shapes come from macOS's Courier New, which the bottle maps the face to.
//!
//! Without the font file the 3×5 debug font ([`super::font`]) is used (Latin only; Cyrillic letters are skipped).
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static WEB_FONT_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The web front (`port/web/src/lib.rs`): the fonts the local server (xf-serve `--fonts`) offers under `/fonts`, read
/// through `xf::io` as `<dir>/<name>`; searched after `XF_FONT_DIR`. Set once, before the first text is drawn.
pub fn set_web_font_dir(dir: impl Into<PathBuf>) {
    let _ = WEB_FONT_DIR.set(dir.into());
}

/// Places searched for a system font file (Courier New, Arial) by name: `XF_FONT_DIR`, the web front's font dir, then
/// the system font dirs. Every system font load goes through this list (text.rs, computer.rs, doc.rs).
pub fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = std::env::var_os("XF_FONT_DIR") {
        v.push(PathBuf::from(d));
    }
    if let Some(d) = WEB_FONT_DIR.get() {
        v.push(d.clone());
    }
    v.extend(
        [
            "/System/Library/Fonts/Supplemental",
            "/Library/Fonts",
            "/usr/share/fonts/truetype/msttcorefonts",
            "C:\\Windows\\Fonts",
        ]
        .iter()
        .map(PathBuf::from),
    );
    v
}

pub struct Face {
    font: skrifa::FontRef<'static>,
    hint: Option<skrifa::outline::HintingInstance>,
    px: f32,
    /// cell advance, whole pixels
    pub advance: i32,
    /// ascent, whole pixels (baseline = top + ascent)
    pub ascent: i32,
    /// per-glyph advances (Arial) instead of the fixed cell (Courier New)
    pub proportional: bool,
    glyphs: Mutex<HashMap<char, Glyph>>,
}

/// A monochrome glyph: `w`×`h` coverage bits with the top-left at (x, −y_top) from the pen point on the baseline
#[derive(Clone, Default)]
struct Glyph {
    x: i32,
    top: i32,
    w: usize,
    h: usize,
    bits: Vec<bool>,
    /// advance, whole pixels
    adv: i32,
}

/// Outline pen collecting flattened contours (pixel units, y up)
#[derive(Default)]
struct Poly {
    contours: Vec<Vec<(f32, f32)>>,
}

impl Poly {
    fn last(&self) -> (f32, f32) {
        self.contours.last().and_then(|c| c.last().copied()).unwrap_or((0.0, 0.0))
    }
}

impl skrifa::outline::OutlinePen for Poly {
    fn move_to(&mut self, x: f32, y: f32) {
        self.contours.push(vec![(x, y)]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        if let Some(c) = self.contours.last_mut() {
            c.push((x, y));
        }
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        for i in 1..=8 {
            let t = i as f32 / 8.0;
            let u = 1.0 - t;
            self.line_to(u * u * x0 + 2.0 * u * t * cx + t * t * x, u * u * y0 + 2.0 * u * t * cy + t * t * y);
        }
    }
    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        for i in 1..=12 {
            let t = i as f32 / 12.0;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.line_to(a * x0 + b * c0x + c * c1x + d * x, a * y0 + b * c0y + c * c1y + d * y);
        }
    }
    fn close(&mut self) {}
}

/// Non-zero winding at point (px, py) (the TrueType fill rule), every contour closed
fn winding(p: &Poly, px: f32, py: f32) -> i32 {
    let mut w = 0;
    for c in &p.contours {
        for i in 0..c.len() {
            let (a, b) = (c[i], c[(i + 1) % c.len()]);
            if (a.1 <= py) != (b.1 <= py) {
                let x = a.0 + (py - a.1) / (b.1 - a.1) * (b.0 - a.0);
                if x > px {
                    w += if b.1 > a.1 { 1 } else { -1 };
                }
            }
        }
    }
    w
}

impl Face {
    pub fn load(files: &[&str], px: f32) -> Option<Face> {
        let data = files
            .iter()
            .flat_map(|f| font_dirs().into_iter().map(move |d| d.join(f)))
            .find_map(|p| crate::io::read(&p).ok())?;
        Face::from_bytes(data, px)
    }

    /// A face from the bytes of a TrueType file (e.g. the game's own .ttr fonts, read from the install directory).
    pub fn from_bytes(data: Vec<u8>, px: f32) -> Option<Face> {
        use skrifa::MetadataProvider;
        let data: &'static [u8] = Box::leak(data.into_boxed_slice());
        let font = skrifa::FontRef::new(data).ok()?;
        let size = skrifa::instance::Size::new(px);
        let loc = skrifa::instance::LocationRef::default();
        let hint = skrifa::outline::HintingInstance::new(
            &font.outline_glyphs(),
            size,
            loc,
            skrifa::outline::HintingOptions {
                engine: skrifa::outline::Engine::Interpreter,
                target: skrifa::outline::Target::Mono,
            },
        )
        .ok();
        let gm = font.glyph_metrics(size, loc);
        let m = font.charmap().map('M')?;
        let advance = gm.advance_width(m).unwrap_or(px * 0.6).round() as i32;
        let ascent = font.metrics(size, loc).ascent.round() as i32;
        Some(Face { font, hint, px, advance, ascent, proportional: false, glyphs: Mutex::new(HashMap::new()) })
    }

    fn glyph(&self, ch: char) -> Glyph {
        use skrifa::MetadataProvider;
        let Some(gid) = self.font.charmap().map(ch) else { return Glyph::default() };
        let adv = self
            .font
            .glyph_metrics(skrifa::instance::Size::new(self.px), skrifa::instance::LocationRef::default())
            .advance_width(gid)
            .unwrap_or(0.0)
            .round() as i32;
        let outlines = self.font.outline_glyphs();
        let Some(o) = outlines.get(gid) else { return Glyph::default() };
        let mut poly = Poly::default();
        let settings = match &self.hint {
            Some(h) => skrifa::outline::DrawSettings::hinted(h, false),
            None => skrifa::outline::DrawSettings::unhinted(
                skrifa::instance::Size::new(self.px),
                skrifa::instance::LocationRef::default(),
            ),
        };
        if o.draw(settings, &mut poly).is_err() || poly.contours.is_empty() {
            return Glyph { adv, ..Glyph::default() };
        }
        let pts = poly.contours.iter().flatten();
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in pts {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        let (gx0, gx1) = (x0.floor() as i32, x1.ceil() as i32);
        let (gy0, gy1) = (y0.floor() as i32, y1.ceil() as i32);
        let (w, h) = ((gx1 - gx0).max(0) as usize, (gy1 - gy0).max(0) as usize);
        let mut bits = vec![false; w * h];
        for row in 0..h {
            // row 0 = the top pixel row (y up in font space)
            let cy = (gy1 - row as i32) as f32 - 0.5;
            for col in 0..w {
                let cx = (gx0 + col as i32) as f32 + 0.5;
                bits[row * w + col] = winding(&poly, cx, cy) != 0;
            }
        }
        Glyph { x: gx0, top: gy1, w, h, bits, adv }
    }

    /// Draw `s` with its text rect's top-left at (x, top); returns the x after the text.
    pub fn draw(&self, fb: &mut [u8], x: i32, top: i32, s: &str, c: [u8; 3]) -> i32 {
        self.draw_clip(fb, x, top, s, c, i32::MAX)
    }

    /// [`Face::draw`], pixels at y ≥ `bottom` left out (a panel's clip rect).
    pub fn draw_clip(&self, fb: &mut [u8], x: i32, top: i32, s: &str, c: [u8; 3], bottom: i32) -> i32 {
        self.draw_clip_rect(fb, x, top, s, c, i32::MAX, bottom)
    }

    /// [`Face::draw`], pixels at x ≥ `right` or y ≥ `bottom` left out (DrawText's clip to its rect).
    pub fn draw_clip_rect(&self, fb: &mut [u8], x: i32, top: i32, s: &str, c: [u8; 3], right: i32, bottom: i32) -> i32 {
        self.draw_clip_box(fb, x, top, s, c, i32::MIN, right, bottom)
    }

    /// [`Face::draw_clip_rect`], pixels above `clip_top` left out too (a scrolled panel's view rect, H1810).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_clip_box(&self, fb: &mut [u8], mut x: i32, top: i32, s: &str, c: [u8; 3], clip_top: i32, right: i32, bottom: i32) -> i32 {
        let Ok(mut g) = self.glyphs.lock() else { return x };
        let base = top + self.ascent;
        for ch in s.chars() {
            let gl = g.entry(ch).or_insert_with(|| self.glyph(ch));
            let (gx, gy) = (x + gl.x, base - gl.top);
            for row in 0..gl.h {
                for col in 0..gl.w {
                    if gl.bits[row * gl.w + col] {
                        let (px, py) = (gx + col as i32, gy + row as i32);
                        if py < bottom && px < right && py >= clip_top {
                            super::fill(fb, px, py, px + 1, py + 1, c);
                        }
                    }
                }
            }
            x += if self.proportional { gl.adv } else { self.advance };
        }
        x
    }

    /// Width of `s` in pixels (sum of advances).
    pub fn width(&self, s: &str) -> i32 {
        if !self.proportional {
            return s.chars().count() as i32 * self.advance;
        }
        let Ok(mut g) = self.glyphs.lock() else { return 0 };
        s.chars().map(|ch| g.entry(ch).or_insert_with(|| self.glyph(ch)).adv).sum()
    }
}

/// H301: baseline of a Courier New −12 row below the row's top (fo-055 k01)
pub const COURIER12_BASELINE: i32 = 12;

/// Courier New, 12 px (lfHeight −12: the dialogue panels, H161 / H262)
pub fn courier12() -> Option<&'static Face> {
    static F: OnceLock<Option<Face>> = OnceLock::new();
    F.get_or_init(|| {
        let mut f = Face::load(&["Courier New.ttf", "cour.ttf", "Courier_New.ttf"], 12.0)?;
        f.ascent = COURIER12_BASELINE;
        Some(f)
    })
    .as_ref()
}

/// Draw with Courier New 12, or the debug font if it is missing (then 7 px per character so rects still line up).
pub fn draw_courier(fb: &mut [u8], x: i32, top: i32, s: &str, c: [u8; 3]) -> i32 {
    draw_courier_clip(fb, x, top, s, c, i32::MAX)
}

/// [`draw_courier`] clipped below `bottom`.
pub fn draw_courier_clip(fb: &mut [u8], x: i32, top: i32, s: &str, c: [u8; 3], bottom: i32) -> i32 {
    match courier12() {
        Some(f) => f.draw_clip(fb, x, top, s, c, bottom),
        None => {
            if top + 12 <= bottom {
                for (i, ch) in s.chars().enumerate() {
                    super::font::text(fb, x + i as i32 * 7, top + 2, &ch.to_string(), c);
                }
            }
            x + s.chars().count() as i32 * 7
        }
    }
}

/// [`draw_courier_clip`] clipped above `clip_top` too (H1810: a scrolled dialogue-box panel).
pub fn draw_courier_clip_rows(fb: &mut [u8], x: i32, top: i32, s: &str, c: [u8; 3], clip_top: i32, bottom: i32) -> i32 {
    match courier12() {
        Some(f) => f.draw_clip_box(fb, x, top, s, c, clip_top, i32::MAX, bottom),
        None if top >= clip_top => draw_courier_clip(fb, x, top, s, c, bottom),
        None => x + s.chars().count() as i32 * 7,
    }
}

/// HYPOTHESIS H865: the phone display's style (set up by PhoneInit, FUN_004c7f00: Courier New, lfHeight −15, then
/// weight 700): Courier New Bold 15 px, advance 9; baseline at
/// the row's top + 12 (fo-100 k08..k12: caps' bottom row 150 / 167 / 184 under the display's top 139)
pub fn courier15_bold() -> Option<&'static Face> {
    static F: OnceLock<Option<Face>> = OnceLock::new();
    F.get_or_init(|| {
        let mut f = Face::load(&["Courier New Bold.ttf", "courbd.ttf", "Courier_New_Bold.ttf"], 15.0)?;
        f.ascent = 12;
        Some(f)
    })
    .as_ref()
}

/// H310: Arial, 12 px (text style 3 at size 12: the icons' roll-over text style)
pub fn arial12() -> Option<&'static Face> {
    static F: OnceLock<Option<Face>> = OnceLock::new();
    F.get_or_init(|| {
        let mut f = Face::load(&["Arial.ttf", "arial.ttf"], 12.0)?;
        f.proportional = true;
        Some(f)
    })
    .as_ref()
}
