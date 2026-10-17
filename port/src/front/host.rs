//! The platform-independent part of an interactive front (the SDL window `bin/xf.rs`, the web worker `port/web/`):
//! what a key, a mouse button or a pointer move does to the [`Session`], and the software sound mixer that follows
//! [`Session::audio_cues`]. The platform only maps its events to these calls and moves frames / samples out.
use super::{Cue, Session};
use crate::media::audio::Pcm;
use std::sync::Arc;

/// A key by name → the Windows virtual-key code the game receives (H174): Enter / Backspace / Tab / Space / Delete,
/// letters and digits (also keypad digits). `name` is an SDL key name ("Return", "A", "Keypad 7") or a DOM
/// `KeyboardEvent.key` ("Enter", "a", "7").
pub fn vk(name: &str) -> Option<u32> {
    match name {
        "Return" | "Keypad Enter" | "Enter" => return Some(0x0d),
        "Backspace" => return Some(0x08),
        "Tab" => return Some(0x09),
        "Space" | " " => return Some(0x20),
        "Delete" => return Some(0x2e),
        "Escape" | "Esc" => return Some(0x1b),
        "Up" | "ArrowUp" => return Some(0x26),
        "Down" | "ArrowDown" => return Some(0x28),
        // VK_OEM_MINUS / VK_OEM_PERIOD / VK_OEM_COMMA (the ING / login filter FUN_00486e60 keeps '-', '.', ',')
        "-" | "Minus" | "Keypad -" => return Some(0xbd),
        "." | "Period" | "Keypad ." => return Some(0xbe),
        "," | "Comma" => return Some(0xbc),
        _ => {}
    }
    let mut ch = name.chars();
    match (ch.next(), ch.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as u32),
        _ => name.strip_prefix("Keypad ").and_then(|d| d.parse::<u32>().ok()).filter(|d| *d < 10).map(|d| 0x30 + d),
    }
}

/// HYPOTHESIS H867 (static, the original's key filter FUN_00458280 / H660 + post-hoc `traces/fo-088.jsonl` k25: a `t:` step typing a 12-character number with two '-',
/// the ING field's KeyPress action 0xc69f executed 14 times): a typed '-' / '.' / ',' is a WM_KEYDOWN of VK_OEM_MINUS /
/// PERIOD / COMMA (not in that filter's ignore list → a key event with key 0xbd / 0xbe / 0xbc, which the ING / login
/// filter drops) and TranslateMessage's WM_CHAR (a second key event with the character, kept) — two key events. Letters, digits
/// and the space come as one (their keydown is ignored). The key events of one typed character, in order.
pub fn key_events_of_char(c: char) -> Vec<u32> {
    match c {
        '-' | '.' | ',' => vec![vk_of_char(c), c as u32],
        c => vec![vk_of_char(c)],
    }
}

/// The WM_CHAR byte TranslateMessage adds after a keydown of VK_OEM_MINUS / PERIOD / COMMA (H867).
pub fn char_of_oem_vk(vk: u32) -> Option<u32> {
    match vk {
        0xbd => Some('-' as u32),
        0xbe => Some('.' as u32),
        0xbc => Some(',' as u32),
        _ => None,
    }
}

/// The virtual key of a typed character (VkKeyScanA on a US layout): letters upper-cased, '-' / '.' / ',' → VK_OEM_*
pub fn vk_of_char(c: char) -> u32 {
    match c {
        '-' => 0xbd,
        '.' => 0xbe,
        ',' => 0xbc,
        c => c.to_ascii_uppercase() as u32,
    }
}

/// Space / Enter while media plays: end it now (fallback when a file cannot be decoded). False if nothing plays.
pub fn end_playing(s: &mut Session) -> bool {
    // H2066 / H2063: the key closes the state box / ends Alt+P's pause instead
    if alt_modal(s).is_some() {
        return true;
    }
    // C241: keys skip no intro movie
    if s.intro_playing() {
        return false;
    }
    match s.r.playing() {
        Some(m) => {
            s.r.media_ended(m);
            true
        }
        None => false,
    }
}

