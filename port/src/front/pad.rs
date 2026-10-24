//! Game controller support (a port feature: the original has none) — one model for both fronts (the SDL window's
//! GameController, the browser's Gamepad API). The pad only makes the input the mouse and the keys already make: a
//! pointer move, a left / right button down / up at the pointer, Esc, F1, F10 ([`PadOut`]); the host feeds those
//! through its own mouse / key paths, so the game sees nothing new (headless runs, traces and the e2e never use it).
//!
//! HYPOTHESIS H2410 (port choice, comfort — no original behaviour to match). The map:
//!
//! - left stick: the pointer. [`Mode::Free`]: a speed curve (radial dead zone, the deflection squared, a ramp to twice
//!   the speed after holding the stick near full), slower while a trigger (LT / RT) is held. [`Mode::Centred`] (as the
//!   game's PlayStation version): the pointer rests at the View's centre, the stick's position places it around that
//!   centre (full deflection = the View's edges), let go it glides back to the centre; a snap or a mouse move parks it
//!   where it went until the stick moves again.
//! - D-pad / right stick: snap the pointer to the next live click target in that direction ([`snap`]; held: repeats);
//!   LB / RB: the previous / next target in reading order ([`cycle`]). The targets are the hit test's own list
//!   ([`targets`]: `Runner::clickables`, each at a point where the hit test finds it on top), so a snap lands only on
//!   something that reacts; the menu screen, the "Try again?" box and the window's settings screen have theirs.
//! - A = the left button (down / up: a held arrow repeats, an idea icon drags), B = the right button (skips a video).
//! - X = the pointer to the inventory strip at the bottom (the panel shows as for the mouse) and back.
//! - Y = hotspots (F1), Start = the game's menu (Esc), Back / View = the port settings (F10).
use super::host::Button;
use super::Session;
use crate::world::run::Clickable;

/// Buttons ([`PadIn::buttons`] bits; the SDL GameController / the browser's "standard" gamepad layout).
pub const A: u32 = 1 << 0;
pub const B: u32 = 1 << 1;
pub const X: u32 = 1 << 2;
pub const Y: u32 = 1 << 3;
pub const LB: u32 = 1 << 4;
pub const RB: u32 = 1 << 5;
pub const BACK: u32 = 1 << 6;
pub const START: u32 = 1 << 7;
pub const L3: u32 = 1 << 8;
pub const R3: u32 = 1 << 9;
pub const UP: u32 = 1 << 10;
pub const DOWN: u32 = 1 << 11;
pub const LEFT: u32 = 1 << 12;
pub const RIGHT: u32 = 1 << 13;

/// The stick's radial dead zone (of full deflection)
pub const DEADZONE: f32 = 0.18;
/// The stick's outer dead zone: past this length it counts as full deflection (real sticks rarely report 1.0 — a
/// DualShock 4 tops out around 0.9 on some axes, less on the diagonals)
pub const OUTER: f32 = 0.9;
/// free pointer: pixels per second at full deflection, normal speed
pub const BASE_SPEED: f32 = 420.0;
/// free pointer: held near full deflection this long (ms), the speed ramps up ...
pub const ACCEL_AFTER_MS: f32 = 350.0;
/// ... over this long to [`ACCEL_MAX`] times
pub const ACCEL_RAMP_MS: f32 = 600.0;
pub const ACCEL_MAX: f32 = 2.0;
/// a trigger held past this: the precision speed ([`PRECISION`] times)
pub const TRIGGER: f32 = 0.5;
pub const PRECISION: f32 = 0.3;
/// centred pointer: the rest point (the View's centre: the movie area (20, 90)–(620, 330)); a full deflection reaches
/// the picture's edges (the inventory bar, the icons and the menus lie outside the View), so the reach differs above
/// and below the rest point
pub const CENTRE: (f32, f32) = (320.0, 210.0);
pub const PICTURE: [f32; 4] = [0.0, 0.0, 639.0, 479.0];
/// centred pointer: the glide's time constant (ms)
pub const GLIDE_MS: f32 = 30.0;
/// snaps: the right stick counts past this deflection; a held direction repeats after [`REPEAT_FIRST_MS`], then every
/// [`REPEAT_MS`]
pub const SNAP_STICK: f32 = 0.55;
pub const REPEAT_FIRST_MS: i64 = 380;
pub const REPEAT_MS: i64 = 170;
/// X: the point in the inventory strip (the panel's rect is y 420..480)
pub const STRIP_Y: i32 = 452;
pub const STRIP_TOP: i32 = 420;

