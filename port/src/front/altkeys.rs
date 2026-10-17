//! The original's Alt hotkeys (WM_SYSKEYDOWN → the app's Alt-key handler FUN_0046c390, H1868) for both
//! fronts: [`alt_key`] does what the key does to the [`Session`] and tells the host what is its part (volume, exit,
//! the settings that follow). Static only (a static pass + this module's, H2060–H2069), NOT traced.
//!
//! | keys | original (the handler's case) | port |
//! |---|---|---|
//! | Alt+0…9 | 0x30–0x39: the master volume preference = (max / 9) × digit | the port's volume setting := digit × 100 / 9 % (H2061) |
//! | Alt+A | 0x41: the Artificial Intuition preference and variable 0x369 := its negation, then the AI icon is refreshed | var 0x369 flipped (H2062) |
//! | Alt+M / R / T / V | 0x4d / 0x52 / 0x54 / 0x56: the matching preference flipped (no variable written) | vars 0x3d9 / 0x3d6 / 0x3da / 0x1072 flipped (H2062) |
//! | Alt+C | 0x43: closed captions (H343) | the F3 toggle |
//! | Alt+P | 0x50: pause — freeze + layout 0xf5e8, again = resume | the same; any key / click resumes too (H2063) |
//! | Alt+S | 0x53: the save screen | the save screen (H2064) |
//! | Alt+X | 0x58: freeze, then exit | the window quits; the web does nothing (H2065) |
//! | Shift+Alt+F | 0x46: FPS counter (20 frame times, 1000 / avg) | drawn top-left (H2067) |
//! | Shift+Alt+H | 0x48: hotspot overlay (likely) | F1's overlay (H2068) |
//! | Shift+Alt+I | 0x49: a MessageBox with the current node and the four scores (Asta, Loss, Paranoia, XTrack) | a port-drawn box (H2066) |
//!
//! Not done: Alt+L (an empty function), Shift+Alt+J (the Typist text effect off — not modelled).
use super::{fill, rect, Session, SCREEN_H, SCREEN_W};

/// H2062: Alt+letter → the boolean Preference variable it flips, with its name (PREF_VARS, H189).
pub const PREF_KEYS: [(u32, u32, &str); 5] = [
    (0x41, 0x369, "Artificial Intuition"),
    (0x4d, 0x3d9, "Subliminal Messages"),
    (0x52, 0x3d6, "Rollover Text"),
    (0x54, 0x3da, "Transitions"),
    (0x56, 0x1072, "High Quality Video"),
];

/// H2066: the state box's variables — iCurrNode, iAstaScore, iUberLoss, iUberParanoia, iUberXTrack (names from
/// XFiles.gam).
pub const STATE_VARS: [u32; 5] = [0x4c3, 0x583, 0x35c, 0x35b, 0x35a];

/// H2067: the FPS counter's ring of frame times (20 entries).
pub const FPS_SAMPLES: usize = 20;

/// The Alt keys' display state (port side).
#[derive(Clone, Debug, Default)]
pub struct AltState {
    /// H2063: Alt+P's pause is on (the core's freeze + layout 0xf5e8)
    pub paused: bool,
    /// H2066: the Shift+Alt+I box's text (shown until a key / a click; the game stands still meanwhile)
    pub msgbox: Option<String>,
    /// H2067: the FPS counter (Shift+Alt+F) — the last frame times, ms
    pub fps: Option<Fps>,
}

/// H2067: a ring of the last [`FPS_SAMPLES`] frame times.
#[derive(Clone, Debug, Default)]
pub struct Fps {
    ring: Vec<u64>,
    next: usize,
}

impl Fps {
    pub fn push(&mut self, ms: u64) {
        if self.ring.len() < FPS_SAMPLES {
            self.ring.push(ms);
        } else {
            self.ring[self.next] = ms;
        }
        self.next = (self.next + 1) % FPS_SAMPLES;
    }

