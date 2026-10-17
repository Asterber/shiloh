//! The state of a running game as a save ([`crate::save::SaveState`]).
//!
//! HYPOTHESIS H405 (static readers of H65 / H66 / H67 / H126 / H178 / H186 / H262 read backwards,
//! checked only by the port's own round trip, not against a save the original wrote at a known state): a save holds
//! - every VCEnabled flag and VCVariable value of the session (the world's tables; an unknown (tainted) variable keeps
//!   the base file's value),
//! - VCGameState Node / Location / ViewPoint / View = the current ones (the saves of the original hold the View
//!   of the last transition: post-hoc 33 / 33 bot end saves `data/bot/fo-0NN-end.x` vs the last `transition_new` of
//!   `fo-0NN`, checked with a probe),
//! - la0 = the VCInventory objects alive, activation order (all saves: 0x442b, 0x442f first = creation order),
//! - l64 = the looping players (type 7, H100) [AssetRef, 1, position ms] (all saves: flag 1; 0x76bf FO.amv, 0xcc41,
//!   0x16daa),
//! - l154 = the running timers [id, remaining ms, periodic] (fo-054-end.x [5, 2000, 1], fo-2b3-user.x [0, 0x733b, 0]),
//! - f1a4 = the CharView of the shown dialogue box (fo-028-end.x 0x590c, fo-056-end.x 0x55e9),
//! - the asked Conversations (0x56), PDA notes (0x58), e-mail inbox / read / pending (0x5a..0x5c).
//! ldc / l118 (EvidenceIcons) are empty in every save of the original and not tracked: kept from the base.
use super::run::Runner;
use super::{class, World};
use crate::save::SaveState;