/// One reading of the pad: sticks −1..1 (y down), triggers 0..1, [`A`]… bits.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadIn {
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub lt: f32,
    pub rt: f32,
    pub buttons: u32,
}

impl PadIn {
    /// Anything pressed or deflected past the dead zone (the host picks the pad that is used).
    pub fn active(&self) -> bool {
        self.buttons != 0
            || [self.lx, self.ly, self.rx, self.ry].iter().any(|v| v.abs() > DEADZONE)
            || self.lt > TRIGGER
            || self.rt > TRIGGER
    }
}

/// How the left stick moves the pointer (settings `pad`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Free,
    Centred,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadCfg {
    pub mode: Mode,
    /// the free pointer's speed, 1.0 = normal
    pub speed: f32,
}

impl PadCfg {
    /// From the settings (`pad` = off / free / centred, `pad_speed` = percent); None = the controller is off.
    pub fn from_settings(set: &super::settings::Settings) -> Option<PadCfg> {
        let mode = match set.get("pad") {
            "free" => Mode::Free,
            "centred" => Mode::Centred,
            // no "off": a pad that switched itself off could only be switched back with the mouse
            _ => Mode::Free,
        };
        let speed = set.get("pad_speed").parse::<f32>().unwrap_or(100.0) / 100.0;
        Some(PadCfg { mode, speed })
    }
}

impl Default for PadCfg {
    fn default() -> PadCfg {
        PadCfg { mode: Mode::Free, speed: 1.0 }
    }
}

/// A key the pad stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKey {
    /// Esc: the game's menu
    Menu,
    /// F10: the port settings
    Settings,
    /// F1: the hotspot overlay
    Hotspots,
}

/// What the host does: the mouse's and the keys' own inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadOut {
    /// the pointer moved to a picture point
    Move(i32, i32),
    /// a button went down / up at the pointer
    Down(Button),
    Up(Button),
    Key(PadKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn v(self) -> (f32, f32) {
        match self {
            Dir::Up => (0.0, -1.0),
            Dir::Down => (0.0, 1.0),
            Dir::Left => (-1.0, 0.0),
            Dir::Right => (1.0, 0.0),
        }
    }
}

/// A click target: its rect (x1, y1, x2, y2, exclusive) and the point a snap goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub rect: [i32; 4],
    pub at: (i32, i32),
}

impl Target {
    pub fn of_rect(r: [i32; 4]) -> Target {
        Target { rect: r, at: ((r[0] + r[2]) / 2, (r[1] + r[3]) / 2) }
    }
    fn contains(&self, (x, y): (i32, i32)) -> bool {
        x >= self.rect[0] && x < self.rect[2] && y >= self.rect[1] && y < self.rect[3]
    }
}

/// The stick past the radial dead zone, its length rescaled to 0..1 between [`DEADZONE`] and [`OUTER`] (direction
/// kept).
pub fn deadzone(x: f32, y: f32) -> (f32, f32) {
    let m = (x * x + y * y).sqrt();
    if m <= DEADZONE || !m.is_finite() {
        return (0.0, 0.0);
    }
    let k = ((m - DEADZONE) / (OUTER - DEADZONE)).min(1.0) / m;
    (x * k, y * k)
}

/// Free pointer: the speed (px/s) of a dead-zoned stick of length `m`, held near full for `held_ms`.
pub fn free_speed(m: f32, held_ms: f32, speed: f32, precision: bool) -> f32 {
    let ramp = ((held_ms - ACCEL_AFTER_MS) / ACCEL_RAMP_MS).clamp(0.0, 1.0);
    let accel = 1.0 + (ACCEL_MAX - 1.0) * ramp;
    BASE_SPEED * speed * m * m * accel * if precision { PRECISION } else { 1.0 }
}

/// Free pointer: `pos` moved `dt_ms` by the dead-zoned stick `s`, kept on the picture.
pub fn free_step(pos: (f32, f32), s: (f32, f32), dt_ms: f32, held_ms: f32, speed: f32, precision: bool) -> (f32, f32) {
    let m = (s.0 * s.0 + s.1 * s.1).sqrt();
    if m == 0.0 {
        return pos;
    }
    let d = free_speed(m, held_ms, speed, precision) * dt_ms / 1000.0;
    clamp_picture((pos.0 + s.0 / m * d, pos.1 + s.1 / m * d))
}

fn clamp_picture(p: (f32, f32)) -> (f32, f32) {
    (p.0.clamp(0.0, super::SCREEN_W as f32 - 1.0), p.1.clamp(0.0, super::SCREEN_H as f32 - 1.0))
}