    /// 1000 / the average frame time (0 with no samples / zero times).
    pub fn fps(&self) -> u32 {
        let sum: u64 = self.ring.iter().sum();
        if sum == 0 {
            return 0;
        }
        (1000 * self.ring.len() as u64 / sum) as u32
    }
}

/// What an Alt key did — the host's part is in the variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AltOut {
    /// nothing (not an Alt hotkey, or not now)
    None,
    /// H2061: the host's volume setting := this percent
    Volume(u32),
    /// H2062: a Preference flipped (its name, its new value)
    Pref(&'static str, bool),
    /// closed captions on / off (the host's `captions` setting follows)
    Captions(bool),
    /// H2063: paused / resumed
    Pause(bool),
    /// H2064: the save screen opened
    SaveScreen,
    /// H2065: exit (the window quits; the web ignores it)
    Exit,
    /// H2067: the FPS counter on / off
    Fps(bool),
    /// H2068: the hotspot overlay on / off (the host's `overlay` setting follows)
    Hotspots(bool),
    /// H2066: the state box shows
    StateBox,
}

impl AltOut {
    /// A log line ("" for None).
    pub fn describe(&self) -> String {
        let on = |b: &bool| if *b { "on" } else { "off" };
        match self {
            AltOut::None => String::new(),
            AltOut::Volume(v) => format!("Alt: volume {v} %"),
            AltOut::Pref(n, b) => format!("Alt: {n} {}", on(b)),
            AltOut::Captions(b) => format!("Alt+C: captions {}", on(b)),
            AltOut::Pause(b) => format!("Alt+P: {}", if *b { "paused (any key resumes)" } else { "resumed" }),
            AltOut::SaveScreen => "Alt+S: the save screen".into(),
            AltOut::Exit => "Alt+X: exit".into(),
            AltOut::Fps(b) => format!("Shift+Alt+F: FPS counter {}", on(b)),
            AltOut::Hotspots(b) => format!("Shift+Alt+H: hotspots {}", on(b)),
            AltOut::StateBox => "Shift+Alt+I: state box".into(),
        }
    }
}

/// H2061: Alt+digit's volume in percent — the original's (max / 9) × digit with the port's max 100, rounded (Alt+0 =
/// silence, Alt+9 = full).
pub fn volume_pct(digit: u32) -> u32 {
    ((digit.min(9) * 100 + 4) / 9).min(100)
}

/// H2066: the state box's text — the same five values as the original's box, in the port's own wording.
pub fn state_text(s: &Session) -> String {
    let v = |i: usize| s.r.w.vars.get(STATE_VARS[i]).unwrap_or(0);
    format!("Node {}\nScores: Asta {}, Loss {}, Paranoia {}, X-Track {}", v(0) as u32, v(1), v(2), v(3), v(4))
}

