//! HYPOTHESIS H730 (static reading, the rounds hold on C166–C168): the Smolnikoff shootout's native procedures (Node 6,
//! "The Russians"). Three goons; each round DoSmolShootout picks a standing goon and sets the
//! View of its next appearance (the pick Views are read from `XFiles.exe`), SmolGoonShot1 marks it shot and mirrors the
//! shot flags into three game variables, SmIsWillShot1 / SmGoonShoots decide whether a goon's shot hits Willmore by a
//! chance that depends on the action-scene difficulty and the goon's appearance.
//!
//! Draws use the H550 generator (`World::live_rand_value`). Outside live play a draw is unknown: the state then becomes
//! unknown until the next InitSmShootout and the procedures fall back to H192's generic handling (logged). The e2e
//! harness instead feeds the original's picks and outcomes from its trace (H1320).
use super::{class, event, Msg, Out, PostSrc, RegCause, World};
use std::collections::VecDeque;

/// The View when every goon is down (first floor clear, C167)
const ALL_DOWN_VIEW: u32 = 0x1458;
/// The AssetRef played when a goon hits Willmore
const WILL_SHOT_ASSET: u32 = 0x52b0;
/// Pref_cActionSceneDifficulty (RegPreferences' 13th variable)
const VAR_ACTION_DIFFICULTY: u32 = 0x3df;

/// The action-scene difficulty preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    fn from_pref(c: u8) -> Difficulty {
        match c {
            b'E' => Difficulty::Easy,
            b'H' => Difficulty::Hard,
            _ => Difficulty::Normal,
        }
    }
}

/// A goon's appearance on screen: its first, or its second or later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Appearance {
    First,
    Later,
}

/// Percent chance that a goon's shot hits Willmore: rows = appearance, columns = easy / normal / hard.
const HIT_PERCENT: [[i32; 3]; 2] = [[20, 40, 60], [50, 65, 80]];

fn hit_percent(appearance: Appearance, difficulty: Difficulty) -> i32 {
    let row = match appearance {
        Appearance::First => 0,
        Appearance::Later => 1,
    };
    let col = match difficulty {
        Difficulty::Easy => 0,
        Difficulty::Normal => 1,
        Difficulty::Hard => 2,
    };
    HIT_PERCENT[row][col]
}

/// One goon's state.
#[derive(Clone, Copy, Debug, Default)]
struct Goon {
    /// The game variable that mirrors `shot` (0 = not bound)
    var: u32,
    /// How often the goon was picked (0..=2; later picks stay at 2)
    picks: u32,
    shot: bool,
}

impl Goon {
    /// The appearance after its latest pick (None before the first)
    fn appearance(&self) -> Option<Appearance> {
        match self.picks {
            0 => None,
            1 => Some(Appearance::First),
            _ => Some(Appearance::Later),
        }
    }
}

/// H730: the shootout's state.
#[derive(Clone, Debug, Default)]
pub struct Smol {
    goons: [Goon; 3],
    /// The goon on screen (index), if any
    target: Option<usize>,
    /// A draw was unknown since the last InitSmShootout (replay / trace: no live generator)
    unknown: bool,
}

impl Smol {
    /// The first standing goon from `drawn` on, in the cyclic order 1 → 2 → 3 → 1 (None when all are down)
    fn standing_from(&self, drawn: usize) -> Option<usize> {
        (0..3).map(|k| (drawn + k) % 3).find(|&i| !self.goons[i].shot)
    }

    /// Goon `i` appears: the View of its next appearance from the pick table, its pick count moved on
    fn appear(&mut self, i: usize, table: &[[u32; 3]]) -> u32 {
        let g = &mut self.goons[i];
        let view = table.get(i).map_or(0, |row| row[g.picks as usize]);
        g.picks = (g.picks + 1).min(2);
        if view == 0 {
            ALL_DOWN_VIEW
        } else {
            view
        }
    }
}