/// Centred pointer: where a dead-zoned stick `s` places the pointer — around [`CENTRE`], the stick's circle stretched
/// to the picture's rectangle [`PICTURE`] (a full diagonal reaches its corner).
pub fn centred_target(s: (f32, f32)) -> (f32, f32) {
    let m = (s.0 * s.0 + s.1 * s.1).sqrt();
    let big = s.0.abs().max(s.1.abs());
    if big == 0.0 {
        return CENTRE;
    }
    let k = m / big;
    let reach_x = if s.0 < 0.0 { CENTRE.0 - PICTURE[0] } else { PICTURE[2] - CENTRE.0 };
    let reach_y = if s.1 < 0.0 { CENTRE.1 - PICTURE[1] } else { PICTURE[3] - CENTRE.1 };
    let (x, y) = (CENTRE.0 + s.0 * k * reach_x, CENTRE.1 + s.1 * k * reach_y);
    (x.clamp(PICTURE[0], PICTURE[2]), y.clamp(PICTURE[1], PICTURE[3]))
}

/// Centred pointer: `pos` glides `dt_ms` towards `to` (exponential, [`GLIDE_MS`]; there within half a pixel).
pub fn glide(pos: (f32, f32), to: (f32, f32), dt_ms: f32) -> (f32, f32) {
    let k = 1.0 - (-dt_ms / GLIDE_MS).exp();
    let p = (pos.0 + (to.0 - pos.0) * k, pos.1 + (to.1 - pos.1) * k);
    if (p.0 - to.0).abs() < 0.5 && (p.1 - to.1).abs() < 0.5 {
        to
    } else {
        p
    }
}

/// The target a snap from `from` towards `dir` lands on: one whose point lies ahead (not the one under `from`) within
/// the cone of twice as far across as along (else anywhere ahead), the distance along the direction plus 2.5 × the
/// distance across it smallest. None = nothing that way.
pub fn snap(from: (i32, i32), dir: Dir, targets: &[Target]) -> Option<usize> {
    let (dx, dy) = dir.v();
    let best = |cone: bool| {
        let mut best: Option<(f32, usize)> = None;
        for (i, t) in targets.iter().enumerate() {
            if t.contains(from) {
                continue;
            }
            let (vx, vy) = ((t.at.0 - from.0) as f32, (t.at.1 - from.1) as f32);
            let along = vx * dx + vy * dy;
            let across = (vx * dy - vy * dx).abs();
            if along < 1.0 || (cone && across > 2.0 * along) {
                continue;
            }
            let score = along + 2.5 * across;
            if best.is_none_or(|(b, _)| score < b) {
                best = Some((score, i));
            }
        }
        best.map(|b| b.1)
    };
    best(true).or_else(|| best(false))
}

/// LB / RB: the next (`forward`) or previous target in reading order (rows top to bottom, then left to right) from
/// the one under `from` (or from the point itself), wrapping around.
pub fn cycle(from: (i32, i32), forward: bool, targets: &[Target]) -> Option<usize> {
    if targets.is_empty() {
        return None;
    }
    let key = |p: (i32, i32)| (p.1, p.0);
    let mut order: Vec<usize> = (0..targets.len()).collect();
    order.sort_by_key(|&i| key(targets[i].at));
    // the target the pointer is on: the one snapped to, else the smallest one under it
    let area = |t: &Target| (t.rect[2] - t.rect[0]) as i64 * (t.rect[3] - t.rect[1]) as i64;
    let cur = order.iter().position(|&i| targets[i].at == from).or_else(|| {
        let i = order.iter().copied().filter(|&i| targets[i].contains(from)).min_by_key(|&i| area(&targets[i]))?;
        order.iter().position(|&j| j == i)
    });
    let n = order.len();
    let pos = match (cur, forward) {
        (Some(c), true) => (c + 1) % n,
        (Some(c), false) => (c + n - 1) % n,
        (None, true) => order.iter().position(|&i| key(targets[i].at) > key(from)).unwrap_or(0),
        (None, false) => order.iter().rposition(|&i| key(targets[i].at) < key(from)).unwrap_or(n - 1),
    };
    Some(order[pos])
}