/// H1660: Alt+X while the intro plays (WM_SYSKEYDOWN 'X' in the intro's loop FUN_00408cf0) ends the program (the intro
/// routine returns 0).
/// False (nothing done) when no intro plays.
pub fn intro_abort(s: &mut Session) -> bool {
    if !s.intro_playing() {
        return false;
    }
    s.intro = None;
    s.menu.request = Some(crate::world::devices::MenuChoice::Quit);
    true
}

/// What the host must do after an input: the menu's New / Quit (H404) or a Load chosen on the load screen (H403).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostReq {
    New,
    Quit,
    Load(std::path::PathBuf),
}

/// Take the session's pending host request (a host polls it after its inputs / ticks).
pub fn request(s: &mut Session) -> Option<HostReq> {
    use crate::world::devices::MenuChoice;
    if let Some(p) = s.load_req.take() {
        return Some(HostReq::Load(p));
    }
    match s.menu.request.take() {
        Some(MenuChoice::New) => Some(HostReq::New),
        Some(MenuChoice::Quit) => Some(HostReq::Quit),
        _ => None,
    }
}

/// The save / load screens (H140 / H70) are shown: Esc goes to their focused item (SaveKeyPress, H403).
fn save_screen_shown(s: &Session) -> bool {
    s.r.w.layouts().iter().any(|l| l.0 == 0x14269 || l.0 == 0x142ca)
}

/// A key press (virtual-key code) to the focused InterfaceItem (H174); returns a log line.
///
/// Esc (H169: the original's menu key path FUN_004525f0) opens the game's menu; on the save / load screens it goes to the
/// focused slot item (SaveKeyPress → SaveLoadComit(0), H403 — which of the two the original does there is not traced);
/// while the menu shows it is "Return" (port choice, H404). Other keys are ignored while the menu shows.
pub fn key(s: &mut Session, code: u32) -> String {
    // C241 (H1660): the intro's loop takes no key (keys are never delivered later); Alt+X: [`intro_abort`]
    if s.intro_playing() {
        return format!("key {code:#x} (intro plays: ignored)");
    }
    // H2066: the state box (Shift+Alt+I) closes on a key; H2063 (port choice): any key ends Alt+P's pause
    if let Some(l) = alt_modal(s) {
        return format!("key {code:#x}: {l}");
    }
    // H1700: the "Try again?" box's modal loop takes no key (port choice: only its buttons answer)
    if s.r.rewind_box {
        return format!("key {code:#x} (\"Try again?\" box: ignored)");
    }
    if s.r.menu_open {
        // Esc = Return only in a running game (port choice, H404): with no game, Return is a New Game (C243)
        if code == 0x1b && s.r.game_running {
            s.menu_choose(crate::world::devices::MenuChoice::Resume);
            return "Esc: menu → Return".into();
        }
        return format!("key {code:#x} (menu shows: ignored)");
    }
    if code == 0x1b && !(save_screen_shown(s) && s.r.key_press(code)) {
        s.r.open_menu();
        return "Esc: menu".into();
    }
    let taken = s.r.key_press(code);
    // H867: the keydown's WM_CHAR is a second key event
    if let Some(ch) = char_of_oem_vk(code) {
        s.r.key_press(ch);
    }
    format!("key {code:#x}{}", if taken { "" } else { " (no focus)" })
}

/// An Alt key (H1868 / H2060: WM_SYSKEYDOWN, the original's Alt-key handler) — `vk` the key's virtual-key code ('0'–'9', 'A'–'Z'),
/// `shift` = Shift held too: [`super::altkeys::alt_key`]. Alt keys never reach the focused item as key presses.
pub fn alt(s: &mut Session, vk: u32, shift: bool) -> super::altkeys::AltOut {
    super::altkeys::alt_key(s, vk, shift)
}

/// The Alt keys' modal states take an input: the state box closes, Alt+P's pause ends. None = neither shows.
fn alt_modal(s: &mut Session) -> Option<&'static str> {
    if s.alt.msgbox.take().is_some() {
        return Some("state box closed");
    }
    if s.alt.paused {
        super::altkeys::resume(s);
        return Some("Alt+P pause ended");
    }
    None
}

