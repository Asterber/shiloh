//! H2401 (port feature, the game language; static HDB comparison + port runs, NOT traced): the EN DVD Edition 2.05
//! HDB's own layout for the French pictures, run over RU 1.00 data.
//!
//! The 2.05 HDB holds 41 RelocateIFace actions the RU 1.00 HDB lacks (Q-ING1, H2392), on the Activation of
//! InterfaceItems both HDBs share (same objIds, same layouts): the e-mail compose screen's Send button, the Options
//! pages' check boxes, the ING record's labels, the Load screen's button, … and gives 32 shared items another region
//! (the help screen's text panels, the ING record's labels: wider or taller, for the longer French / German texts).
//! The French pictures and texts are drawn over RU data (H2391), so those screens kept the RU layout under them. Here
//! the Activation of such an item takes the 2.05 region, then runs the 2.05 trigger's variable writes (its trigger
//! locals) and RelocateIFace actions, in order, with the conditions evaluated on the session's variables
//! (scGameLanguage follows the pictures, H2391). Nothing is written but trigger locals of a context the RU HDB does not
//! use and the item's rect (`px.item_rects`, H179; dropped with the layout) — saves are unaffected.
use std::collections::HashMap;

use super::action::{Action, Operands, ProcCall};
use super::{class, event};
use crate::hdb::{Hdb, Value};

/// VCAction records
const ACTION: u32 = 0x41;

/// The 2.05 layout of the game's InterfaceItems that differ there
#[derive(Default)]
pub struct LangRelocs {
    /// per item: the action lists (variable writes and RelocateIFace, in order) of the other HDB's Activation triggers
    /// that hold a RelocateIFace the game's HDB has no action for
    pub items: HashMap<u32, Vec<Vec<Action>>>,
    /// per item: the other HDB's region where it differs from the game's
    pub regions: HashMap<u32, [i16; 4]>,
}

/// The trigger-local context the 2.05 lists run in (no RU trigger has this id)
pub const LOCALS_CTX: u32 = 0xffff_fff0;

fn is_relocate(a: &Action) -> bool {
    a.kind() == 6 && ProcCall::parse(a.body()).is_some_and(|p| p.name == "RelocateIFace")
}

fn region(h: &Hdb, item: u32) -> Option<[i16; 4]> {
    match h.read(class::INTERFACE_ITEM, item)?.field("region")? {
        Value::Region { rect, .. } => Some(*rect),
        _ => None,
    }
}

impl LangRelocs {
    /// `own` = the game data's HDB, `other` = the HDB of the language whose pictures are drawn (the 2.05 HDB)
    pub fn from_hdbs(own: &Hdb, other: &Hdb) -> LangRelocs {
        let mut lr = LangRelocs::default();
        let Some(c) = other.class(class::INTERFACE_ITEM) else { return lr };
        for &(item, _) in &c.objects {
            if !own.has(class::INTERFACE_ITEM, item) {
                continue;
            }
            if let (Some(a), Some(b)) = (region(own, item), region(other, item)) {
                if a != b {
                    lr.regions.insert(item, b);
                }
            }
            let Some(r) = other.read(class::INTERFACE_ITEM, item) else { continue };
            for t in other.list_of(class::TRIGGER_LIST, r.u32("f48")) {
                let Some(tr) = other.read(class::TRIGGER, t) else { continue };
                if tr.u32("fe8") != event::ACTIVATION as u32 {
                    continue;
                }
                let list: Vec<Action> =
                    other.list_of(class::ACTION_LIST, tr.u32("fe4")).into_iter().filter_map(|id| Action::load(other, id)).collect();
                if !list.iter().any(|a| is_relocate(a) && !own.has(ACTION, a.id)) {
                    continue;
                }
                lr.items.entry(item).or_default().push(list.into_iter().filter(|a| a.kind() == 0 || is_relocate(a)).collect());
            }
        }
        lr
    }
}

impl super::World<'_> {
    /// H2401: an Activation of `item` takes the other HDB's region and moves for it ([`LangRelocs`]); the last move
    /// that holds wins
    pub(super) fn lang_relocate(&mut self, item: u32) {
        let Some(lr) = self.lang_relocs.clone() else { return };
        if let Some(&r) = lr.regions.get(&item) {
            self.px.item_rects.insert(item, r);
        }
        let Some(lists) = lr.items.get(&item) else { return };
        for list in lists {
            self.vars.reset_locals(LOCALS_CTX);
            for a in list {
                if let Some(c) = a.condition() {
                    if self.vars.condition_in(&c, Some(LOCALS_CTX)) != Some(true) {
                        continue;
                    }
                }
                if a.kind() == 0 {
                    if let Some(p) = Operands::parse(a.body()) {
                        self.vars.apply_in(&p, Some(LOCALS_CTX));
                    }
                } else if let Some(p) = ProcCall::parse(a.body()) {
                    if let &[l, t, r, b, ..] = p.args.as_slice() {
                        let r = self.relocated_right(item, l as i16 as i32, r as i16 as i32);
                        self.px.item_rects.insert(item, [l as i16, t as i16, r as i16, b as i16]);
                    }
                }
            }
        }
    }
}
