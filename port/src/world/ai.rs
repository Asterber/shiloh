//! HYPOTHESES H621–H623: the artificial-intuition ("AI") icon, the game's EvidenceIcon 0xdb79 (bar 6 at
//! (530,48), picture 0xdd7c = XG/56700.pff entries 0..7). XT/80249.xtx (the game's help): "the icon starts spinning
//! when you are close to something important; a click on it then takes you to the important object".
//!
//! State (static reading, `RE_NOTES` H621–H623; addresses there):
//! - SetAIList(trigger, list) remembers a hint ActionList and the trigger its actions are posted with (list 0 clears
//!   both); ClearAIList clears them and the glow (H621).
//! - SetAIGlow(n): 0 stops the icon's animation; 1 starts it; n > 1 starts it and names the View a click goes to (H622).
//! - The save keeps the list: VCGameState f54 = action list, f58 = trigger (data: fo-056-end.x 0xe324 / 0xe31d after
//!   `traces/fo-056.jsonl` 152.82 SetAIList(0xe31d, 0xe324); fo-057-end.x 0 / 0 after 118.11 ClearAIList).
use super::action::Action;
use super::{class, Msg, PostSrc, World};

/// The AI icon (EvidenceIcon record built at game start, H71)
pub const AI_ICON: u32 = 0xdb79;

/// Pref_bArtificialIntuition (the port keeps the preferences as their variables, H81)
pub const PREF_AI: u32 = 0x369;

impl World<'_> {
    /// Pref_bArtificialIntuition on (a missing variable counts as on, as the registry default)
    pub fn ai_pref_on(&self) -> bool {
        self.vars.get(PREF_AI).is_none_or(|v| v != 0)
    }
}

/// The AI icon's native state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ai {
    /// the VCTrigger the list's actions are posted with (0 = none)
    pub trigger: u32,
    /// the VCActionList of hints (0 = none)
    pub list: u32,
    /// the position in the list the next click starts from
    pub cursor: usize,
    /// SetAIGlow ≥ 1: the icon's animation runs
    pub glow: bool,
    /// SetAIGlow's View (> 1), the click's destination
    pub target: u32,
}

impl World<'_> {
    /// H621 / H622: SetAIList / ClearAIList / SetAIGlow (native state only; no registration, event or post — as H94).
    pub(super) fn ai_proc(&mut self, name: &str, args: &[u32]) {
        match (name, args) {
            ("SetAIList", &[trigger, list, ..]) if list != 0 => {
                if self.ai.list != list {
                    self.ai.cursor = 0;
                }
                self.ai.trigger = trigger;
                self.ai.list = list;
            }
            ("SetAIList", _) => {
                self.ai.trigger = 0;
                self.ai.list = 0;
                self.ai.cursor = 0;
            }
            ("ClearAIList", _) => {
                self.ai = Ai::default();
            }
            ("SetAIGlow", &[n, ..]) => {
                self.ai.glow = n != 0;
                self.ai.target = if n > 1 { n } else { 0 };
            }
            _ => {}
        }
    }

    /// The list's actions in order.
    fn ai_actions(&self) -> Vec<u32> {
        if self.ai.list == 0 {
            return Vec::new();
        }
        self.h.list_of(class::ACTION_LIST, self.ai.list)
    }

    /// The action's condition with the AI trigger (no condition → true; not evaluable → false).
    fn ai_cond(&self, action: u32) -> bool {
        let Some(a) = Action::load(self.h, action) else { return false };
        match a.condition() {
            None => true,
            Some(c) => self.vars.condition_in(&c, Some(self.ai.trigger)).unwrap_or(false),
        }
    }

    /// HYPOTHESIS H621: the icon is lit (not dimmed) while it glows or an action of the list has a holding condition.
    /// The original re-evaluates at SetAIList / ClearAIList and on its own events; the port evaluates on every frame
    /// (display only).
    pub fn ai_lit(&self) -> bool {
        self.ai.glow || self.ai_actions().into_iter().any(|a| self.ai_cond(a))
    }

    /// HYPOTHESIS H623, a click on the icon: glowing → Set View to SetAIGlow's View
    /// (if any); else the hint list is walked once round from the remembered position, posting every action whose
    /// condition holds, until a non-variable action was posted. The next click starts after the last action looked at.
    pub fn ai_click(&mut self) {
        if self.ai.glow {
            if self.ai.target != 0 && self.h.has(class::VIEW, self.ai.target) {
                self.engine_set_view(self.ai.target);
            }
            return;
        }
        let hints = self.ai_actions();
        if hints.is_empty() {
            return;
        }
        let start = self.ai.cursor % hints.len();
        let round = hints.iter().cycle().skip(start).take(hints.len());
        let mut looked = 0;
        for &a in round {
            looked += 1;
            if !self.ai_cond(a) {
                continue;
            }
            let kind = Action::load(self.h, a).map_or(0, |x| x.kind());
            if kind == 1 && self.dbox_layout().is_some() {
                self.dbox_close();
            }
            self.post(Msg { action: a, trigger: self.ai.trigger }, PostSrc::Engine);
            if kind != 0 {
                break;
            }
        }
        self.ai.cursor = (start + looked) % hints.len();
    }

    /// The save's AI list (VCGameState f54 list, f58 trigger).
    pub(super) fn ai_seed(&mut self) {
        let Some(&(id, _)) = self.gam.class(class::GAME_STATE).and_then(|c| c.objects.first()) else { return };
        let Some(r) = self.gam.read(class::GAME_STATE, id) else { return };
        let (list, trigger) = (r.u32("f54"), r.u32("f58"));
        if list != 0 && self.h.has(class::ACTION_LIST, list) {
            self.ai.list = list;
            self.ai.trigger = trigger;
        }
    }
}
