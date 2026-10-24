//! Movie hotspots and what shooting them does (H1280–H1283, H1322, H1520, H1703).
//!
//! A video's `.HOT` file (H1280, `media::hot`) lists hotspots: id, rect in movie pixels, active window t0..t1 of movie
//! time. While its window is open a hotspot is a click target (H1281), placed at the movie rect + (20, 90) and resized
//! by the action-scene difficulty. A click on it is a shot: depending on the id and on what the player holds, one or
//! two reactions run ([`reactions`]). Each reaction is a [`Hit`] of some scene: the current View goes, a cViewState
//! value is written, a "kill movie" plays, and the scene continues (re-entered, another View, nothing).
//!
//! The front supplies the hotspots of the movies it plays ([`Runner::movie_hotspots`], times relative to the asset's
//! start); the runner times them on its clock from the player's start (state 1).
use super::run::{Clickable, Cmd, Runner};
use super::sched::media_inputs_h51;
use super::{class, event, Out, TransSrc, World};
use crate::media::hot::Hotspot;

/// Owner of a movie-hotspot target (no HDB object): this tag | the hotspot id (ids are < 0x10000)
pub const MOVIE_HOT_TAG: u32 = 0xfd1d_0000;
const TAG_MASK: u32 = 0xffff_0000;

/// H450: the held object's type of the gun
const GUN_TYPE: u8 = 7;
/// H1543: the current inventory value of the stiletto
const STILETTO: i32 = 0x17;
/// The ids traced first (H1322: the Smolnikoff goons, the stairs gunman, the barricade) — the only ones that react
/// with `Rules::h1520_movie_hot_all_cases` off
const FIRST_TRACED_IDS: [u32; 5] = [201, 211, 221, 222, 223];

/// How a scene goes on after a hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    Stay,
    /// The removed View is entered again
    ReEnter,
    /// An engine Set View (H192)
    SetView(u32),
    /// An AssetRef posted as a player
    Play(u32),
}

/// The two gunmen of a two-man scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Man {
    One,
    Two,
}

impl Man {
    /// The position in the View's asset list of the movie with the other man still standing, and its cViewState
    fn survivor(self) -> (usize, u8) {
        match self {
            Man::One => (2, b't'),
            Man::Two => (1, b'd'),
        }
    }
}

/// A hit in a two-man scene: which man, and what happens once both are down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwoMen {
    pub man: Man,
    pub done_view_state: Option<u8>,
    pub done: Then,
}

/// A random layout set read from `XFiles.exe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layouts {
    MelShot,
    SuicideNote,
}

/// One hit reaction of a scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub scene: &'static str,
    /// Trace entries the reaction holds on ("" = static reading only)
    pub evidence: &'static str,
    pub two_men: Option<TwoMen>,
    pub view_state: Option<u8>,
    pub stop_players: bool,
    pub freeze: bool,
    pub layouts: Option<Layouts>,
    /// Some app objects are removed (not modelled, logged)
    pub clears_objects: bool,
    pub kill: Option<u32>,
    pub then: Then,
}

const HIT: Hit = Hit {
    scene: "",
    evidence: "",
    two_men: None,
    view_state: None,
    stop_players: false,
    freeze: false,
    layouts: None,
    clears_objects: false,
    kill: None,
    then: Then::Stay,
};

const fn nsa(man: Man, kill: u32, evidence: &'static str) -> Hit {
    let done = TwoMen { man, done_view_state: Some(b'a'), done: Then::SetView(0x1fca) };
    Hit { scene: "NSA porch", evidence, two_men: Some(done), kill: Some(kill), then: Then::ReEnter, ..HIT }
}

const fn pair(man: Man, kill: u32) -> Hit {
    let done = TwoMen { man, done_view_state: None, done: Then::Play(0x9630) };
    Hit { scene: "two-man scene", two_men: Some(done), kill: Some(kill), then: Then::ReEnter, ..HIT }
}

const fn goon(kill: u32, evidence: &'static str) -> Hit {
    Hit { scene: "Smolnikoff goon", evidence, kill: Some(kill), ..HIT }
}

/// A guard-like hit: cViewState 'r', the kill movie, then `then`
const fn guard(scene: &'static str, kill: u32, then: Then, evidence: &'static str) -> Hit {
    Hit { scene, evidence, view_state: Some(b'r'), kill: Some(kill), then, ..HIT }
}

