//! Closed-loop runner: drives a [`World`] without a trace — the world's own outputs (registrations, lifecycle fires,
//! posts) are fed back as inputs, a command queue orders them, a media player turns "asset started" into "asset
//! ended". A front-end only adds user input (clicks) and the media clock ([`Runner::media_ended`]).
//!
//! Rules used (RE_NOTES):
//! - C19/C20: firing an event on a registered owner posts its triggers' ActionLists (`World::fire`).
//! - C24: one command queue; whatever is posted while a command runs goes before the older pending commands, in post
//!   order. Action messages, lifecycle fires, user clicks are commands.
//! - C23: a transition's lifecycle runs as queued steps (Deact old View → its posts run → Deact old VCNavs, old
//!   ViewPoint → Act new ViewPoint → Act new View → …); type-1 assets are registered and activated, a video's
//!   Deactivation fires when it ends.
//! - C26 + C28: the View's Activation is command type 0 on the View object (kind 4) and is not dispatched while a
//!   video raises the media block (cells (0,4)…); action messages (kind 0x17) are never blocked by it.
//! - C21/C22: conditions; operand kinds 1/4 are not evaluable → value from [`Runner::uneval`] (default false, logged).
//! - HYPOTHESIS H59 (when `Rules::h59_timer`): type-2 timers run on the runner clock ([`Runner::advance`]); an expiry is
//!   a queued command firing Timer (7) on the subscribed owners. H58/H60 objects (layouts, items, icons, their assets)
//!   are ordinary registrations + Act/Deact fires of the world and need nothing here (their assets are not media).
//!
//! Scheduling details that are NOT decoded (the original's scheduler FUN_00402830, H47) and are approximated here —
//! each is observed in fo-005/fo-009 but not derived from code:
//! - A1: Act/Deact of the View background is synchronous (fo-005 Deact 0x115bc right after Deact 0x2b3, before its
//!   posts run; fo-009 Act 0x9852 before the View's posts run).
//! - A2: nav-hotspot Activations of a View entry run after everything already queued (fo-009 0x4be: Act 0x4c4.. after
//!   the background's posts 0x15a09..0x15a0d); hotspots built by an Enable action are activated at once (fo-009 0x2b9).
//!   Only with `Rules::h76_entry_hotspot_order` off (ablation); with it on, H76 replaces A2: the world holds a View step's
//!   follow-up fires and the runner places them with `Cmd::Held` at the C24 insert point (RE_NOTES H76). With
//!   `Rules::h89_queue_model` (H89) there is no hold: the world hands out View cmd 11 and the exit's rest as commands
//!   (`Cmd::World`) in post order, the background's Act is queued like any fire (A1 only for the exit Deactivation).
//! - A3: one media player, FIFO: a played asset is activated at once but plays after the previous one ends (fo-009
//!   0x4d6a → 0x6455 → 0x13b94, fba5 → 76bf); when one ends the next starts, then the ended one's Deactivation fires.
//!   The Activations of a View entry's VCExplorables (H55) are queued like the nav-hotspots' (A2, fo-005 139.58).

use super::sched::{media_inputs_h51, Blocks};
use super::{class, event, HotspotSrc, Msg, Out, RegCause, Rules, World};
use crate::hdb::{Hdb, Value};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    Action(Msg),
    Fire { owner: u32, ev: u8 },
    /// several fires in one command (old View's Deactivation + its background's, A1)
    Fires(Vec<(u32, u8)>),
    /// Activation of a transition's new View: command type 0 on the View object (kind 4, C28)
    ActView(u32),
    /// user click on a nav-hotspot: Mouse Click on the VCNav owner, then nav_exec (fo-009: FIRE Click → transition)
    NavClick(u32),
    /// user click on another registered owner (Mouse Click, C19)
    Click(u32),
    /// HYPOTHESIS H59: timer `id` expired (the expiry queues a command on the command list, FUN_0042fa20): Timer (7) on
    /// every subscribed registered owner
    Timer(u8),
    /// HYPOTHESIS H65: end of the restore window (the restore posts the type-0x14 command with value 0 after it,
    /// FUN_00493090; approximated: at the back of the queue when the window opens — after everything the restore queues)
    RestoreEnd,
    /// HYPOTHESIS H67: a restored media player's command (Load, VCGameState l64) — registers and activates its AssetRef
    Player(u32),
    /// HYPOTHESIS H68: an icon click's state-0 command on an EmotionIcon
    IconOff(u32),
    /// HYPOTHESIS H74: the command of a CharView's media wrapper (FUN_00480fe0)
    CharViewPlayer(u32),
    /// HYPOTHESIS H76: the place of a View step's held lifecycle fires (world hold id): posted at the C24 insert point
    /// after the step's own posts (entry: the View's and its new background's; exit: behind the old View's and
    /// background's Deactivation command, whose posts then go in front of it); dispatched → the world emits them
    Held(u64),
    /// HYPOTHESIS H126: the dialogue box's command (builds its Conversations)
    Dbox(u32),
    /// HYPOTHESIS H137: a click in the dialogue box's close box (FUN_004bdb70)
    DboxClose,
    /// HYPOTHESIS H922: the box command's closing state-1 command
    DboxShow(u32),
    /// C107: the button-up while an IdeaIcon is dragged, over this owner (None: over nothing)
    IdeaDrop(Option<u32>),
    /// HYPOTHESIS H651: the end of a played line's sequence (Conversation cmd 0xd + the View's reshow command)
    DboxSeqEnd(u32),
    /// H1123: the View's reshow command after the players a line's end waited for have ended
    DboxReshow,
    /// HYPOTHESIS H89: an engine command of the world (View cmd 11, the rest of an exit), posted at the C24 insert point
    World(super::WCmd),
    /// HYPOTHESIS H152: a click on an inventory item (FUN_0049c180): selection, then Mouse Click
    InvClick(u32),
    /// H450: a View click reached the held gun
    GunClick,
    /// H450 / H821: the gun's shot ended (the shot's wait loop returned) — the View's Mouse Click and the video
    GunShotDone,
    /// HYPOTHESIS H1282: the click of movie hotspot `id` (the movie-hotspot target's click handler) on the video `movie`
    MovieHot { id: u32, movie: u32 },
    /// HYPOTHESIS H601: an engine-queued mouse command (FUN_004cf7e0): hit test at (x, y) when it runs, then as a click
    MouseAt(i32, i32),
}

/// H1203: the longest delay winmm's `timeSetEvent` accepts under the CrossOver bottle (wPeriodMax)
pub const TIMER_MAX_DELAY: u64 = 0xffff;
/// H2063: the pause screen of Alt+P (FUN_00519290: "Pause Game.", item 0xf5ea = x.pff #483 over the whole screen)
pub const ALT_PAUSE_LAYOUT: u32 = 0xf5e8;

/// A running timer (H59): id, due time (runner clock, ms), period (Some = periodic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timer {
    pub id: u8,
    pub due: u64,
    pub period: Option<u64>,
    /// H890: paused (no native handle) with this delay; re-armed by the resume
    pub paused: Option<u64>,
}

/// A clickable object on screen: owner + rect (x1, y1, x2, y2), screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clickable {
    pub owner: u32,
    pub cls: u32,
    pub rect: [i16; 4],
    /// HotSpot / region type (0x11c left, 0x11d right, 0x11e forward, 0x11f…; 0x4454, 0x1bfc for objects)
    pub kind: u32,
}

/// H130: the View rect's top / bottom (the View rect is (20, 90)–(620, 330), set at static init)
const VIEW_TOP: i32 = 90;
const VIEW_BOTTOM: i32 = 330;
/// H150: `Clickable::kind` of an icon placed by its bar
pub const ICON_KIND: u32 = 0x1cc;

/// H1700: a core's diagnostic histories ([`Runner::take_history`])
#[derive(Clone, Debug, Default)]
pub struct History {
    pub transitions: Vec<(u32, u32)>,
    pub posts: Vec<Msg>,
    pub execs: Vec<u32>,
    pub conds: Vec<(u32, u32, Option<bool>)>,
    pub regs: Vec<u32>,
    pub lifecycle: Vec<(u32, u8)>,
    pub activations: Vec<u32>,
    pub log: Vec<String>,
}

/// H2170: the last hit point, set by [`Runner::hit`] (`&self`). A Mutex, not a Cell: xfplan's searches share Runners
/// between threads (`par_map`), so the Runner must stay `Sync`.
#[derive(Default, Debug)]
pub struct HitPoint(std::sync::Mutex<Option<(i32, i32)>>);