/// HYPOTHESIS H406: the sound gain the game's preferences ask for — Pref Volume (var 0x3d3, 0..100, H189; the
/// dialogue / music / effect volumes 0x3d0..0x3d2 are not applied: which sounds they scale is not decoded); 1.0 when
/// unknown.
pub fn volume(s: &Session) -> f32 {
    s.r.w.vars.get(0x3d3).map_or(1.0, |v| (v as f32 / 100.0).clamp(0.0, 1.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

/// A mouse button pressed at `at` (a 640×480 picture point; None = outside the picture). Returns a log line.
///
/// Right button while a video plays: skip it to its media end (Deactivation fires) — the user's way of skipping in
/// the original (E9: fo-010 intro, 0x204); whether in-game videos skip too is not traced. H322: a left click while a
/// video plays is not delivered (no Mouse Click in the traces) and does not end it — H1900: it goes through the hit
/// chain, where the playing video takes the movie area (H452, the band's full width H1901) and the dialogue box, the
/// movie-hotspot targets and the icon / inventory bands keep working (C220, C132). H1680 (hypothesis, port choice):
/// not while the save / load screen shows — it covers the video (menu → Load during the intro), and its Load / Save /
/// Cancel must take the click.
pub fn button(s: &mut Session, b: Button, at: Option<(i32, i32)>) -> Option<String> {
    // C241 (H1660): during the intro either button down ends the current movie only (WM_LBUTTONDOWN / WM_RBUTTONDOWN)
    if s.intro_playing() {
        let _ = b;
        return s.intro_skip();
    }
    // H2066 / H2063: a click closes the state box / ends Alt+P's pause (and goes no further)
    if let Some(l) = alt_modal(s) {
        return Some(format!("click: {l}"));
    }
    if s.r.menu_open {
        let (x, y) = at?;
        return match b {
            Button::Left => s.menu_click(x, y).map(|c| format!("menu: {c:?}")),
            Button::Right => None,
        };
    }
    // H1700: the "Try again?" box takes the left button only; H1703: the frozen game takes no button (no video skip)
    if s.r.rewind_box || s.r.frozen {
        let (x, y) = at?;
        return match b {
            Button::Left => {
                s.click_at(x, y);
                Some(format!("click ({x},{y}) on the {}", if s.r.frozen { "frozen game" } else { "\"Try again?\" box" }))
            }
            Button::Right => None,
        };
    }
    if let Some(v) = s.r.video_playing().filter(|_| !save_screen_shown(s)) {
        // H1900: the left click goes through the hit chain, whose playing player (H452) takes the movie area
        let rules = s.r.w.rules;
        let chain = rules.h1900_video_click_hit_chain && rules.h452_video_player_hit && rules.h130_hit_chain;
        return match b {
            Button::Right => {
                s.r.media_ended(v);
                Some(format!("skip video {v:#x}"))
            }
            Button::Left if chain => {
                let (x, y) = at?;
                match s.press_at(x, y) {
                    Some(c) if c.owner == crate::world::devices::VIDEO_OWNER => {
                        Some(format!("click ({x},{y}) ignored: video {v:#x} playing there (H452; right click skips)"))
                    }
                    Some(c) => Some(format!("click {:#x} (class {:#x}) while video {v:#x} plays", c.owner, c.cls)),
                    None => None,
                }
            }
            Button::Left => Some("click ignored: video playing (right click skips)".into()),
        };
    }
    let (x, y) = at?;
    match b {
        Button::Right => s.r.right_click(x, y).map(|c| format!("right click {:#x} (class {:#x})", c.owner, c.cls)),
        // C107: the button-up follows ([`release`]) — an IdeaIcon's answer waits for it
        Button::Left => s.press_at(x, y).map(|c| format!("click {:#x} (class {:#x})", c.owner, c.cls)),
    }
}

/// A mouse button released at `at` (None = outside the picture): the left button's up drops a dragged IdeaIcon (C107:
/// released on the icon → its idea's answer plays; H921 elsewhere). Returns a log line when it dropped one.
pub fn release(s: &mut Session, b: Button, at: Option<(i32, i32)>) -> Option<String> {
    if b != Button::Left || !s.r.button_down {
        return None;
    }
    let icon = s.r.w.idea_drag;
    s.release_at(at);
    icon.map(|i| format!("idea icon {i:#x} released"))
}

/// The mouse wheel turned by `lines` notches at `at` (picture point; None = off the picture), `lines` < 0 = up (away
/// from the user). HYPOTHESIS H1815 — a port convenience, the original has no wheel: [`Session::wheel`]. Returns a log
/// line when something scrolled.
pub fn wheel(s: &mut Session, at: Option<(i32, i32)>, lines: i32) -> Option<String> {
    let (x, y) = at?;
    s.wheel(x, y, lines).then(|| format!("wheel {lines} at ({x},{y})"))
}

/// What the platform shows as its own pointer after a move ([`motion`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// H307 / H331: the game's cursor is drawn into the frame — hide the system pointer
    Hidden,
    /// over a clickable
    Hand,
    Arrow,
}

/// The pointer moved to `at` (picture point; None = outside the picture): hover goes to the core.
pub fn motion(s: &mut Session, at: Option<(i32, i32)>) -> Pointer {
    // the intro: no cursor of the game is decoded there (port: the system pointer)
    if s.intro_playing() {
        return Pointer::Arrow;
    }
    if s.alt.msgbox.is_some() {
        return Pointer::Arrow; // H2066: the state box (the system pointer)
    }
    if s.r.menu_open {
        // the menu: the system pointer (the game's cursor over the menu is not decoded)
        let over = at.is_some_and(|(x, y)| s.menu_hover(x, y).is_some());
        return if over { Pointer::Hand } else { Pointer::Arrow };
    }
    if s.r.rewind_box {
        // H1700: the box's buttons (the system pointer; the game's cursor over the box is not decoded)
        s.mouse = at;
        let over = at.is_some_and(|(x, y)| super::rewind::button_at(x, y).is_some());
        return if over { Pointer::Hand } else { Pointer::Arrow };
    }
    let over = at.is_some_and(|(x, y)| s.hover_at(x, y).is_some());
    if at.is_some() && super::cursor::shape(s).is_some() {
        Pointer::Hidden
    } else if over {
        Pointer::Hand
    } else {
        Pointer::Arrow
    }
}

/// One voice per sounding asset, nearest-sample resampling to the output rate.
pub struct Voice {
    pub asset: u32,
    pcm: Arc<Pcm>,
    /// position in source frames (fractional)
    pos: f64,
    start: usize,
    end: usize,
    looped: bool,
    /// H1795: rate trim (1.0 = the source's rate) — a voice a little off its cue is pulled back gently, not moved
    trim: f64,
}

/// Software mixer (interleaved stereo i16 output).
#[derive(Default)]
pub struct Mix {
    pub voices: Vec<Voice>,
    pub rate: u32,
    /// output gain, 0.0..=1.0
    pub volume: f32,
}

impl Voice {
    /// The position in source frames (the next frame mixed) and the source's sample rate (for the A/V sync probe).
    pub fn pos(&self) -> (f64, u32) {
        (self.pos, self.pcm.rate)
    }
}

impl Mix {
    /// Mix the voices into `out` (interleaved stereo); finished voices are dropped.
    pub fn fill(&mut self, out: &mut [i16]) {
        let rate = self.rate.max(1) as f64;
        let vol = self.volume;
        out.fill(0);
        for v in self.voices.iter_mut() {
            let ch = v.pcm.channels.max(1) as usize;
            let step = v.pcm.rate as f64 / rate * v.trim;
            for o in out.chunks_exact_mut(2) {
                let mut i = v.pos as usize;
                if i >= v.end {
                    if !v.looped {
                        break;
                    }
                    v.pos = v.start as f64 + (v.pos - v.end as f64);
                    i = v.pos as usize;
                }
                let l = (v.pcm.samples.get(i * ch).copied().unwrap_or(0) as f32 * vol) as i32;
                let r = if ch > 1 { (v.pcm.samples.get(i * ch + 1).copied().unwrap_or(0) as f32 * vol) as i32 } else { l };
                o[0] = (o[0] as i32 + l).clamp(-32768, 32767) as i16;
                o[1] = (o[1] as i32 + r).clamp(-32768, 32767) as i16;
                v.pos += step;
            }
        }
        self.voices.retain(|v| v.looped || (v.pos as usize) < v.end);
    }

    /// Make the voices follow the session's cues: new cues start at their position, voices without a cue stop, a
    /// voice more than 200 ms away from its cue is moved to it; a cue with another range (e.g. the menu's RIGHT.mov
    /// hover range of another item, H1770) restarts its voice.
    pub fn sync(&mut self, cues: &[Cue]) {
        self.sync_to(cues, 0.0, 0.2, 0.0);
    }

    /// [`Mix::sync`] with the cues taken `offset_s` later (H1795: the next frame mixed is heard that much after the
    /// picture of the cue's time is seen; clamped to the sound's range, wrapped for a loop) and a tolerance `tol_s`.
    /// Within it, a non-looped voice's rate is trimmed by at most `max_trim` (0.003 = 0.3 %, inaudible) towards its
    /// cue — the audio device's clock drifts from the wall clock the cues follow (H1795).
    pub fn sync_to(&mut self, cues: &[Cue], offset_s: f64, tol_s: f64, max_trim: f64) {
        self.voices.retain(|v| cues.iter().any(|c| c.asset == v.asset));
        for c in cues {
            let rate = c.pcm.rate as f64;
            let tol = rate * tol_s;
            let mut want = c.pos as f64 + offset_s * rate;
            if c.looped && c.end > c.start {
                let len = (c.end - c.start) as f64;
                want = c.start as f64 + (want - c.start as f64).rem_euclid(len);
            }
            let want = want.clamp(c.start as f64, c.end as f64);
            match self.voices.iter_mut().find(|v| v.asset == c.asset) {
                Some(v) => {
                    if (v.start, v.end, v.looped) != (c.start, c.end, c.looped) || !Arc::ptr_eq(&v.pcm, &c.pcm) {
                        *v = Voice {
                            asset: c.asset,
                            pcm: c.pcm.clone(),
                            pos: want,
                            start: c.start,
                            end: c.end,
                            looped: c.looped,
                            trim: 1.0,
                        };
                    } else if (v.pos - want).abs() > tol && !(c.looped && v.looped) {
                        v.pos = want;
                        v.trim = 1.0;
                    } else if !c.looped && !v.looped {
                        // a quarter of the error (s) per second, capped
                        let err = (want - v.pos) / rate;
                        v.trim = 1.0 + (err * 0.25).clamp(-max_trim, max_trim);
                    }
                }
                None => self.voices.push(Voice {
                    asset: c.asset,
                    pcm: c.pcm.clone(),
                    pos: want,
                    start: c.start,
                    end: c.end,
                    looped: c.looped,
                    trim: 1.0,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn vk_names() {
        use super::vk;
        // SDL names (bin/xf.rs) and DOM KeyboardEvent.key (port/web) give the same codes
        assert_eq!((vk("Return"), vk("Enter"), vk("Keypad Enter")), (Some(0x0d), Some(0x0d), Some(0x0d)));
        assert_eq!((vk("Space"), vk(" ")), (Some(0x20), Some(0x20)));
        assert_eq!((vk("A"), vk("a"), vk("7"), vk("Keypad 7")), (Some(0x41), Some(0x41), Some(0x37), Some(0x37)));
        assert_eq!((vk("Backspace"), vk("Tab"), vk("Delete")), (Some(0x08), Some(0x09), Some(0x2e)));
        assert_eq!((vk("F5"), vk("Shift"), vk("é")), (None, None, None));
        assert_eq!((vk("Escape"), vk("Up"), vk("ArrowDown")), (Some(0x1b), Some(0x26), Some(0x28)));
    }

    #[test]
    fn sync_restarts_a_voice_on_a_new_range() {
        use super::{Arc, Cue, Mix, Pcm};
        let pcm = Arc::new(Pcm { rate: 1000, channels: 1, samples: vec![1; 2000] });
        let cue = |start, pos, end| Cue { asset: 7, pcm: pcm.clone(), pos, start, end, looped: false };
        let mut m = Mix { rate: 1000, volume: 1.0, ..Default::default() };
        m.sync(&[cue(100, 150, 600)]);
        m.sync(&[cue(100, 250, 600)]); // within 200 ms: kept where it is
        assert_eq!((m.voices[0].pos, m.voices[0].end), (150.0, 600));
        // H1770: the menu's hover moved to another item: RIGHT.mov's range changes, the voice restarts in it
        m.sync(&[cue(800, 810, 1300)]);
        assert_eq!((m.voices.len(), m.voices[0].pos, m.voices[0].start, m.voices[0].end), (1, 810.0, 800, 1300));
    }
}