/// Kill movie only
const fn kill(scene: &'static str, kill: u32, evidence: &'static str) -> Hit {
    Hit { scene, evidence, kill: Some(kill), ..HIT }
}

const SUICIDE: Hit = Hit {
    scene: "shot at the wrong moment (suicide note)",
    evidence: "C251",
    view_state: Some(b'S'),
    freeze: true,
    layouts: Some(Layouts::SuicideNote),
    ..HIT
};
const NSA_ONE: Hit = nsa(Man::One, 0x54a0, "C210 / C211");
const NSA_TWO: Hit = nsa(Man::Two, 0x549e, "C211");
const PAIR_ONE: Hit = pair(Man::One, 0x54d6);
const PAIR_TWO: Hit = pair(Man::Two, 0x54d4);
const STAIRS_GUNMAN: Hit = guard("Smolnikoff stairs gunman", 0x6f28, Then::ReEnter, "C170");
const BARRICADE: Hit = guard("Smolnikoff barricade", 0x52f6, Then::SetView(0x157d), "C171");
const GOON_1: Hit = goon(0x52ee, "C167 / C168");
const GOON_2: Hit = goon(0x52f0, "C166 / C167 / C168");
const GOON_3: Hit = goon(0x52f2, "C167 / C168");
const GUARD_1: Hit = guard("Secret Base guard 1", 0x5534, Then::ReEnter, "C214");
const GUARD_2: Hit = guard("Secret Base guard 2", 0x553a, Then::ReEnter, "C222");
const GUARD_3: Hit = guard("Secret Base guard 3", 0x5530, Then::ReEnter, "");
const BASE_431: Hit = guard("Secret Base 431", 0xadc3, Then::SetView(0xacfb), "C219");
const BASE_432: Hit = guard("Secret Base 432", 0xbf76, Then::SetView(0x38b7), "");
const INCOMPC_STILETTO: Hit =
    Hit { scene: "INCOMPC", view_state: Some(b'z'), stop_players: true, kill: Some(0xbf75), ..HIT };
const INCOMPC_GUN: Hit =
    Hit { scene: "INCOMPC", evidence: "C254", view_state: Some(b'z'), stop_players: true, kill: Some(0x138da), ..HIT };
const SCULLY: Hit = Hit { view_state: Some(b'w'), ..guard("keyguard, Scully", 0xbf7e, Then::SetView(0xc0de), "C253") };
const COOK_AUTOPSY_441: Hit = Hit { view_state: Some(b'z'), ..kill("autopsy room, Cook", 0x5516, "C252") };
const COOK_AUTOPSY_442: Hit = guard("autopsy room, Cook", 0x5518, Then::Stay, "C224");
const TRAP_MULDER: Hit = Hit { scene: "'trap Mulder', red button", evidence: "C308", view_state: Some(b't'), ..HIT };
const SCENE_461: Hit = kill("461", 0x138da, "");
const COOK_BLAST_STILETTO: Hit = kill("blast door, Cook", 0x5b8a, "C220");
const COOK_BLAST_GUN: Hit = Hit { view_state: Some(b'd'), ..guard("blast door, Cook", 0xbf7e, Then::SetView(0xc0de), "") };
const COOK_BLAST_463: Hit = kill("blast door, Cook", 0x5b86, "C256");
const SCENE_470: Hit = Hit { scene: "470", view_state: Some(b'b'), clears_objects: true, ..HIT };
const ARREST: Hit = Hit { scene: "shot at anything (arrest)", freeze: true, kill: Some(0x4cc8), ..HIT };
const MEL: Hit = Hit {
    scene: "Mel shot",
    view_state: Some(b'm'),
    stop_players: true,
    layouts: Some(Layouts::MelShot),
    then: Then::ReEnter,
    ..HIT
};