impl World<'_> {
    /// The world's part of a save over `base` (fields not tracked keep the base's values).
    pub fn save_state(&self, base: &SaveState) -> SaveState {
        let mut st = base.clone();
        let mut en: Vec<((u32, u32), u8)> = self.enabled.iter().map(|(&k, &f)| (k, f)).collect();
        en.sort();
        st.enabled = en;
        let keep: std::collections::HashMap<u32, (i32, u8)> = base.vars.iter().map(|&(id, v, t)| (id, (v, t))).collect();
        st.vars = self
            .vars
            .stored()
            .into_iter()
            .filter_map(|(id, v, t)| match v {
                Some(v) => Some((id, v, t)),
                None => keep.get(&id).map(|&(v, t)| (id, v, t)),
            })
            .collect();
        // H650 / H475: a string variable whose text the session changed (SetLanguage) is saved with that text (the writer
        // gives it a new VCString objId); the others keep the base's
        if self.rules.h650_string_compare {
            for (var, text) in self.vars.changed_texts() {
                let mut t = text;
                t.push(0);
                match st.strings.iter_mut().find(|s| s.0 == var) {
                    Some(s) => s.1 = t,
                    None => st.strings.push((var, t)),
                }
            }
        }
        if let (Some(n), Some(l), Some(vp), Some(v)) = (self.node, self.location, self.viewpoint, self.view) {
            st.start = [n, l, vp, v];
        }
        st.char_view = self.dbox.as_ref().map_or(0, |d| d.0);
        if self.rules.h1586_save_view_layout {
            // H1586: the VCGameState's f1a8 = the shown layout set as View (the Load's H732 shows it again): fo-060-end.x
            // 0x14968 (SetIFLAsView in fo-060), 0 in every other save; the latest one if several
            st.extra.f1a8 = self.layouts_as_view.iter().rev().copied().find(|&l| self.layouts.iter().any(|x| x.0 == l)).unwrap_or(0);
        }
        if self.rules.h205_evidence_icon_type3 && self.rules.h702_evidence_save {
            // H702 (port fix; layout of ldc / l118 as read by the Load, H66 / H205): the built EvidenceIcons —
            // non-kit [id, state 1] in ldc, kit ids in l118 — instead of the base save's lists (a port save made after
            // collecting evidence lost them and the Crime Lab showed no evidence icon after its Load)
            let ev = self.icon_objects().iter().filter(|o| o.cls == class::EVIDENCE_ICON);
            st.evidence = ev.clone().filter(|o| !o.kit).map(|o| [o.id, self.icon_state(o.id) as u32]).collect();
            st.kit = ev.filter(|o| o.kit).map(|o| o.id).collect();
        }
        if self.rules.h1603_flight_saved && self.dev.flight_known {
            st.extra.b190[..3].copy_from_slice(&self.dev.flight); // H1603: the three flight globals
        }
        if self.rules.h262_dbox_rows {
            st.conv_history = self.conv_history.clone();
        }
        if self.rules.h178_pda_notes {
            st.notes = self.px.notes.clone();
        }
        if self.rules.h621_ai_icon {
            // H621: VCGameState f54 = the AI action list, f58 = its trigger
            st.extra.f4c[2] = self.ai.list;
            st.extra.f4c[3] = self.ai.trigger;
        }
        if self.rules.h793_photo_album {
            // H982: the session's photos (H793: picture, downloaded) as VCPhoto records — the base's record of the same
            // picture reused in order (its thumbnail and undecoded fields kept), else the original's record layout
            // without a thumbnail (Photo::new)
            let mut left = base.photos.clone();
            st.photos = self
                .pc
                .photos
                .iter()
                .enumerate()
                .map(|(i, &(pic, down))| {
                    // C306: a shot's record matches the base's only with the same f68 / rect (its thumbnail is that frame)
                    let frame = self.pc.photo_frames.get(i).copied().flatten();
                    let same = |b: &crate::save::Photo| b.picture() == pic && frame.is_none_or(|(t, r)| b.f68() == t && b.rect() == r);
                    let mut ph = match left.iter().position(same) {
                        Some(i) => left.remove(i),
                        None => {
                            let mut ph = crate::save::Photo::new(pic, down);
                            if let Some((t, r)) = frame {
                                ph.set_frame(t, r);
                            }
                            ph
                        }
                    };
                    ph.set_downloaded(down);
                    ph
                })
                .collect();
        }
        if self.rules.h186_email {
            st.inbox = self.px.inbox.clone();
            st.read = self.px.read.clone();
            st.pending = self.px.pending.clone();
        }
        st
    }
}

impl Runner<'_> {
    /// The runner's part of a save over `base`: the world's, the inventory objects and the timers (H405). The looping
    /// players' positions are the front-end's (`Session::save_state`); here they get position 0.
    pub fn save_state(&self, base: &SaveState) -> SaveState {
        let mut st = self.w.save_state(base);
        st.inventories = self.active.iter().copied().filter(|&o| self.w.h.has(class::INVENTORY, o)).collect();
        st.timers = self
            .timers
            .iter()
            .map(|t| {
                // H890: a paused timer keeps its delay; H1581: a running periodic one is saved with its period
                let left = t.paused.unwrap_or_else(|| match t.period {
                    Some(p) if self.w.rules.h1581_save_timer_period => p,
                    _ => t.due.saturating_sub(self.now),
                });
                [t.id as u32, left.min(u32::MAX as u64) as u32, t.period.is_some() as u32]
            })
            .collect();
        // H1600: the flag is the player's looping flag — 1 for a looping (mode 2 / restored flag-1) player only
        let flag = |a| (!self.w.rules.h1600_save_player_flag || self.w.player_loops(a)) as u32;
        st.players = self.save_players().into_iter().map(|a| [a, flag(a), 0]).collect();
        st
    }

    /// H405: the players a save keeps — active AssetRefs with a looping player (H100), activation order.
    pub fn save_players(&self) -> Vec<u32> {
        self.active.iter().copied().filter(|&a| self.w.player_saved(a)).collect()
    }
}