/// The key `vk` (a Windows virtual-key code: '0'–'9', 'A'–'Z') pressed with Alt (and Shift when `shift`): what it
/// does. The session's part is done here; the host does its part from the result.
pub fn alt_key(s: &mut Session, vk: u32, shift: bool) -> AltOut {
    // C241: the intro's loop takes Alt+X only (front::host::intro_abort)
    if s.intro_playing() {
        if vk == 0x58 && super::host::intro_abort(s) {
            return AltOut::Exit;
        }
        return AltOut::None;
    }
    // the state box is modal: its key closes it (front::host::key)
    if s.alt.msgbox.is_some() {
        s.alt.msgbox = None;
        return AltOut::None;
    }
    // H2063: while paused only Alt+P / Alt+X act; other keys resume (front::host::key)
    if s.alt.paused && vk != 0x50 && vk != 0x58 {
        resume(s);
        return AltOut::Pause(false);
    }
    match (vk, shift) {
        (0x30..=0x39, _) => AltOut::Volume(volume_pct(vk - 0x30)),
        (0x46, true) => {
            let on = s.alt.fps.is_none();
            s.alt.fps = on.then(Fps::default);
            AltOut::Fps(on)
        }
        (0x48, true) => {
            s.overlay = !s.overlay;
            AltOut::Hotspots(s.overlay)
        }
        (0x49, true) => {
            s.alt.msgbox = Some(state_text(s));
            AltOut::StateBox
        }
        (0x43, _) => {
            let on = !s.r.w.closed_captioning();
            s.r.set_closed_captioning(on);
            AltOut::Captions(on)
        }
        (0x50, _) => {
            let on = !s.alt.paused;
            if s.r.alt_pause(on) {
                s.alt.paused = on;
                AltOut::Pause(on)
            } else {
                AltOut::None
            }
        }
        (0x53, _) => {
            if s.r.alt_save_screen() {
                AltOut::SaveScreen
            } else {
                AltOut::None
            }
        }
        (0x58, _) => {
            if s.alt.paused {
                resume(s);
            }
            AltOut::Exit
        }
        (k, _) => match PREF_KEYS.iter().find(|p| p.0 == k) {
            Some(&(_, var, name)) => s.r.alt_toggle_pref(var).map_or(AltOut::None, |on| AltOut::Pref(name, on)),
            None => AltOut::None,
        },
    }
}

/// H2063: Alt+P's pause ends (Alt+P again, any key, a click).
pub fn resume(s: &mut Session) {
    if std::mem::take(&mut s.alt.paused) {
        s.r.alt_pause(false);
    }
}

/// H2066 / H2067: the state box and the FPS counter over a frame.
pub fn draw(s: &Session, fb: &mut [u8]) {
    if let Some(f) = &s.alt.fps {
        let t = format!("FPS {}", f.fps());
        fill(fb, 2, 2, 8 + t.len() as i32 * 8, 18, [0, 0, 0]);
        super::font::text(fb, 6, 5, &t, [255, 255, 0]);
    }
    if let Some(m) = &s.alt.msgbox {
        draw_box(fb, m);
    }
}

/// The state box in the port's dialog style (the settings screen's colours, Arial 12), centred, with an OK button.
fn draw_box(fb: &mut [u8], text: &str) {
    const BG: [u8; 3] = [10, 16, 20];
    const LINE: [u8; 3] = [40, 70, 90];
    const FG: [u8; 3] = [200, 215, 220];
    const ACCENT: [u8; 3] = [120, 200, 240];
    let lines: Vec<&str> = text.lines().collect();
    let (w, h) = (360, 52 + 18 * lines.len() as i32 + 30);
    let (x0, y0) = ((SCREEN_W as i32 - w) / 2, (SCREEN_H as i32 - h) / 2);
    fill(fb, x0, y0, x0 + w, y0 + h, BG);
    rect(fb, x0, y0, x0 + w - 1, y0 + h - 1, LINE);
    super::settings::label(fb, x0 + 14, y0 + 10, "The X-Files", ACCENT);
    for (i, l) in lines.iter().enumerate() {
        super::settings::label(fb, x0 + 14, y0 + 36 + 18 * i as i32, l, FG);
    }
    let (bx, by) = (x0 + w / 2 - 30, y0 + h - 34);
    rect(fb, bx, by, bx + 60, by + 22, ACCENT);
    super::settings::label(fb, bx + 22, by + 4, "OK", FG);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_steps() {
        assert_eq!((0..=9).map(volume_pct).collect::<Vec<_>>(), [0, 11, 22, 33, 44, 56, 67, 78, 89, 100]);
    }

    #[test]
    fn fps_ring() {
        let mut f = Fps::default();
        assert_eq!(f.fps(), 0);
        for _ in 0..FPS_SAMPLES {
            f.push(50);
        }
        assert_eq!(f.fps(), 20);
        // the ring keeps the last 20: 20 × 10 ms → 100 fps
        for _ in 0..FPS_SAMPLES {
            f.push(10);
        }
        assert_eq!(f.fps(), 100);
    }
}