/// What a hotspot needs and does.
enum Response {
    /// With the gun; `shot` = the gun fires first
    Gun { shot: bool, hit: &'static Hit },
    /// Whatever is held
    Anything(&'static Hit),
    /// With the stiletto `stiletto` only, otherwise [`Response::Gun`]
    StilettoElseGun { stiletto: &'static Hit, shot: bool, gun: &'static Hit },
    /// With the gun: shot + the hit; then with the stiletto the hit once more
    GunThenStiletto(&'static Hit),
}

const fn gun(hit: &'static Hit) -> Response {
    Response::Gun { shot: false, hit }
}
const fn gun_shot(hit: &'static Hit) -> Response {
    Response::Gun { shot: true, hit }
}

/// The hotspot ids of the game's `.HOT` files that react, in id order.
static HOTSPOTS: &[(&[u32], Response)] = &[
    (&[100, 500], gun_shot(&SUICIDE)),
    (&[101, 103], gun(&NSA_ONE)),
    (&[102, 104], gun(&NSA_TWO)),
    (&[111, 113, 121, 131, 141], gun(&PAIR_ONE)),
    (&[112, 114, 122, 132, 142], gun(&PAIR_TWO)),
    (&[201], gun(&STAIRS_GUNMAN)),
    (&[211], gun(&BARRICADE)),
    (&[221], gun(&GOON_1)),
    (&[222], gun(&GOON_2)),
    (&[223], gun(&GOON_3)),
    (&[401], gun(&GUARD_1)),
    (&[411], gun(&GUARD_2)),
    (&[421], gun(&GUARD_3)),
    (&[431], gun(&BASE_431)),
    (&[432], gun(&BASE_432)),
    (&[433], Response::StilettoElseGun { stiletto: &INCOMPC_STILETTO, shot: true, gun: &INCOMPC_GUN }),
    (&[434], gun(&SCULLY)),
    (&[441], Response::Anything(&COOK_AUTOPSY_441)),
    (&[442], gun(&COOK_AUTOPSY_442)),
    (&[451], Response::Anything(&TRAP_MULDER)),
    (&[461], Response::GunThenStiletto(&SCENE_461)),
    (&[462], Response::StilettoElseGun { stiletto: &COOK_BLAST_STILETTO, shot: false, gun: &COOK_BLAST_GUN }),
    (&[463], Response::Anything(&COOK_BLAST_463)),
    (&[470], Response::GunThenStiletto(&SCENE_470)),
    (&[600], gun_shot(&ARREST)),
    (&[666], gun_shot(&MEL)),
];

/// One reaction to a shot: the gun fires first (`shot`, not waited for, H821), then the hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reaction {
    pub shot: bool,
    pub hit: &'static Hit,
}

/// The reactions, in order, to a click on hotspot `id` with the gun held (`gun`) and / or the stiletto as the current
/// inventory value (`stiletto`). Empty = nothing happens.
pub fn reactions(id: u32, gun: bool, stiletto: bool) -> Vec<Reaction> {
    let Some((_, resp)) = HOTSPOTS.iter().find(|(ids, _)| ids.contains(&id)) else { return Vec::new() };
    let with_gun = |shot: bool, hit| if gun { vec![Reaction { shot, hit }] } else { Vec::new() };
    match *resp {
        Response::Gun { shot, hit } => with_gun(shot, hit),
        Response::Anything(hit) => vec![Reaction { shot: false, hit }],
        Response::StilettoElseGun { stiletto: hit, .. } if stiletto => vec![Reaction { shot: false, hit }],
        Response::StilettoElseGun { shot, gun: hit, .. } => with_gun(shot, hit),
        Response::GunThenStiletto(hit) => {
            let mut v = with_gun(true, hit);
            if stiletto {
                v.push(Reaction { shot: false, hit });
            }
            v
        }
    }
}

/// Is `owner` a movie-hotspot target?
pub fn is_movie_hot(owner: u32) -> bool {
    owner & TAG_MASK == MOVIE_HOT_TAG
}

/// What a movie-hotspot target does to the cursor when the pointer is over it (H2350).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverCursor {
    /// This VCCursor
    Cursor(u32),
    /// The gunsight flag set, the null cursor (= the Win32 gunsight, C301)
    Gunsight,
    /// Nothing set: the cursor stays as it was
    Keep,
}

/// HYPOTHESIS H2350 (static decode of the target's cursor pick, vt 0x6549c0 +0x68; fo-264 / fo-265 / fo-220
/// hold) — the cursor over the target of hotspot `id` for the current inventory value `held` (None: nothing held): a
/// switch on id − 433: 441 / 451 / 463 → the fist 0x4454 whatever is held; 433 / 461 / 470 → with the stiletto the
/// stiletto item's cursor 0x7cbb; 462 → with the stiletto the animated stiletto 0xe2ef; for these when the stiletto is
/// not held, and for every other id: with the gun (value 9) the gunsight, else nothing set.
pub fn hover_cursor(id: u32, held: Option<i32>) -> HoverCursor {
    const FIST: u32 = 0x4454;
    const STILETTO_ITEM: u32 = 0x7cbb;
    const STILETTO_ANIM: u32 = 0xe2ef;
    const GUN: i32 = 9;
    match (id, held) {
        (441 | 451 | 463, _) => HoverCursor::Cursor(FIST),
        (433 | 461 | 470, Some(STILETTO)) => HoverCursor::Cursor(STILETTO_ITEM),
        (462, Some(STILETTO)) => HoverCursor::Cursor(STILETTO_ANIM),
        (_, Some(GUN)) => HoverCursor::Gunsight,
        _ => HoverCursor::Keep,
    }
}

/// H1281: the target's rect in movie pixels for the difficulty char — 'E' grown by trunc(15 %) of the width
/// / height on each side and clipped to the 600 × 240 movie; 'H' shrunk by trunc(12.5 %), not for a dimension of
/// ≤ 10 px; else as in the file. An empty result is (0, 0, 0, 0).
pub fn target_rect(r: [i16; 4], difficulty: u8) -> [i16; 4] {
    let [l, t, rt, b] = r.map(|v| v as i32);
    let (w, h) = (rt - l, b - t);
    let shrink = |side: i32| if side > 10 { -((side as f64 * 0.125) as i32) } else { 0 };
    let (dx, dy, clip) = match difficulty {
        b'E' => ((w as f64 * 0.15) as i32, (h as f64 * 0.15) as i32, true),
        b'H' => (shrink(w), shrink(h), false),
        _ => (0, 0, false),
    };
    let (mut l, mut t, mut rt, mut b) = (l - dx, t - dy, rt + dx, b + dy);
    if rt - l < 1 || b - t < 1 {
        return [0; 4];
    }
    if clip {
        (l, t, rt, b) = (l.max(0), t.max(0), rt.min(600), b.min(240));
        if rt <= l || b <= t {
            return [0; 4];
        }
    }
    [l as i16, t as i16, rt as i16, b as i16]
}

impl World<'_> {
    /// An AssetRef posted as a player (as the gun's video, H450)
    fn post_player(&mut self, asset: u32) {
        if self.h.has(class::ASSET_REF, asset) {
            self.cv_assets.push_back((asset, 0));
            self.out.push(Out::PlayerPost { asset });
        }
    }

