//! HYPOTHESIS H76 (`Rules::h76_entry_hotspot_order`, RE_NOTES H76; post-hoc fo-009 0x4be, fo-021 0x3ef): the lifecycle
//! fires that follow a View step run as queued commands behind every action already queued — including the actions the
//! step's own synchronous fires post (A1: the background's Activation / Deactivation).
//!
//! - Entry: the Activations of the nav-hotspots (and Explorables, H55) built by the View's Activation wait for the View's
//!   and its new background's posts to run (fo-009 304.61..310.44 0x15a09..0x15a0d, then 310.90 Act 0x4c4..; fo-021
//!   258.86 0x159e5/0x159e6 → Enable-built 0xd242 / 0xd244 Act at once (C24 front), then 263.30 Act 0x3f4, 0x3f9).
//! - Exit: after Deact(old View), Deact(old background) their posts run first (C23 "its posts run"), then the remaining old
//!   nav-hotspots' Deactivations, the old Explorables', the ViewPoint change and the new View's Activation (fo-021
//!   275.48 0x159e9 / 0x159ea Disable 0xd242 / 0xd244 → Deact at once, 278.90 Deact 0x3f4, 0x3f9, 279.62 Act 0xd237).
//!
//! The world has no command queue for fires (the replay feeds the trace's fires; the closed-loop runner orders its own,
//! A2), so the held fires carry a marker into the action queue: `at` = number of queued messages ahead of them (front
//! insertions, C24, before the marker move it back), `wait` = owners whose fire (whose posts) is still to come. When
//! nothing is ahead and nothing is awaited, the fires are emitted.
use super::{event, Out, World};

#[derive(Clone, Debug)]
pub(super) enum Step {
    /// Activations held at a View entry
    Entry(Vec<u32>),
    /// the rest of a transition after the old View's Deactivation (`World::exit_tail`)
    Exit { dest: u32, vp: Option<u32> },
}

#[derive(Clone, Debug)]
pub(super) struct Held {
    id: u64,
    step: Step,
    at: usize,
    wait: Vec<(u32, u8)>,
}

impl<'a> World<'a> {
    fn h76_on(&self) -> bool {
        self.rules.h76_entry_hotspot_order && !self.rules.h89_queue_model // H89 replaces the hold
    }

    /// Exit step: hold the transition's tail behind the old View's and background's Deactivation posts.
    pub(super) fn h76_hold_exit(&mut self, dest: u32, vp: Option<u32>, old_bg: Option<u32>) -> bool {
        if !self.h76_on() {
            return false;
        }
        let mut wait = Vec::new();
        // the old View's Deactivation is the one pushed by `transition` (C23); its background's goes with it (A1)
        let old_view = self.out.iter().rev().find_map(|o| match o {
            Out::Fire { owner, event: event::DEACTIVATION } if self.h.has(super::class::VIEW, *owner) => Some(*owner),
            _ => None,
        });
        for o in old_view.into_iter().chain(old_bg) {
            if self.registered.contains_key(&o) {
                wait.push((o, event::DEACTIVATION));
            }
        }
        // the tail steps are posted by the transition's command: C24 front insertion, ahead of older pending actions
        // (fo-005 160.24..160.44 Deact 0x396 .. 0x2e0 before 0xac67 / 0x119eb, posted at 157.87 / 157.98)
        let at = if self.started { self.ins.min(self.queue.len()) } else { self.queue.len() };
        self.h76_seq += 1;
        self.held = Some(Held { id: self.h76_seq, step: Step::Exit { dest, vp }, at, wait });
        self.h76_settle();
        true
    }

    /// Entry step: take the Activations `view_activated` emitted since `mark` (all but the background's) and hold them
    /// behind what is queued; if the View got a new background, also behind that background's Activation posts.
    pub(super) fn h76_hold_entry(&mut self, mark: usize, bg_before: Option<u32>) {
        if !self.h76_on() {
            return;
        }
        let bg = self.background.filter(|&b| Some(b) != bg_before);
        let mut fires = Vec::new();
        let mut i = mark;
        while i < self.out.len() {
            match self.out[i] {
                Out::Fire { owner, event: event::ACTIVATION } if Some(owner) != bg => {
                    fires.push(owner);
                    self.out.remove(i);
                }
                _ => i += 1,
            }
        }
        // (not filtered by `registered`: the replay registers the background only when the trace does, after this)
        let wait = bg.map(|b| (b, event::ACTIVATION)).into_iter().collect();
        // posted by the View's Activation command after its own posts: C24 front insertion, ahead of older pending
        // actions (fo-013 132.19..133.82 the gated Acts of 0x2b9, 0x2c2, 0x4f0, 0x1432c before the restored timer message
        // 135.44, H67) — not behind everything queued (A2 of the runner)
        let at = if self.started { self.ins.min(self.queue.len()) } else { self.queue.len() };
        self.h76_seq += 1;
        self.held = Some(Held { id: self.h76_seq, step: Step::Entry(fires), at, wait });
        self.h76_settle();
    }

    /// `owner` fired `ev` (its posts are queued): no longer awaited.
    pub(super) fn h76_fired(&mut self, owner: u32, ev: u8) {
        if let Some(h) = &mut self.held {
            h.wait.retain(|&x| x != (owner, ev));
        }
        self.h76_settle();
    }

    pub(super) fn h76_inserted(&mut self, i: usize) {
        if let Some(h) = &mut self.held {
            if i <= h.at {
                h.at += 1;
            }
        }
    }

    pub(super) fn h76_removed(&mut self, i: usize) {
        if let Some(h) = &mut self.held {
            if i < h.at {
                h.at -= 1;
            }
        }
    }

    /// queue position `i` lies behind the held fires
    pub(super) fn h76_behind(&self, i: usize) -> bool {
        self.held.as_ref().is_some_and(|h| i >= h.at)
    }

    /// Emit the held fires if nothing is ahead of them and nothing awaited.
    pub(super) fn h76_settle(&mut self) {
        if self.h76_external {
            return; // the runner releases it (`h76_release_id`)
        }
        if self.held.as_ref().is_some_and(|h| h.at == 0 && h.wait.is_empty()) {
            self.h76_release();
        }
    }

    /// Closed-loop runner: the runner's queue orders the hold (a marker command posted at the C24 insert point after the
    /// step's own posts; whatever is posted later by commands ahead of it goes before it, as in the lock-step model).
    pub fn h76_set_external(&mut self) {
        self.h76_external = true;
    }

    /// Id of the hold currently pending (None = nothing held).
    pub fn h76_held(&self) -> Option<u64> {
        self.held.as_ref().map(|h| h.id)
    }

    /// The runner reached the marker of hold `id`: emit its fires (nothing if a newer step already released it).
    pub fn h76_release_id(&mut self, id: u64) {
        if self.h76_held() == Some(id) {
            self.h76_release();
        }
    }

    /// Emit the held fires now.
    pub(super) fn h76_release(&mut self) {
        let Some(h) = self.held.take() else { return };
        match h.step {
            Step::Entry(fires) => {
                for o in fires {
                    self.out.push(Out::Fire { owner: o, event: event::ACTIVATION });
                }
            }
            Step::Exit { dest, vp } => self.exit_tail(dest, vp),
        }
    }
}