impl HitPoint {
    pub fn set(&self, v: Option<(i32, i32)>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = v;
    }
    pub fn get(&self) -> Option<(i32, i32)> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Clone for HitPoint {
    fn clone(&self) -> HitPoint {
        HitPoint(std::sync::Mutex::new(self.get()))
    }
}

#[derive(Clone)]
pub struct Runner<'a> {
    pub w: World<'a>,
    /// C107: the front holds the left button down (between [`Runner::button_press`] and [`Runner::button_release`]);
    /// false = clicks are down + up at once (headless steps, `click`)
    pub button_down: bool,
    /// C107: a button-up that came before the press's command ran (its target)
    release_target: Option<Option<u32>>,
    q: VecDeque<Cmd>,
    ins: usize,
    seen: usize,
    blocks: Blocks,
    /// owners whose Activation ran and whose Deactivation did not (activation order)
    pub active: Vec<u32>,
    /// every Activation that ran, in order (logs / tests)
    pub activations: Vec<u32>,
    /// every Activation / Deactivation that ran (owner, event), in order (tests: closed loop vs trace)
    pub lifecycle: Vec<(u32, u8)>,
    /// owner -> sequence number of its last registration (hit-test stacking, A4)
    reg_seq: HashMap<u32, usize>,
    reg_n: usize,
    /// media player FIFO (A3); head = playing
    pub media: VecDeque<u32>,
    /// H100: assets whose looping player passed its media end and stays (left the FIFO, no Deactivation); the front-end
    /// restarts their playback
    looped: Vec<u32>,
    /// H1601: where a restored player's media starts (ms into it), taken by the front-end when it starts
    start_offset: HashMap<u32, u64>,
    backgrounds: HashSet<u32>,
    /// H55: Explorables registered by a View's activation whose Activation is still to be queued (A2, like nav-hotspots)
    entry_explorables: HashSet<u32>,
    /// View background currently shown (H53)
    pub background: Option<u32>,
    /// condition values for actions whose operands the core cannot evaluate (kinds 1/4); default false
    pub uneval: HashMap<u32, bool>,
    /// H841: per action, the original's traced values of a RandGen-dependent condition in order — consumed one per
    /// unevaluable dispatch before falling back to [`Runner::uneval`]
    pub uneval_seq: HashMap<u32, VecDeque<bool>>,
    pub transitions: Vec<(u32, u32)>,
    /// executed actions (id) and unimplemented cases, for logs / tests
    pub execs: Vec<u32>,
    /// every condition decided at dispatch: (action, trigger, value; None = not evaluable, `uneval` used)
    pub conds: Vec<(u32, u32, Option<bool>)>,
    /// every post (action, trigger), in post order
    pub posts: Vec<Msg>,
    pub log: Vec<String>,
    last_hotspot_src: Option<HotspotSrc>,
    /// number of dispatched commands
    pub dispatched: usize,
    hovered: Option<u32>,
    /// runner clock (ms), set by the front-end ([`Runner::advance`]); timers (H59) are due on it
    pub now: u64,
    pub timers: Vec<Timer>,
    /// registrations taken from outside ([`Runner::external_register`]) — inputs the core could not produce
    pub externals: Vec<u32>,
    /// every registration, in order
    pub regs: Vec<u32>,
    /// C33: the application start (H69) has run
    started: bool,
    /// H76: id of the world's hold whose marker (`Cmd::Held`) is queued
    marked: Option<u64>,
    /// H150: icon bar placement (rects of the icons placed at their Activation)
    pub icon_bars: super::icons::Bars,
    /// H150: picture size (w, h) per AssetRef — an input from the media files (front-end); an icon whose picture size is
    /// unknown gets no rect
    pub asset_sizes: HashMap<u32, (i16, i16)>,
    /// H152: the selected inventory item
    pub inv_selected: Option<u32>,
    /// H170: the inventory panel (kind 0x11) is shown (state 1); with its 1000 ms hide after an inventory Enable
    pub inv_panel: bool,
    pub panel_hide_at: Option<u64>,
    /// H451: the tool-comment player due at (time, AssetRef, CharView)
    pub(super) tool_comment_at: Option<(u64, u32, u32)>,
    /// H733: no command is dispatched before this game time (FadeToBlack's busy wait)
    pub stall_until: Option<u64>,
    /// H170: inventory objects registered by a type-3 Enable whose Activation is still to come
    pub(super) inv_enabled: HashSet<u32>,
    /// H169: the in-game menu is open (opened by the top-right object, closed by [`Runner::menu_choose`])
    pub menu_open: bool,
    /// H2340: a screen the menu opened (Save / Load / Options / Help) shows over the live menu (cleared when the menu
    /// shows again)
    pub menu_screen: bool,
    /// H403: save / load requests of the save screens (`Out::SaveGame` / `Out::LoadGame`) for the front-end, in order
    pub save_req: Vec<super::Out>,
    /// H361: the previous hover point (the panel's Roll Into test)
    pub(super) last_hover: Option<(i32, i32)>,
    /// every player registration in order: (runner clock, asset, its cause) — logs / tests of what plays
    pub plays: Vec<(u64, u32, RegCause)>,
    /// H1123: `plays.len()` when the last line's end command was posted (the players started from there on)
    seq_end_mark: usize,
    /// H1123: the players a line's end waits for (each a player sequence; the View's reshow command after the last ends)
    reshow_wait: Vec<u32>,
    /// every player state switch the runner made (runner clock, asset, state 1 started / 0 stopped) — logs / tests
    pub media_log: Vec<(u64, u32, u32)>,
    /// H821: the gun's shot playing now (its AssetRef) and what runs at its end (the shot's wait loop waits for it)
    pub(super) gun_shot: Option<(u32, Option<Cmd>)>,
    /// H821: every gun shot started: (runner clock, AssetRef) — logs / tests
    pub shots: Vec<(u64, u32)>,
    /// H2170: the point of the last hit test (the held camera's click uses it for its viewfinder)
    pub(super) last_hit: HitPoint,
    /// H823: a game runs (the two game-running flags set: the menu shows return.pic and the Save item); cleared by a game
    /// end without the rewind box (H182, the original's game end FUN_004cc870)
    pub game_running: bool,
    /// H1700: the modal "Try again?" box (AssetRef 0xd3b8) shows — the game stands still until YES / NO
    /// ([`Runner::rewind_answer`]; YES's Load is the front's, `Session::rewind_yes`)
    pub rewind_box: bool,
    /// H1700: the rewind snapshot of the last SaveGameState since the game's Load / start (None = the loaded save itself)
    pub rewind_snap: Option<crate::save::SaveState>,
    /// H1703: the game is frozen (the freeze of hotspots 100 / 500 / 600, H1520): no game time, no world input
    pub frozen: bool,
    /// H1281 input (the front, from the `.HOT` files): per AssetRef, the hotspots of its movie, times relative to the
    /// asset's start
    pub movie_hotspots: HashMap<u32, Vec<crate::media::hot::Hotspot>>,
    /// H1281: runner clock at which each playing player started (state 1)
    pub(super) movie_start: HashMap<u32, u64>,
}