    /// H1322: the current View goes — its Deactivation and its movie's at once (the movie
    /// hotspot's target goes with the movie), its hotspots and explorables as queued commands (fo-185 78.414: after
    /// the View's posts). Returns the removed View.
    fn remove_view_at_hit(&mut self) -> Option<u32> {
        let v = self.view.take();
        if let Some(v) = v {
            self.out.push(Out::Fire { owner: v, event: event::DEACTIVATION });
        }
        if let Some(bg) = self.background.take() {
            self.out.push(Out::Fire { owner: bg, event: event::DEACTIVATION });
        }
        for n in std::mem::take(&mut self.hotspots) {
            self.fire_cmd(n, event::DEACTIVATION);
        }
        for x in std::mem::take(&mut self.explorables) {
            self.fire_cmd(x, event::DEACTIVATION);
        }
        self.exit_char_views();
        v
    }

    /// RegViewState's variable := `c` (type 0 char)
    fn write_view_state(&mut self, c: u8) {
        if let Some(var) = self.view_state_var {
            self.vars.set_native(var, Some(c as i32), 0);
        }
    }

    /// H1520: `man` was hit in the two-man scene of `view`. Its asset list's flags are set directly
    /// (no Enable action, no background re-choice: the View goes next). While the movie with both men (the first
    /// entry) is on, only the survivor's movie stays enabled; otherwise none does and the scene is over (true). Every
    /// two-man View lists the three movies.
    fn two_men_hit(&mut self, view: u32, man: Man) -> bool {
        let list = self.h.read(class::VIEW, view).map_or(0, |r| r.u32("f2c"));
        let items = self.h.list_of(class::ASSET_REF_LIST, list);
        let both_standing = items.first().is_some_and(|&a| self.is_enabled(class::ASSET_REF, a));
        let survivor = if both_standing {
            let (k, c) = man.survivor();
            self.write_view_state(c);
            Some(k)
        } else {
            None
        };
        for (k, &a) in items.iter().enumerate() {
            self.enabled.insert((class::ASSET_REF, a), (Some(k) == survivor) as u8);
        }
        survivor.is_none()
    }