impl World<'_> {
    /// Pref_cActionSceneDifficulty: 'E' / 'H' / other
    pub(super) fn action_difficulty(&self) -> u8 {
        self.vars.get(VAR_ACTION_DIFFICULTY).unwrap_or(0) as u8
    }

    fn smol_difficulty(&self) -> Difficulty {
        Difficulty::from_pref(self.action_difficulty())
    }

    /// InitSmShootout's native part (the action's own writes zero the variables, H91): binds the variables that exist
    pub(super) fn smol_init(&mut self, args: &[u32]) {
        let mut s = Smol::default();
        for (g, &v) in s.goons.iter_mut().zip(args) {
            if self.vars.has(v) {
                g.var = v;
            }
        }
        self.px.smol = s;
    }

    fn smol_timer_stop(&mut self, n: u32) {
        self.engine_timer_stops.push_back(n as u8);
        self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
    }

    /// H730 procedures; true = handled, false = left to the generic handling.
    pub(super) fn smol_proc(&mut self, action: u32, name: &str, args: &[u32]) -> bool {
        match (name, args) {
            ("DoSmolShootout", _) => self.smol_next_round(),
            ("SmolGoonShot1", _) => self.smol_goon_shot(),
            ("SmIsWillShot1", &[timer, ..]) => self.smol_target_shoots(action, timer),
            ("SmGoonShoots", &[result, appearance, timer, ..]) => self.smol_goon_shoots(result, appearance, timer),
            _ => false,
        }
    }

    /// DoSmolShootout
    fn smol_next_round(&mut self) -> bool {
        if self.px.smol.unknown || self.h.exe.smol_pick_views.is_empty() {
            return false;
        }
        if self.live_rand.is_none() {
            if let Some(view) = self.smol_trace_picks.pop_front() {
                self.smol_pick_from_trace(view);
                return true;
            }
        }
        let target = if self.px.smol.goons.iter().all(|g| g.shot) {
            None
        } else {
            let Some(drawn) = self.live_rand_value(1, 3) else {
                self.px.smol.unknown = true;
                return false;
            };
            self.px.smol.standing_from(drawn as usize - 1)
        };
        self.px.smol.target = target;
        let view = match target {
            Some(i) => self.px.smol.appear(i, &self.h.exe.smol_pick_views),
            None => ALL_DOWN_VIEW,
        };
        self.engine_set_view(view);
        true
    }

    /// SmolGoonShot1
    fn smol_goon_shot(&mut self) -> bool {
        if self.px.smol.unknown {
            return false;
        }
        if let Some(i) = self.px.smol.target {
            self.px.smol.goons[i].shot = true;
        }
        for g in self.px.smol.goons {
            if g.var != 0 {
                self.vars.set_native(g.var, Some(g.shot as i32), 2);
            }
        }
        true
    }

    /// SmIsWillShot1(timer): the goon on screen shoots; a hit stops `timer` and plays Willmore's death
    fn smol_target_shoots(&mut self, action: u32, timer: u32) -> bool {
        if self.px.smol.unknown {
            return false;
        }
        let Some(i) = self.px.smol.target else { return true };
        let hit = match self.px.smol.goons[i].appearance() {
            None => false,
            Some(a) => {
                let chance = hit_percent(a, self.smol_difficulty());
                match self.live_rand_value(0, 100) {
                    Some(r) => r < chance,
                    // H1320: the original's outcome (e2e input)
                    None => match self.smol_trace_will.pop_front() {
                        Some(h) => h,
                        None => return false,
                    },
                }
            }
        };
        if hit {
            self.smol_timer_stop(timer);
            self.smol_play(action, WILL_SHOT_ASSET);
        }
        true
    }

    /// SmGoonShoots(result, appearance, timer): variable `appearance` holds 2 (first) or 3 (later);
    /// `result` := hit, and a miss stops `timer`
    fn smol_goon_shoots(&mut self, result: u32, appearance: u32, timer: u32) -> bool {
        let Some(value) = self.vars.get(appearance) else { return false };
        let appearance = match value {
            2 => Some(Appearance::First),
            3 => Some(Appearance::Later),
            _ => None,
        };
        let hit = match appearance {
            None => false,
            Some(a) => {
                let chance = hit_percent(a, self.smol_difficulty());
                match self.live_rand_value(0, 100) {
                    Some(r) => r < chance,
                    None => return false,
                }
            }
        };
        if !hit {
            self.smol_timer_stop(timer);
        }
        if self.vars.get(result) != Some(hit as i32) {
            self.vars.set_native(result, Some(hit as i32), 2);
        }
        true
    }

    /// H1320: DoSmolShootout with the original's pick `view` (trace input): the goon whose row holds that View becomes
    /// the target, its pick count just past the entry; 0x1458 = none left.
    fn smol_pick_from_trace(&mut self, view: u32) {
        let s = &mut self.px.smol;
        s.target = None;
        for (i, row) in self.h.exe.smol_pick_views.iter().enumerate() {
            if let Some(j) = row.iter().position(|&v| v == view) {
                s.target = Some(i);
                s.goons[i].picks = (j as u32 + 1).min(2);
            }
        }
        self.engine_set_view(view);
    }

    /// An AssetRef played as H194's play (REG + Activation)
    fn smol_play(&mut self, action: u32, asset: u32) {
        if self.h.has(class::ASSET_REF, asset) && !self.looping_players.contains(&asset) {
            let tl = self.trigger_list_of(asset);
            self.played.insert(asset);
            self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::PlayAsset { action } });
            self.out.push(Out::Fire { owner: asset, event: event::ACTIVATION });
        }
    }
}