impl<'a> Runner<'a> {
    pub fn new(h: &'a Hdb, gam: &'a Hdb, rules: Rules) -> Runner<'a> {
        // H76 (when on): the world holds a View step's follow-up fires, the runner's queue places them (`Cmd::Held`)
        let mut w = World::new(h, gam, rules);
        w.h76_set_external();
        Runner {
            w,
            marked: None,
            q: VecDeque::new(),
            ins: 0,
            seen: 0,
            blocks: Blocks::default(),
            active: Vec::new(),
            activations: Vec::new(),
            lifecycle: Vec::new(),
            reg_seq: HashMap::new(),
            reg_n: 0,
            media: VecDeque::new(),
            looped: Vec::new(),
            start_offset: HashMap::new(),
            backgrounds: HashSet::new(),
            entry_explorables: HashSet::new(),
            background: None,
            uneval: HashMap::new(),
            uneval_seq: HashMap::new(),
            transitions: Vec::new(),
            execs: Vec::new(),
            conds: Vec::new(),
            posts: Vec::new(),
            log: Vec::new(),
            last_hotspot_src: None,
            dispatched: 0,
            hovered: None,
            now: 0,
            timers: Vec::new(),
            externals: Vec::new(),
            regs: Vec::new(),
            started: false,
            icon_bars: Default::default(),
            asset_sizes: HashMap::new(),
            inv_selected: None,
            inv_panel: false,
            panel_hide_at: None,
            tool_comment_at: None,
            stall_until: None,
            button_down: false,
            release_target: None,
            inv_enabled: HashSet::new(),
            menu_open: false,
            menu_screen: false,
            save_req: Vec::new(),
            last_hover: None,
            plays: Vec::new(),
            seq_end_mark: 0,
            reshow_wait: Vec::new(),
            media_log: Vec::new(),
            gun_shot: None,
            last_hit: Default::default(),
            shots: Vec::new(),
            game_running: true,
            rewind_box: false,
            rewind_snap: None,
            frozen: false,
            movie_hotspots: HashMap::new(),
            movie_start: HashMap::new(),
        }
    }

    /// New game. What runs between the menu and the first View is not decoded; every trace (fo-005..fo-009) shows
    /// the same entry: VCTitle 0x12b is registered with TriggerList 0x5bdd (= its field fa0, H56) and activated; its trigger 0x66d6 posts 26 actions ending with SetStartView
    /// 0xcdbe (C25). Objects registered before it (cursors, 0xdb79, 0x14cb, 0xdd7c) all have TL 0 — no effect.
    /// HYPOTHESIS H65 (C30, C31): game started from a save — the runner's world must have been built with the save as
    /// its gam (VCEnabled, VCVariable, VCGameState). The start is the new game's; SetStartView takes the saved scene.
    /// Returns the saved (Node, Location, ViewPoint, View), None if the gam is not a save (then: a new game).
    pub fn load_game(&mut self) -> Option<[u32; 4]> {
        let start = self.w.load_saved_start();
        self.new_game();
        start
    }

    /// HYPOTHESIS H1661 (static): the application's cold start up to the main menu — the startup (FUN_0041b060, C33 /
    /// H69) and the scheduler's run of its queue, then (the front's intro movies, H1660, then) the main-menu object
    /// posted with no game running (the game-running flag 0: no Save item, no return.pic). The menu is not opened here:
    /// the front opens it after the intro ([`Runner::open_main_menu`]).
    pub fn boot(&mut self) {
        if self.w.rules.h69_startup && !self.started {
            self.started = true;
            self.w.startup();
            self.drain();
            self.run(10_000);
        }
        self.game_running = false;
    }

    /// H1661: the main-menu object posted — the menu shows, no game runs.
    pub fn open_main_menu(&mut self) {
        self.game_running = false;
        self.menu_open = true;
    }

    pub fn new_game(&mut self) {
        // C33 (H69): the application start runs before the menu, once
        if self.w.rules.h69_startup && !self.started {
            self.started = true;
            self.w.startup();
            self.drain();
            // the scheduler runs the start's queue (cursor pictures) before the menu appears
            self.run(10_000);
        }
        // C33 (H71): the game start — EvidenceIcon 0xdb79, VCGameState, VCTitle 0x12b (activated)
        if self.w.rules.h71_game_start {
            self.w.game_start();
            self.drain();
            return;
        }
        let tl = self.w.trigger_list_of(0x12b).expect("VCTitle 0x12b");
        self.w.register_tl(0x12b, tl);
        self.push_back(Cmd::Fire { owner: 0x12b, ev: event::ACTIVATION });
    }

    /// Register an owner from outside (an input for objects whose creation rule the core does not have, e.g. a
    /// trace-observed registration) and activate it.
    pub fn external_register(&mut self, owner: u32, tl: u32) {
        self.externals.push(owner);
        self.note_reg(owner);
        self.w.register_tl(owner, tl);
        self.push_back(Cmd::Fire { owner, ev: event::ACTIVATION });
    }

    /// Fire an event on a registered owner from outside (an input the core cannot generate yet, e.g. Timer
    /// Expiration — type-2 actions are not implemented).
    pub fn external_fire(&mut self, owner: u32, ev: u8) {
        self.push_back(Cmd::Fire { owner, ev });
    }

    fn note_reg(&mut self, owner: u32) {
        self.reg_n += 1;
        self.reg_seq.insert(owner, self.reg_n);
        self.regs.push(owner);
    }

    pub(super) fn push_back(&mut self, c: Cmd) {
        self.q.push_back(c);
    }

    /// C24: posted while a command runs → before the older pending commands, in post order.
    pub(super) fn post(&mut self, c: Cmd) {
        let i = self.ins.min(self.q.len());
        self.q.insert(i, c);
        self.ins += 1;
    }

    pub fn pending(&self) -> usize {
        self.q.len()
    }

    /// C26/C28: the command waits while its (type, kind) cell is raised.
    fn blocked(&self, c: &Cmd) -> bool {
        match c {
            Cmd::ActView(_) => self.blocks.blocked(0, super::sched::kind::K4),
            _ => false,
        }
    }

    /// Dispatch commands until the queue is empty, everything left is blocked, or `budget` commands ran.
    /// Returns the number dispatched.
    pub fn run(&mut self, budget: usize) -> usize {
        let mut n = 0;
        while n < budget {
            // H733: the main loop is busy
            match self.stall_until {
                Some(t) if self.now < t => break,
                Some(_) => self.stall_until = None,
                None => {}
            }
            let Some(i) = self.q.iter().position(|c| !self.blocked(c)) else {
                self.pda_resume_check();
                break;
            };
            let c = self.q.remove(i).unwrap();
            self.ins = 0;
            self.dispatch(c);
            n += 1;
        }
        self.dispatched += n;
        n
    }

    fn dispatch(&mut self, c: Cmd) {
        match c {
            Cmd::Action(m) => {
                let c = self.w.condition(&m);
                self.conds.push((m.action, m.trigger, c));
                let v = match c {
                    Some(v) => v,
                    None => {
                        let v = match self.uneval_seq.get_mut(&m.action).and_then(|q| q.pop_front()) {
                            Some(v) => v,
                            None => self.uneval.get(&m.action).copied().unwrap_or(false),
                        };
                        self.log.push(format!("condition of {:#x} not evaluable (operand kind 1/4) -> {v}", m.action));
                        v
                    }
                };
                if v {
                    self.execs.push(m.action);
                    // H1122: SetView's handler sees the queue behind the running message
                    self.w.set_view_queued =
                        self.q.iter().any(|c| matches!(c, Cmd::Action(x) if self.w.is_set_view_action(x.action)));
                    self.w.execute(m);
                    self.w.set_view_queued = false;
                }
            }
            Cmd::Fire { owner, ev } => self.fire(owner, ev),
            Cmd::Fires(v) => {
                for (i, (o, e)) in v.into_iter().enumerate() {
                    if i > 0 && e == event::DEACTIVATION && self.w.rules.h124_deact_own_dispatch {
                        // H124: the background's Deactivation is its own (later) dispatch — its posts go to the head
                        self.drain();
                        self.ins = 0;
                    }
                    self.fire(o, e);
                }
            }
            Cmd::ActView(v) => self.fire(v, event::ACTIVATION),
            Cmd::NavClick(n) => {
                self.w.fire(n, event::MOUSE_CLICK);
                self.w.click_nav(n);
            }
            Cmd::Click(o) => {
                // H701 (provisional): an IdeaIcon dropped on a CharView replaces the CharView's click
                // C107: the button-down on an IdeaIcon of the box (Mouse Click, drag, box state 0); its answer comes
                // with the button-up — at once for a click without a held button (headless steps)
                if self.w.idea_icon_press(o) {
                    if !self.button_down {
                        let t = self.release_target.take().unwrap_or(Some(o));
                        self.w.idea_icon_release(t);
                    }
                } else if !self.w.idea_click(o) {
                    self.w.fire(o, event::MOUSE_CLICK);
                    if self.w.rules.h623_ai_click && o == super::ai::AI_ICON {
                        self.w.ai_click(); // H623 (ai.rs)
                    }
                    if self.w.rules.h155_char_view_click {
                        self.w.char_view_clicked(o); // H155
                    }
                }
            }
            Cmd::GunClick => {
                // H821: the shot first — with a game-over video it plays to its end before the View's Mouse Click (fo-082:
                // 1.845 s), without one it is only restarted
                if self.w.gun_view_video() != 0 {
                    if !self.gun_shot_start(Some(Cmd::GunShotDone)) {
                        self.w.gun_view_click();
                    }
                } else {
                    self.gun_shot_start(None);
                    self.w.gun_view_click();
                }
            }
            Cmd::GunShotDone => {
                self.w.gun_view_click();
            }
            Cmd::MovieHot { id, movie } => self.movie_hot_click(id, movie),
            Cmd::MouseAt(x, y) => {
                // H601: dispatched like the user's mouse command (command type 2 → hit test): the click's
                // own commands run next, ahead of the older pending ones
                let n = self.q.len();
                if let Some(c) = self.hit(x, y) {
                    self.click_clickable(&c);
                }
                let mut new: Vec<Cmd> = self.q.drain(n..).collect();
                while let Some(c) = new.pop() {
                    self.q.push_front(c);
                }
            }
            Cmd::InvClick(item) => {
                // H152 (the original's inventory click, FUN_0049c180): dialogue box removed, held object dropped,
                // selection, then Mouse Click
                if self.w.dbox_layout().is_some() {
                    self.w.dbox_close();
                }
                if self.w.rules.h362_drop_resets_inv_var {
                    self.drop_held(); // the held object dropped: cursor removed, variable reset (H362)
                }
                let old = self.inv_selected.replace(item);
                let active = self.active.clone();
                self.w.inventory_select(old, item, &|o| active.contains(&o));
                self.w.fire(item, event::MOUSE_CLICK);
            }
            Cmd::Timer(id) => {
                for o in self.w.timer_subscribers(id) {
                    if self.w.rules.h261_timer_periodic_alive && !self.active.contains(&o) {
                        continue; // H261 (a): the command reaches live objects only
                    }
                    self.w.timer_ctx_next(id); // H90: the fire's event context
                    self.fire(o, event::TIMER);
                }
            }
            Cmd::RestoreEnd => self.w.end_restore(),
            Cmd::Player(_) => {
                self.w.run_player();
            }
            Cmd::IconOff(icon) => {
                self.w.icon_off(icon);
            }
            Cmd::CharViewPlayer(cv) => {
                self.w.char_view_player(cv);
            }
            Cmd::Dbox(_) => {
                self.w.dbox_run();
            }
            Cmd::IdeaDrop(t) => self.w.idea_icon_release(t),
            Cmd::DboxClose => {
                self.w.dbox_close();
            }
            Cmd::DboxSeqEnd(conv) => {
                // H1123: cell (10,4) — a player started by the posts that ran ahead of this command (the last asset's Deactivation)
                self.reshow_wait = self.plays[self.seq_end_mark.min(self.plays.len())..]
                    .iter()
                    .map(|p| p.1)
                    .filter(|a| self.media.contains(a))
                    .collect();
                self.w.media_busy = !self.reshow_wait.is_empty();
                self.w.dbox_seq_end(conv);
                self.w.media_busy = false;
            }
            Cmd::DboxReshow => self.w.dbox_reshow_after_media(),
            Cmd::DboxShow(list) => self.w.dbox_show_cmd(list),
            Cmd::Held(id) => self.w.h76_release_id(id),
            Cmd::World(c) => self.w.run_cmd(c),
        }
        self.device_requests(); // H165 / H167 / H168
        self.drain();
    }

    fn fire(&mut self, owner: u32, ev: u8) {
        if ev == event::ACTIVATION || ev == event::DEACTIVATION {
            self.lifecycle.push((owner, ev));
        }
        match ev {
            event::ACTIVATION => {
                self.active.retain(|&o| o != owner);
                self.active.push(owner);
                self.activations.push(owner);
            }
            event::DEACTIVATION => {
                self.active.retain(|&o| o != owner);
                // H125: a player removed by the world (the SwapDisc travel removes every media player) leaves
                // the loop set and the FIFO (a media end already took it out of the FIFO: no-op)
                self.looped.retain(|&a| a != owner);
                if let Some(i) = self.media.iter().position(|&a| a == owner) {
                    self.media_remove(i);
                }
            }
            _ => {}
        }
        if self.w.rules.h150_icon_bars && (ev == event::ACTIVATION || ev == event::DEACTIVATION) {
            self.icon_lifecycle(owner, ev);
        }
        if self.backgrounds.contains(&owner) {
            match ev {
                event::ACTIVATION => {
                    self.background = Some(owner);
                    self.movie_start.insert(owner, self.now); // H1281: a View movie with hotspots (the goon Views)
                }
                event::DEACTIVATION if self.background == Some(owner) => {
                    self.background = None;
                    self.movie_start.remove(&owner);
                }
                _ => {}
            }
        }
        self.w.fire(owner, ev);
        if ev == event::ROLL_INTO {
            self.w.char_view_rolled_into(owner); // H820
        }
    }

    /// Port viewer (not game behaviour): show InterfaceLayout `layout` as the engine's type-5 show (H70), e.g. a
    /// close-up the bot's saves do not reach (the journal 0xb130, the Bible 0xec66 — front::doc tests, `xf --layout`).
    pub fn viewer_show_layout(&mut self, layout: u32) {
        self.w.engine_layout(layout, 0);
        self.drain();
    }

    /// Feed the world's new outputs back (registrations, fires, posts) and empty its internal queue (the runner owns
    /// the order).
    pub(super) fn drain(&mut self) {
        while self.seen < self.w.out.len() {
            let o = self.w.out[self.seen].clone();
            self.seen += 1;
            match o {
                Out::Post { msg, .. } => {
                    self.posts.push(msg);
                    self.post(Cmd::Action(msg));
                }
                Out::Reg { owner, tl, cause } => {
                    self.note_reg(owner);
                    match tl {
                        // H74: the CharView's media wrapper (TL 0) is a second object; the CharView keeps its TL
                        _ if matches!(cause, RegCause::CharViewPlayer { .. }) => {}
                        Some(t) => self.w.register_tl(owner, t),
                        None => self.log.push(format!("register {owner:#x}: TriggerList rule unknown (C20)")),
                    }
                    if matches!(
                        cause,
                        RegCause::PlayAsset { .. } | RegCause::ConversationPlay { .. } | RegCause::CharViewAsset { .. }
                    ) {
                        self.plays.push((self.now, owner, cause.clone()));
                    }
                    match cause {
                        RegCause::Background { .. } => {
                            self.backgrounds.insert(owner);
                        }
                        RegCause::PlayAsset { .. } | RegCause::ConversationPlay { .. } => self.media_push(owner),
                        // H1601: a restored player plays from its saved position
                        RegCause::RestoredPlayer { .. } if self.w.rules.h1601_restored_player_plays => {
                            if let Some(pos) = self.w.take_restored_position(owner) {
                                self.start_offset.insert(owner, pos as u64);
                            }
                            self.media_push(owner);
                        }
                        // H263: the CharView wrapper's player ends at its media end like any other
                        RegCause::CharViewAsset { .. } if self.w.rules.h263_char_view_player_end => self.media_push(owner),
                        RegCause::Explorable { action: None } => {
                            self.entry_explorables.insert(owner);
                        }
                        RegCause::InventoryEnable { .. } => {
                            self.inv_enabled.insert(owner); // H170
                        }
                        _ => {}
                    }
                }
                Out::Fire { owner, event: ev } => {
                    let after_view_deact = self.ins > 0
                        && matches!(self.q.get(self.ins - 1),
                            Some(Cmd::Fire { owner: v, ev: event::DEACTIVATION }) if self.w.h.has(class::VIEW, *v));
                    if self.backgrounds.contains(&owner) && ev == event::DEACTIVATION && after_view_deact {
                        // A1: the old View's background goes with the View's Deactivation step
                        if let Some(Cmd::Fire { owner: v, .. }) = self.q.get(self.ins - 1).cloned() {
                            self.q[self.ins - 1] = Cmd::Fires(vec![(v, event::DEACTIVATION), (owner, ev)]);
                        }
                    } else if self.backgrounds.contains(&owner) && !self.w.rules.h89_queue_model {
                        self.fire(owner, ev); // A1: background Act/Deact synchronous (H89: its cmd 0, queued)
                    } else if ev == event::ACTIVATION && self.w.h.has(class::VIEW, owner) {
                        self.post(Cmd::ActView(owner));
                    } else if self.w.rules.h76_entry_hotspot_order || self.w.rules.h89_queue_model {
                        // H76: entry Activations come released by `Cmd::Held` (the hold's place replaces A2)
                        self.entry_explorables.remove(&owner);
                        self.post(Cmd::Fire { owner, ev });
                    } else if ev == event::ACTIVATION
                        && self.w.hotspots.contains(&owner)
                        && matches!(self.last_hotspot_src, Some(HotspotSrc::ViewEntry { .. }))
                    {
                        self.push_back(Cmd::Fire { owner, ev }); // A2
                    } else if ev == event::ACTIVATION && self.entry_explorables.remove(&owner) {
                        self.push_back(Cmd::Fire { owner, ev }); // A2 for a View entry's Explorables (fo-005 139.58)
                    } else {
                        self.post(Cmd::Fire { owner, ev });
                    }
                }
                Out::Hotspot { src, .. } => self.last_hotspot_src = Some(src),
                // H59: a type-2 action (re)starts or stops a timer; expiries come from the clock ([`Runner::advance`])
                Out::Timer { id, start } => {
                    self.timers.retain(|t| t.id != id);
                    self.q.retain(|c| *c != Cmd::Timer(id)); // H59: pending expirations dropped (mode 1)
                    if let Some((ms, periodic)) = start {
                        let ms = ms as u64;
                        let mut t = Timer { id, due: self.now + ms, period: periodic.then_some(ms.max(1)), paused: None };
                        if self.no_native_timer(ms) {
                            // H1203: timeSetEvent refused the delay — the timer stays listed with no handle (as paused)
                            t.paused = Some(ms);
                        }
                        self.timers.push(t);
                    }
                }
                Out::Transition { view, kind, .. } => {
                    self.transitions.push((view, kind));
                    if self.w.rules.h890_timer_pause && (1..=4).contains(&kind) {
                        // H890: the transition effect (FUN_00464330) pauses the timers, then resumes them (fo-118: a
                        // re-arm after every kind-1..3 transition, the 45-s timer fired 45.0 s after the last one)
                        self.timers_pause();
                        self.timers_resume();
                    }
                }
                Out::Restore { on: true } => self.push_back(Cmd::RestoreEnd),
                Out::PlayerPost { asset } => self.post(Cmd::Player(asset)),
                Out::PlayerStop { asset, .. } => self.player_stop(asset),
                // H1060: the travel's removal of every player — those of the FIFO (playing or queued) get their
                // Deactivation like the world's looping ones (taken out of the FIFO when it runs, `fire`)
                Out::TravelStopPlayers => {
                    let gun = self.gun_shot.as_ref().map(|g| g.0);
                    let mut v: Vec<u32> = self.media.iter().copied().filter(|&a| Some(a) != gun).collect();
                    v.dedup();
                    for a in v {
                        self.post(Cmd::Fire { owner: a, ev: event::DEACTIVATION });
                    }
                }
                Out::IconOff { icon } => self.post(Cmd::IconOff(icon)),
                // H203–H206: the icon's state command places / takes it out (approximated at once)
                Out::IconState { icon, state } if self.w.rules.h150_icon_bars => {
                    if state {
                        self.place_icon(icon);
                    } else {
                        self.icon_bars.remove(icon);
                    }
                }
                Out::CharViewPlayer { char_view } => self.post(Cmd::CharViewPlayer(char_view)),
                Out::DboxPost { list } => self.post(Cmd::Dbox(list)),
                Out::DboxSeqEndPost { conv } => {
                    self.seq_end_mark = self.plays.len();
                    self.post(Cmd::DboxSeqEnd(conv))
                }
                Out::DboxShowPost { list } => self.post(Cmd::DboxShow(list)),
                Out::Command(c) => self.post(Cmd::World(c)),
                Out::MouseCommand { x, y } => self.post(Cmd::MouseAt(x, y)),
                // H182 / H191: the game ended / a menu object was posted — the front-end decides what follows
                Out::GameEnd { action, rewind } => {
                    self.log.push(format!("action {action:#x}: H182 game end (rewind box {rewind})"));
                    if rewind && self.w.rules.h1700_rewind_box {
                        // H1700 (C244): the box (FUN_004d66e0) — its null-owner REG, then its modal loop
                        self.rewind_box = true;
                        self.regs.push(0);
                    }
                    if !rewind && self.w.rules.h823_game_end_main_menu {
                        // H823 / C71: the game-running flags cleared, the game-end object → the main menu (fo-091 0xc0f2)
                        self.game_running = false;
                        self.menu_open = true;
                    }
                }
                Out::SaveGameState { action } => {
                    // H1700 (C245): the rewind snapshot — the whole game state as a save over the session's gam
                    let base = crate::save::SaveState::from_gam(self.w.gam);
                    self.rewind_snap = Some(self.save_state(&base));
                    self.log.push(format!("action {action:#x}: H1700 SaveGameState: rewind snapshot at View {:x?}", self.w.view));
                }
                Out::Freeze { hotspot } => {
                    // H1703: the freeze pauses the timers; the front stops the game's clock
                    self.timers_pause();
                    self.frozen = true;
                    self.log.push(format!("H1703: hotspot {hotspot}: the game frozen"));
                }
                Out::SaveGame { .. } | Out::LoadGame { .. } => {
                    self.log.push(format!("H403: {o:?}"));
                    self.save_req.push(o.clone());
                }
                Out::MainMenu { action } => {
                    self.menu_open = true;
                    self.menu_screen = false;
                    self.log.push(format!("action {action:#x}: H191 menu object posted"));
                }
                Out::Stall { action, ms } => {
                    self.stall_until = Some(self.now + ms as u64);
                    self.log.push(format!("action {action:#x}: H733 main loop busy {ms} ms"));
                }
                Out::Unimplemented { action, what } => self.log.push(format!("action {action:#x}: {what}")),
                Out::UnknownType { action, typ } => self.log.push(format!("action {action:#x}: type {typ:#x} not implemented")),
                _ => {}
            }
        }
        // H76: a new hold of the world gets its place now, behind everything this command posted (incl. the background's
        // synchronous Act/Deact posts, A1)
        if let Some(id) = self.w.h76_held() {
            if self.marked != Some(id) {
                self.marked = Some(id);
                self.post(Cmd::Held(id));
            }
        }
        while self.w.next().is_some() {}
        self.w.out.clear();
        self.seen = 0;
    }

    /// Robustness for a long interactive session (the window / web loop, not the headless tools): the diagnostic
    /// histories (execs, conds, posts, lifecycle, activations, regs, plays, media_log, transitions, externals, log) only
    /// grow — a periodic timer in a scene adds ~16 conditions / posts a second (monkey: fo-047-end, 180 000 in two game
    /// hours). Each one longer than `max` keeps its newest `max / 2` entries. Nothing in the core reads them back.
    pub fn trim_history(&mut self, max: usize) {
        fn trim<T>(v: &mut Vec<T>, max: usize) {
            if v.len() > max {
                v.drain(..v.len() - max / 2);
            }
        }
        trim(&mut self.execs, max);
        trim(&mut self.conds, max);
        trim(&mut self.posts, max);
        trim(&mut self.lifecycle, max);
        trim(&mut self.activations, max);
        trim(&mut self.regs, max);
        trim(&mut self.plays, max);
        trim(&mut self.media_log, max);
        trim(&mut self.transitions, max);
        trim(&mut self.externals, max);
        trim(&mut self.log, max);
    }

    /// Advance the runner clock to `now` (ms): every timer due by then expires — a Timer command goes to the back of the
    /// queue (posted from outside any dispatch), a periodic timer is re-armed, a one-shot one removed (H59).
    pub fn advance(&mut self, now: u64) {
        self.now = self.now.max(now);
        self.w.clock = self.now; // display aid (H312)
        let mut due: Vec<Timer> = self.timers.iter().filter(|t| t.paused.is_none() && t.due <= self.now).copied().collect();
        due.sort_by_key(|t| t.due);
        for t in due {
            self.push_back(Cmd::Timer(t.id));
            match t.period {
                Some(p) => {
                    if let Some(x) = self.timers.iter_mut().find(|x| x.id == t.id) {
                        x.due = self.now + p;
                    }
                }
                None => self.timers.retain(|x| x.id != t.id),
            }
        }
        self.panel_tick(); // H170
        self.tool_comment_tick(); // H451
    }

    /// H1700: the "Try again?" box answered. NO (C255) = H823's game end: no game running, the game-end object
    /// (H895), the main menu (H823). YES only closes the box here — its Load of the rewind snapshot is the front's
    /// (`Session::rewind_yes`: a new core). False if no box shows.
    pub fn rewind_answer(&mut self, yes: bool) -> bool {
        if !std::mem::take(&mut self.rewind_box) {
            return false;
        }
        self.log.push(format!("H1700: \"Try again?\" {}", if yes { "YES" } else { "NO" }));
        if !yes {
            self.w.px.game_end_object = true;
            self.game_running = false;
            self.menu_open = true;
        }
        true
    }

    /// H1703: the menu's Return unfreezes (the timers re-armed).
    pub fn unfreeze(&mut self) {
        if std::mem::take(&mut self.frozen) {
            self.timers_resume();
            self.log.push("H1703: the menu's Return: the game unfrozen".into());
        }
    }

    /// HYPOTHESIS H2062 (static; the original's key handler FUN_0046c390): Alt+A / M / R / T / V flip a boolean
    /// Preference — the port's prefs are the RegPreferences variables (H189), so the variable `var` is negated
    /// (type 2); on the menu screen too (the next New Game takes it with the host's Preferences, C86). Returns the new
    /// value; None when the rule is off or the variable is missing.
    pub fn alt_toggle_pref(&mut self, var: u32) -> Option<bool> {
        if !self.w.rules.h2060_alt_keys {
            return None;
        }
        let on = self.w.vars.get(var)? == 0;
        let t = self.w.vars.typ(var).unwrap_or(2);
        self.w.vars.set_native(var, Some(on as i32), t);
        self.log.push(format!("H2062: Alt key: var {var:#x} := {}", on as i32));
        if var == super::ai::PREF_AI {
            self.ai_icon_refresh(); // C299: the AI icon refreshed after the flip
        }
        Some(on)
    }

    /// HYPOTHESIS H2063 (static; the key handler's Alt+P case): Alt+P — on: H1703's freeze (the timers paused) and
    /// layout 0xf5e8 ("Pause Game.", x.pff #483 full screen) shown as an engine type-5 show (mode 0); off: the layout
    /// hidden (mode 1) and the game unfrozen. False when nothing changed
    /// (rule off, no game, the menu shows, or already frozen by the game itself, H1520: hotspots 100 / 500 / 600).
    pub fn alt_pause(&mut self, on: bool) -> bool {
        if !self.w.rules.h2060_alt_keys || !self.game_running || self.menu_open || self.rewind_box {
            return false;
        }
        if on {
            if self.frozen {
                return false;
            }
            self.timers_pause();
            self.frozen = true;
            self.w.engine_layout(ALT_PAUSE_LAYOUT, 0);
        } else {
            self.w.engine_layout(ALT_PAUSE_LAYOUT, 1);
            if std::mem::take(&mut self.frozen) {
                self.timers_resume();
            }
        }
        self.log.push(format!("H2063: Alt+P: {}", if on { "paused (layout 0xf5e8)" } else { "resumed" }));
        self.drain();
        true
    }

    /// HYPOTHESIS H2064 (static; the key handler's Alt+S case): Alt+S opens the save screen (layout 0x14269) as the
    /// menu's Save does, but not from the menu — its Save / Cancel bring no menu back (H140's from-menu flag 0). False
    /// when the rule is off, no game runs, the menu or a save / load screen shows.
    pub fn alt_save_screen(&mut self) -> bool {
        let shown = self.w.layouts().iter().any(|l| l.0 == 0x14269 || l.0 == 0x142ca);
        if !self.w.rules.h2060_alt_keys || !self.game_running || self.menu_open || self.rewind_box || self.frozen || shown {
            return false;
        }
        self.w.menu_screen(0x14269, false);
        self.log.push("H2064: Alt+S: the save screen".into());
        self.drain();
        true
    }

    /// H1700: the runner's histories (transitions, posts, executions, conditions, registrations, lifecycle, activations,
    /// log), taken out — a rewind's new core continues them ([`Runner::continue_history`]: e2e windows, tests)
    pub fn take_history(&mut self) -> History {
        History {
            transitions: std::mem::take(&mut self.transitions),
            posts: std::mem::take(&mut self.posts),
            execs: std::mem::take(&mut self.execs),
            conds: std::mem::take(&mut self.conds),
            regs: std::mem::take(&mut self.regs),
            lifecycle: std::mem::take(&mut self.lifecycle),
            activations: std::mem::take(&mut self.activations),
            log: std::mem::take(&mut self.log),
        }
    }

    /// H1700: `h` (an earlier core's histories) in front of this core's own.
    pub fn continue_history(&mut self, h: History) {
        fn pre<T>(new: &mut Vec<T>, mut old: Vec<T>) {
            old.append(new);
            *new = old;
        }
        pre(&mut self.transitions, h.transitions);
        pre(&mut self.posts, h.posts);
        pre(&mut self.execs, h.execs);
        pre(&mut self.conds, h.conds);
        pre(&mut self.regs, h.regs);
        pre(&mut self.lifecycle, h.lifecycle);
        pre(&mut self.activations, h.activations);
        pre(&mut self.log, h.log);
    }

    /// H890 (the original's timer pause / resume, FUN_0048e640): every running timer is killed and keeps its delay — a
    /// periodic one its period, a one-shot one its remaining time (if > 0; else its delay is unchanged: a native expiry
    /// the core already queued).
    pub(super) fn timers_pause(&mut self) {
        let now = self.now;
        for t in self.timers.iter_mut().filter(|t| t.paused.is_none()) {
            t.paused = Some(match t.period {
                Some(p) => p,
                None => t.due.saturating_sub(now),
            });
        }
    }

    /// H890: every paused timer is re-armed with its stored delay from now.
    pub(super) fn timers_resume(&mut self) {
        let now = self.now;
        let rule = self.w.rules.h1203_timer_max_delay;
        for t in self.timers.iter_mut() {
            match t.paused {
                // H1203: the re-arm is refused again — still no handle (fo-168 206.496, fo-170 191.418)
                Some(d) if rule && d > TIMER_MAX_DELAY => {}
                Some(d) => {
                    t.paused = None;
                    t.due = now + d;
                }
                None => {}
            }
        }
    }

    /// H1203 (static; the original's timer arm, FUN_00428d50: `timeSetEvent` with the delay, the handle stored only if
    /// it returns non-zero; CrossOver's winmm.dll `timeSetEvent` returns 0 for a delay outside 1..=0xffff ms,
    /// `timeGetDevCaps` wPeriodMax = 0xffff): a timer armed with a delay above 65535 ms never runs. Only delays >
    /// 0xffff are modelled (the 0 case has no trace).
    fn no_native_timer(&self, ms: u64) -> bool {
        self.w.rules.h1203_timer_max_delay && ms > TIMER_MAX_DELAY
    }

    /// H890 (FUN_004a4fa0, at the end of each scheduler pass): after a PDA pause, once no layout is shown the flag is
    /// cleared and the timers resumed.
    fn pda_resume_check(&mut self) {
        if self.w.dev.pda_paused && self.w.layouts().is_empty() {
            self.w.dev.pda_paused = false;
            self.timers_resume();
        }
    }

    /// One-shot timers still running (H451's engine timer included).
    pub fn one_shot_timers(&self) -> usize {
        // H1700 / H1703: the box's modal loop / the frozen game — no timer expires
        if self.rewind_box || self.frozen {
            return 0;
        }
        // H1203: a timer with no native handle never expires — nothing to wait for
        self.timers.iter().filter(|t| t.period.is_none() && !t.paused.is_some_and(|d| self.no_native_timer(d))).count()
            + self.tool_comment_at.is_some() as usize
    }

    // ---- media player (A3, C23, C26) ---------------------------------------------------------------------------

    fn media_push(&mut self, asset: u32) {
        let before = self.started_fifo();
        self.media.push_back(asset);
        self.media_starts(&before);
    }

    /// H280: whether a player holds the FIFO (the ones behind it wait): every player under A3; with H280 only a video
    /// kind (H51 inputs; unknown kind → holds, as A3)
    fn media_holds(&self, asset: u32) -> bool {
        !self.w.rules.h280_sound_no_hold || media_inputs_h51(self.w.h, asset).map_or(true, |(video, _)| video)
    }

    /// The players playing now, in FIFO order: A3 the front only; H280 every player from the front up to and including
    /// the first one that holds the FIFO (a video kind) — sounds before it play alongside.
    pub fn started(&self) -> Vec<u32> {
        let mut out = self.started_fifo();
        // H821: the gun's shot plays alongside (its player is the gun's own, not in the FIFO)
        if let Some((a, _)) = self.gun_shot {
            if !out.contains(&a) {
                out.push(a);
            }
        }
        out
    }

    fn started_fifo(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for &a in &self.media {
            out.push(a);
            if self.media_holds(a) {
                break;
            }
        }
        out
    }

    /// H821 (the original's gun-shot player, FUN_004cb440): the gun's shot player (AssetRef [`World::gun_shot_asset`])
    /// (re)started; `then` runs at its end (the shot's wait loop), None = not waited for. False if there is no shot
    /// (rule off / no AssetRef).
    pub(super) fn gun_shot_start(&mut self, then: Option<Cmd>) -> bool {
        let asset = self.w.gun_shot_asset();
        if !self.w.rules.h821_gun_shot_sound || asset == 0 {
            return false;
        }
        if let Some((_, Some(c))) = self.gun_shot.take() {
            self.push_back(c);
        }
        self.shots.push((self.now, asset));
        self.gun_shot = Some((asset, then));
        true
    }

    /// State 1 (C26 block switch) for the players started since `before`.
    fn media_starts(&mut self, before: &[u32]) {
        for a in self.started_fifo() {
            if !before.contains(&a) {
                self.media_state(a, 1);
            }
        }
    }

    /// Take FIFO entry `i` out: state 0 if it was playing, then state 1 for the players that start because of it.
    fn media_remove(&mut self, i: usize) {
        let before = self.started_fifo();
        let Some(a) = self.media.remove(i) else { return };
        if before.contains(&a) {
            self.media_state(a, 0);
        }
        self.media_starts(&before);
    }

    fn media_state(&mut self, asset: u32, state: u32) {
        self.media_log.push((self.now, asset, state));
        if state == 1 {
            self.movie_start.insert(asset, self.now); // H1281
        } else {
            self.movie_start.remove(&asset);
        }
        // H51 (confirmed C27): video kind from the file extension, f090 = f40 & 0x40
        if let Some((video, f090)) = media_inputs_h51(self.w.h, asset) {
            self.blocks.media(state, video, f090);
        }
    }

    /// The asset shown as playing: the head of the media FIFO (A3); with H280 the video among the started players if
    /// one plays, else the head. None iff nothing is queued or playing.
    /// The video-kind player playing now (H280: a sound alongside it is not one), if any.
    pub fn video_playing(&self) -> Option<u32> {
        self.started().into_iter().find(|&a| media_inputs_h51(self.w.h, a).map_or(true, |(video, _)| video))
    }

    pub fn playing(&self) -> Option<u32> {
        let st = self.started();
        st.iter().copied().find(|&a| self.media_holds(a)).or_else(|| st.first().copied())
    }

    /// H1601: the position (ms) `asset`'s media starts at — a restored player's saved position, else 0 (taken once)
    pub fn take_start_offset(&mut self, asset: u32) -> u64 {
        self.start_offset.remove(&asset).unwrap_or(0)
    }

    /// Assets whose looping player (H100) passed its media end and still exists — the front-end plays them again.
    pub fn looping(&self) -> Vec<u32> {
        self.looped.iter().copied().filter(|&a| self.w.player_loops(a) && self.active.contains(&a)).collect()
    }

    /// The front-end's clock says `asset` ended: the next media starts, then its Deactivation fires (C23). HYPOTHESIS
    /// H100: a looping player (type 1/7 mode 2) is not removed at media end (the media end checks its looping flag) —
    /// no Deactivation (fo-023: 0xcc41 never deactivated); it leaves the FIFO (A3 has one player; in fo-023 0x57f4
    /// plays at once while 0xcc41 loops) and goes to [`Runner::looping`].
    pub fn media_ended(&mut self, asset: u32) {
        if self.gun_shot.as_ref().is_some_and(|g| g.0 == asset) {
            // H821: the gun's shot ended — no removal, no Deactivation (the player stays with the gun); the shot's
            // wait returns
            if let Some((_, Some(c))) = self.gun_shot.take() {
                self.push_back(c);
            }
            return;
        }
        let Some(i) = self.media.iter().position(|&a| a == asset) else { return };
        self.media_remove(i);
        if self.w.player_loops(asset) {
            if !self.looped.contains(&asset) {
                self.looped.push(asset);
            }
            return;
        }
        self.push_back(Cmd::Fire { owner: asset, ev: event::DEACTIVATION });
        // H1123: after the last player the View's reshow command runs (behind the Deactivation's posts)
        let waited = self.reshow_wait.contains(&asset);
        self.reshow_wait.retain(|&a| a != asset);
        if waited && self.reshow_wait.is_empty() && self.w.dbox_reshow_waiting {
            self.push_back(Cmd::DboxReshow);
        }
    }

    /// HYPOTHESIS H100 mode 3: a stopped player (in the FIFO, or a looping one past its media end) ends like a media end:
    /// it leaves the FIFO / the loop set and its Deactivation goes to the back of the queue (fo-025: Deact 0xcc41 after
    /// the actions 0xd281's trigger had queued). The world has already cleared its looping flag.
    fn player_stop(&mut self, asset: u32) {
        if let Some(i) = self.looped.iter().position(|&a| a == asset) {
            self.looped.remove(i);
            self.push_back(Cmd::Fire { owner: asset, ev: event::DEACTIVATION });
        } else if self.media.contains(&asset) {
            self.media_ended(asset);
        }
    }

    // ---- input --------------------------------------------------------------------------------------------------

    /// The pointer moved onto `owner` (None = onto nothing): Roll Out Of the previous owner, Roll Into the new one
    /// (events 5 / 4, C19). On the traced sessions these events posted nothing.
    pub fn hover(&mut self, owner: Option<u32>) {
        if owner == self.hovered {
            return;
        }
        if let Some(o) = self.hovered.take() {
            if self.active.contains(&o) {
                self.push_back(Cmd::Fire { owner: o, ev: event::ROLL_OUT_OF });
            }
        }
        if let Some(o) = owner {
            self.push_back(Cmd::Fire { owner: o, ev: event::ROLL_INTO });
        }
        self.hovered = owner;
    }

    /// A click that hit `c` (the topmost clickable at the point): the dialogue box's body (H136) swallows it, its close
    /// box closes it (H137); anything else → [`Runner::click`].
    pub fn click_clickable(&mut self, c: &Clickable) -> bool {
        if c.cls == class::CONVERSATION_LIST {
            match c.kind {
                super::dbox::KIND_CLOSE => self.push_back(Cmd::DboxClose),
                super::dbox::KIND_TAB_TALK => self.w.dbox_select_tab(0), // H502
                super::dbox::KIND_TAB_HISTORY => self.w.dbox_select_tab(1),
                // H1810 / H1812: the scroller (display only)
                super::dbox::KIND_SCROLL_UP => self.w.dbox_scroll_step(super::dbox::Scroll::LineUp),
                super::dbox::KIND_SCROLL_DOWN => self.w.dbox_scroll_step(super::dbox::Scroll::LineDown),
                // KIND_SCROLL_BAR needs the point: `Session::click_hit` → `World::dbox_bar_press`
                _ => {}
            }
            return true;
        }
        self.click(c.owner)
    }

    /// The pointer left the picture (e.g. to the game menu's buttons): Roll Out Of the hovered owner, and the next point in
    /// the inventory strip enters it from outside (H361).
    pub fn pointer_off(&mut self) {
        self.hover(None);
        self.last_hover = None;
    }

    /// The front's left button went down (a click follows through [`Runner::click`]); the button-up is
    /// [`Runner::button_release`]. Only a dragged IdeaIcon (C107) waits for it.
    pub fn button_press(&mut self) {
        self.button_down = true;
        self.release_target = None;
    }

    /// The front's left button went up over `target` (the topmost clickable owner under the pointer): a dragged IdeaIcon
    /// is dropped there (C107 / H921).
    pub fn button_release(&mut self, target: Option<u32>) {
        if !self.button_down {
            return;
        }
        self.button_down = false;
        if self.w.idea_drag.is_some() {
            self.push_back(Cmd::IdeaDrop(target));
        } else if self.q.iter().any(|c| matches!(c, Cmd::Click(_))) {
            self.release_target = Some(target); // the press's command has not run yet
        }
    }

    /// User click on an owner: a nav-hotspot of the current View → NavClick, the shown dialogue box (its VCConversationList)
    /// → close (H137), any other active owner → Mouse Click (a line of the dialogue box: H135 in the world's fire).
    pub fn click(&mut self, owner: u32) -> bool {
        if self.engine_click(owner) {
            return true; // H169 / H170
        }
        if self.w.dbox_layout().is_some_and(|l| l.list == owner) && !self.w.hotspots.contains(&owner) {
            self.push_back(Cmd::DboxClose);
            true
        } else if self.w.hotspots.contains(&owner) {
            self.push_back(Cmd::NavClick(owner));
            true
        } else if self.w.rules.h152_inv_click && self.w.h.has(class::INVENTORY, owner) && self.active.contains(&owner) {
            // H152: a click on the selected item does nothing
            if self.inv_selected != Some(owner) {
                self.push_back(Cmd::InvClick(owner));
            }
            true
        } else if self.active.contains(&owner) {
            self.push_back(Cmd::Click(owner));
            true
        } else {
            false
        }
    }

    /// What can be clicked now, topmost first: active nav-hotspots of the current View (rect of the VCNav's HotSpot) and
    /// active owners that have a region (VCExplorable, VCInterfaceItem).
    /// A4 (hypothesis for overlapping rects): topmost = most recently *registered*. "Most recently activated" is REFUTED by
    /// fo-009: in View 0x4cc Explorable 0x7fca (rect 85,102–465,277, Act 337.10) covers drawer item 0xe4bf (Act 331.25),
    /// yet the user's click at 340.55 went to 0xe4bf — registered later (330.31) than 0x7fca (328.73).
    pub fn clickables(&self) -> Vec<Clickable> {
        let h = self.w.h;
        let region = |cls: u32, id: u32| match h.read(cls, id).and_then(|r| r.field("region").cloned()) {
            Some(Value::Region { kind, rect }) => Some((kind, rect)),
            _ => None,
        };
        let mut out = Vec::new();
        let mut order: Vec<u32> = self.active.iter().rev().copied().collect();
        order.sort_by_key(|o| std::cmp::Reverse(self.reg_seq.get(o).copied().unwrap_or(0))); // stable
        for o in order {
            if self.w.hotspots.contains(&o) {
                let hs = h.read(class::NAV, o).map(|r| r.u32("hotSpot")).unwrap_or(0);
                if let Some((kind, rect)) = region(class::HOTSPOT, hs) {
                    out.push(Clickable { owner: o, cls: class::NAV, rect, kind });
                }
            } else {
                for cls in [class::EXPLORABLE, class::INTERFACE_ITEM] {
                    if let Some((kind, rect)) = region(cls, o) {
                        // H179: RelocateIFace / RelocateIFacID moved the item's hit rect
                        let rect = match self.w.px.item_rects.get(&o) {
                            Some(&r) if cls == class::INTERFACE_ITEM => r,
                            _ => rect,
                        };
                        out.push(Clickable { owner: o, cls, rect, kind });
                    }
                }
            }
        }
        if self.w.rules.h130_hit_chain {
            // H130 (the original's hit-test chain, FUN_00519e40): InterfaceItems of layouts not set as View, then of
            // layouts set as View — each: layouts from the most recently shown, inside one smallest key, ties container
            // order (H121); then Explorables, then nav-hotspots, each earliest added to its kind list (cmd 11 =
            // Activation order) first. The View-band test (y in 90..330) is in `hit`
            let mut ordered = Vec::new();
            for as_view in [false, true] {
                for (l, layout_items) in self.w.layouts().iter().rev() {
                    if self.w.layout_as_view(*l) != as_view {
                        continue;
                    }
                    let mut v: Vec<&Clickable> = layout_items
                        .iter()
                        .filter_map(|(i, _)| out.iter().find(|c| c.cls == class::INTERFACE_ITEM && c.owner == *i))
                        .collect();
                    v.sort_by_key(|c| self.w.item_key(c.owner)); // stable: ties keep container order
                    ordered.extend(v.into_iter().cloned());
                }
                if !as_view {
                    // H2170 (category 3): the held camera, after the InterfaceItems of non-View layouts
                    ordered.extend(self.camera_clickable());
                    // H136: the dialogue box's category comes after the non-View layouts' items and before the View ones'
                    ordered.extend(self.dbox_clickables());
                    // H1281 (category 6): movie-hotspot targets (kind 0x1d)
                    ordered.extend(self.movie_hot_clickables());
                    // H452 (category 7): a playing video player's display rect, before the View layouts' items
                    ordered.extend(self.video_player_clickable());
                }
            }
            // items of no shown layout (not expected): after, earliest registered first (H77)
            let mut loose: Vec<Clickable> = out
                .iter()
                .filter(|c| c.cls == class::INTERFACE_ITEM && !ordered.iter().any(|o| o.owner == c.owner))
                .cloned()
                .collect();
            loose.reverse();
            ordered.extend(loose);
            if self.w.rules.h155_char_view_click {
                // H155: CharViews (kind 6) come before Explorables in the View band, list order; rect = f2c
                for &o in self.w.char_views() {
                    if !self.active.contains(&o) {
                        continue;
                    }
                    if let Some(Value::Region { kind, rect }) = h.read(class::CHAR_VIEW, o).and_then(|r| r.field("f2c").cloned()) {
                        ordered.push(Clickable { owner: o, cls: class::CHAR_VIEW, rect, kind });
                    }
                }
            }
            for cls in [class::EXPLORABLE, class::NAV] {
                for &o in &self.active {
                    if let Some(c) = out.iter().find(|c| c.cls == cls && c.owner == o) {
                        ordered.push(*c);
                    }
                }
            }
            // H450: the held object of type 7 (the gun) after the nav-hotspots, in the View band
            ordered.extend(self.gun_clickable());
            if self.w.rules.h150_icon_bars {
                ordered.extend(self.icon_clickables()); // H150: the bands' icon categories (band test in `hit`)
            }
            ordered.extend(self.engine_clickables()); // H169 / H170: engine objects with no HDB owner
            out = ordered;
        } else if self.w.rules.h121_iitem_key_order {
            // H121: layouts from the most recently shown; inside one, smallest key first, ties in container
            // order. Every InterfaceItem stays above the View's clickables (H77)
            let (items, rest): (Vec<Clickable>, Vec<Clickable>) =
                out.into_iter().partition(|c| c.cls == class::INTERFACE_ITEM);
            let mut ordered = Vec::new();
            for (_, layout_items) in self.w.layouts().iter().rev() {
                let mut v: Vec<&Clickable> =
                    layout_items.iter().filter_map(|(i, _)| items.iter().find(|c| c.owner == *i)).collect();
                v.sort_by_key(|c| self.w.item_key(c.owner)); // stable: ties keep container order
                ordered.extend(v.into_iter().cloned());
            }
            // items of no shown layout (not expected): after, earliest registered first (H77)
            let mut loose: Vec<Clickable> = items.into_iter().filter(|c| !ordered.iter().any(|o| o.owner == c.owner)).collect();
            loose.reverse();
            ordered.extend(loose);
            ordered.extend(self.dbox_clickables()); // H136: the box's category comes after the layouts' items
            ordered.extend(rest);
            out = ordered;
        } else if self.w.rules.h77_iitem_hit_order {
            // H77 (fo-022 546.46 / 558.49): InterfaceItems above the View's clickables, the earliest registered on top
            let (mut items, rest): (Vec<Clickable>, Vec<Clickable>) =
                out.into_iter().partition(|c| c.cls == class::INTERFACE_ITEM);
            items.reverse(); // `out` is most-recent-first (A4); stable for equal sequence numbers
            items.extend(self.dbox_clickables());
            items.extend(rest);
            out = items;
        } else {
            let mut d = self.dbox_clickables();
            d.extend(out);
            out = d;
        }
        out
    }

    /// HYPOTHESIS H150: an Activation places an icon in its bar (the icon's state 1), a Deactivation takes it out.
    /// An EmotionIcon is placed only while enabled (C15: its rect is set by the Enable's state 1, H61 / H68 switch it off)
    fn icon_lifecycle(&mut self, owner: u32, ev: u8) {
        let Some(bar) = super::icons::bar_of(&self.w, owner) else { return };
        if ev == event::DEACTIVATION {
            self.icon_bars.remove(owner);
            return;
        }
        // H203–H206: with the icon states tracked, the state commands (`Out::IconState`) place these classes
        let r = self.w.rules;
        let by_state = match bar {
            0 => r.h206_idea_icon_type3,
            1 => r.h203_action_icon_type3,
            3 => r.h204_emotion_icon_type3,
            4 | 5 => r.h205_evidence_icon_type3,
            _ => false,
        };
        if by_state {
            return;
        }
        if bar == 3 && !self.w.is_enabled(class::EMOTION_ICON, owner) {
            return;
        }
        if self.panel_activation(owner) {
            return; // H170: placed by the panel (or not while it is hidden)
        }
        self.place_icon(owner);
    }

    /// H343: switch closed captioning (the original's key handler, key 'C' / the port's F3 and `--subtitles`): var
    /// 0xa0d set to `on` (type 2, as PrefClosedCap writes it; the port's preferences are the variables, H81); with
    /// [`super::Rules::h343_cc_relayout`] the placed icons of bars 1–3 are placed again from the new start, in
    /// placement order. No trigger runs (nothing is posted).
    pub fn set_closed_captioning(&mut self, on: bool) {
        if self.w.closed_captioning() == on {
            return;
        }
        self.w.vars.set_native(super::icons::VAR_CLOSED_CAPTIONING, Some(on as i32), 2);
        if !self.w.rules.h343_cc_relayout || !self.w.rules.h150_icon_bars {
            return;
        }
        let again: Vec<u32> = self
            .icon_bars
            .placed
            .iter()
            .copied()
            .filter(|&o| matches!(super::icons::bar_of(&self.w, o), Some(1..=3)))
            .collect();
        for &o in &again {
            self.icon_bars.remove(o);
        }
        for o in again {
            self.place_icon(o);
        }
    }

    /// C299 / H2083 (`traces/fo-250.jsonl`; the original's AI-icon refresh, FUN_00452780): with the pref off the AI
    /// icon is taken out of its bar, with it on placed again (if its object is active).
    pub fn ai_icon_refresh(&mut self) {
        if !self.w.rules.h2083_ai_icon_pref {
            return;
        }
        let ai = super::ai::AI_ICON;
        if self.w.ai_pref_on() {
            if self.active.contains(&ai) {
                self.place_icon(ai);
            }
        } else {
            self.icon_bars.remove(ai);
        }
    }

    /// H150: place `owner` in its bar (the icon's state 1) with the size of its picture.
    pub(super) fn place_icon(&mut self, owner: u32) {
        if owner == super::ai::AI_ICON && self.w.rules.h2083_ai_icon_pref && !self.w.ai_pref_on() {
            return; // C299: no AI icon while Pref_bArtificialIntuition is off
        }
        let Some(bar) = super::icons::bar_of(&self.w, owner) else { return };
        let Some(&(w, h)) = self.w.icon_picture(owner).and_then(|a| self.asset_sizes.get(&a)) else { return };
        let (cc, rollover) = self.w.bar_modes();
        let alt = if bar == 0 { rollover } else { cc };
        self.icon_bars.place(bar, owner, (w as i32, h as i32), alt);
    }

    /// HYPOTHESIS H150: placed active icons as clickables in the chain's band order — bottom: ActionIcons (bar 1),
    /// EvidenceIcons (4), EmotionIcons (3), Inventory (2); top: IdeaIcons (0). Bar 6 (0xdb79) is never hit here (View
    /// picture height ≠ 400). Inside a category: placement order (list from the head, H131).
    fn icon_clickables(&self) -> Vec<Clickable> {
        let mut v = Vec::new();
        // H623: bar 6 (the AI icon) after the IdeaIcons — the original tests it first in the top band, but only the AI
        // icon lies at (530,48), so the order there does not matter
        let bars: &[usize] = if self.w.rules.h623_ai_click { &[1, 4, 3, 2, 0, 6] } else { &[1, 4, 3, 2, 0] };
        for &bar in bars {
            for &o in &self.icon_bars.placed {
                match self.icon_bars.rects.get(&o) {
                    Some(&(b, rect)) if b == bar && self.active.contains(&o) => {
                        let cls = [class::ACTION_ICON, class::EVIDENCE_ICON, class::EMOTION_ICON, class::INVENTORY, 0x4a]
                            .into_iter()
                            .find(|&c| self.w.h.has(c, o))
                            .unwrap_or(0);
                        v.push(Clickable { owner: o, cls, rect, kind: ICON_KIND });
                    }
                    _ => {}
                }
            }
        }
        v
    }

    /// HYPOTHESIS H136: the shown dialogue box's clickables, topmost first — its lines (owner = Conversation, only those
    /// activated), the close box (owner = VCConversationList, kind [`super::dbox::KIND_CLOSE`]), the body (same owner,
    /// kind [`super::dbox::KIND_BODY`]: hit, does nothing).
    fn dbox_clickables(&self) -> Vec<Clickable> {
        let Some(l) = self.w.dbox_layout() else { return Vec::new() };
        let mut v: Vec<Clickable> = l
            .lines
            .iter()
            .filter(|(c, _)| self.active.contains(c))
            .map(|&(c, rect)| Clickable { owner: c, cls: class::CONVERSATION, rect, kind: 0 })
            .collect();
        // H1810: after the view's lines, the scroller's arrows and bar (only with more than one position)
        if let Some(sc) = &l.scroll {
            for (rect, kind) in [
                (sc.up, super::dbox::KIND_SCROLL_UP),
                (sc.down, super::dbox::KIND_SCROLL_DOWN),
                (sc.bar, super::dbox::KIND_SCROLL_BAR),
            ] {
                v.push(Clickable { owner: l.list, cls: class::CONVERSATION_LIST, rect, kind });
            }
        }
        // H502: after the selected panel's lines, the panels' tab rects. While the talk panel is empty the box's hit
        // test leaves the HISTORY tab's point to another engine widget (not decoded) — assumed to select the HISTORY
        // panel as well (AG check BB4)
        if self.w.rules.h502_dbox_history_tab {
            v.push(Clickable { owner: l.list, cls: class::CONVERSATION_LIST, rect: l.tabs[0], kind: super::dbox::KIND_TAB_TALK });
            v.push(Clickable { owner: l.list, cls: class::CONVERSATION_LIST, rect: l.tabs[1], kind: super::dbox::KIND_TAB_HISTORY });
        }
        v.push(Clickable { owner: l.list, cls: class::CONVERSATION_LIST, rect: l.close, kind: super::dbox::KIND_CLOSE });
        v.push(Clickable { owner: l.list, cls: class::CONVERSATION_LIST, rect: l.rect, kind: super::dbox::KIND_BODY });
        v
    }

    /// Topmost clickable containing screen point (x, y).
    /// HYPOTHESIS H1815 (port choice, the mouse wheel): the clickable InterfaceItem nearest to `(x, y)` whose Mouse
    /// Click calls a line step of a scrolled list — MainScroll 0 / 1, ScrollHeaders 0 / 1, ScrollAddrs (k, 0 / 1),
    /// PhotoScroll 0 / 1 (`down` = the second of each pair).
    pub fn wheel_item(&self, x: i32, y: i32, down: bool) -> Option<u32> {
        use super::action::{Action, ProcCall};
        let h = self.w.h;
        let want = down as i32;
        let steps = |owner: u32| -> bool {
            let Some(r) = h.read(class::INTERFACE_ITEM, owner) else { return false };
            h.list_of(class::TRIGGER_LIST, r.u32("f48")).into_iter().any(|t| {
                let Some(tr) = h.read(class::TRIGGER, t) else { return false };
                tr.u32("fe8") == event::MOUSE_CLICK as u32
                    && h.list_of(class::ACTION_LIST, tr.u32("fe4")).into_iter().filter_map(|a| Action::load(h, a)).any(|a| {
                        a.kind() == 6
                            && ProcCall::parse(a.body()).is_some_and(|p| match (p.name.as_str(), p.args.as_slice()) {
                                ("MainScroll" | "ScrollHeaders" | "PhotoScroll", &[d, ..]) => d as i32 == want,
                                ("ScrollAddrs", &[_, d, ..]) => d as i32 == want,
                                _ => false,
                            })
                    })
            })
        };
        let dist = |r: [i16; 4]| {
            let cx = x.clamp(r[0] as i32, r[2] as i32 - 1);
            let cy = y.clamp(r[1] as i32, r[3] as i32 - 1);
            (cx - x).pow(2) + (cy - y).pow(2)
        };
        self.clickables()
            .into_iter()
            .filter(|c| c.cls == class::INTERFACE_ITEM && steps(c.owner))
            .min_by_key(|c| dist(c.rect))
            .map(|c| c.owner)
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<Clickable> {
        self.last_hit.set(Some((x, y))); // H2170: the camera's shot needs the point
        // H130: CharViews / Explorables / nav-hotspots are tested only when the point's y is inside the View rect (top
        // 90, bottom 330)
        let band = !self.w.rules.h130_hit_chain || (VIEW_TOP..VIEW_BOTTOM).contains(&y);
        self.clickables().into_iter().find(|c| {
            let [x1, y1, x2, y2] = c.rect.map(|v| v as i32);
            let in_band = match c.cls {
                class::INTERFACE_ITEM | class::CONVERSATION | class::CONVERSATION_LIST => true,
                _ if c.owner == super::devices::PANEL_OWNER || c.owner == super::devices::MENU_OWNER => true,
                // H150: icons only in their band (bottom y ≥ 330, top y < 90)
                _ if self.icon_bars.rects.contains_key(&c.owner) && c.kind == ICON_KIND => {
                    match super::icons::bar_of(&self.w, c.owner) {
                        Some(b) if super::icons::bar_bottom(b) => y >= VIEW_BOTTOM,
                        Some(_) => y < VIEW_TOP,
                        None => false,
                    }
                }
                _ => band,
            };
            in_band && x >= x1 && x < x2 && y >= y1 && y < y2
        })
    }

    /// Condition values the core cannot evaluate, as observed in a trace (action id → value): an input from the
    /// environment (system values), like a click.
    pub fn uneval_from_trace(trace: &[crate::replay::TraceEv], h: &Hdb) -> HashMap<u32, bool> {
        let mut m = HashMap::new();
        for e in trace {
            if let crate::replay::Ev::Cond { action, value } = e.ev {
                if let Some(c) = super::action::Action::load(h, action).and_then(|a| a.condition()) {
                    if !matches!(c.lhs_kind, 0 | 2 | 3 | 5) || !matches!(c.rhs_kind, 0 | 2 | 3 | 5) {
                        m.insert(action, value);
                    }
                }
            }
        }
        m
    }

    /// HYPOTHESIS H603 (static; the original's RandGen, FUN_0049f6d0): RandGen / RandGenRange draw
    /// from a generator seeded with timeGetTime() at every call (one LCG step x·0x41c64e6d + 0x3039, mod (hi − lo + 1) +
    /// lo) — not reproducible. The traced value of a condition that reads a variable after a traced RandGen draw on it
    /// (and before a type-0 write of it) is therefore an input like [`Runner::uneval_from_trace`]'s (used only while
    /// the core's variable is unknown); the last such value per action.
    pub fn uneval_random_from_trace(trace: &[crate::replay::TraceEv], h: &Hdb) -> HashMap<u32, bool> {
        Self::uneval_random_seq_from_trace(trace, h).into_iter().filter_map(|(a, q)| q.back().map(|&v| (a, v))).collect()
    }

    /// H841: [`Runner::uneval_random_from_trace`] keeping every traced value in order (a periodic Timer's random
    /// sound chooser, fo-090 0x8e73: one draw per fire, a different branch each time)
    pub fn uneval_random_seq_from_trace(trace: &[crate::replay::TraceEv], h: &Hdb) -> HashMap<u32, VecDeque<bool>> {
        let draws = random_actions(h);
        let mut drawn: HashSet<u32> = HashSet::new();
        let mut m = HashMap::new();
        for e in trace {
            match e.ev {
                crate::replay::Ev::Exec { action, .. } => {
                    if let Some(&var) = draws.get(&action) {
                        drawn.insert(var);
                    } else if let Some(a) = super::action::Action::load(h, action).filter(|a| a.kind() == 0) {
                        if let Some(var) = super::action::be32(a.body(), 0) {
                            drawn.remove(&var);
                        }
                    }
                }
                crate::replay::Ev::Cond { action, value } => {
                    if let Some(c) = super::action::Action::load(h, action).and_then(|a| a.condition()) {
                        if (c.lhs_kind == 0 && drawn.contains(&c.lhs)) || (c.rhs_kind == 0 && drawn.contains(&c.rhs)) {
                            m.entry(action).or_insert_with(VecDeque::new).push_back(value);
                        }
                    }
                }
                _ => {}
            }
        }
        m
    }
}

/// H603: the RandGen / RandGenRange actions of the HDB → the variable each writes (first argument)
pub fn random_actions(h: &Hdb) -> HashMap<u32, u32> {
    let mut v = HashMap::new();
    if let Some(c) = h.class(0x41) {
        for &(id, _) in &c.objects {
            let Some(a) = super::action::Action::load(h, id) else { continue };
            if a.kind() != 6 {
                continue;
            }
            if let Some(p) = super::action::ProcCall::parse(a.body()) {
                if matches!(p.name.as_str(), "RandGen" | "RandGenRange") {
                    if let Some(&var) = p.args.first() {
                        v.insert(id, var);
                    }
                }
            }
        }
    }
    v
}