    /// H1322 / H1520: the steps of a hit on hotspot `hotspot`. The parts not modelled are logged.
    fn gun_hit(&mut self, hit: &Hit, hotspot: u32, log: &mut Vec<String>) {
        if !self.rules.h1322_movie_hot_view_removal {
            if let Some(k) = hit.kill {
                self.post_player(k);
            }
            return;
        }
        let mut then = hit.then;
        if let (Some(tm), Some(view)) = (hit.two_men, self.view) {
            if self.two_men_hit(view, tm.man) {
                if let Some(c) = tm.done_view_state {
                    self.write_view_state(c);
                }
                then = tm.done;
            }
        }
        let removed = self.remove_view_at_hit();
        if let Some(c) = hit.view_state {
            self.write_view_state(c);
        }
        if hit.stop_players {
            self.out.push(Out::TravelStopPlayers);
        }
        if hit.freeze {
            if self.rules.h1703_suicide_freeze {
                self.out.push(Out::Freeze { hotspot });
                log.push(format!("H1703: hotspot {hotspot}: the game frozen (the command drop / screen fade not modelled)"));
            } else {
                log.push(format!("H1520: hotspot {hotspot}: the freeze (commands dropped, screen fade, objects frozen) not modelled"));
            }
        }
        if let Some(set) = hit.layouts {
            let set = match set {
                Layouts::MelShot => &self.h.exe.mel_shot_layouts,
                Layouts::SuicideNote => &self.h.exe.suicide_note_layouts,
            };
            if let Some(last) = set.len().checked_sub(1) {
                let k = self.live_rand_value(0, last as i32).unwrap_or(0).clamp(0, last as i32) as usize;
                let layout = set[k];
                self.engine_layout(layout, 0); // an engine type-5 show (H70)
            }
        }
        if hit.clears_objects {
            log.push(format!("H1520: hotspot {hotspot}: the removal of some app objects not modelled"));
        }
        if let Some(k) = hit.kill {
            self.post_player(k);
        }
        match then {
            Then::Stay => {}
            Then::ReEnter => {
                if let Some(v) = removed {
                    let vp = self.viewpoint_of(v);
                    self.transition(v, 0, TransSrc::SetView { action: 0 }, vp, &[]);
                }
            }
            Then::SetView(v) => self.engine_set_view(v),
            Then::Play(a) => self.post_player(a),
        }
    }
}

impl Runner<'_> {
    /// H1281: the active movie-hotspot targets, list order (the View background and each started video from its
    /// start, its hotspots by t0 — targets are listed in the order they appeared); screen rects (movie rect + (20,
    /// 90)). The asset is in `kind`.
    pub(super) fn movie_hot_clickables(&self) -> Vec<Clickable> {
        if !self.w.rules.h1281_movie_hotspots || self.movie_hotspots.is_empty() {
            return Vec::new();
        }
        let d = self.w.action_difficulty();
        let mut v: Vec<(u64, Clickable)> = Vec::new();
        // the View's background movie (H53: a goon View's movie is its f2c asset) and the started players
        for a in self.background.into_iter().chain(self.started()) {
            let (Some(hs), Some(&t0)) = (self.movie_hotspots.get(&a), self.movie_start.get(&a)) else { continue };
            if !media_inputs_h51(self.w.h, a).is_some_and(|(video, _)| video) {
                continue;
            }
            let t = self.now.saturating_sub(t0);
            for h in hs.iter().filter(|h| (h.t0 as u64) <= t && t < h.t1 as u64) {
                let [l, tp, r, b] = target_rect(h.rect, d);
                if r <= l || b <= tp {
                    continue;
                }
                let rect = [l + 20, tp + 90, r + 20, b + 90];
                v.push((t0 + h.t0 as u64, Clickable { owner: MOVIE_HOT_TAG | (h.id & 0xffff), cls: 0, rect, kind: a }));
            }
        }
        v.sort_by_key(|x| x.0); // stable
        v.into_iter().map(|x| x.1).collect()
    }

    /// H1282 / H1520: a shot at hotspot `id` of `movie` — its [`reactions`] (the gun = held type 7; rule H1282 off =
    /// as if the gun were held), each with the gun's shot first if it has one (H821: not waited for).
    pub(super) fn movie_hot_click(&mut self, id: u32, movie: u32) {
        let gun = self.w.dev.held == Some(GUN_TYPE) || !self.w.rules.h1282_movie_hot_gun;
        if !self.w.rules.h1520_movie_hot_all_cases && !FIRST_TRACED_IDS.contains(&id) {
            self.log.push(format!("H1282: movie hotspot {id} of {movie:#x} clicked — not modelled (H1520 off)"));
            return;
        }
        let stiletto = self.w.inventory_value() == STILETTO;
        let rs = reactions(id, gun, stiletto);
        if rs.is_empty() {
            self.log.push(format!("H1282: movie hotspot {id} of {movie:#x} clicked — nothing (no reaction; without the gun: C165)"));
            return;
        }
        for r in rs {
            let evidence = if r.hit.evidence.is_empty() { "untraced" } else { r.hit.evidence };
            self.log.push(format!(
                "H1520: hotspot {id} hit → {} ({evidence}){} (gunsight cursor not modelled)",
                r.hit.scene,
                if r.shot { " after the gun's shot" } else { "" }
            ));
            if r.shot {
                self.gun_shot_start(None);
            }
            let mut log = Vec::new();
            self.w.gun_hit(r.hit, id, &mut log);
            self.log.extend(log);
        }
    }

    /// H1281: the click of a target owner (from [`Runner::engine_click`]): queued like a mouse command's handler.
    pub(super) fn movie_hot_engine_click(&mut self, owner: u32) -> bool {
        if !is_movie_hot(owner) {
            return false;
        }
        // the playing movie that holds this id now (the topmost target's asset)
        let movie = self.movie_hot_clickables().into_iter().find(|c| c.owner == owner).map_or(0, |c| c.kind);
        self.push_back(Cmd::MovieHot { id: owner & 0xffff, movie });
        true
    }
}

