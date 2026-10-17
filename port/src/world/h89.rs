//! HYPOTHESIS H89 (`Rules::h89_queue_model`, RE_NOTES H89; static + H76's traces): the View steps' follow-up lifecycle
//! fires are ordinary commands of the application's one depth-first queue: a post inserts right after the running
//! message (the cursor moves onto it, so one dispatch's posts stay in post order) and the scheduler (FUN_00402830)
//! restarts at the head after every dispatch. This is C24's insert point, so the world's
//! queue (and the runner's) express it directly once the steps are commands instead of synchronous fires:
//! - View cmd 0 posts View cmd 11 ([`WCmd::ViewBuild`]: builds the background object, whose cmd 0 =
//!   Activation is posted right behind it) *before* its trigger actions, CharViews, View cmd 10, nav-hotspots,
//!   Explorables (each a command) → depth-first, the background's Activation and everything it posts run first;
//! - the old View's Deactivation (with its background's, A1) is one command, the rest of the exit
//!   ([`WCmd::ExitTail`]: obj_remove of the hotspots / CharViews / Explorables, ViewPoint change, new View
//!   cmd 0) another behind it, so the Deactivation's posts run first.
//!
//! Replaces H76's hold (`world/h76.rs`) when on. In the lock-step replay the trace supplies the fires: a fire command at
//! the head emits its prediction and, if the owner's triggers post on that event, waits for the trace's fire (its posts
//! go in front). A dispatch of a message that lies behind queued commands runs them first (fallback, counted in
//! [`World::h89_flushes`]). In the closed-loop runner the world does not queue: commands go out in post order
//! (`Out::Fire`, `Out::Command`) and the runner's C24 queue runs them ([`World::run_cmd`]).
use super::{event, Msg, Out, World};