/// The click targets on screen now: the menu screen's items (and the port's settings button under them), the "Try
/// again?" box's buttons, else every clickable of the hit test at a point where it is the one hit (the engine's band
/// objects the held gun / camera / a playing video stand for are left out: they take the whole View). None during the
/// intro.
pub fn targets(s: &Session) -> Vec<Target> {
    use crate::world::devices::{CAMERA_OWNER, GUN_OWNER, VIDEO_OWNER};
    if s.intro.is_some() || s.alt.msgbox.is_some() {
        return Vec::new();
    }
    if s.r.menu_open {
        let mut v: Vec<Target> = super::menu::ITEMS
            .iter()
            .filter(|(c, ..)| *c != crate::world::devices::MenuChoice::Save || s.r.game_running)
            .map(|(_, r, ..)| Target::of_rect(*r))
            .collect();
        v.push(Target::of_rect(super::settings::MENU_BUTTON));
        return v;
    }
    if s.r.rewind_box {
        return vec![Target::of_rect(super::rewind::YES_RECT), Target::of_rect(super::rewind::NO_RECT)];
    }
    let list = s.r.clickables();
    let mut out = Vec::new();
    for c in &list {
        if [GUN_OWNER, CAMERA_OWNER, VIDEO_OWNER].contains(&c.owner) {
            continue;
        }
        // one target per point (an owner listed twice, e.g. in two bands, is one stop)
        if let Some(at) = reachable(s, &list, c).filter(|at| !out.iter().any(|t: &Target| t.at == *at)) {
            out.push(Target { rect: c.rect.map(|v| v as i32), at });
        }
    }
    out
}

/// A point of `c`'s rect where the hit test finds `c` on top: its centre if so, else the nearest such point to it on
/// an 8-pixel grid.
fn reachable(s: &Session, list: &[Clickable], c: &Clickable) -> Option<(i32, i32)> {
    let [x1, y1, x2, y2] = c.rect.map(|v| v as i32);
    let (x1, y1, x2, y2) = (x1.max(0), y1.max(0), x2.min(super::SCREEN_W as i32), y2.min(super::SCREEN_H as i32));
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    let on_top = |x: i32, y: i32| s.r.hit_in(list, x, y).is_some_and(|h| h == c);
    let (cx, cy) = ((x1 + x2) / 2, (y1 + y2) / 2);
    if on_top(cx, cy) {
        return Some((cx, cy));
    }
    let mut best: Option<(i32, (i32, i32))> = None;
    for y in (y1 + 2..y2).step_by(8) {
        for x in (x1 + 2..x2).step_by(8) {
            let d = (x - cx).pow(2) + (y - cy).pow(2);
            if best.is_none_or(|b| d < b.0) && on_top(x, y) {
                best = Some((d, (x, y)));
            }
        }
    }
    best.map(|b| b.1)
}

/// The pad's state between readings.
#[derive(Clone, Debug)]
pub struct Pad {
    /// the pointer (picture point)
    pub pos: (f32, f32),
    /// the pad moved the pointer last (the host hides its system pointer and draws one where none of the game's shows)
    pub driving: bool,
    prev: u32,
    /// free pointer: how long the stick has been near full deflection
    held_ms: f32,
    /// the snap direction held and the time to its next repeat
    repeat: Option<(Dir, i64)>,
    /// centred pointer: parked where a snap / the mouse put it (no glide back) until the stick moves
    parked: bool,
    /// X: where the pointer was before it went to the inventory strip
    back: Option<(f32, f32)>,
    sent: Option<(i32, i32)>,
}

impl Default for Pad {
    fn default() -> Pad {
        Pad { pos: CENTRE, driving: false, prev: 0, held_ms: 0.0, repeat: None, parked: false, back: None, sent: None }
    }
}

impl Pad {
    pub fn point(&self) -> (i32, i32) {
        (self.pos.0.round() as i32, self.pos.1.round() as i32)
    }

    /// The mouse moved to `at` (last input wins: the pad goes on from there).
    pub fn mouse_moved(&mut self, at: (i32, i32)) {
        self.pos = (at.0 as f32, at.1 as f32);
        self.sent = Some(at);
        self.driving = false;
        self.parked = true;
    }