/// H1320 (e2e input): from a trace of the original, the View each DoSmolShootout execution set (the next transition
/// after it) and SmIsWillShot1's outcomes (hit = REG of 0x52b0 within 0.2 s of it), in order.
pub fn picks_from_trace(trace: &[crate::replay::TraceEv], h: &crate::hdb::Hdb) -> (VecDeque<u32>, VecDeque<bool>) {
    use crate::replay::Ev;
    let name = |a: u32| -> Option<String> {
        let x = super::action::Action::load(h, a).filter(|x| x.kind() == 6)?;
        super::action::ProcCall::parse(x.body()).map(|p| p.name)
    };
    let (mut picks, mut will) = (VecDeque::new(), VecDeque::new());
    let (mut pending, mut will_t) = (false, f64::NEG_INFINITY);
    for e in trace {
        match e.ev {
            Ev::Exec { action, .. } => match name(action).as_deref() {
                Some("DoSmolShootout") => pending = true,
                Some("SmIsWillShot1") => {
                    will.push_back(false);
                    will_t = e.t;
                }
                _ => {}
            },
            Ev::Transition { view, .. } if pending => {
                picks.push_back(view);
                pending = false;
            }
            Ev::Reg { owner: WILL_SHOT_ASSET, .. } if e.t - will_t < 0.2 => {
                if let Some(b) = will.back_mut() {
                    *b = true;
                }
            }
            _ => {}
        }
    }
    (picks, will)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_shot(shot: [bool; 3]) -> Smol {
        let mut s = Smol::default();
        for (g, x) in s.goons.iter_mut().zip(shot) {
            g.shot = x;
        }
        s
    }

    #[test]
    fn a_shot_goon_passes_to_the_next_standing_one() {
        let none = with_shot([false; 3]);
        assert_eq!((0..3).map(|r| none.standing_from(r)).collect::<Vec<_>>(), vec![Some(0), Some(1), Some(2)]);
        assert_eq!(with_shot([true, false, false]).standing_from(0), Some(1));
        assert_eq!(with_shot([true, true, false]).standing_from(0), Some(2));
        assert_eq!(with_shot([false, true, false]).standing_from(1), Some(2));
        assert_eq!(with_shot([false, true, true]).standing_from(1), Some(0));
        assert_eq!(with_shot([false, false, true]).standing_from(2), Some(0));
        assert_eq!(with_shot([true, false, true]).standing_from(2), Some(1));
        assert_eq!(with_shot([true; 3]).standing_from(0), None);
    }

    #[test]
    fn hit_chances() {
        assert_eq!(hit_percent(Appearance::First, Difficulty::from_pref(b'E')), 20);
        assert_eq!(hit_percent(Appearance::First, Difficulty::from_pref(b'S')), 40);
        assert_eq!(hit_percent(Appearance::Later, Difficulty::from_pref(b'H')), 80);
        assert_eq!(Goon { picks: 0, ..Goon::default() }.appearance(), None);
        assert_eq!(Goon { picks: 2, ..Goon::default() }.appearance(), Some(Appearance::Later));
    }

    #[test]
    fn appearances_walk_the_pick_row() {
        // made-up View ids (the game's are read from the exe, `Hdb::exe`)
        let table = [[0x101, 0x102, 0x103], [0x201, 0x202, 0x203], [0x301, 0, 0x303]];
        let mut s = Smol::default();
        assert_eq!([0, 0, 0, 0].map(|_| s.appear(0, &table)), [0x101, 0x102, 0x103, 0x103]);
        assert_eq!([0, 0].map(|_| s.appear(2, &table)), [0x301, ALL_DOWN_VIEW]);
    }
}