/// The front's input for one asset: its movie file's hotspots with the times made relative to the asset's start in the
/// file (`start` ms; a window ending before it is dropped).
pub fn relative_to_start(hs: &[Hotspot], start: u32) -> Vec<Hotspot> {
    hs.iter()
        .filter(|h| h.t1 > start)
        .map(|h| Hotspot { t0: h.t0.saturating_sub(start), t1: h.t1 - start, ..*h })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H2350: the jump table of the target's cursor pick (indexed by id − 433)
    #[test]
    fn h2350_hover_cursor_table() {
        use HoverCursor::*;
        for id in [441, 451, 463] {
            for held in [None, Some(9), Some(0x17), Some(3)] {
                assert_eq!(hover_cursor(id, held), Cursor(0x4454));
            }
        }
        for id in [433, 461, 470] {
            assert_eq!(hover_cursor(id, Some(0x17)), Cursor(0x7cbb));
        }
        assert_eq!(hover_cursor(462, Some(0x17)), Cursor(0xe2ef));
        for id in [221, 433, 450, 462, 471] {
            assert_eq!(hover_cursor(id, Some(9)), Gunsight);
            assert_eq!(hover_cursor(id, None), Keep);
            assert_eq!(hover_cursor(id, Some(3)), Keep);
        }
    }

    #[test]
    fn rects_by_difficulty() {
        let r = [406, 105, 447, 152]; // XV/47749.HOT: 41 × 47
        assert_eq!(target_rect(r, b'M'), r);
        assert_eq!(target_rect(r, b'E'), [400, 98, 453, 159]); // 6 / 7 more per side
        assert_eq!(target_rect(r, b'H'), [411, 110, 442, 147]); // 5 / 5 less
        assert_eq!(target_rect([2, 1, 12, 9], b'H'), [2, 1, 12, 9]); // sides ≤ 10 px stay
        assert_eq!(target_rect([590, 230, 600, 240], b'E'), [589, 229, 600, 240]); // clipped to the movie
        assert!(is_movie_hot(MOVIE_HOT_TAG | 223) && !is_movie_hot(0xffff_ff18));
        let hs = [Hotspot { id: 1, rect: r, t0: 500, t1: 900 }, Hotspot { id: 2, rect: r, t0: 0, t1: 100 }];
        assert_eq!(relative_to_start(&hs, 200), vec![Hotspot { id: 1, rect: r, t0: 300, t1: 700 }]);
    }

    #[test]
    fn every_hotspot_id_is_listed_once() {
        let mut ids: Vec<u32> = HOTSPOTS.iter().flat_map(|(ids, _)| ids.iter().copied()).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }
}