    /// One reading `inp`, `dt_ms` after the last: the inputs to make. `targets` is asked only for a snap.
    pub fn update(&mut self, inp: &PadIn, dt_ms: u64, cfg: &PadCfg, targets: &mut dyn FnMut() -> Vec<Target>) -> Vec<PadOut> {
        let dt = dt_ms.min(250) as f32;
        let pressed = inp.buttons & !self.prev;
        let released = self.prev & !inp.buttons;
        self.prev = inp.buttons;
        let mut out = Vec::new();
        let start = self.pos;
        let stick = deadzone(inp.lx, inp.ly);
        let m = (stick.0 * stick.0 + stick.1 * stick.1).sqrt();
        match cfg.mode {
            Mode::Free => {
                self.held_ms = if m > 0.9 { self.held_ms + dt } else { 0.0 };
                let precision = inp.lt > TRIGGER || inp.rt > TRIGGER;
                self.pos = free_step(self.pos, stick, dt, self.held_ms, cfg.speed, precision);
            }
            Mode::Centred => {
                if m > 0.0 {
                    self.parked = false;
                }
                let to = if m > 0.0 { centred_target(stick) } else if self.parked { self.pos } else { CENTRE };
                self.pos = glide(self.pos, to, dt);
            }
        }
        // snaps: the D-pad, else the right stick's main axis
        let dpad = |b: u32| inp.buttons & b != 0;
        let rs = deadzone(inp.rx, inp.ry);
        let held = if dpad(UP) {
            Some(Dir::Up)
        } else if dpad(DOWN) {
            Some(Dir::Down)
        } else if dpad(LEFT) {
            Some(Dir::Left)
        } else if dpad(RIGHT) {
            Some(Dir::Right)
        } else if inp.rx.abs().max(inp.ry.abs()) > SNAP_STICK && rs != (0.0, 0.0) {
            Some(if inp.rx.abs() > inp.ry.abs() {
                if inp.rx > 0.0 { Dir::Right } else { Dir::Left }
            } else if inp.ry > 0.0 {
                Dir::Down
            } else {
                Dir::Up
            })
        } else {
            None
        };
        let fire = match (held, self.repeat) {
            (None, _) => {
                self.repeat = None;
                None
            }
            (Some(d), Some((rd, next))) if rd == d => {
                let next = next - dt as i64;
                if next <= 0 {
                    self.repeat = Some((d, next + REPEAT_MS));
                    Some(d)
                } else {
                    self.repeat = Some((d, next));
                    None
                }
            }
            (Some(d), _) => {
                self.repeat = Some((d, REPEAT_FIRST_MS));
                Some(d)
            }
        };
        let from = self.point();
        let jump = |pad: &mut Pad, i: Option<usize>, ts: &[Target]| {
            if let Some(t) = i.and_then(|i| ts.get(i)) {
                pad.pos = (t.at.0 as f32, t.at.1 as f32);
                pad.parked = true;
            }
        };
        if let Some(d) = fire {
            let ts = targets();
            jump(self, snap(from, d, &ts), &ts);
        }
        if pressed & (LB | RB) != 0 {
            let ts = targets();
            jump(self, cycle(from, pressed & RB != 0, &ts), &ts);
        }
        if pressed & X != 0 {
            if from.1 < STRIP_TOP {
                self.back = Some(self.pos);
                self.pos = (self.pos.0, STRIP_Y as f32);
            } else {
                self.pos = self.back.take().unwrap_or(CENTRE);
            }
            self.parked = true;
        }
        if self.pos != start {
            self.driving = true;
        }
        let p = self.point();
        if self.driving && self.sent != Some(p) {
            self.sent = Some(p);
            out.push(PadOut::Move(p.0, p.1));
        }
        for (bit, b) in [(A, Button::Left), (B, Button::Right)] {
            if pressed & bit != 0 {
                if self.sent != Some(p) {
                    self.sent = Some(p);
                    out.push(PadOut::Move(p.0, p.1));
                }
                out.push(PadOut::Down(b));
            }
            if released & bit != 0 {
                out.push(PadOut::Up(b));
            }
        }
        for (bit, k) in [(Y, PadKey::Hotspots), (START, PadKey::Menu), (BACK, PadKey::Settings)] {
            if pressed & bit != 0 {
                out.push(PadOut::Key(k));
            }
        }
        out
    }

    /// The pad was switched off / unplugged: the buttons it holds go up.
    pub fn release_all(&mut self) -> Vec<PadOut> {
        let mut out = Vec::new();
        for (bit, b) in [(A, Button::Left), (B, Button::Right)] {
            if self.prev & bit != 0 {
                out.push(PadOut::Up(b));
            }
        }
        self.prev = 0;
        self.repeat = None;
        out
    }
}

/// The session side of a [`PadOut`] — what the mouse's own events do (front::host): a move, a button down / up at the
/// pointer. Keys and the hosts' own layers (the settings screen, the menu screen's settings button) stay the host's.
/// Returns a log line.
pub fn apply(s: &mut Session, o: PadOut, at: (i32, i32)) -> Option<String> {
    match o {
        PadOut::Move(x, y) => {
            super::host::motion(s, Some((x, y)));
            None
        }
        PadOut::Down(b) => super::host::button(s, b, Some(at)),
        PadOut::Up(b) => super::host::release(s, b, Some(at)),
        PadOut::Key(_) => None,
    }
}