/// An engine command (not an action message) of the H89 queue model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WCmd {
    /// lifecycle fires run as one command, in order (one owner; or the old View's Deactivation + its background's, A1)
    Fires(Vec<(u32, u8)>),
    /// View cmd 11 (command 0xb of the View): chooses and builds the View's background (H53)
    ViewBuild(u32),
    /// the rest of a transition after the old View's Deactivation: old nav-hotspots', CharViews', Explorables'
    /// Deactivations, the ViewPoint change and the new View's Activation, each its own command
    ExitTail { dest: u32, vp: Option<u32> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum QItem {
    Msg(Msg),
    Cmd(WCmd),
}

impl<'a> World<'a> {
    pub(super) fn h89_on(&self) -> bool {
        self.rules.h89_queue_model
    }

    /// Post an engine command at the C24 insert point (runner: hand it out in post order).
    pub(super) fn post_cmd(&mut self, c: WCmd) {
        if self.h76_external {
            match c {
                WCmd::Fires(v) => {
                    for (owner, event) in v {
                        self.out.push(Out::Fire { owner, event });
                    }
                }
                c => self.out.push(Out::Command(c)),
            }
            return;
        }
        let at = if self.started { self.ins.min(self.queue.len()) } else { self.queue.len() };
        self.queue.insert(at, QItem::Cmd(c));
        if self.started {
            self.ins += 1;
        }
    }

    /// Fire owner `o` with `ev` as a queued command (H89) or at once (as before).
    pub(super) fn fire_cmd(&mut self, o: u32, ev: u8) {
        if self.h89_on() {
            self.post_cmd(WCmd::Fires(vec![(o, ev)]));
        } else {
            self.out.push(Out::Fire { owner: o, event: ev });
        }
    }

    /// The owner's triggers post something on `ev` (its fire's posts must run before the next command).
    fn h89_will_post(&self, o: u32, ev: u8) -> bool {
        if ev == event::ACTIVATION && self.restoring {
            return false; // C30: a gated Activation fires no trigger
        }
        let tl = self.registered.get(&o).copied().or_else(|| self.trigger_list_of(o)).unwrap_or(0);
        tl != 0 && self.triggers(tl).iter().any(|t| t.1 == ev)
    }

    /// Run a command (the runner's dispatch, or the world's own queue in lock-step).
    pub fn run_cmd(&mut self, c: WCmd) {
        match c {
            WCmd::Fires(v) => {
                for (o, ev) in v {
                    self.out.push(Out::Fire { owner: o, event: ev });
                    if !self.h76_external && self.h89_will_post(o, ev) {
                        if let Some(i) = self.h89_prefired.iter().position(|&x| x == (o, ev)) {
                            self.h89_prefired.remove(i);
                        } else {
                            self.h89_await.push((o, ev));
                        }
                    }
                }
            }
            WCmd::ViewBuild(v) => {
                if self.view == Some(v) {
                    self.choose_background();
                }
            }
            WCmd::ExitTail { dest, vp } => {
                // a newer transition took over (runner: stale command)
                if self.pending_entry == Some(dest) {
                    self.exit_tail(dest, vp);
                }
            }
        }
    }

    /// `owner` fired `ev` (trace / runner): no longer awaited; a fire that comes before its queued command is noted.
    pub(super) fn h89_fired(&mut self, owner: u32, ev: u8) {
        if !self.h89_on() || self.h76_external {
            return;
        }
        if let Some(i) = self.h89_await.iter().position(|&x| x == (owner, ev)) {
            self.h89_await.remove(i);
        } else if self.queue.iter().any(|q| matches!(q, QItem::Cmd(WCmd::Fires(v)) if v.contains(&(owner, ev)))) {
            self.h89_prefired.push((owner, ev));
        }
    }

    /// Run the commands at the head while no fire's posts are awaited (scheduler restart at the head).
    pub(super) fn h89_pump(&mut self) {
        if !self.h89_on() || self.h76_external {
            return;
        }
        while self.h89_await.is_empty() {
            if !matches!(self.queue.first(), Some(QItem::Cmd(_))) {
                break;
            }
            let Some(QItem::Cmd(c)) = Some(self.queue.remove(0)) else { unreachable!() };
            self.begin_dispatch();
            self.run_cmd(c);
        }
    }

    /// Fallback: a message is dispatched while commands lie ahead of it (an awaited fire never came, or the trace runs
    /// out of order) — run those commands first, in queue order (`action` None = the first message).
    pub(super) fn h89_flush_to(&mut self, action: Option<u32>) {
        if !self.h89_on() || self.h76_external {
            return;
        }
        loop {
            let i = self
                .queue
                .iter()
                .position(|q| matches!(q, QItem::Msg(m) if action.is_none_or(|a| m.action == a)))
                .unwrap_or(self.queue.len());
            let Some(j) = (0..i).find(|&j| matches!(self.queue[j], QItem::Cmd(_))) else { break };
            let QItem::Cmd(c) = self.queue.remove(j) else { unreachable!() };
            self.h89_flushes += 1;
            self.h89_await.clear();
            self.started = true;
            self.ins = j;
            self.run_cmd(c);
        }
    }

    /// Fallback of a new transition: every command still queued runs first (as H76's release of an older hold).
    pub(super) fn h89_flush_all(&mut self) {
        if !self.h89_on() || self.h76_external {
            return;
        }
        // the running dispatch's insert point, counted in messages (commands only post commands: the messages stay)
        let k = self.queue[..self.ins.min(self.queue.len())].iter().filter(|q| matches!(q, QItem::Msg(_))).count();
        let started = self.started;
        while let Some(j) = self.queue.iter().position(|q| matches!(q, QItem::Cmd(_))) {
            let QItem::Cmd(c) = self.queue.remove(j) else { unreachable!() };
            self.h89_flushes += 1;
            self.h89_await.clear();
            self.started = true;
            self.ins = j;
            self.run_cmd(c);
        }
        self.started = started;
        self.ins = k;
    }
}