/// The pointer the host draws when the pad drives it and no cursor of the game shows there (the menu screen, the
/// settings, the "Try again?" box): an arrow, its tip at (x, y); `hot` = over something clickable (yellow).
pub fn draw_pointer(fb: &mut [u8], x: i32, y: i32, hot: bool) {
    const ARROW: [&str; 17] = [
        "X",
        "XX",
        "X.X",
        "X..X",
        "X...X",
        "X....X",
        "X.....X",
        "X......X",
        "X.......X",
        "X........X",
        "X.....XXXXX",
        "X..X..X",
        "X.X X..X",
        "XX  X..X",
        "X    X..X",
        "     X..X",
        "      XX",
    ];
    let fill = if hot { [255, 220, 90] } else { [255, 255, 255] };
    for (dy, row) in ARROW.iter().enumerate() {
        for (dx, ch) in row.bytes().enumerate() {
            let c = match ch {
                b'X' => [0, 0, 0],
                b'.' => fill,
                _ => continue,
            };
            let (px, py) = (x + dx as i32, y + dy as i32);
            if (0..super::SCREEN_W as i32).contains(&px) && (0..super::SCREEN_H as i32).contains(&py) {
                let o = (py as usize * super::SCREEN_W + px as usize) * 3;
                fb[o..o + 3].copy_from_slice(&c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pad: &mut Pad, inp: PadIn, ms: u64, cfg: &PadCfg, ts: &[Target]) -> Vec<PadOut> {
        let mut all = Vec::new();
        for _ in 0..ms / 10 {
            all.extend(pad.update(&inp, 10, cfg, &mut || ts.to_vec()));
        }
        all
    }

    #[test]
    fn dead_zone_and_curve() {
        assert_eq!(deadzone(0.1, -0.1), (0.0, 0.0));
        let (x, y) = deadzone(1.0, 0.0);
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        let (x, _) = deadzone(0.54, 0.0);
        assert!((x - 0.5).abs() < 1e-3);
        // the outer dead zone: a stick that tops out short of 1.0 still reaches full deflection
        assert_eq!(deadzone(0.92, 0.0), (1.0, 0.0));
        let c = centred_target(deadzone(0.0, -0.93));
        assert_eq!(c, (320.0, 0.0));
        // the curve: half deflection a quarter of the speed; precision and the ramp
        assert!((free_speed(0.5, 0.0, 1.0, false) - BASE_SPEED / 4.0).abs() < 1e-3);
        assert!((free_speed(1.0, 0.0, 1.0, true) - BASE_SPEED * PRECISION).abs() < 1e-3);
        assert_eq!(free_speed(1.0, ACCEL_AFTER_MS, 1.0, false), BASE_SPEED);
        assert_eq!(free_speed(1.0, ACCEL_AFTER_MS + ACCEL_RAMP_MS, 1.0, false), BASE_SPEED * ACCEL_MAX);
        assert_eq!(free_speed(1.0, 1e9, 2.0, false), BASE_SPEED * ACCEL_MAX * 2.0);
    }

    #[test]
    fn free_pointer_moves_accelerates_and_stays_on_the_picture() {
        let cfg = PadCfg::default();
        let mut pad = Pad { pos: (100.0, 100.0), ..Default::default() };
        let right = PadIn { lx: 1.0, ..Default::default() };
        let out = run(&mut pad, right, 100, &cfg, &[]);
        // 100 ms at full deflection (no ramp yet): 42 px
        assert_eq!(pad.point(), (142, 100));
        assert_eq!(out.last(), Some(&PadOut::Move(142, 100)));
        assert!(pad.driving);
        // held: faster after the ramp
        let x0 = pad.pos.0;
        run(&mut pad, right, 300, &cfg, &[]);
        let slow = pad.pos.0 - x0;
        run(&mut pad, right, 600, &cfg, &[]);
        let x1 = pad.pos.0;
        run(&mut pad, right, 300, &cfg, &[]);
        assert!(pad.pos.0 - x1 > slow * 1.5 || pad.pos.0 == 639.0);
        run(&mut pad, right, 5000, &cfg, &[]);
        assert_eq!(pad.point(), (639, 100));
        // the stick at rest: no move, nothing sent
        assert!(run(&mut pad, PadIn::default(), 100, &cfg, &[]).is_empty());
        // the mouse moved: the pad goes on from there
        pad.mouse_moved((10, 20));
        assert!(!pad.driving);
        run(&mut pad, PadIn { ly: 1.0, lt: 1.0, ..Default::default() }, 100, &cfg, &[]);
        assert_eq!(pad.point(), (10, 20 + (BASE_SPEED * PRECISION / 10.0).round() as i32));
    }

    #[test]
    fn centred_pointer_maps_the_stick_and_returns() {
        let cfg = PadCfg { mode: Mode::Centred, speed: 1.0 };
        assert_eq!(centred_target((0.0, 0.0)), CENTRE);
        assert_eq!(centred_target((1.0, 0.0)), (639.0, 210.0));
        assert_eq!(centred_target((-1.0, 0.0)), (0.0, 210.0));
        assert_eq!(centred_target((0.0, -1.0)), (320.0, 0.0));
        assert_eq!(centred_target((0.0, 1.0)), (320.0, 479.0));
        assert_eq!(centred_target((0.0, 0.5)), (320.0, 344.5));
        // a full diagonal reaches the picture's corner
        let d = std::f32::consts::FRAC_1_SQRT_2;
        let c = centred_target((d, d));
        assert!((c.0 - 639.0).abs() < 1.0 && (c.1 - 479.0).abs() < 1.0);
        let mut pad = Pad::default();
        run(&mut pad, PadIn { lx: 1.0, ..Default::default() }, 200, &cfg, &[]);
        assert_eq!(pad.point(), (639, 210));
        // released: back at the centre quickly
        run(&mut pad, PadIn::default(), 100, &cfg, &[]);
        let p = pad.point();
        assert!((p.0 - 320).abs() <= 20, "{p:?}");
        run(&mut pad, PadIn::default(), 200, &cfg, &[]);
        assert_eq!(pad.point(), (320, 210));
        // a snap parks it there; the stick moved and let go: back to the centre
        let ts = [Target::of_rect([500, 100, 540, 140])];
        run(&mut pad, PadIn { buttons: RIGHT, ..Default::default() }, 10, &cfg, &ts);
        run(&mut pad, PadIn::default(), 500, &cfg, &ts);
        assert_eq!(pad.point(), (520, 120));
        run(&mut pad, PadIn { ly: 1.0, ..Default::default() }, 50, &cfg, &ts);
        run(&mut pad, PadIn::default(), 500, &cfg, &ts);
        assert_eq!(pad.point(), (320, 210));
    }

    #[test]
    fn snap_picks_the_target_ahead() {
        let t = |x, y| Target::of_rect([x - 10, y - 10, x + 10, y + 10]);
        let ts = [t(100, 100), t(300, 110), t(250, 300), t(200, 100), t(40, 400)];
        // from the first, right: the nearer in line (200, 100), not the farther one
        assert_eq!(snap((100, 100), Dir::Right, &ts), Some(3));
        assert_eq!(snap((200, 100), Dir::Right, &ts), Some(1));
        // nothing further right
        assert_eq!(snap((300, 110), Dir::Right, &ts), None);
        // down from (200, 100): (250, 300) is ahead; (40, 400) farther off the line
        assert_eq!(snap((200, 100), Dir::Down, &ts), Some(2));
        assert_eq!(snap((250, 300), Dir::Left, &ts), Some(4));
        // up from (250, 300): (300, 110) is a little nearer than (200, 100)
        assert_eq!(snap((250, 300), Dir::Up, &ts), Some(1));
        // the target under the pointer is not taken even if its centre is ahead
        assert_eq!(snap((95, 95), Dir::Right, &ts), Some(3));
        // a near target outside the cone loses to a farther one on the line; with nothing in the cone it is taken
        let ts2 = [t(150, 240), t(400, 100)];
        assert_eq!(snap((100, 100), Dir::Right, &ts2), Some(1));
        assert_eq!(snap((100, 100), Dir::Right, &ts2[..1]), Some(0));
        assert_eq!(snap((0, 0), Dir::Up, &ts2), None);
    }

    #[test]
    fn cycle_goes_in_reading_order() {
        let t = |x, y| Target::of_rect([x - 5, y - 5, x + 5, y + 5]);
        let ts = [t(300, 50), t(100, 50), t(50, 200)];
        assert_eq!(cycle((100, 50), true, &ts), Some(0));
        assert_eq!(cycle((300, 50), true, &ts), Some(2));
        assert_eq!(cycle((50, 200), true, &ts), Some(1));
        assert_eq!(cycle((100, 50), false, &ts), Some(2));
        // from a free point: the next after it
        assert_eq!(cycle((200, 50), true, &ts), Some(0));
        assert_eq!(cycle((200, 50), false, &ts), Some(1));
        assert_eq!(cycle((0, 0), false, &ts), Some(2));
        assert_eq!(cycle((0, 0), true, &[]), None);
    }

    #[test]
    fn buttons_snaps_and_repeats() {
        let cfg = PadCfg::default();
        let t = |x, y| Target::of_rect([x - 5, y - 5, x + 5, y + 5]);
        let ts: Vec<Target> = (0..6).map(|i| t(100 + 50 * i, 100)).collect();
        let mut pad = Pad { pos: (100.0, 100.0), ..Default::default() };
        // the D-pad held: one snap at once, the repeats after the first delay
        let out = run(&mut pad, PadIn { buttons: RIGHT, ..Default::default() }, 10, &cfg, &ts);
        assert_eq!(out, vec![PadOut::Move(150, 100)]);
        run(&mut pad, PadIn { buttons: RIGHT, ..Default::default() }, 360, &cfg, &ts);
        assert_eq!(pad.point(), (150, 100));
        run(&mut pad, PadIn { buttons: RIGHT, ..Default::default() }, 30, &cfg, &ts);
        assert_eq!(pad.point(), (200, 100));
        run(&mut pad, PadIn { buttons: RIGHT, ..Default::default() }, REPEAT_MS as u64, &cfg, &ts);
        assert_eq!(pad.point(), (250, 100));
        // the right stick snaps too
        run(&mut pad, PadIn::default(), 10, &cfg, &ts);
        run(&mut pad, PadIn { rx: -0.9, ry: 0.2, ..Default::default() }, 10, &cfg, &ts);
        assert_eq!(pad.point(), (200, 100));
        // A down / up, B, the keys
        let o = pad.update(&PadIn { buttons: A, ..Default::default() }, 10, &cfg, &mut || ts.clone());
        assert_eq!(o, vec![PadOut::Down(Button::Left)]);
        let o = pad.update(&PadIn { buttons: A, ..Default::default() }, 10, &cfg, &mut || ts.clone());
        assert!(o.is_empty());
        let o = pad.update(&PadIn { buttons: B | START, ..Default::default() }, 10, &cfg, &mut || ts.clone());
        assert_eq!(o, vec![PadOut::Up(Button::Left), PadOut::Down(Button::Right), PadOut::Key(PadKey::Menu)]);
        assert_eq!(pad.release_all(), vec![PadOut::Up(Button::Right)]);
        let o = pad.update(&PadIn { buttons: Y | BACK, ..Default::default() }, 10, &cfg, &mut || ts.clone());
        assert_eq!(o, vec![PadOut::Key(PadKey::Hotspots), PadOut::Key(PadKey::Settings)]);
        // LB / RB cycle; X to the strip and back
        run(&mut pad, PadIn::default(), 10, &cfg, &ts);
        run(&mut pad, PadIn { buttons: RB, ..Default::default() }, 10, &cfg, &ts);
        assert_eq!(pad.point(), (250, 100));
        run(&mut pad, PadIn { buttons: X, ..Default::default() }, 10, &cfg, &ts);
        assert_eq!(pad.point(), (250, STRIP_Y));
        run(&mut pad, PadIn::default(), 10, &cfg, &ts);
        run(&mut pad, PadIn { buttons: X, ..Default::default() }, 10, &cfg, &ts);
        assert_eq!(pad.point(), (250, 100));
        // a click after a mouse move goes where the mouse is
        pad.mouse_moved((7, 8));
        let o = pad.update(&PadIn { buttons: A, ..Default::default() }, 10, &cfg, &mut || ts.clone());
        assert_eq!(o, vec![PadOut::Down(Button::Left)]);
    }

    #[test]
    fn settings_give_the_config() {
        let mut set = super::super::settings::Settings::default();
        assert_eq!(PadCfg::from_settings(&set), Some(PadCfg { mode: Mode::Free, speed: 1.0 }));
        set.set("pad", "centred").unwrap();
        set.set("pad_speed", "150").unwrap();
        assert_eq!(PadCfg::from_settings(&set), Some(PadCfg { mode: Mode::Centred, speed: 1.5 }));
        // no "off" (a pad could switch itself off and only the mouse could switch it back)
        assert!(set.set("pad", "off").is_err());
    }
}
