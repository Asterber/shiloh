//! Headless engine core ("world"): trigger registration, events, the message queue, the action
//! interpreter and view transitions — only rules backed by traces of the original (RE_NOTES §1,
//! C-numbers). Rules that are hypotheses are marked `HYPOTHESIS` here, sit behind [`Rules`] switches,
//! and are named after their RE_NOTES H-number.
//!
//! The world does not invent inputs: user clicks, media ends, and every event whose cause is not a
//! confirmed rule are fed from outside (a player front-end, or the trace replay driver).
//! Everything the world decides is appended to [`World::out`].

pub mod action;
pub mod ai;
pub mod pda;
pub mod dbox;
pub mod devices;
mod h76;
mod h89;
pub mod icons;
pub mod moviehot;
pub mod procs;
pub mod run;
mod saving;
pub mod sched;
pub mod shootout;
pub mod vars;

use crate::hdb::{Hdb, Value};
use action::{be32, Action, Operands, ProcCall};
use std::collections::{HashMap, HashSet};
pub use h89::WCmd;
use h89::QItem;
use vars::{SetResult, Vars};

/// Event codes (the original's event-name table, H44; firing sites C19).
pub mod event {
    pub const MOUSE_CLICK: u8 = 0;
    pub const DOUBLE_CLICK: u8 = 1;
    pub const MOUSE_MOVE: u8 = 2;
    pub const KEY_PRESS: u8 = 3;
    pub const ROLL_INTO: u8 = 4;
    pub const ROLL_OUT_OF: u8 = 5;
    pub const DROP: u8 = 6;
    pub const TIMER: u8 = 7;
    pub const ACTIVATION: u8 = 8;
    pub const DEACTIVATION: u8 = 9;
}

/// Class ids (RE_NOTES §5.1).
pub mod class {
    pub const TITLE: u32 = 0x27;
    pub const NODE: u32 = 0x28;
    pub const LOCATION: u32 = 0x29;
    pub const VIEWPOINT: u32 = 0x2a;
    pub const VIEW: u32 = 0x2b;
    pub const CHAR_VIEW: u32 = 0x2d;
    pub const CHAR_VIEW_LIST: u32 = 0x2e;
    pub const CONVERSATION: u32 = 0x31;
    pub const CONVERSATION_LIST: u32 = 0x32;
    pub const NAV: u32 = 0x33;
    pub const NAV_LIST: u32 = 0x34;
    pub const ASSET_REF: u32 = 0x35;
    pub const ASSET_REF_LIST: u32 = 0x36;
    pub const EXPLORABLE: u32 = 0x37;
    pub const EXPLORABLE_LIST: u32 = 0x38;
    pub const HOTSPOT: u32 = 0x39;
    pub const CURSOR: u32 = 0x3d;
    pub const ACTION_LIST: u32 = 0x42;
    pub const ENABLED: u32 = 0x46;
    pub const ACTION_ICON: u32 = 0x47;
    pub const GAME_STATE: u32 = 0x57;
    pub const EMOTION_ICON: u32 = 0x48;
    pub const EVIDENCE_ICON: u32 = 0x49;
    pub const INVENTORY: u32 = 0x4b;
    pub const IFACE_LAYOUT: u32 = 0x4e;
    pub const INTERFACE_ITEM: u32 = 0x4f;
    pub const TRIGGER: u32 = 0x51;
    pub const TRIGGER_LIST: u32 = 0x52;
}

/// A queued action message (the action + the trigger that posted it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Msg {
    pub action: u32,
    pub trigger: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PostSrc {
    /// trigger of `owner` matched the fired event (C19)
    Event { owner: u32, event: u8 },
    /// type-6 procedure DoActionList run by `action` (C25)
    Proc { action: u32 },
    /// HYPOTHESIS H67: a Load re-posts the save's running timers (VCGameState l154) as type-2 action messages the engine
    /// builds itself (no VCAction record: action id 0, no trigger; FUN_004a1250); posted by the start-icon builder, not
    /// through the trigger path, so not an `act_post`
    Restore,
    /// HYPOTHESIS H70: a message the engine builds outside the trigger path (no VCAction record: action id 0), e.g. the
    /// main menu's "Load" (type 5 show 0x142ca)
    Engine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransSrc {
    /// nav click (C1–C3, C6)
    Nav { vcnav: u32 },
    /// type-4 Set View action (C18)
    SetView { action: u32 },
    /// type-6 SetStartView (C25)
    StartView { action: u32 },
    /// HYPOTHESIS H125 (C40): type-6 SwapDisc — travel to another Node / Location / ViewPoint / View
    SwapDisc { action: u32 },
    /// HYPOTHESIS H183: type-6 CharTeleport — to another View of the current Node where the clicked character is
    CharTeleport { action: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotspotSrc {
    /// type-3 Enable of a VCNav of the current View (C16)
    Enable { action: u32 },
    /// built when the new View is activated, enabled VCNavs only (C15, C23)
    ViewEntry { view: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegCause {
    TransitionView,
    /// HYPOTHESIS H50: ViewPoint of a View = the VCViewPoint whose list contains it
    ViewPoint,
    NavHotspot,
    /// HYPOTHESIS H48: type-1 plays the first enabled AssetRef of the list
    PlayAsset {
        action: u32,
    },
    StartNode,
    StartLocation,
    /// HYPOTHESIS H125 (C40): SwapDisc builds the new Node and Location runtime objects
    TravelNode {
        action: u32,
    },
    TravelLocation {
        action: u32,
    },
    /// HYPOTHESIS H53: background AssetRef of the current View
    Background {
        view: u32,
    },
    /// HYPOTHESIS H58: type-5 show creates the VCIFaceLayout's runtime object (TL 0)
    IfaceLayout {
        action: u32,
    },
    /// HYPOTHESIS H58: an enabled VCInterfaceItem of a layout being activated (TL f48)
    IfaceItem {
        layout: u32,
    },
    /// HYPOTHESIS H58/H60: AssetRef of an activated InterfaceItem (f44) or EmotionIcon (f30, f2c, f3c) — first
    /// enabled element of the list (the same chooser as type 1 / H48)
    ObjectAsset {
        of: u32,
    },
    /// HYPOTHESIS H101: a type-3 Enable of a VCInventory builds its runtime object (TL f40)
    InventoryEnable {
        action: u32,
    },
    /// HYPOTHESIS H60: PreloadEmIcon(icon) creates the VCEmotionIcon's runtime object (TL f40)
    EmotionIcon {
        action: u32,
    },
    /// HYPOTHESIS H203–H206: a type-3 Enable builds an icon object (classes 0x47–0x4a, through the common icon base;
    /// TL f40)
    IconEnable {
        action: u32,
    },
    /// HYPOTHESIS H700 (static; the box's state handler FUN_00435d90): the dialogue box's state 1 builds every
    /// enabled IdeaIcon of its CharView's f4c IdeaMapList
    BoxIdea {
        char_view: u32,
    },
    /// HYPOTHESIS H55: VCExplorable of the current View's f38 list (TL f44): `action` = None at the View's activation
    /// (every enabled member), Some = a type-3 Enable turned it on
    Explorable {
        action: Option<u32>,
    },
    /// HYPOTHESIS H66: SetStartView creates the start's icon objects (VCInventory 0x4b, VCEmotionIcon 0x48, VCActionIcon
    /// 0x47, load only VCEvidenceIcon 0x49 — all through the common icon base, FUN_00477940); `load` = from the save's
    /// lists
    StartIcon {
        action: u32,
        load: bool,
    },
    /// HYPOTHESIS H67: a Load re-creates the media players of the save's VCGameState l64 list (posted by the start-icon
    /// builder; the AssetRef registers when it is dispatched)
    /// HYPOTHESIS H69: application start (plays 0xf3d6, builds the cursors; FUN_0041b060)
    Startup,
    /// HYPOTHESIS H71: game start FUN_0048cd70 (new game and Load)
    GameStart,
    RestoredPlayer {
        action: u32,
    },
    /// HYPOTHESIS H74: an enabled VCCharView of the activated View's f30 VCCharViewList (TL f54), built by the View's
    /// activation before the nav-hotspots (FUN_0046d320)
    CharView {
        view: u32,
    },
    /// HYPOTHESIS H74: the CharView's activation builds its media wrapper (owner = the same VCCharView, literal TL 0)
    /// unless the restore window is open (FUN_00436f80)
    CharViewPlayer {
        char_view: u32,
    },
    /// HYPOTHESIS H74: the wrapper's command plays the first enabled AssetRef of the CharView's f50 list (the type-1
    /// chooser) through a media player (the AssetRef registers when it is dispatched; FUN_00480fe0)
    CharViewAsset {
        char_view: u32,
    },
    /// HYPOTHESIS H126: DisplayDBox(CharView) / a Load whose VCGameState names an open box builds the CharView's
    /// VCConversationList (f48) runtime object (TL 0)
    DialogBox {
        char_view: u32,
    },
    /// HYPOTHESIS H126: the dialogue box's command builds every enabled VCConversation (class 0x31) of the list (TL f3c)
    Conversation {
        list: u32,
    },
    /// HYPOTHESIS H135: a click on a Conversation line plays the first enabled AssetRef of its f30, then of its f34 (a
    /// play sequence of media players, FUN_00486f50 — the AssetRef registers when its player runs)
    ConversationPlay {
        conv: u32,
    },
    /// HYPOTHESIS H188: an object a type-6 procedure built (EnableKitIcon: an EvidenceIcon)
    Proc {
        action: u32,
    },
}

/// Everything the world decides, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    /// predicted trigger registration (owner, TL by C20; None = TL rule unknown for the owner class)
    Reg {
        owner: u32,
        tl: Option<u32>,
        cause: RegCause,
    },
    Post {
        msg: Msg,
        src: PostSrc,
    },
    /// condition of a dispatched action (C21/C22); None = not evaluable (operand kind 1/4)
    Cond {
        action: u32,
        value: Option<bool>,
    },
    Exec {
        action: u32,
        typ: u8,
    },
    SetVar {
        action: u32,
        result: SetResult,
    },
    Transition {
        view: u32,
        kind: u32,
        src: TransSrc,
    },
    Hotspot {
        vcnav: u32,
        src: HotspotSrc,
    },
    HotspotRemoved {
        vcnav: u32,
    },
    /// H65: the restore window opens (SetStartView with a saved start; type-0x14 command value 1)
    Restore {
        on: bool,
    },
    /// predicted lifecycle event (C23)
    Fire {
        owner: u32,
        event: u8,
    },
    /// action type the interpreter does not implement (no confirmed rule)
    UnknownType {
        action: u32,
        typ: u8,
    },
    /// HYPOTHESIS H59: type-2 timer action. `start` = Some((ms, periodic)) starts timer `id` (an existing timer
    /// with the same id is killed first), None = the timer `id` is stopped (and pending expirations dropped)
    Timer {
        id: u8,
        start: Option<(u32, bool)>,
    },
    /// HYPOTHESIS H67: a Load posted the media player of a saved AssetRef; its AssetRef is registered and
    /// activated when the player's command runs ([`World::run_player`])
    PlayerPost {
        asset: u32,
    },
    /// HYPOTHESIS H100 (mode 3): a type-1/7 stop of `asset`'s player. `known` = the world knows the player exists (a
    /// looping one); the runner also stops a non-looping player it holds in its media FIFO. The stopped player ends like
    /// a media end (removal → Deactivation of the AssetRef, queued: the runner pushes it to the back; FUN_0048b8a0);
    /// the replay counts a `known` stop as the predicted Deactivation.
    PlayerStop {
        asset: u32,
        known: bool,
    },
    /// HYPOTHESIS H1060: the SwapDisc travel removes every media player (FUN_00493090) — the runner deactivates the
    /// players of its media FIFO (played, non-looping: the world does not track which still exist)
    TravelStopPlayers,
    /// HYPOTHESIS H68: a click on an EmotionIcon posted the state-0 command of `icon`; it runs [`World::icon_off`]
    IconOff {
        icon: u32,
    },
    /// HYPOTHESIS H203–H206: an icon's state command (queued with the new state): the state handler places it in its
    /// bar on 1 and takes it out on 0 (H150); the world sends only changes
    IconState {
        icon: u32,
        state: bool,
    },
    /// HYPOTHESIS H74: the CharView media wrapper of `char_view` was posted; its command runs
    /// [`World::char_view_player`]
    CharViewPlayer {
        char_view: u32,
    },
    /// HYPOTHESIS H89: an engine command posted at the C24 insert point (closed-loop runner only; the lock-step world
    /// queues it itself); it runs [`World::run_cmd`]
    Command(WCmd),
    /// HYPOTHESIS H601: a mouse command at (x, y) queued by the engine (FUN_004cf7e0) — hit-tested when it runs
    MouseCommand {
        x: i32,
        y: i32,
    },
    /// HYPOTHESIS H126: the dialogue box (VCConversationList runtime object) was posted; its command runs
    /// [`World::dbox_run`]
    DboxPost {
        list: u32,
    },
    /// HYPOTHESIS H651: the end of a played line (`conv`) as a queue command behind the last asset's Deactivation posts;
    /// it runs [`World::dbox_seq_end`]
    DboxSeqEndPost {
        conv: u32,
    },
    /// HYPOTHESIS H922: the box command's closing state-1 command (FUN_004767a0) is a queue command; it runs
    /// [`World::dbox_show_cmd`]
    DboxShowPost {
        list: u32,
    },
    /// H160 (replay check): the dialogue box (VCConversationList `list`) gets a state command (the box's state handler,
    /// FUN_00435d90): 1 = shown (lines in the talk list), 0 = hidden — the value is not in the traces
    DboxState {
        list: u32,
        state: u8,
    },
    /// HYPOTHESIS H182: EndGame — the game-end object (FUN_00429d60, run by the main loop); `rewind`
    /// = the "Try again" box was offered instead (auto rewind on, the variable was ≠ 0: the answer is the user's)
    GameEnd {
        action: u32,
        rewind: bool,
    },
    /// HYPOTHESIS H1700: SaveGameState (FUN_004d9450) — the runner takes the rewind snapshot ("Try again?" YES loads it)
    SaveGameState {
        action: u32,
    },
    /// HYPOTHESIS H1703: a shot at movie hotspot `hotspot` froze the game
    Freeze {
        hotspot: u32,
    },
    /// HYPOTHESIS H191: ReturnMainMenu / ReturnFromHelp posted a menu object (FUN_0048f670)
    MainMenu {
        action: u32,
    },
    /// HYPOTHESIS H403: SaveLoadComit(1) in save mode writes the game as slot `slot` named `name` (FUN_005173a0: Temp.gam
    /// copied to `<name>.x`) — the front-end writes the file (`crate::save`)
    SaveGame {
        slot: usize,
        name: String,
    },
    /// HYPOTHESIS H403 / H95: SaveLoadComit(1) in load mode loads slot `slot` (FUN_005177e0) — the
    /// front-end switches the session to that file
    LoadGame {
        slot: usize,
    },
    /// HYPOTHESIS H733: the main loop is busy for `ms` (FadeToBlack's 2000 ms wait FUN_005521c0): no queued command runs
    /// until the game clock has moved on by `ms`
    Stall {
        action: u32,
        ms: u32,
    },
    /// known action type, but this case has no confirmed rule (e.g. a type-6 procedure)
    Unimplemented {
        action: u32,
        what: String,
    },
}

/// The 24 variables of RegPreferences (action 0x7e68's arguments, H81), in argument order.
pub const PREF_VARS: [u32; 24] = [
    0x3d4, 0x369, 0x3e0, 0xa0d, 0x3d6, 0x3e1, 0x3e3, 0x3e2, 0x3dc, 0x3d9, 0x3da, 0x3db, 0x3df, 0x3d5, 0x3de, 0x3dd, 0x3d7,
    0x3d0, 0x3d2, 0x3d3, 0x3d1, 0xa0c, 0x1072, 0x13b6,
];

/// C86 (fo-108): the original's registry Preferences where they differ from XFiles.gam — the host
/// Preferences a cold start of the port begins with: Pref_bArtificialIntuition 1, Pref_cAutoSetDepth 'H',
/// Pref_bSubliminalMessages 1, Pref_cFlashlightBeam 'W', Pref_bSkipClips 1, Pref_cAutoSetResolution 'Y'.
pub const C86_HOST_PREFS: [(u32, i32); 6] =
    [(0x369, 1), (0x3d5, b'H' as i32), (0x3d9, 1), (0x3de, b'W' as i32), (0x3e3, 1), (0xa0c, b'Y' as i32)];

/// Switches for rules that are hypotheses (off = confirmed rules only).
#[derive(Clone, Copy, Debug, Default)]
pub struct Rules {
    /// H48: type-1 "play AssetRefList" starts the first AssetRef of the list that is enabled (C15 table)
    pub h48_play_first_enabled: bool,
    /// H53: the View background = first enabled AssetRef of View.f2c, chosen at view entry and re-chosen after
    /// every Enable of an AssetRef; a change registers (plays) the new one
    pub h53_background: bool,
    /// H58: type 5 shows (blob byte 0) / hides (1) a VCIFaceLayout: show registers the layout (TL 0) and, when it is
    /// activated, its enabled InterfaceItems (TL f48), each activated with the first enabled AssetRef of its f44 list;
    /// hide deactivates every item and its asset. SetIFLAsView: no trigger-level effect
    pub h58_iface_layout: bool,
    /// H59: type 2 = timer `[u32 ms][u8 id][u8 mode | 0x80 periodic]` (mode 0 start, 1 stop); on expiry Timer (7)
    /// fires on every registered owner subscribed to the id (an event-7 trigger's action condition with an
    /// event-context operand)
    pub h59_timer: bool,
    /// H60: PreloadEmIcon(icon) registers the VCEmotionIcon (TL f40) unless it exists; its activation registers and
    /// activates the first enabled AssetRef of each of its lists f30, f2c, f3c
    pub h60_preload_em_icon: bool,
    /// H55: VCExplorables of the current View (View.f38 VCExplorableList): at the View's activation every enabled member
    /// is registered (after the nav-hotspots, before the background) and activated; a type-3 Enable of a member registers
    /// and activates it at once, a Disable of an active one deactivates it at once; leaving the View deactivates the
    /// active ones after the nav-hotspots
    pub h55_explorable: bool,
    /// H61: a type-3 Disable of an active VCEmotionIcon (created by PreloadEmIcon, H60) deactivates it, then its assets
    pub h61_icon_disable: bool,
    /// H65 (C30, C31): a game started from a save (its gam has a VCGameState 0x57) runs the new game's start, but
    /// SetStartView takes the save's Node/Location/ViewPoint/View and opens the restore window: while it is open an
    /// Activation (mode 1, a flag of the save) fires no trigger; Deactivations fire. The window closes after everything
    /// the restore queued (runner: a command at the back of the queue; replay: the trace's gate)
    pub h65_load: bool,
    /// H66: SetStartView (FUN_00477940) creates, after the View: every VCInventory (new game: each enabled one, objId
    /// order; Load: the save's VCGameState la0 list), every enabled VCEmotionIcon, every enabled VCActionIcon, and on a
    /// Load the VCEvidenceIcons of the save's ldc / l118 lists; each is registered and activated (l118: not activated).
    /// Inventory and EmotionIcon activation builds the assets like an EmotionIcon (H60)
    pub h66_start_icons: bool,
    /// H67: type 7 plays an AssetRefList like type 1 (H48: first enabled; the persistent player, e.g. 0x16767 → 0x76bf
    /// FO.amv); a Load restores, before the start icons, the save's VCGameState l64 players (12-byte records [AssetRef]
    /// [looping flag][position]: the AssetRef registered and activated) and l154 timers (12-byte records [id][ms]
    /// [periodic] → a type-2 start message, id 0, posted; when run it starts timer `id` with the remaining `ms`)
    pub h67_saved_media_timers: bool,
    /// H68: Mouse Click on an EmotionIcon (the icon's click handler, FUN_0049bfe0): after its triggers' actions are
    /// posted, every EmotionIcon object (creation order) gets a state-0 command, queued behind those actions; when it
    /// runs, an active icon is deactivated with its assets (as H61) and the object is removed
    pub h68_icon_click_off: bool,
    /// H69: application start, before the menu — 0xf3d6 (NavBeep.amv) played, the 12 cursors of the cursor table and a
    /// null-owner cursor built (TL 0, H57); each cursor in turn shows its first enabled AssetRef of f38 (registered only if
    /// no object for it exists yet), then is activated
    pub h69_startup: bool,
    /// H70: the main menu's "Load" button posts an engine-built type-5 message (show 0x142ca, action id 0) that shows the
    /// load-screen layout 0x142ca like a type-5 action (H58)
    pub h70_menu_layout: bool,
    /// H71: game start (FUN_0048cd70, new game and Load): the EvidenceIcon 0xdb79 object (built once, then activated:
    /// assets as H60), the VCGameState (save's objId, new game 0x14cb, TL 0, not activated) and every VCTitle (TL fa0,
    /// activated)
    pub h71_game_start: bool,
    /// H72: (a) SaveLoadInit gives each built save-slot item (load screen 0x1438a..c, save screen 0x1427c..e) a new text
    /// asset (owner 0, TL 0, activated); (b) an Enable (state ≠ 0) of a disabled InterfaceItem of a shown layout builds
    /// that item like the layout's activation does (TL f48, Act, its f44 asset)
    pub h72_save_screen: bool,
    /// H73: operand kind 1 = trigger-local variables (Integer[8], Boolean[12], Character[4] of the runtime trigger),
    /// zeroed when an event fires the trigger (FUN_004d2710), read/written by the trigger's messages
    pub h73_trigger_locals: bool,
    /// H75: the nav click handler (FUN_0046ae70) builds and posts the transition only if the current View differs from
    /// the VCNav's destView; the old View is deactivated only then. A VCNav to the current View: no transition, no
    /// lifecycle events (its Mouse Click fires as for any nav click)
    pub h75_nav_same_view: bool,
    /// H77 (fo-022): hit-test stacking — every active InterfaceItem is above every View-level clickable (nav-hotspot,
    /// Explorable), and among InterfaceItems the EARLIEST registered is topmost; View-level clickables keep A4 (most
    /// recently registered on top)
    pub h77_iitem_hit_order: bool,
    /// H74: VCCharViews (class 0x2d, TL f54): the View's activation builds every enabled member of View.f30 (VCCharViewList)
    /// before its nav-hotspots, each posted (activated); the activation (after its triggers' posts) builds a media wrapper
    /// (same owner, TL 0) unless restoring, whose command plays the first enabled AssetRef of f50 (none → removed);
    /// leaving the View deactivates them after the nav-hotspots (and after the Explorables unless H88)
    pub h74_char_view: bool,
    /// H80: the Reg* procedures bind VCVariables to native globals; only RegCurrInvVar(a, b) (a set to b, type 1) and
    /// RegViewState(a) (a set to 0, type 0) write a variable; the rest have no world-state effect
    pub h80_reg_vars: bool,
    /// H81: RegPreferences(24 variables) takes the preferences' "write variables to preferences" path (no variable change)
    pub h81_reg_preferences: bool,
    /// H82: DeselectInv(item) has no effect while no inventory item is held (the core has no selection)
    pub h82_deselect_inv: bool,
    /// H83: FadeBack(var) has no trigger-level effect: one type-21 command per runtime object of the app's kind lists 4,
    /// 0xc, 0xe, 0xf, 0x12, 0x1f, 0x22, 0x10, 0x1b, whose handler (FUN_00458b50) only installs / removes a display filter
    /// object; the core does not model these display commands
    pub h83_fade_back: bool,
    /// H91: InitSmShootout(a, b, c) binds three VCVariables and sets each to 0 (a type-2 variable write)
    pub h91_init_sm_shootout: bool,
    /// H92: SetLanguage, SendEmail, EmailFlushMess, StartVideoText, RollHdCredits have no trigger-level effect (native
    /// string / e-mail list / text overlay / credits roller state, native C++ timers only)
    pub h92_native_display_procs: bool,
    /// H93: SetFLightProps(n, c1, c2) stores three flashlight globals; only with the current inventory value 8 and a held
    /// item it refreshes that item (the core holds no item)
    pub h93_flight_props: bool,
    /// H94: SetAIList / ClearAIList / SetAIGlow change the EvidenceIcon object's native state only (no registration,
    /// event or post while that object exists — it is built at game start, H71)
    pub h94_ai_list: bool,
    /// H95: SetIItemFocus (keyboard focus), SaveSlotSelect in load mode (slot marker position) and SaveLoadComit(1) in
    /// load mode (starts the Load, which the core gets as its save, H65) have no trigger-level effect
    pub h95_save_screen_procs: bool,
    /// H96: PDAInitAddr collects the enabled AssetRefs of InterfaceItem 0xb0fb's f44 list (the address cards) and shows
    /// the current one (index kept across calls, starts at 0) in item 0x89f8: the item's old asset is deactivated, the
    /// new AssetRef registered (TL 0) and activated
    pub h96_pda_init_addr: bool,
    /// H97: PDAFlipAddr(m), only with ≥ 2 addresses: 0 next / 1 (and others) previous card shown as H96; 2 / 3 post two
    /// engine-built type-3 messages (action 0) enabling / disabling AssetRefs 0xa79a, 0xa79c; 4 rebuilds the list
    pub h97_pda_flip_addr: bool,
    /// H76: a View step's follow-up lifecycle fires wait for the actions queued before them (entry: nav-hotspot /
    /// Explorable Activations behind the View's and its background's posts; exit: old nav-hotspots' Deactivations, the
    /// ViewPoint change and the new View's Activation behind the old View's and background's Deactivation posts); a
    /// Disable of a built nav-hotspot deactivates it at once (`world/h76.rs`)
    pub h76_entry_hotspot_order: bool,
    /// H89: the View steps' follow-up fires are commands of the depth-first queue (insert after the running message,
    /// restart at the head = C24): View cmd 0 posts View cmd 11 (background) before its trigger actions, CharViews,
    /// nav-hotspots, Explorables; the old View's (+ background's) Deactivation is one command, the rest of the exit
    /// another behind it (`world/h89.rs`). Replaces H76's hold when on (H76's Disable → Deact at once is kept)
    /// On in `Rules::all()` since H1241 (`traces/fo-172.jsonl` 121.946–122.028, C161: the background 0xb511's Enables run
    /// before the View 0x38f's earlier-posted 0x13b9c Disables — the H89 prediction "background first", the H76 runner
    /// ran them in post order)
    pub h89_queue_model: bool,
    /// H90: operand kind 4 = key into the event-context map the firing event copies into the trigger; read by the
    /// operand evaluator (FUN_005d54f0; no trigger → 0). Timer (7): map {0: expired timer id}; other events' maps not
    /// decoded (not evaluable)
    pub h90_event_ctx: bool,
    /// H100: type 1 / 7 blob `[u32 AssetRefList][u8 mode]` (FUN_0047a4e0): mode 1 play, 2 play a looping player (marked
    /// looping; at media end only non-looping players are removed), 3 stop (not traced; no play), other modes nothing.
    /// Modes 1 / 2 create no player when one for the chosen AssetRef exists (the app's player list); the core knows only
    /// looping players (and restored ones with flag ≠ 0) to exist
    pub h100_player_mode: bool,
    /// H87: a type-3 Enable/Disable of a VCCharView (only on a change of its C15 flag): Enable builds it like the View
    /// activation does (TL f54, posted → Activation + media wrapper) iff it is a member of the current View's f30;
    /// Disable: the built CharView gets a state-0 command (Deactivation) if active and is removed (FUN_00492170)
    pub h87_char_view_enable: bool,
    /// H88: leaving a View (FUN_00438770) removes the app's runtime objects of kinds 5 (nav-hotspots), 6 (CharViews),
    /// 0x14 (Explorables), 0x1d in that order → Deactivations hotspots → CharViews → Explorables
    pub h88_exit_remove_order: bool,
    /// H58 order (Q14 a): a layout's built items sit in a sorted container (FUN_004390e0; key = InterfaceItem f4c &
    /// 0xff): an item whose key is ≥ the last one's is appended, a smaller one goes before the first element with key ≥
    /// its own. Both the show posts and the hide removals walk it (Activations and hide Deactivations in that order)
    pub h58_item_order: bool,
    /// H101: a type-3 Enable of a VCInventory (only on a change of its C15 flag; FUN_0049bbc0): the runtime object is
    /// built (TL f40) and posted + activated (at once, or from a 500 ms native timer's command list when a certain
    /// object is not active — the delay is not modelled); its activation (as H60 / H66) builds the first enabled
    /// AssetRef of f30, f2c, f3c. Inventory 0x442b takes another path (not decoded: logged); Disable not modelled
    /// (logged)
    pub h101_inventory_enable: bool,
    /// H102: LoginInit (on the argument's byte; FUN_00468f00 for 'C'): 'C' → if VCVariable 0x1352 exists, items 0xc67b
    /// then 0xc68c — each whose runtime object exists gets a text asset (as H72 a: the item's old asset removed, REG
    /// owner 0 TL 0 posted → Act, held by the item, so a layout hide removes it right after its item); then
    /// SetIItemFocus 0xc63b / 0xc63c and item 0xfbe5 moved — no event. 'P' → not decoded: logged
    pub h102_login_init: bool,
    /// H103 (replay only): a View entered by a transition of kind 1..4 runs the transition effect (FUN_00464330) in its
    /// state command; it pauses and resumes every timer — a one-shot timer that had already expired natively (its id
    /// still pending) is re-armed with its full delay and expires a second time
    pub h103_timer_pause_resume: bool,
    /// H124: a Deactivation runs as its own queue command (cmd 1, which fires event 9; FUN_00425150), so its trigger
    /// posts go to the C24 insert point of that dispatch (the queue head), in front of everything pending. A View's
    /// background is removed by the View's cmd 12 (a cmd 1 + cmd 12, posted at the head), i.e. one dispatch after the
    /// View's Deactivation: the background's Deactivation posts run before the View's (fo-029 157.24 0x145ea posted after
    /// 0x8815 156.29, run first). Runner: the old View's and background's Deactivations stay one command (A1 for the
    /// lifecycle order) but the background's posts go in front
    pub h124_deact_own_dispatch: bool,
    /// H123: one runtime asset object per AssetRef (looked up before one is built; FUN_00446580): an InterfaceItem whose
    /// asset object already exists (held by another shown item) only re-posts it (one more holder) — no registration,
    /// no Activation; a hide deactivates it only when its last holder is removed
    pub h123_shared_asset_object: bool,
    /// H125: SwapDisc(Node, Location, ViewPoint, View, var) = travel (C40): the new Node, Location, ViewPoint and View are
    /// registered (in that order) and transition_new(View, kind 0); the current View is left as by a transition, then the
    /// old ViewPoint, Location and Node are deactivated, the looping media players removed, and the new Node, Location,
    /// ViewPoint and View activated in that order; the variable is only read (disc number)
    pub h125_swap_disc: bool,
    /// H126: the dialogue box — DisplayDBox(CharView) (and a Load whose VCGameState names a CharView's open box) builds
    /// the CharView's f48 VCConversationList object (TL 0, posted, no Activation fire) unless the box of that CharView
    /// exists (another one is removed first); its command builds the list's enabled VCConversations (TL f3c), each
    /// activated; a Conversation's activation builds the first enabled AssetRef of its f2c. The CharView's Deactivation
    /// removes the box: each Conversation deactivated, then its asset
    pub h126_dialog_box: bool,
    /// H121 (static; the InterfaceItem hit test FUN_00439330, post-hoc fo-030): the shown layouts are walked from the most
    /// recently shown; in the first layout with an item containing the point, the item with the smallest z (the layout's
    /// base z + the key f4c & 0xff) wins, ties → earlier in the layout's sorted item container (strict `<`).
    /// InterfaceItems stay above the View's clickables (H77). Replaces H77's "earliest registered on top" when on;
    /// layouts set as View are not distinguished
    pub h121_iitem_key_order: bool,
    /// H122 (static FUN_00446790, post-hoc fo-029 / fo-030): a type-3 Disable (C15 flag change) of an InterfaceItem
    /// whose item object is built: removed from every shown layout containing it and the object removed at once →
    /// Deactivation of the item, then its asset (H123 holders)
    pub h122_iitem_disable: bool,
    /// H135 (static FUN_0046d0d0, `dbox.rs`): a Mouse Click on a line of the shown dialogue box hides the box and plays
    /// the first enabled AssetRef of the Conversation's f30, then f34, before its TL f3c event-0 posts; nothing to play →
    /// the Conversation is removed after the fire and the box stays shown
    pub h135_conversation_click: bool,
    /// H136 (static; the box's hit test FUN_004819b0, `dbox.rs`): the shown box's lines and close box are clickables
    /// (above nav-hotspots / Explorables, below InterfaceItems); the box body swallows other points
    pub h136_dbox_hit: bool,
    /// H137 (static FUN_004bdb70): a click in the close box removes the box and its Conversations
    pub h137_dbox_close: bool,
    /// H138 (static FUN_00476340): type-3 Enable of a member of the open box's list builds its Conversation; Disable of a
    /// built one removes it; H139: at the end of a played line a disabled Conversation is removed
    pub h138_conversation_enable: bool,
    /// H130 (static; the hit test FUN_00519e40, post-hoc fo-022..fo-030): the hit test is a fixed chain of categories,
    /// the first with a hit wins. Of the port's clickables: InterfaceItems of layouts not set as View (H121), then —
    /// after categories the port does not offer — those of layouts set as View by SetIFLAsView; then, only if the
    /// point's y is in the View rect's 90..330 (the View rect 20,90–620,330; x not compared): CharViews (kind 6, not
    /// offered), **Explorables** (kind 0x14), **nav-hotspots** (kind 5), each its app list from the head = earliest
    /// added (cmd 11 appends) first. Replaces A4 (most recently registered on top) for View-level clickables
    pub h130_hit_chain: bool,
    /// H140: the in-game save screen — the Esc menu's "Save" (FUN_00515b50) sets save mode and the "return to the menu"
    /// flag (1 unless its mode argument is 0 / 3 / 4) and posts an engine-built type-5 show of layout 0x14269 — the H58
    /// show path
    pub h140_save_screen_open: bool,
    /// H141: game start (FUN_0048cd70, new game and Load) removes every object of the app kinds 0x1e (layouts), 0x1f
    /// (items) … (27 kinds) — the shown layouts and their items / assets go without a traced Deactivation (fo-030
    /// 82.19–89.45: obj_remove only, the load screen 0x142ca)
    pub h141_game_start_layouts: bool,
    /// H142: save-mode slot procedures — SaveSlotSelect(slot) (FUN_005168a0, save mode): every save-slot item (0x1427c,
    /// 0x1427d, 0x1427e) gets a new text asset (as H72 a / H102), the selected slot is stored, markers 0x14281 / 0x14572
    /// moved (rects only); SaveKeyPress on a character key (not Tab / Down / Up / Enter / Esc / 0x29 / 0x2c / 0x2e)
    /// rebuilds the selected slot item's text asset — the key code is not in the traces; SaveLoadInit's slot assets are
    /// held by their items
    pub h142_save_slot_text: bool,
    /// H143: SaveLoadComit (FUN_00516ae0): arg 0 (cancel) and arg ≠ 0 in save mode when the save is written post engine-
    /// built type-5 hides — (0x14269 save mode / 0x142ca load mode, 1) for cancel, (0x14269, 1) for the save; then, with
    /// the "return to the menu" flag: (0x14269, 1), (0x142ca, 1) and a new main-menu object (not modelled). The
    /// "overwrite?" box (a null-owner registration) depends on the save files on disk and the user's answer — the core
    /// assumes the save goes through and does not emit the box
    pub h143_save_comit: bool,
    /// H145 (replay): a media end (the non-looping player removed, FUN_0048b8a0; the media cell names its AssetRef in f28)
    /// is an input — when the time comes is the media's wall-clock duration under the tracer — and the core deactivates
    /// the AssetRef it played (H84)
    pub h145_media_end_input: bool,
    /// H146: StopAllTimers (FUN_0049fde0) posts, for every timer of the timer list, an engine-built type-2 message (mode 1
    /// = stop, action 0); the core keeps the running timers (type-2 mode 0 / restored ones start, mode 1 stops, a
    /// one-shot's Timer fire ends it)
    pub h146_stop_all_timers: bool,
    /// H147: RandGen(var, n) (FUN_004ffb30): the variable is set to a random value in [0, n], written only if it differs
    /// (type 1) — for the core the value is unknown from then on
    pub h147_rand_gen: bool,
    /// H148: SetIsNode3Done / SetIsNode6Done (FUN_004ad480 for Node 3) write the variable bound by RegNode3Vars' 4th /
    /// RegNode6Vars' 2nd argument (value, type 2) from the RegCurrNode variable and the other bound ones: Node3 = (cur = 3
    /// ∧ var 0x1402 = 1 ∧ a1 = 1 ∧ ((a3 = 1 ∧ a2 > 2) ∨ (a3 = 0 ∧ a2 > 1))); Node6 = (cur = 6 ∧ a1 = 1); nothing unless
    /// every binding exists (RegNode3Vars binds only with all four variables, RegNode6Vars with both)
    pub h148_node_done: bool,
    /// H149: DispEvIcons(n) (FUN_004efb20) only sends state commands to the EvidenceIcon objects by their own flag and
    /// stores n ≠ 0 — no state command, registration or event follows any of the 7 traced calls (fo-027 / fo-028 /
    /// fo-029): the core's no-op
    pub h149_disp_ev_icons: bool,
    /// H150 (static; the icon bars' placement FUN_004373e0, `icons.rs`): icons (Action / Emotion / Evidence / Idea) and
    /// inventory items are clickables at the rect their bar gives them when they become active (bottom band y ≥ 330:
    /// ActionIcons, EvidenceIcons, EmotionIcons (shown only while enabled, C15), Inventory; top band y < 90: IdeaIcons)
    pub h150_icon_bars: bool,
    /// H152 (static FUN_0049c180): an inventory click selects the item (cursor object of its f38) and fires Mouse Click;
    /// a click on the selected item does nothing
    pub h152_inv_click: bool,
    /// H153 (static FUN_0049b410): OnInvClick(var) sets RegCurrInvVar's variable to the argument's value
    pub h153_on_inv_click: bool,
    /// H155 (static FUN_00436080): CharViews are View-band clickables (f2c region, above Explorables);
    /// a click fires Mouse Click (TL f54) then, with no inventory value held, DisplayDBox if a Conversation / IdeaIcon of
    /// the CharView is enabled
    pub h155_char_view_click: bool,
    /// HYPOTHESIS H1408 (replay harness): a traced Mouse Click (event 0) on a CharView also runs
    /// [`World::char_view_clicked`] in the trace-driven replay (the runner's Click command does; fo-162's H601 teleport
    /// click built no box before)
    pub h1408_replay_char_view_click: bool,
    /// HYPOTHESIS H1409: a clicked line's f34 asset in the f30 asset's movie file gets no player of its own — the f30
    /// player runs on to its end (11 / 11 traced same-file lines, 60 / 61 other-file lines play f34; see RE_NOTES)
    pub h1409_conv_tail_merge: bool,
    /// C231 (the original's saves: 5372 VCConversationHistory entries over 160 `data/bot/*.x`, every one a Conversation
    /// with HDB f40 = 0xff; the f40 = 0 rows clicked in fo-200 (0x7573 / 0x7576 / 0x757c) are absent, and the hint row
    /// 0x757c stays on the talk panel, `traces/fo-200.jsonl.shots/k13.png`): the line click's history bookkeeping
    /// (FUN_0046d0d0) runs only for a line whose f40 is set
    pub c231_conv_history_flag: bool,
    /// H160: the end of a played dialogue line shows the box again (FUN_0048b920; after the sequence's last player,
    /// media cell (10,4) not raised): through a View command the app's box gets state 1 if it is inactive and still has
    /// a line; else it is removed — that branch is not modelled (no traced case)
    pub h160_dbox_reshow: bool,
    /// H165 (static FUN_00467fb0, `devices.rs`): OnIntInvClick — PDA refused (var 0x149d set to 1) while a video is on
    /// screen; panel hidden (prefs 'H') and selection cleared
    pub h165_on_int_inv_click: bool,
    /// H166 (static; the phone procedures, FUN_004c7f00 and its neighbours): the phone's number buffer, display,
    /// phone-book cards, key mapping, PhoneSend's number table; DoActionListID
    pub h166_phone: bool,
    /// H167 (static FUN_004a1c70): Enable of the PDA 0x442b rebuilds the inventory, PDA first
    pub h167_pda_enable: bool,
    /// H168 (static FUN_0050a570): DeselectInv with a held object clears the selection
    pub h168_deselect_held: bool,
    /// H169 (static FUN_004525f0): the top-right engine object (501,0)–(640,34) opens the in-game menu (runner)
    pub h169_game_menu: bool,
    /// H170 (static FUN_00437050): prefs 'H' — inventory rects only while the panel is shown (runner)
    pub h170_inv_panel: bool,
    /// H171 (static; the five tool click procedures, FUN_004e1b30 …): On{Binoc,Camera,FLight,Gun,NVision}Click =
    /// OnInvClick's variable write + a held object of its type
    pub h171_held_tools: bool,
    /// H172 (static FUN_004969b0): right click = Double Click on the hit object (runner)
    pub h172_right_click: bool,
    /// H174 (static, partial): SetIItemFocus + key press → event 3 on the focused item (runner)
    pub h174_key_focus: bool,
    /// H176: SetIItemText(item, asset, …) (FUN_0047de90): a text AssetRef (kind 4) is shown in the item as H96's asset
    /// swap (old asset Deact, REG + Act of the new one) if the item's object exists
    pub h176_item_text: bool,
    /// H177: PageTurnSound (FUN_004b3930) posts action list 0x16934 with trigger 0x1692f
    pub h177_page_turn_sound: bool,
    /// H178: EnablePDANote(asset) adds the AssetRef to the game state's VCPDANotes list (FUN_004e6660; no trigger-level
    /// effect); the notepad procedures are logged
    pub h178_pda_notes: bool,
    /// H179: RelocateIFace(l, t, r, b, ctx 1) / RelocateIFacID(l, t, r, b, item) move a built InterfaceItem's hit rect
    /// (FUN_004be110); the RelocateIFace item = the trigger's owner (event-context key 1)
    pub h179_relocate_iface: bool,
    /// H180: display / debug / native-state procedures with no trigger-level effect (text panel, overlays, tints, empty
    /// workers, debug registrations)
    pub h180_native_noops: bool,
    /// H181: StopEnvSounds stops the audio-only players (the players with an empty display rect, FUN_004cc9c0),
    /// StopPlayAssets the video ones — as H100 mode 3
    pub h181_stop_players: bool,
    /// H182: EndGame(var) (FUN_004cc870): game end (or the auto-rewind box, var set to 0); GetPlayFBIShot / GetPlayBones /
    /// GetPlayCancer: var set to a once-only native flag (1 the first time, then 0), type 2
    pub h182_end_game: bool,
    /// H183: CharTeleport (FUN_004c56a0): only inside the current Node and if the clicked character has an enabled
    /// CharView in the target View — Location (if it differs) and View replaced, transition kind 0
    pub h183_char_teleport: bool,
    /// H184: IsIItemHeld(item, var) (FUN_004b3850): var = whether the mouse button is held on the item — 1 while a front
    /// holds the left button down on that item (H1814), else 0 (the core's instantaneous clicks), type 2
    pub h184_iitem_held: bool,
    /// H185: SetShanksVar(var) (FUN_004c4710): the 16 rules over the RegShanksNode2/3/6 bindings, type 2; Node 2 tail
    /// delivers e-mail 0x15978 and sets 0x13bc
    pub h185_shanks_var: bool,
    /// H186: e-mail lists (inbox / read / pending of the game state): SendEmail → pending, EmailFlushMess → inbox,
    /// EmailInit('a'/'b') posts its list when inbox ≠ read counts, GetCurrEmail / MarkEmailRead / EmailDelete /
    /// DisplayInBox / ScrollHeaders on the display list, InitEmailRead's engine enables
    pub h186_email: bool,
    /// H187: NukeAllIcons (FUN_0049f3c0, per object kind): active objects deactivated and removed (held object,
    /// EmotionIcons, inventory, layouts modelled)
    pub h187_nuke_icons: bool,
    /// H188: EnableKitIcon(icon) (FUN_004effd0): EvidenceIcon flag set to 1, object built (TL f40), posted → Activation
    pub h188_kit_icon: bool,
    /// H189: preference procedures — volume bars (engine enables + var = level·11, type 1), toggles (type 2), char
    /// cycles (type 0), PrefDefaults presets, AudioInit
    pub h189_prefs: bool,
    /// H190: search terminals — INGClear / INGInit / MediaInit / *KeyPressed (Enter posts; typed text logged)
    pub h190_terminals: bool,
    /// H191: login screens — LoginClick / LoginKeyPress buffers and focus, PswdConnect / PwbkOK checks against the text
    /// assets, message box (engine show 0xead3 + Enable); ReturnMainMenu / ReturnFromHelp → `Out::MainMenu`
    pub h191_login: bool,
    /// H192: shootouts — SmolGoonShot1 / SmGoonShoots variables (random → unknown), timer stop; DrawGunRauch → engine
    /// type-4 Set View (FUN_004b3bc0)
    pub h192_shootout: bool,
    /// H193: SetIsNode2/4/5Done, SetIsLink2/3Done (as H148), IsoChamberStat, RandGenRange
    pub h193_node_link_done: bool,
    /// H194: PlayAssetAt(asset, x, y) (FUN_00491de0): a non-looping player for the asset unless one exists (as type 1)
    pub h194_play_asset_at: bool,
    /// H195: PhotoInit's four engine enables, TakePhoto's count (the album is not modelled)
    pub h195_photos: bool,
    /// H196: PDA address-book procedures (logged: row tables not modelled)
    pub h196_addr_book: bool,
    /// H197: JournalInit / JournalFlip (pages of item 0xc10e in 0xb522 / 0xb523, two variables), BibleInit / BibleFlip
    /// (pages of 0x13c0b in 0x13be4 / 0x13be5, buttons 0x13be6 / 0x13be7)
    pub h197_journal_bible: bool,
    /// H198: SelectInv(item) (FUN_0050a390) — logged when the item is in the inventory (synthetic click not modelled)
    pub h198_select_inv: bool,
    /// H199: type-3 Disable of a VCInventory (FUN_00436c20): the object is dropped if held (selection cleared), deactivated
    /// if active and removed; with prefs 'V' the other active inventory objects get state 0 then 1 (re-placed)
    pub h199_inventory_disable: bool,
    /// H200 (fo-037 J1 / k11 trace, post-hoc): OnIntInvClick's selection clear (synchronous) runs before the panel's
    /// queued state-0 command (dispatched later) — so the clicked item is not "the selected one" when the panel hides:
    /// every inventory object leaves the bar and the next show re-places them in activation order (PDA first again).
    /// Off: the old order (panel hide first, the clicked item stays placed)
    pub h200_deselect_before_panel_hide: bool,
    /// H201 (fo-037 trace, post-hoc): PhoneButton's re-Disable of the key picture (an engine type-3 message built by a
    /// 100 ms native timer) is posted at the back of the queue — it ran after the whole chain queued at that time
    /// (fo-037 400.24 PhoneButton → 433.73 exec 0 type 3, after 0xf31b … 0x15556). Off: posted at once at the C24 insert
    /// point
    pub h201_phone_key_post_back: bool,
    /// H203 (static FUN_004c0490): ActionIcon type-3 Enable builds the object (REG TL f40, Activation with the H60
    /// assets) and its activation gives state 1 (bar 1); Disable takes the object out (state 0, removal → Deactivation);
    /// H66 start ActionIcons get their assets and state 1
    pub h203_action_icon_type3: bool,
    /// H204 (static FUN_004923b0; post-hoc fo-005..fo-026): EmotionIcon Enable gives an existing state-0 object state 1
    /// (no event) or builds a new one; Disable (flag change only) = state 0 + removal (H61's Deactivations); start icons
    /// (H66) get state 1
    pub h204_emotion_icon_type3: bool,
    /// H205 (static FUN_004a1940): EvidenceIcon Enable builds the object (no state); Disable of a state-1 one re-places
    /// the others (state 0 for all, removal, state 1 for the others); DispEvIcons(n) = state (n ≠ 0) for the non-kit
    /// icons
    pub h205_evidence_icon_type3: bool,
    /// H206 (static FUN_004a1420): IdeaIcon Enable builds the object only while the dialogue box is shown and its
    /// CharView has the icon in an IdeaMap (state 1, bar 0); Disable = state 0 + removal, the others state 1
    pub h206_idea_icon_type3: bool,
    /// H207 (static; the ActionIcon / EmotionIcon deactivation handlers, FUN_004c0610): an icon's Deactivation clears its
    /// VCEnabled flag, so a later Disable is no flag change (nothing) and a later Enable builds a new object. On in
    /// `all()` since H980: the original's fo-056-end.x has the clicked icons 0x6c47 / 0x6c58 / 0x6c5e at
    /// flag 0 with no Disable for them (a port save kept 1 and its Load showed the old choice's icons). Cost: fo-005's
    /// icon Deacts (H68 input missing there) are no longer paired by order with the later Disable's H61 Deacts (that
    /// Disable now does nothing, as in the trace)
    pub h207_icon_deact_clears_flag: bool,
    /// H260 (static; the string-resource reader FUN_00447650): the FBI login.
    /// (a) a kind-0 AssetRef's text is a string resource of XfilesT.dll (0xe231 = string 937, the login name, `pe.rs`);
    /// (b) LoginInit 'C' focuses the name field 0xc63b, or, when IFL_bHasLoggedInToFBIComputer 0x1352 ≠ 0, fills the
    /// name with upper(text 0xe231) and focuses the password field 0xc63c; (c) a successful PswdConnect does not run
    /// LoginInit 'C' (only the failure branch does, after the message box)
    pub h260_login_text: bool,
    /// H261 (H208's cause; static H59 + fo-054): (a) the Timer command fires event 7 only on subscribers whose object is
    /// alive (activated, not deactivated) — a View left earlier (0x4d5, subscribed to timer 5 through trigger 0xb6a2) is
    /// not visited; (b) replay: a periodic timer (blob byte 5 & 0x80, H59) is re-armed at each expiry instead of dropped
    /// (CharView 0x579c's Activation trigger 0xb6d4 → 0x14559 starts timer 5, 2000 ms, periodic, if 0xad1 == 0x25e)
    pub h261_timer_periodic_alive: bool,
    /// H262 (static; the box's row layout FUN_0041c4d0 + fo-055 screenshots + the saves' VCConversationHistory): dialogue
    /// rows — a clicked line goes to the conversation history
    /// (seeded from the save's 0x56 record) and is drawn in the HISTORY panel, not the talk panel; a row = a DrawText
    /// DT_WORDBREAK | DT_CALCRECT block (Courier New 12: 7 px per character, 212 px wrap width, 15 px per text line, rect
    /// as wide as its longest line), the next block 7 px below; box x clamped to the View rect 20..620
    pub h262_dbox_rows: bool,
    /// H263 (H210; post-hoc fo-056 / fo-057): a CharView wrapper's player (H74) is a media player like the others — it
    /// raises the media cells, runs its clip once and is removed at its media end (→ Deactivation). Runner: its asset
    /// joins the media FIFO, so the front-end ends it after the clip
    pub h263_char_view_player_end: bool,
    /// H280 (runner, post-hoc from the media players' state switches in every trace fo-008..fo-059): a sound
    /// player (.amv / .mus — not a video kind, H51) does not hold A3's one-player FIFO. Every player from the front of the
    /// FIFO up to and including the first video-kind one plays at once; a player queued behind a video waits for it.
    /// 12/12 starts that waited, waited for a video (.xmv); 13/13 starts with another player in state 1 had a sound
    /// there (e.g. fo-027 556.98 video 0x11867 starts while the mode-1 .amv 0x66a2 plays). Off: A3 (one player, FIFO)
    pub h280_sound_no_hold: bool,
    /// H320 (front-end; post-hoc over every trace with the player-state group, fo-008..fo-062): a View background (H53,
    /// View.f2c) is not a media player — 0 of 507 background registrations get a player state switch,
    /// against 167 of 173 registrations of type-1/7 play targets — so it is taken to make no sound. On: the front-end
    /// gives the background no audio cue. Off: the background's sound track loops with its picture (the port before)
    pub h320_background_silent: bool,
    /// H520 (player report "the Comity TV has no sound" + the bot's screen-change marks: the TV close-up 0xfcc3's
    /// 6 s background movie still changes 16–18 s after entry in fo-059 / fo-065 / fo-067 / fo-068; NOT traced for sound —
    /// the tracer mutes audio, E7): a View background that is a whole movie file (empty range, not a still; e.g. the
    /// Comity TVs 0xfcbd XV/64701.xmv, 0xfcbc XV/64700.xmv) is a running QuickTime movie, so its sound track plays,
    /// looped with its picture — H320's silence stays for ranged backgrounds (a sub-range of a story movie). On: the
    /// front-end gives such a background a looped audio cue even with H320. Off: H320 alone (every background silent)
    pub h520_whole_movie_background_sound: bool,
    /// H343 (static decode, NOT traced): switching closed captioning while playing (the key handler's 'C' case,
    /// FUN_0046c390) re-places the ActionIcon, Inventory and EmotionIcon bars at the other start (H150's (·,340) /
    /// (·,351), (10,420) / (10,425)). On: [`run::Runner::set_closed_captioning`] re-places the placed icons of bars 1–3.
    /// Off: the icons stay until re-placed
    pub h343_cc_relayout: bool,
    /// H360 (static; the CharView click FUN_00436080; post-hoc fo-065 k13: CharView 0x58cb with the badge held →
    /// refusal player 0x4e1f): a CharView click with an inventory value held — if no event-0 trigger
    /// action of its TL is conditioned on that value (the reaction test), a refusal slot is picked by the value's class
    /// and the slot's AssetRef (from the disabled first Conversation of f48: its f38 list; slots 7 / 8 = 0x4cc8 / 0x4cc6)
    /// is played; an empty slot / slot 10 → DisplayDBox as with nothing held. Off: logged only (the port before)
    pub h360_char_view_held_click: bool,
    /// H361 (static FUN_00468330; post-hoc fo-060 k11 161.645, fo-065 k14 128.577 held drop): the pointer entering the
    /// bottom strip (0,420)–(640,480) from outside while an object is held drops it (selection cleared, the selected
    /// item taken out of the bar) before the panel is shown (runner)
    pub h361_panel_roll_in_drop: bool,
    /// H362 (static: the deactivation handler of every kind-0x18 held-object class, FUN_00437c20; end saves after the
    /// Esc drop hold 0x15a = 0, fo-065 k14 re-posts the held object): a dropped held object's Deactivation sets
    /// RegCurrInvVar's variable back to its default (Inventory_None) — the "deselect" of the item value (runner drop
    /// paths)
    pub h362_drop_resets_inv_var: bool,
    /// H363 (post-hoc fo-065 128.577–128.578: cursor 0x7c8c removed → Deact, then its picture 0xbb1c removed → Deact; REG
    /// of both again at the next selection 128.638 / 128.639): removing the selection's cursor object (FUN_004b0710)
    /// also deactivates the cursor's picture AssetRef
    pub h363_cursor_asset_deact: bool,
    /// H403 (static; the save-screen procedures, FUN_00516280 SaveKeyPress among them): the save / load
    /// screens' native data — slot names (`World::save_names`, set by the front-end), the typed name buffer
    /// (SaveSlotSelect clears it, SaveKeyPress edits it: Tab / Down next slot, Up previous, Enter = SaveLoadComit(1),
    /// Esc = SaveLoadComit(0), other keys typed), SaveLoadComit(1) → `Out::SaveGame` / `Out::LoadGame`, and with the
    /// "return to the menu" flag a new menu object (`Out::MainMenu`) after the hides
    pub h403_save_screen_data: bool,
    /// H470 (static, `save::build`): a game started from XFiles.gam is saved by building the save's classes from
    /// XFiles.gam itself (no save of the original needed); off = H401's template
    pub h470_save_builder: bool,
    /// H450 (static; the hit chain's band category: the held object of type 7 after CharViews / Explorables /
    /// nav-hotspots): a View click with the gun held that hits nothing else = a shot: the current Location picks a
    /// game-over video (0x4cc6 / 0x4cc8 / 0xa83e, or none), the click goes on to the View object's Mouse Click, then the
    /// video's player is posted (owner the gun) (runner + world)
    pub h450_gun_view_click: bool,
    /// H451 (static FUN_004e2160, called by OnBinocClick / OnCameraClick / OnFLightClick / OnNVisionClick when the value
    /// changes; post-hoc fo-073 k1: timer armed 57.831, REG 0x4d52 59.833): selecting the
    /// binoculars / flashlight (refusal slot 2) or night vision (slot 3) plays the first CharView of the View's slot asset
    /// 2000 ms later (a "don't point that at me" comment) (runner clock)
    pub h451_tool_comment: bool,
    /// H452 (static FUN_0045a0a0, H130 category 7: active players whose display rect holds the point come before the View
    /// band; a player's click handler is a no-op; post-hoc fo-073 k2: a click on Cook at 61.607 while 0x4d52 played
    /// (player state 1 59.94 .. media end 62.691) gave no Mouse Click; H322): a playing video swallows a click in the
    /// movie area (runner hit test)
    pub h452_video_player_hit: bool,
    /// H820 (static; the CharView's hover handler FUN_00443010; post-hoc fo-081 61.62 REG 0x4e15 on the clerk, fo-091
    /// 104.993 REG 0x4e25 on Skinner): the pointer moving onto a CharView while the gun (9), handcuffs (10) or value 0x17
    /// is held plays its refusal slot 5 (unless a Roll Into trigger reacts to the value) — the ~1 s hover video that
    /// swallows a click made during it (H452)
    pub h820_char_view_hover_refusal: bool,
    /// H821 (static: the application start FUN_0041b060 names AssetRef 0x87b7 as the gunshot; trace fo-091 101.148 REG
    /// 0x87b7 at the gun's activation; post-hoc fo-082: View Mouse Click 1.845 s after the button-down): the gun's shot
    /// is AssetRef 0x87b7 (the gunshot); a View shot plays it (to its end before the View's Mouse Click and the video
    /// when the Location has one), a CharView refusal with the gun plays it to its end before the refusal player.
    /// Refutes H453's "the shot is empty". Off: no shot (H453)
    pub h821_gun_shot_sound: bool,
    /// H823 (H182 static FUN_004cc870: two game-state flags cleared, then the game-end object; C71 / fo-091: post 0xc0f2
    /// EndGame → the main menu, its 4th item reading Return — no return.pic): a game end without the rewind box shows
    /// the main menu with no game running (no return.pic, no Save item). Off: the game end is only logged
    pub h823_game_end_main_menu: bool,
    /// H570 (static FUN_00468f00; post-hoc fo-069 k22, fo-070 k23): LoginInit 'C' fills the name and focuses the
    /// password field also when the preference Pref_bArtificialIntuition 0x369 is on
    pub h570_login_art_intuit: bool,
    /// H573 (static; the text-asset builder FUN_004be1e0): SetIItemText / the record texts show kind-0 string resources
    /// (the ING field labels: name, agency, …) as texts
    pub h573_string_text: bool,
    /// H574 (static FUN_0047de90): SetIItemText's style (colour 'b' / 'w', 'H' = Arial bold else Courier New, size, wrap
    /// 'y', align 'c' / 'l' / 'r') is kept for the front-end
    pub h574_item_text_style: bool,
    /// H575 (static; the ING procedures, FUN_00486470 the typed text among them, and the exe's ING record tables): the
    /// ING terminal — typed text, search by name / phone / plate, record display
    pub h575_ing: bool,
    /// H576 (static FUN_004be560; ScrollTextInit): text panels (ING field notes, the e-mail reader)
    pub h576_text_panels: bool,
    /// H577 (static FUN_0048c680): the inbox rows' header lines and read markers, the read mail's text
    pub h577_mail_rows: bool,
    /// H578 (static FUN_004867e0; fo-069 k22 caret pixels): caret items follow the typed text
    pub h578_carets: bool,
    /// H500 (display; static: the hit chain tests the layouts' items before the box's category; post-hoc fo-054 429.42 /
    /// fo-067 65.47: a document layout shown at the end of an answer while the box gets state 1 again; player report):
    /// shown IFaceLayouts' items are drawn over the dialogue box (front-end only)
    pub h500_layouts_over_dbox: bool,
    /// H501 (static FUN_0041c740, the first branch H163 described): after a played line, a box on its TALK tab with no
    /// talk line left whose CharView has no enabled IdeaIcon is removed, so the next CharView click builds a new box
    /// (DisplayDBox)
    pub h501_dbox_removed_when_empty: bool,
    /// H502 (static; the box's state handler FUN_0043ead0: talk / history panels registered under keys 0 / 1, the
    /// history one selected when no talk line is left; a click on a panel's tab selects it): the box opens on the
    /// HISTORY tab when no talk line is left; the TALK / HISTORY tabs switch the panels; the history panel lists the
    /// box's Conversations in the history set
    pub h502_dbox_history_tab: bool,
    /// H503 (static; the line click FUN_0046d0d0): a click on a history line replays it — the history entry's count + 1,
    /// box state 0, f30 then f38 then f34 (the `first` path), then the Mouse Click fires as for a talk line (TL f3c event
    /// 0)
    pub h503_history_replay: bool,
    /// H1810–H1813 (static; the panels' scroll handler FUN_00481ec0): the box panels scroll — the up / down arrows a line,
    /// the bar above / below the thumb a page, the thumb dragged; the lines' hit rects follow the scroll and are clipped
    /// to the panel
    pub h1810_dbox_scroll: bool,
    /// H601 (post-hoc fo-067 k10 / fo-068 k18 + static: the media wrapper's removal handler FUN_0047e830, H86): the
    /// CharView a CharTeleport targets gets, when its media wrapper is removed, a mouse command at the centre of its f2c
    /// rect — hit-tested like a click (runner + world)
    pub h601_teleport_click: bool,
    /// H860 (static; the type-4 handler FUN_0046a950; post-hoc `traces/fo-110.jsonl` 71.876–71.877, k7 action 0x14b20 →
    /// View 0x869: three objects removed, REG Location 0x6b1, REG ViewPoint, transition_new): Set View's blob Node /
    /// Location / ViewPoint are honoured — a changed Node rebuilds Node, Location, ViewPoint, View; a changed Location
    /// rebuilds Location, ViewPoint (even with the same objId), View; the old ones are deactivated as in a CharTeleport
    /// travel (H183). Off: only the View (and ViewPoint) change
    pub h860_set_view_location: bool,
    /// H621 / H622 (static FUN_00508e40, save fields f54 / f58; screenshots of fo-056..fo-080): SetAIList /
    /// ClearAIList / SetAIGlow keep the AI icon's list, trigger, glow and target View (`world/ai.rs`); the icon is lit
    /// while it glows or a list action's condition holds, dimmed otherwise
    pub h621_ai_icon: bool,
    /// H623 (static FUN_005091d0, the game's help XT/80249.xtx; NOT traced, and the hit chain tests bar 6 only
    /// for a 400-high View picture): the AI icon is clickable in the top band — glowing →
    /// Set View to the target View, else the next hint of the list is posted
    pub h623_ai_click: bool,
    /// H1923 (C292: `traces/fo-220.jsonl` 57.754, the ActionIcon 0x5ffe's movie started at the pointer's arrival; static
    /// FUN_004631b0): a movie-kind icon picture without a roll-over picture plays only while hovered
    /// (front `ui::draw_bars`); off = H311's always-looping movie
    pub h1923_icon_movie_on_hover: bool,
    /// H624 (static; the note-pad procedures, FUN_004e6220 …, and the exe's note-pad tables; NOT traced): the
    /// PDA note pad's day, day count, scroll and address link (`world/pda.rs`); the front draws the day's notes
    pub h624_note_pad: bool,
    /// H620 (display; screenshots fo-057 k13, fo-076 k05, fo-077 k29 of the original, post-hoc): under a shown
    /// layout with a full-screen InterfaceItem the scene (View, Explorables, earlier layouts, bars, dialogue box) is
    /// drawn darkened (`front::pda`)
    pub h620_dim_under_layout: bool,
    /// H650 (static; the condition evaluator FUN_005d5ef0; the original's 47 saves hold scGameLanguage 0xf9d =
    /// 'english'): `==` / `!=` with a string VCVariable operand compare the texts (byte compare, "" = NULL; a
    /// 1-character string against a number by its character), and SetLanguage writes its AssetRef's text ('english',
    /// XfilesT.dll string 1091) into the variable. Off: raw VCString objIds are compared and SetLanguage does nothing
    /// (H92)
    pub h650_string_compare: bool,
    /// H651 (post-hoc `traces/fo-066.jsonl` 242.893–242.944, k16: the answer's last asset 0x74aa Deactivation posts
    /// 0x74ae / 0x74af / 0x74b1 / 0x74b0 (Enable Conversations 0x5874 / 0x5877 / 0x587d / 0x587a, each REG), 0x8288,
    /// 0xb7e6, 0xd0b1, 0xf8f3 — all dispatched before the Conversation's removal command (0x586e) and the View command
    /// that gives the box state 1 with the four new lines): the end of a line's
    /// sequence (H139 / H160 / H501) is a queue command behind the last Deactivation's posts, not done at the
    /// Deactivation. Off: decided at the Deactivation (a line whose follow-ups are enabled by its last asset's
    /// Deactivation found no line left and the box was removed — H215)
    pub h651_dbox_reshow_queued: bool,
    /// H700 (static; NOT traced — no trace has an IdeaIcon): the dialogue box's state handler FUN_00435d90
    /// builds, on state 1, every IdeaIcon enabled in table 9 that an IdeaMap of the box's CharView f4c list names (built,
    /// posted, given state 1); on state 0 every IdeaIcon of the box except the dragged one gets state 0 and is removed.
    /// Off: IdeaIcons enabled while no box is shown never appear (H206 builds them only inside a box)
    pub h700_dbox_idea_icons: bool,
    /// H701 (PROVISIONAL port model — the original's drag / drop is NOT decoded, RE_NOTES H701): a click on an
    /// IdeaIcon picks it up (after its Mouse Click); the next click, if on the box's CharView whose f4c IdeaMapList maps the
    /// icon (IdeaMap f28) to a Conversation (f2c), builds that Conversation (REG TL f3c, Activation) and plays it like a
    /// clicked line (H135) instead of the CharView's Mouse Click; any other click just drops it. Off: an IdeaIcon click does
    /// nothing more than its Mouse Click (the port before)
    pub h701_idea_drop: bool,
    /// C107 (traced, `traces/fo-124.jsonl` 63.96–64.37; C104–C106, C110; replaces H701's click-click model, REFUTED as the
    /// required gesture): one plain click on an IdeaIcon of the shown box plays its answer. Button down → the icon's Mouse
    /// Click, a drag object, box state 0 and every other IdeaIcon removed (H700, the dragged one spared); button
    /// up on the icon → the IdeaMap's f2c Conversation (of the box's CharView f4c list) built (REG TL f3c), the icon's event
    /// 6 (TL f40 = 0: nothing) and removal, the Conversation's Activation, then played as a clicked line (H135); its
    /// sequence's end removes it and re-shows the box, which rebuilds the icons (69.317: box state 1, REG of the IdeaIcons;
    /// the icon is not used up, R38). Off: H701 (if on) or the icon's Mouse Click only
    pub c107_idea_click: bool,
    /// H922 (post-hoc `traces/fo-127.jsonl` 40.262–40.268, the Load's box of Wong 0x5bec: REG of the five
    /// Conversations, the box's state command posted (40.263), the Conversations' Activations register their assets
    /// (40.264–40.266), then the box's state handler runs (rows 40.266) and builds the IdeaIcons (40.267)): the state-1
    /// command that ends the box's
    /// command (H136) is queued behind the Conversations' Activations, not run at once. Off: at once (H136's approximation)
    pub h922_dbox_show_queued: bool,
    /// C61 (confirmed, `traces/fo-076.jsonl`: the stereo clicked twice 8 s apart → one REG 0xcc6b; H321's static decode
    /// of the play action, FUN_0047a4e0; again `traces/fo-127.jsonl` 119.102: 0x8b1e executed, no REG of 0x5bbe whose player
    /// from 114.916 ended at 119.927): a type-1 / type-7 play of an AssetRef whose player still exists (registered by a
    /// play, no Deactivation since) creates no player — no REG, no Activation. Off: a second REG + Activation (the port
    /// before)
    pub c61_one_player: bool,
    /// H702 (port save fix; no original save has an EvidenceIcon to compare with): a save writes the built
    /// EvidenceIcons (ldc [id, state 1], l118 kit ids) instead of the base save's lists, and a Load marks as kit only the
    /// l118 ones. Off: the base's lists are kept and an ldc entry with active = 0 loads as a kit icon (the port before)
    pub h702_evidence_save: bool,
    /// H790 (static; the media-index procedures, FUN_004f0ae0 MediaSearch among them, and the exe's article
    /// table; NOT traced): the media index — MediaInit / MSKeyPressed (typed key words, max 0x19, caret 0xfbba) /
    /// MediaSearch (key words of 16 key items → the article in the text panel, Forward 0xd6fc, the 0xd927 asset enable)
    pub h790_media_index: bool,
    /// H791 (static; the text scroller FUN_00550c40; NOT traced): ScrollTextList fills the (one) main text
    /// scroller with an asset's text in an item's rect; MainScroll 0..3 scrolls the computer's text panels by a line /
    /// a page (the front measures the text)
    pub h791_text_scroll: bool,
    /// H792 (static: the PFF animation reader FUN_0040b6c0, XG/69328.pff header; display): a caret item's
    /// picture is the 2-entry PFF animation (black bar 500 ms, white 500 ms) looped from the item's show — the caret blinks
    pub h792_caret_blink: bool,
    /// H793 (static; the photo-album procedures, FUN_004e2870 …, and the exe's album table; NOT traced): the
    /// photo album (TakePhoto adds a VCPhoto of a picture,
    /// Download / Enlarge / Delete / Scroll; thumbnails 80×60, enlargement 160×120)
    pub h793_photo_album: bool,
    /// H794 (static FUN_004d8c60 and the exe's dot-item table; NOT traced): SBaseInit / SBaseKeyPress — a dot
    /// item per typed character, the caret 0xfbbc
    pub h794_sbase: bool,
    /// H795 (static: the plate search FUN_004fa880 and RegUberVars): the first NSA plate search adds 2 to
    /// iUberParanoia (0x35b, type 1) and writes 0x1269 as type 2
    pub h795_uber_paranoia: bool,
    /// H796 (static FUN_0042a120; NOT traced): ForwardObject 'I' / 'M' fills the compose screen's subject
    /// (0x9df5) and enclosure (0x9df4, "MKM1013.DAT" … cycling) / body panel, Send 0x9e36 disabled
    pub h796_forward: bool,
    /// H797 (static; the address-picker procedures, FUN_00475170 …, and the exe's address / reply tables; NOT
    /// traced): the computer's address picker (To / Cc rows, selection markers, scrolling) and EmailSend's reply mails
    /// for a forwarded ING record / media article (the PDA's 'P' / 'D' / 'N' paths are not modelled)
    pub h797_addr_lists: bool,
    /// H799 (display; fo-069 k22 / k23 + the decode that a shown object keeps its place in the draw order):
    /// under a dimming layout (H620) the items of earlier layouts that are not as-View close-ups are drawn above the icon
    /// bars — the AI icon stays hidden under the computer's full-screen layouts when a second layout (the address picker
    /// 0x9dd3 / 0x9df8, the message box) opens over them. Off: bars over every earlier layout (as H620 had it)
    pub h799_layouts_over_bars: bool,
    /// H730 (`shootout.rs`): the Smolnikoff shootout's native state — DoSmolShootout's target / View,
    /// SmolGoonShot1's flags, SmIsWillShot1 / SmGoonShoots' hit draws (live generator H550; unknown → H192)
    pub h730_smol_shootout: bool,
    /// H732: a Load shows the save's VCIFaceLayout (the VCGameState's as-View layout, H67 (a)) as a
    /// SetIFLAsView layout
    pub h732_load_layout: bool,
    /// H733: FadeToBlack stalls the main loop for 2000 ms (`Out::Stall`; fo-082 post-hoc)
    pub h733_fade_stall: bool,
    /// H734: SelectInv(item) clicks the inventory item (drop held, panel shown, H152 click command)
    pub h734_select_inv: bool,
    /// H736: SetFLightProps with the flashlight held re-selects it (SelectInv on the held item, as H734)
    pub h736_flight_reselect: bool,
    /// H890 (static; the timer pause / resume FUN_0048e640; traced `traces/fo-119.attempt1/fo-119.jsonl` (C98) and
    /// `traces/fo-118.jsonl` 91.55–173.85 (C101)): timer pause / resume. Pause kills every running timer and stores its
    /// delay — a periodic timer's full period, a one-shot timer's remaining time; resume re-arms every paused timer with
    /// that delay from now. The PDA click pauses (OnIntInvClick sets the pause flag); the scheduler's end resumes once no
    /// layout is shown. A View transition of kind 1..4 runs the transition effect: pause + resume — a periodic timer
    /// restarts its period
    pub h890_timer_pause: bool,
    /// H892 (static FUN_00550f80; R35 `traces/fo-114.jsonl.shots/k16.png`): MainScroll 2 (page up) goes to
    /// the top when the text is scrolled by at most the view's height less min(line, gap)
    pub h892_page_up_top: bool,
    /// H894 (display; R37 `traces/fo-116.jsonl.burst/k15-0.png`, post-hoc, mechanism not decoded): only the first shown
    /// dimming layout darkens what lies under it; a later layout (the address picker over the compose screen) does not
    /// darken the earlier layouts' items
    pub h894_dim_once: bool,
    /// H895 (static: the game start FUN_0048cd70; `traces/fo-112.jsonl` 83.382): the game-end object's pass through the
    /// game start hits the game-start media reset but registers nothing (its mode 1)
    pub h895_game_end_no_start: bool,
    /// H896 (replay only; `traces/fo-115.jsonl` 168.24–176.83): the traced key events (key_in) give the KeyPress
    /// procedures their key code
    pub h896_replay_keys: bool,
    /// H1060 (static FUN_00493090 — every player of the app's player list removed when the SwapDisc object
    /// runs; bug report "the warehouse music kept playing at the Crime Lab"): a travel removes the played
    /// (non-looping) players of the media FIFO too, not only the looping ones — their AssetRef's Deactivation as for a
    /// looping player
    pub h1060_travel_stops_played: bool,
    /// H1123 (H160 static: the re-show (FUN_0041c740) returns while media cell (10,4) is raised;
    /// `traces/fo-136.jsonl` 216.02..240.404): a played line whose last asset's Deactivation starts further plays leaves
    /// the box hidden (no H160 show, no H501 removal) — the View's re-show command runs again after the last of those
    /// players ends
    pub h1123_dbox_reshow_after_media: bool,
    /// H1122 (static FUN_0046f210, `traces/fo-133.jsonl` 127.93..127.934): a type-3 Enable / Disable of a
    /// background AssetRef changes nothing while a type-4 (SetView) action message is still queued (the queue is walked
    /// for an action of type 4); else the background is re-chosen (H53)
    pub h1122_bg_skip_on_queued_set_view: bool,
    /// H1040 (static FUN_00517ea0, confirmed post-hoc C-BX1 fo-132 k05):
    /// SetLBoxTint(r, g, b) sets the renderer's View tint, ClearLBoxTint clears it (core: `Devices::lbox_tint`; the
    /// front draws it, H1041)
    pub h1040_lbox_tint: bool,
    /// H1181 (C151 / C152, `traces/fo-161.jsonl` 108.32 / 121.31, `traces/fo-162.jsonl`): H123's one
    /// runtime asset object per AssetRef holds for icon objects too (the icon activation, FUN_00470b40): an
    /// icon whose asset object already exists (held by another active icon or shown item) only re-posts it — no
    /// registration, no Activation; the icon's removal deactivates it only when no other holder is left
    pub h1181_shared_icon_asset: bool,
    /// H1220 (`traces/fo-168.jsonl` 183.799, C157): a type-3 Disable that leaves no member of the current
    /// View's background list (f2c) enabled deactivates the current background at once (static: the AssetRef case of the
    /// type-3 handler, FUN_004201a0, runs the background command). Off: the background is only forgotten
    pub h1220_bg_none_deact: bool,
    /// H1203 (static FUN_00428d50 + CrossOver winmm `timeSetEvent`, C159 / C160): a timer whose delay is
    /// outside 1..=65535 ms gets no native handle (timeSetEvent returns 0) — it stays in the timer list (saved as
    /// pending), never expires, is skipped by the pause and re-tried (and fails again) by every resume. Off: such a
    /// timer runs (Windows NT behaviour: wPeriodMax 1000000)
    pub h1203_timer_max_delay: bool,
    /// H1281 (static FUN_00419e10): a playing video's `.HOT` hotspots
    /// (front input `Runner::movie_hotspots`) are click targets between t0 and t1 of its movie time, hit-test category 6
    /// (after the dialogue box's, before the players' and the View band), rect + (20, 90), resized by the action-scene
    /// difficulty. Off: no movie hotspots
    pub h1281_movie_hotspots: bool,
    /// H1282 (static: the movie-hotspot click handler): a goon target (221–223) acts only while the gun (held type 7)
    /// is held. Off: any click shoots
    pub h1282_movie_hot_gun: bool,
    /// H1322 (C166 / C168 / C170 / C171, fo-178 / fo-184 / fo-185): a hit of a goon (221–223) or barricade (211)
    /// target removes the View at the button-down (the View's and its movie's Deactivation, its nav-hotspots after the
    /// posts; the target goes with the movie) before the kill movie; the stairs gunman (201) re-sets the View
    /// (transition_new of the same View). Off: the kill movie only (the goon movie and the View stay)
    pub h1322_movie_hot_view_removal: bool,
    /// H1520 (static: the gun-hit handler FUN_004b4a20; traced cases C210 / C211 / C214): every target id
    /// of the movie-hotspot click handler runs its gun-hit case (NSA shootout 100–104, the two-man scenes 111–142, Secret
    /// Base 401–470, the shot-anything ids 500 / 600 / 666). Off: only the ids traced before (201 / 211 / 221–223)
    pub h1520_movie_hot_all_cases: bool,
    /// H1420 (static: the binocular view switch FUN_0051bc70, run by OnBinocClick and by the binoculars'
    /// Deactivation; C230, `traces/fo-198.jsonl` 123.43 / 154.899): selecting the binoculars (OnBinocClick, mode 1) and
    /// the binocular object's Deactivation (mode 0) switch the View's background by RegViewSpecial's variable (0x421):
    /// 0x1ec7 ↔ 0x1ec6, 0x4ce8 ↔ 0x11436, 0x4ce4 ↔ 0x11438, 0x4cec ↔ 0x118ea — two engine type-3 messages: Disable the
    /// current background, Enable the other AssetRef (the H53 re-choice starts it). 0x5bbe ↔ 0x11432 swaps a playing
    /// player (logged, not modelled). Off: OnBinocClick changes no background
    pub h1420_binoc_view_swap: bool,
    /// H1500 (H1462, `traces/fo-206.jsonl` 65.991–69.721; static FUN_00446790): a layout's items are
    /// activated one by one from the queue (the show posts each built item; H89: an item's Activation and everything it
    /// posts run before the next item's), and an item enters the app's kind-0x1f list only in its own cmd 11, posted by
    /// its Activation. H122's Disable looks the item up in that list: an item built but not yet activated is not found
    /// — nothing is removed, the item stays in the layout and is activated later (fo-206: InitEmailRead's Disable of IN
    /// BOX 0x116cc at 66.00, its Act 66.010, Deact at the hide 69.721). Off: the Disable removes a built item whose
    /// Activation is still queued
    pub h1500_iitem_disable_needs_active: bool,
    /// H1581 (static save: every periodic timer of the original's 25 saves holds its period, 2000 / 45000 / 1000 / 20000
    /// ms — the pause's stored delay, H890): a port save stores a running periodic timer's period, not its remaining time
    /// (the Load arms [id][ms][periodic] with period ms, H67). Off: the remaining time (a Load then fires it every few ms)
    pub h1581_save_timer_period: bool,
    /// H1582 (`traces/fo-110.jsonl` 127.449 EnableKitIcon 0xcaa1 → REG 0xca9b; fo-110-end.x l118 = [0xca9b]): the kit
    /// EvidenceIcon built by EnableKitIcon is a kit icon object of the app list (saved in l118, DispEvIcons / Disable
    /// see it). Off: built without an icon object (a port save lost it)
    pub h1582_kit_icon_listed: bool,
    /// H1585 (fo-196-end.x / fo-090-end.x l64, see `World::player_saved`): a save lists every type-1 / 7 player still
    /// playing, not only the looping ones. Off: looping players only (H405)
    pub h1585_save_played_players: bool,
    /// H1586 (fo-060-end.x: the VCGameState's as-View layout = 0x14968, the layout fo-060 set as View; H732 shows it at
    /// the Load): a port save writes that field = the shown SetIFLAsView layout (0 = none). Off: the base save's value
    pub h1586_save_view_layout: bool,
    /// H1600 (static: the save FUN_004d9450 writes the player's looping flag as the l64 flag — 0 for a mode-1 player, 1
    /// for a mode-2 one; saves: fo-137-end.x 0x6f62 / 0, fo-196-end.x 0xda7e / 0, fo-090-end.x 0x6f60 / 0, every 0x76bf
    /// / 1): a port save writes a player's flag = 1 only for a looping player. Off: 1 for every saved player (the Load
    /// then made a restored mode-1 sound loop, H1587)
    pub h1600_save_player_flag: bool,
    /// H1601 (`traces/fo-138.jsonl`: the restored 0x6f5e / 0x6f62 of fo-137-end.x start — player state 1 40.292 /
    /// 40.382 — and 0x6f62, saved at 0x292e of its 10586 ms, ends 91 ms later: player state 0 40.473): a restored player
    /// plays (it enters the media FIFO like a played asset) from its saved position. Off:
    /// it is registered and activated but never plays, so it never ends
    pub h1601_restored_player_plays: bool,
    /// H1602 (static: the VCPDANotes (0x58) and VCConversationHistory (0x56) lists' add is an ordered insert by objId;
    /// saves: fo-067-end.x conv 0x587d before 0x5bc7 (added in-session), notes ascending in all 178 bot saves; a Load
    /// keeps the file's order, fo-085-end.x conv 0x5670 last as in the port-written save it loaded): a note / history
    /// line is inserted before the first larger id (appended when not smaller than the last). Off: appended (enabling
    /// order)
    pub h1602_sorted_lists: bool,
    /// H1603 (static: the save FUN_004d9450 writes the three flashlight globals into the VCGameState; the Load sets them
    /// back): SetFLightProps' flashlight globals (`Devices::flight`) are in the save — a Load restores them, a port save
    /// writes them (once known). Off: not restored (0 until the first SetFLightProps), the base save's bytes written
    pub h1603_flight_saved: bool,
    /// H1700 (C244 / C245 / C246–C250 / C253 / C254 / C255, H1633; static FUN_004cc870): EndGame with the box (var ≠ 0,
    /// AutoRewind on) shows the modal "Try again?" box (AssetRef 0xd3b8, a null-owner REG) instead of ending: the game
    /// stands still; YES = a Load of the rewind snapshot (the state of the last SaveGameState since the last Load / new
    /// game, else the loaded save itself), NO = the game end (the main menu, no game running, as H823). SaveGameState
    /// takes the snapshot (`Runner::rewind_snap`). Off: the box is only logged and the game runs on
    pub h1700_rewind_box: bool,
    /// H1703 (static: the gun-hit handler's freeze cases, FUN_004b4a20 — the timers paused and every object of the app
    /// lists but kinds 0x15 / 0x19 / 0x1b / 0x25 frozen; C251: the suicide note, no event for 50 s): the game freezes —
    /// timers paused, no media time, no world input (`Runner::frozen`); the menu still opens, and its Return lifts the
    /// freeze. Off: logged only
    pub h1703_suicide_freeze: bool,
    /// H1900 (static: the hit chain and the players' click / cursor handlers, FUN_0045a0a0; C60, C220, C132): while a
    /// video
    /// plays, a left click goes through the hit chain like any other — the playing player's display rect takes the movie
    /// area (H452: no-op, null cursor), the categories above it (non-View layouts' items, the dialogue box — timed boxes —,
    /// movie-hotspot targets — the shootouts) and the bands outside it (icon bars, inventory: C220's stiletto during the
    /// lure stills) keep working; EmotionIcons the script enables during a video (C132 Cook +15 s, fo-195 / fo-227 /
    /// fo-228 Mr. X +9 s) are clickable. Front (`host::button`); needs H452. Off: every left click during a video is
    /// dropped (the port's earlier H322 gate)
    pub h1900_video_click_hit_chain: bool,
    /// H1901 (port choice from a player report "no nav-hotspot during movies"; the movie's display rect is not
    /// decoded): a playing video's player rect (H452) is the View band's full width (0, 90)–(640, 330), so the
    /// nav-hotspots reaching the screen edges (x < 20, x ≥ 620) are covered too — no nav cursor, no nav click. Off: the
    /// 600×240 movie area (20, 90)–(620, 330)
    pub h1901_video_band_full_width: bool,
    /// HYPOTHESIS H2170 (static: the hit chain's category 3 + the camera's click handler FUN_004f26a0; no trace has a
    /// View click with the camera held): the held camera (kind 0x18, type 2, rect = its viewfinder in the View band)
    /// takes a left / right click in the band above the dialogue box, movie hotspots, a playing video, CharViews,
    /// Explorables and nav-hotspots (only the InterfaceItems of non-View layouts are above it). With its overlay flag set
    /// (iCurrViewSpecial ≠ 1 at the selection, `Devices::held_overlay`) the click is a shot: an exposure used (the game
    /// state's camera count + 1 below 24 — the plate photo's TakePhoto needs one left), then each CharView, then each
    /// Explorable under the viewfinder that reacts to the held camera (H154 test) gets its Mouse Click; the first CharView
    /// with a refusal slot 2 plays it. Never a navigation, never the sedan's photo (that is the camera icon's click list
    /// 0x7ccd, C108). Off: the held camera is ignored by the hit test (clicks go to the View's objects)
    pub h2170_held_camera_click: bool,
    /// H1863 (static FUN_004f9fc0 + C251's shot): SuicideNote shows one of Suicide1..3.txt (0xf594 / 0xf596 /
    /// 0xf598, random 0..2, the live generator; 0xf598 without it) in the note's activated item. Off: logged only
    pub h1863_suicide_note_text: bool,
    /// C86 (fo-108, `traces/fo-108.jsonl`; H493 / H1990): New Game takes the host's Preferences (the original: the
    /// registry `HKCU\Software\Fox Interactive\The X-Files\Preferences`, read back into the 24 RegPreferences
    /// variables) instead of XFiles.gam's values — the front's [`crate::front::Session`] keeps them across New / Load
    /// (`host_prefs`) and writes them at the cold start and at every New Game. Off: XFiles.gam's values (0x369
    /// Pref_bArtificialIntuition = 0 → the AI-only ActionIcons, e.g. 'call Amis' 0x600d at the Tarakan, never offered)
    pub c86_new_game_host_prefs: bool,
    /// C296 (H151 / H2010; `traces/fo-111.jsonl` 689.199–689.447, `traces/fo-220.jsonl` 57.818–57.847,
    /// `traces/fo-243.jsonl` 57.886–57.914): a click on an ActionIcon (its click handler, FUN_004c0660), after its Mouse
    /// Click actions, takes the ActionIcons out — a state-0 command queued behind those actions, then bar_place 0, the
    /// removal and the icon's Deactivation (H207: its VCEnabled flag cleared) — with no Disable in the data. H151 (static)
    /// says every ActionIcon of the app's list; the traces show the clicked one (one icon placed). Off: the icon stays
    /// placed until a Disable (a report: the Tarakan's 'Go to wheel house' 0x12174 stayed at (560,370) for the rest of the
    /// game and kept jumping to View 0x2db9)
    pub c296_action_icon_click_off: bool,
    /// HYPOTHESIS H2060–H2064 (static FUN_0046c390, the WM_SYSKEYDOWN handler, H1868; NOT traced): the original's Alt
    /// hotkeys that touch the core — Alt+A / M / R / T / V flip the Preferences (H2062: variables 0x369 / 0x3d9 / 0x3d6 /
    /// 0x3da / 0x1072), Alt+P pauses (H2063: H1703's freeze + layout 0xf5e8), Alt+S opens the save screen (H2064: mode
    /// 0, not from the menu). Off: the core ignores them (`Runner::alt_*` return false).
    pub h2060_alt_keys: bool,
    /// C299 / H2083 (traced, `traces/fo-250.jsonl`, HB3): the AI icon (EvidenceIcon 0xdb79) exists only while
    /// Pref_bArtificialIntuition (the port: variable 0x369) is on — Alt+A and the Load (FUN_00452780) take it out with
    /// the pref off (a click at (562,63) then hits nothing) and build it with the pref on; its update returns at once
    /// while the pref is off. Off: the port's old behaviour (the icon
    /// ignores the pref)
    pub h2083_ai_icon_pref: bool,
    /// C76 / H2120 (shots `traces/fo-097.jsonl.burst/k01-0.png`, `traces/fo-097.jsonl.shots/k07.png`; colours static:
    /// the box's panel setup FUN_00483610 gives the TALK panel (205,205,205) and the HISTORY panel (46,107,140) as text
    /// colours, hover colours white / (65,140,175)): the already-asked rows on the HISTORY tab are drawn blue. Off:
    /// every row in the talk colours (a bug report)
    pub h2120_history_row_colour: bool,
    /// H2100 (`traces/fo-250.jsonl` / `traces/fo-252.jsonl`, H2084; the variable's value after the Load not separated): a
    /// Load takes the host's Preferences (the prefs object = the registry read at start, then Alt+A / Options) into the
    /// RegPreferences variables — the save's own values (e.g. 0x369 = 1) are overridden (RegPreferences 0x7e68 at the
    /// Load: the prefs object kept the registry's 0 although the save has 1, so the "variables → prefs" branch of
    /// the preferences object (FUN_004ccda0) did not run; its state-5 branch reads the prefs back into the 24
    /// variables, as at New Game, C86); a Save writes the running game's values to the host's Preferences (H2084: the
    /// registry follows after the save sequence; the fronts persist them). Off: a Load keeps the save's values (H1990)
    pub h2100_load_host_prefs: bool,
    /// C301 (`traces/fo-253.jsonl`, H2191; display only): with the gun held (inventory value 9) the null cursor is the
    /// gunsight 0x84 over the View band where the held gun is hit, over a movie-hotspot target, over a playing video in
    /// the band (its cursor handler leaves the band's choice) and over an Explorable / CharView whose cursor pick would
    /// show the item's cursor. Off: the crosshair 0x81 there
    pub c301_gunsight_cursor: bool,
    /// C302 (`traces/fo-253.jsonl`, H332's Explorable half; display only): over an Explorable the selected item's cursor
    /// shows only if one of its Mouse Click triggers reacts to the held value (the reaction test, FUN_004d1f70); else the
    /// Explorable's own region cursor. Off: the item's cursor over every Explorable while an item is selected
    pub c302_item_cursor_reaction: bool,
    /// C306 (`traces/fo-259.jsonl` / fo-260 / fo-261, H2251): the held camera's shot (H2170) is a photo of the video
    /// under the viewfinder — the playing video, else the View's looping background — at its current time (f68, 1/600 s),
    /// cropped to (x0, 0, x0 + 320, 240) with x0 = pointer x − 160 clamped to 0..280; stored in the camera like TakePhoto's
    /// (below 24) and in the save (f68 / rect; the thumbnail = that crop). Not the story's plate photo (TakePhoto 0x1372e,
    /// 0x1459 — the camera icon inside 0xc25's window, C108). Off: the shot only counts the exposure
    pub c306_shot_photo: bool,
    /// HYPOTHESIS H2280 (static: the save-screen procedures, FUN_005168a0 SaveSlotSelect among them; not traced): the
    /// save / load
    /// screens' selection bar follows the selected slot — SaveLoadInit, SaveSlotSelect and the Tab / Up / Down keys
    /// move the bar item (load 0x14578, save 0x14572) to (0xc2, slot · 0x3a + 0xa4), its size kept (the item's rect,
    /// so its picture is drawn there). Off: the bar stays on the first slot whichever slot is selected (bug report
    /// from the web front: "only the first save can be chosen")
    pub h2280_save_slot_bar: bool,
}

impl Rules {
    /// every hypothesis switch on (`replay --hyp`)
    pub fn all() -> Rules {
        Rules {
            h48_play_first_enabled: true,
            h53_background: true,
            h58_iface_layout: true,
            h59_timer: true,
            h60_preload_em_icon: true,
            h55_explorable: true,
            h61_icon_disable: true,
            h65_load: true,
            h66_start_icons: true,
            h67_saved_media_timers: true,
            h68_icon_click_off: true,
            h69_startup: true,
            h70_menu_layout: true,
            h71_game_start: true,
            h72_save_screen: true,
            h73_trigger_locals: true,
            h75_nav_same_view: true,
            h77_iitem_hit_order: true,
            h74_char_view: true,
            h80_reg_vars: true,
            h81_reg_preferences: true,
            h82_deselect_inv: true,
            h83_fade_back: true,
            h91_init_sm_shootout: true,
            h92_native_display_procs: true,
            h93_flight_props: true,
            h94_ai_list: true,
            h95_save_screen_procs: true,
            h96_pda_init_addr: true,
            h97_pda_flip_addr: true,
            h76_entry_hotspot_order: true,
            h89_queue_model: true,
            h90_event_ctx: true,
            h100_player_mode: true,
            h87_char_view_enable: true,
            h88_exit_remove_order: true,
            h58_item_order: true,
            h101_inventory_enable: true,
            h102_login_init: true,
            h103_timer_pause_resume: true,
            h124_deact_own_dispatch: true,
            h123_shared_asset_object: true,
            h125_swap_disc: true,
            h126_dialog_box: true,
            h121_iitem_key_order: true,
            h122_iitem_disable: true,
            h135_conversation_click: true,
            h136_dbox_hit: true,
            h137_dbox_close: true,
            h138_conversation_enable: true,
            h130_hit_chain: true,
            h140_save_screen_open: true,
            h141_game_start_layouts: true,
            h142_save_slot_text: true,
            h143_save_comit: true,
            h145_media_end_input: true,
            h146_stop_all_timers: true,
            h147_rand_gen: true,
            h148_node_done: true,
            h149_disp_ev_icons: true,
            h150_icon_bars: true,
            h152_inv_click: true,
            h153_on_inv_click: true,
            h155_char_view_click: true,
            h1408_replay_char_view_click: true,
            h1409_conv_tail_merge: true,
            c231_conv_history_flag: true,
            h160_dbox_reshow: true,
            h165_on_int_inv_click: true,
            h166_phone: true,
            h167_pda_enable: true,
            h168_deselect_held: true,
            h169_game_menu: true,
            h170_inv_panel: true,
            h171_held_tools: true,
            h172_right_click: true,
            h174_key_focus: true,
            h176_item_text: true,
            h177_page_turn_sound: true,
            h178_pda_notes: true,
            h179_relocate_iface: true,
            h180_native_noops: true,
            h181_stop_players: true,
            h182_end_game: true,
            h183_char_teleport: true,
            h184_iitem_held: true,
            h185_shanks_var: true,
            h186_email: true,
            h187_nuke_icons: true,
            h188_kit_icon: true,
            h189_prefs: true,
            h190_terminals: true,
            h191_login: true,
            h192_shootout: true,
            h193_node_link_done: true,
            h194_play_asset_at: true,
            h195_photos: true,
            h196_addr_book: true,
            h197_journal_bible: true,
            h198_select_inv: true,
            h199_inventory_disable: true,
            h200_deselect_before_panel_hide: true,
            h201_phone_key_post_back: true,
            h203_action_icon_type3: true,
            h204_emotion_icon_type3: true,
            h205_evidence_icon_type3: true,
            h206_idea_icon_type3: true,
            h207_icon_deact_clears_flag: true,
            h260_login_text: true,
            h261_timer_periodic_alive: true,
            h262_dbox_rows: true,
            h263_char_view_player_end: true,
            h280_sound_no_hold: true,
            h320_background_silent: true,
            h520_whole_movie_background_sound: true,
            h343_cc_relayout: true,
            h360_char_view_held_click: true,
            h361_panel_roll_in_drop: true,
            h362_drop_resets_inv_var: true,
            h363_cursor_asset_deact: true,
            h403_save_screen_data: true,
            h470_save_builder: true,
            h450_gun_view_click: true,
            h451_tool_comment: true,
            h452_video_player_hit: true,
            h820_char_view_hover_refusal: true,
            h821_gun_shot_sound: true,
            h823_game_end_main_menu: true,
            h570_login_art_intuit: true,
            h573_string_text: true,
            h574_item_text_style: true,
            h575_ing: true,
            h576_text_panels: true,
            h577_mail_rows: true,
            h578_carets: true,
            h500_layouts_over_dbox: true,
            h501_dbox_removed_when_empty: true,
            h502_dbox_history_tab: true,
            h503_history_replay: true,
            h1810_dbox_scroll: true,
            h601_teleport_click: true,
            h860_set_view_location: true,
            h621_ai_icon: true,
            h623_ai_click: true,
            h1923_icon_movie_on_hover: true,
            h624_note_pad: true,
            h620_dim_under_layout: true,
            h650_string_compare: true,
            h651_dbox_reshow_queued: true,
            h700_dbox_idea_icons: true,
            h701_idea_drop: false,
            c107_idea_click: true,
            h922_dbox_show_queued: true,
            h1060_travel_stops_played: true,
            h1123_dbox_reshow_after_media: true,
            h1122_bg_skip_on_queued_set_view: true,
            h1181_shared_icon_asset: true,
            h1500_iitem_disable_needs_active: true,
            h1581_save_timer_period: true,
            h1582_kit_icon_listed: true,
            h1585_save_played_players: true,
            h1586_save_view_layout: true,
            h1600_save_player_flag: true,
            h1601_restored_player_plays: true,
            h1602_sorted_lists: true,
            h1603_flight_saved: true,
            h1700_rewind_box: true,
            h1703_suicide_freeze: true,
            h1900_video_click_hit_chain: true,
            h1901_video_band_full_width: true,
            h2170_held_camera_click: true,
            h1863_suicide_note_text: true,
            c86_new_game_host_prefs: true,
            c296_action_icon_click_off: true,
            h2060_alt_keys: true,
            h2083_ai_icon_pref: true,
            h2120_history_row_colour: true,
            h2100_load_host_prefs: true,
            c301_gunsight_cursor: true,
            c302_item_cursor_reaction: true,
            c306_shot_photo: true,
            h2280_save_slot_bar: true,
            h1220_bg_none_deact: true,
            h1203_timer_max_delay: true,
            h1281_movie_hotspots: true,
            h1282_movie_hot_gun: true,
            h1322_movie_hot_view_removal: true,
            h1520_movie_hot_all_cases: true,
            c61_one_player: true,
            h702_evidence_save: true,
            h790_media_index: true,
            h791_text_scroll: true,
            h792_caret_blink: true,
            h793_photo_album: true,
            h794_sbase: true,
            h795_uber_paranoia: true,
            h796_forward: true,
            h797_addr_lists: true,
            h799_layouts_over_bars: true,
            h730_smol_shootout: true,
            h732_load_layout: true,
            h733_fade_stall: true,
            h734_select_inv: true,
            h736_flight_reselect: true,
            h890_timer_pause: true,
            h892_page_up_top: true,
            h894_dim_once: true,
            h895_game_end_no_start: true,
            h896_replay_keys: true,
            h1040_lbox_tint: true,
            h1420_binoc_view_swap: true,
        }
    }

    /// The player option "long timers" this rule set runs with (`h1203_timer_max_delay`).
    pub fn long_timers(&self) -> LongTimers {
        if self.h1203_timer_max_delay {
            LongTimers::Wine
        } else {
            LongTimers::Nt
        }
    }

    /// Set the player option "long timers" (`xf --long-timers`, the web page's "long timers" menu).
    pub fn set_long_timers(&mut self, v: LongTimers) {
        self.h1203_timer_max_delay = v == LongTimers::Wine;
    }
}

/// Player option "long timers" (C191 / H1203; port choice): what a timer armed with a delay above
/// 65535 ms does. The only such timer in the game is the Gordon's Hauling bomb (Timer 7, 120 s after the log manifest).
/// - `Wine` (default): as the original under the CrossOver / Wine bottle (and probably Windows 9x, whose MMSYSTEM limit
///   is 0xffff too) — `timeSetEvent` refuses the delay, the timer never fires: no bomb game over (traced, C159–C161).
/// - `Nt`: as the original would on Windows NT (wPeriodMax 1000000) — the timer runs: the bomb goes off 120 s after the
///   manifest, DoActionList(0x10bb0) → EndGame(0xa15) (not traced on NT: the timer path's static reading, C191).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LongTimers {
    #[default]
    Wine,
    Nt,
}

impl LongTimers {
    /// `wine` / `bottle` / `9x` → Wine, `nt` → Nt
    pub fn parse(s: &str) -> Option<LongTimers> {
        match s.to_ascii_lowercase().as_str() {
            "wine" | "bottle" | "9x" => Some(LongTimers::Wine),
            "nt" => Some(LongTimers::Nt),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LongTimers::Wine => "wine",
            LongTimers::Nt => "nt",
        }
    }
}

/// What PDAFlipAddr(m) does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PdaFlip {
    /// show the card `dir` (±1) steps away, wrapping
    Turn(isize),
    /// enable (true) / disable the two switched cards
    Switch(bool),
    /// rebuild the list without showing
    Refresh,
}

impl PdaFlip {
    fn of(mode: u32) -> PdaFlip {
        match mode {
            0 => PdaFlip::Turn(1),
            2 => PdaFlip::Switch(true),
            3 => PdaFlip::Switch(false),
            4 => PdaFlip::Refresh,
            _ => PdaFlip::Turn(-1),
        }
    }
}

/// Number of save / load slots (H403)
const SAVE_SLOTS: usize = 3;
/// The save screen's and the load screen's layouts (H72, H95)
const SAVE_SCREEN: u32 = 0x14269;
const LOAD_SCREEN: u32 = 0x142ca;

/// What a key does on the save / load screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveKey {
    /// move the selection by +1 / −1 slot
    Step(isize),
    /// SaveLoadComit(1) / SaveLoadComit(0)
    Commit(u32),
    /// Select, Print Screen, Delete: ignored, although their virtual-key codes equal the characters `)`, `,`, `.`
    /// (H403, static)
    Ignore,
    /// edit the typed buffer (save mode)
    Edit,
}

impl SaveKey {
    const TAB: u32 = 0x09;
    const ENTER: u32 = 0x0d;
    const ESC: u32 = 0x1b;
    const UP: u32 = 0x26;
    const DOWN: u32 = 0x28;
    const SELECT: u32 = 0x29;
    const PRINT_SCREEN: u32 = 0x2c;
    const DELETE: u32 = 0x2e;
    const BACKSPACE: u32 = 0x08;
    /// the longest name the buffer takes
    const MAX_NAME: usize = 21;

    fn of(key: u32) -> SaveKey {
        match key {
            Self::TAB | Self::DOWN => SaveKey::Step(1),
            Self::UP => SaveKey::Step(-1),
            Self::ENTER => SaveKey::Commit(1),
            Self::ESC => SaveKey::Commit(0),
            Self::SELECT | Self::PRINT_SCREEN | Self::DELETE => SaveKey::Ignore,
            _ => SaveKey::Edit,
        }
    }
}

/// H308: a StartVideoText(text, day var, time var, flag var) request (FUN_004842b0): the text AssetRef and the three
/// variables' values at the call; `serial` counts the calls (a new overlay each)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoText {
    pub action: u32,
    pub asset: u32,
    pub day: i32,
    pub time: i32,
    pub flag: i32,
    pub serial: u32,
    /// H312: the runner's clock (ms) when the request ran (display timing)
    pub at: u64,
}

#[derive(Clone)]
pub struct World<'a> {
    pub h: &'a Hdb,
    pub gam: &'a Hdb,
    pub rules: Rules,
    pub vars: Vars,
    /// VCEnabled objId -> (target objId, target classId) — XFiles.gam (C15)
    enabled_rec: HashMap<u32, (u32, u32)>,
    /// (classId, objId) -> flag; absent = enabled (C15)
    enabled: HashMap<(u32, u32), u8>,
    /// owner -> registered TriggerList (C20)
    registered: HashMap<u32, u32>,
    /// H1122: the closed-loop runner sets it before each action: a type-4 (SetView) action message is still queued
    pub set_view_queued: bool,
    queue: Vec<QItem>,
    /// C24: messages posted while X runs go before the older pending ones, in post order
    ins: usize,
    started: bool,
    pub view: Option<u32>,
    pub viewpoint: Option<u32>,
    /// the current Node / Location (SetStartView, H125 SwapDisc)
    pub node: Option<u32>,
    pub location: Option<u32>,
    /// H125: a SwapDisc's exit still to run: (old ViewPoint, old Location, old Node, new Node, new Location)
    travel: Option<[Option<u32>; 5]>,
    /// H126: the dialogue box: (CharView, VCConversationList, built Conversations with their assets)
    dbox: Option<(u32, u32, Vec<(u32, Option<u32>)>)>,
    /// H126: dialogue boxes posted whose command has not run yet (list)
    dbox_pending: std::collections::VecDeque<u32>,
    /// H136: the box's state is 1 (shown: lines in the talk list, hit-testable)
    dbox_shown: bool,
    /// H262: VCConversationHistory — Conversations asked (clicked from the talk panel), save order
    pub conv_history: Vec<u32>,
    /// H502: the box's selected panel: 0 = TALK, 1 = HISTORY
    pub dbox_tab: u8,
    /// H1810: the box panels' scrollers (TALK, HISTORY): the first row shown (rows = text lines and the half-line gaps
    /// between entries, H791's model), reset to the top when the box is shown again (the box's state 1 refills the panels)
    pub dbox_scroll: [usize; 2],
    /// H1812: the thumb being dragged — the pointer's offset from the thumb's top at the button-down
    pub dbox_drag: Option<i32>,
    /// Display record (H308): the last StartVideoText request — no trigger-level effect (H92), the front-end draws it
    pub video_text: Option<VideoText>,
    /// Display aid (H312): the runner's clock, set by `Runner::advance` (no rule reads it)
    pub clock: u64,
    /// HYPOTHESIS H550 (live play only — the window / web set it; traces, replay, e2e and tests leave it None so a
    /// RandGen value stays an input, H147): `Some(base)` makes RandGen / RandGenRange draw the original's value,
    /// [`rand_gen_value`] seeded with `base + clock` (standing for timeGetTime)
    pub live_rand: Option<u32>,
    /// H1836 (port feature, the game language): the string table that replaces the HDB's own `XfilesT.dll` strings
    /// (`Hdb::res_strings`) — the English data set's, same ids; None = the HDB's (RU). Set by `Session::set_game_lang`.
    pub lang_strings: Option<std::sync::Arc<std::collections::HashMap<u32, String>>>,
    /// H1320 (e2e input, like H1120's latency): without the live generator, the Views DoSmolShootout picked on the
    /// original (the next transition after each of its executions, [`shootout::picks_from_trace`]), in order
    pub smol_trace_picks: std::collections::VecDeque<u32>,
    /// H1320: SmIsWillShot1's draws on the original (hit = REG 0x52b0 right after it), in order
    pub smol_trace_will: std::collections::VecDeque<bool>,
    /// H135 / H139: the Conversation whose line is playing and the played assets not yet deactivated
    dbox_seq: Option<(u32, Vec<u32>)>,
    /// H1123: the runner sets it before a line's end command: a non-looping media player is still in its FIFO
    pub media_busy: bool,
    /// H1123: the line's end found media playing; the box decision waits for the FIFO to empty
    pub dbox_reshow_waiting: bool,
    pub hotspots: Vec<u32>,
    pending_entry: Option<u32>,
    vp_of_view: HashMap<u32, u32>,
    /// H53: background AssetRef currently chosen for the View
    background: Option<u32>,
    /// H58: layouts shown (runtime object exists), with their items and the items' assets, item order
    layouts: Vec<(u32, Vec<(u32, Option<u32>)>)>,
    /// H130: shown layouts set as View by SetIFLAsView; dropped with the layout's hide
    layouts_as_view: Vec<u32>,
    /// H58/H60: runtime objects created but not activated yet: their activation creates the children
    pending_children: Vec<u32>,
    /// H165–H174: native device state (`devices.rs`)
    pub dev: devices::Devices,
    /// H175–H198: native state of the type-6 procedures (`procs.rs`)
    pub px: procs::Procs,
    /// H570–H578: the FBI computer's native screen state (`computer.rs`)
    pub pc: procs::computer::Pc,
    /// H621–H623: the AI icon's native state (`ai.rs`)
    pub ai: ai::Ai,
    /// H624: the PDA note pad's native state (`pda.rs`)
    pub notes_ui: pda::NotePad,
    /// H60: EmotionIcons whose runtime object exists
    icons: Vec<u32>,
    /// H60/H61: EmotionIcons activated and not deactivated, with the assets their activation created
    icons_active: Vec<(u32, Vec<u32>)>,
    /// H1181: active inventory objects and the AssetRefs their activation built
    inv_assets: Vec<(u32, Vec<u32>)>,
    /// H55: VCExplorables of the current View registered and active, registration order
    explorables: Vec<u32>,
    /// H59: owners in registration order with the timer ids their TriggerList subscribes to
    timer_subs: Vec<(u32, Vec<u8>)>,
    /// H65: the save's start (Node, Location, ViewPoint, View), taken by the next SetStartView
    pending_start: Option<[u32; 4]>,
    /// H65: the save's restore mode (a VCGameState field)
    pub restore_mode: i16,
    /// H65 / C30: restore window open
    pub restoring: bool,
    /// H67: restored timer messages posted and not run yet: (id, ms, periodic), post order
    restored_timers: std::collections::VecDeque<(u8, u32, bool)>,
    /// H70: engine-built type-5 messages pending: (layout, mode)
    engine_layouts: std::collections::VecDeque<(u32, u8)>,
    /// H90: the expired timer id of the next Timer fire (its event-context map {0: id})
    timer_ctx: Option<u8>,
    /// H97: engine-built type-3 messages pending: (class, objId, state)
    engine_enables: std::collections::VecDeque<(u32, u32, u8)>,
    /// H80: the variable bound by RegCurrInvVar (read by SetFLightProps, H93)
    curr_inv_var: Option<u32>,
    /// H80: the "no item" value RegCurrInvVar stored (Inventory_None = 0 in XFiles.gam)
    curr_inv_none: i32,
    /// H96: PDA address cards and the shown index
    pda_addrs: Vec<u32>,
    pda_index: usize,
    /// H71: the 0xdb79 object exists
    game_icon: bool,
    /// H140 / H143: the save / load screen returns to the main menu when closed
    save_from_menu: bool,
    /// H142: the selected save slot
    pub save_slot: usize,
    /// H403: the slot names (the front-end fills them from its saves directory)
    pub save_names: [String; 3],
    /// H403: the typed name buffer (None = nothing typed since the slot was selected)
    pub save_edit: Option<String>,
    /// H403: SaveKeyPress ran without a known key (a replayed trace): the typed name is unknown, not empty
    save_name_unknown: bool,
    /// H67: restored media players posted and not run yet (AssetRef), post order
    restored_players: std::collections::VecDeque<(u32, u32)>,
    /// H1601: the saved position (ms) of each restored player (l64 record's third word)
    restored_pos: HashMap<u32, u32>,
    /// H100: AssetRefs with a looping player (never removed at media end)
    looping_players: HashSet<u32>,
    /// H146: timers the core knows to run (id, periodic), start order; engine-built type-2 stop messages pending
    timers_running: Vec<(u8, bool)>,
    engine_timer_stops: std::collections::VecDeque<u8>,
    /// H148: RegCurrNode's variable, RegNode3Vars' four, RegNode6Vars' two
    node_vars: (Option<u32>, Option<[u32; 4]>, Option<[u32; 2]>),
    /// H450: RegViewState's variable
    view_state_var: Option<u32>,
    /// H1420: RegViewSpecial's variable
    pub(super) view_special_var: Option<u32>,
    /// H145: AssetRefs the core played (type 1 / 7, restored or CharView players) and has not deactivated
    played: HashSet<u32>,
    /// C61: AssetRefs whose type-1 / type-7 player exists (registered by a play, not deactivated yet)
    live_players: HashSet<u32>,
    /// H68: icons whose state-0 command is pending
    icons_off: Vec<u32>,
    /// H203–H206: Action / Evidence / Idea icon objects built and not removed (list order)
    icon_objs: Vec<icons::IconObj>,
    /// H203–H206: icons in state 1 (placed)
    icon_state1: Vec<u32>,
    /// H205: DispEvIcons' flag
    ev_icons_shown: bool,
    /// H74: VCCharViews of the current View built at its activation and not deactivated, build order
    char_views: Vec<u32>,
    /// HYPOTHESIS H1409: a played line's f30 AssetRef → the f34 AssetRef of the same movie file its player runs on into
    /// (no second player; the front's clip of the f30 asset ends at the f34 asset's end)
    pub media_tail: HashMap<u32, u32>,
    /// H74: CharViews whose media wrapper registration (TL 0) was predicted and not yet seen by [`World::register`]
    cv_wrapper_regs: Vec<u32>,
    /// H74: CharViews whose wrapper command is pending, post order
    cv_players: Vec<u32>,
    /// H601: the CharView a CharTeleport targets: (View, CharView), kept by the transition object, read by that View's
    /// activation
    teleport_cv: Option<(u32, u32)>,
    /// H601: CharViews built with the teleport-click flag (their wrapper queues a mouse command when it is removed)
    cv_click: Vec<u32>,
    /// H74: (AssetRef, CharView) whose media player is posted and not run yet
    cv_assets: std::collections::VecDeque<(u32, u32)>,
    /// H76: lifecycle fires held behind queued actions
    held: Option<h76::Held>,
    /// H701 (provisional): the IdeaIcon picked up by a click, dropped by the next click; C107: the icon being
    /// clicked (the drag object's icon, spared by the box's state 0)
    pub idea_drag: Option<u32>,
    /// H1814: the owner the left button went down on and is still held on (set by the button-down to the hit object,
    /// cleared by the button-up) — IsIItemHeld's answer (H184)
    pub held_item: Option<u32>,
    /// C107: an idea's answer is being started (H135's state-0 command already came at the button-down)
    idea_answer_playing: bool,
    /// H76 in the closed-loop runner: the runner's command queue places and releases the hold (`World::h76_held`,
    /// `World::h76_release_id`); the world's own queue model does not release it
    h76_external: bool,
    /// H76: id of the last hold
    h76_seq: u64,
    /// H89: fires whose posts are awaited before the next command runs (lock-step)
    h89_await: Vec<(u32, u8)>,
    /// H89: fires that came before their queued command (lock-step)
    h89_prefired: Vec<(u32, u8)>,
    /// H89: commands run out of order because a message behind them was dispatched / a new transition came (lock-step)
    pub h89_flushes: usize,
    pub out: Vec<Out>,
}

/// H1602 (the lists' ordered add, ascending): `x` appended when the list is empty or its last id ≤ `x`, else
/// inserted before the first id ≥ `x` (the walk starts at the head — a loaded list keeps its order, unsorted or not).
/// `sorted` off: appended.
pub(crate) fn list_add(v: &mut Vec<u32>, x: u32, sorted: bool) {
    match v.last() {
        Some(&last) if sorted && x < last => {
            let i = v.iter().position(|&y| y >= x).unwrap_or(v.len());
            v.insert(i, x);
        }
        _ => v.push(x),
    }
}

#[cfg(test)]
mod list_add_tests {
    #[test]
    fn h1602_ordered_insert() {
        // fo-066-end.x → fo-067-end.x: 0x587d goes before 0x5bc7; notes [0xf709] + 0x163aa + 0xf70b
        let mut v = vec![0x5874, 0x5bc7];
        super::list_add(&mut v, 0x587d, true);
        assert_eq!(v, [0x5874, 0x587d, 0x5bc7]);
        let mut n = vec![0xf709];
        super::list_add(&mut n, 0x163aa, true);
        super::list_add(&mut n, 0xf70b, true);
        assert_eq!(n, [0xf709, 0xf70b, 0x163aa]);
        // a loaded unsorted list keeps its order (fo-085-end.x: 0x5670 last): an id above the last one is appended, a
        // smaller one goes before the first id not below it
        let mut u = vec![0x5686, 0x57b5, 0x5670];
        super::list_add(&mut u, 0x5800, true);
        assert_eq!(u, [0x5686, 0x57b5, 0x5670, 0x5800]);
        super::list_add(&mut u, 0x5700, true);
        assert_eq!(u, [0x5686, 0x5700, 0x57b5, 0x5670, 0x5800]);
        let mut off = vec![0xf709, 0x163aa];
        super::list_add(&mut off, 0xf70b, false);
        assert_eq!(off, [0xf709, 0x163aa, 0xf70b]);
    }
}

/// HYPOTHESIS H550 (static, the random draw FUN_00468810): a fresh generator per call, seed = timeGetTime(), one LCG
/// step `seed * 0x41c64e6d + 0x3039`, value = lo + step % (hi − lo + 1) (bounds
/// swapped if hi < lo) — RandGen(var, n) is the range [0, n], both ends included.
pub fn rand_gen_value(seed: u32, lo: i32, hi: i32) -> i32 {
    let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
    let step = seed.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
    let span = (hi as u32).wrapping_sub(lo as u32).wrapping_add(1);
    if span == 0 {
        return step as i32;
    }
    lo.wrapping_add((step % span) as i32)
}

impl<'a> World<'a> {
    /// H550: the drawn value in live play, else None (unknown, H147)
    pub(crate) fn live_rand_value(&self, lo: i32, hi: i32) -> Option<i32> {
        self.live_rand.map(|b| rand_gen_value(b.wrapping_add(self.clock as u32), lo, hi))
    }

    pub fn new(h: &'a Hdb, gam: &'a Hdb, rules: Rules) -> World<'a> {
        let mut enabled_rec = HashMap::new();
        let mut enabled = HashMap::new();
        if let Some(c) = gam.class(class::ENABLED) {
            for &(id, _) in &c.objects {
                if let Some(r) = gam.read(class::ENABLED, id) {
                    let (target, cls) = (r.u32("f28"), r.u32("f2c"));
                    enabled_rec.insert(id, (target, cls));
                    enabled.insert((cls, target), r.u32("f2d") as u8);
                }
            }
        }
        let mut vp_of_view = HashMap::new();
        if let Some(c) = h.class(class::VIEWPOINT) {
            for &(vp, _) in &c.objects {
                for v in h.list_of(class::VIEWPOINT, vp) {
                    vp_of_view.insert(v, vp);
                }
            }
        }
        let mut w = World {
            h,
            gam,
            rules,
            vars: {
                let mut v = Vars::from_gam(gam);
                v.h90_event_ctx = rules.h90_event_ctx;
                v.h650_string_compare = rules.h650_string_compare;
                v
            },
            enabled_rec,
            enabled,
            registered: HashMap::new(),
            queue: Vec::new(),
            set_view_queued: false,
            ins: 0,
            started: false,
            view: None,
            viewpoint: None,
            node: None,
            location: None,
            travel: None,
            dbox: None,
            dbox_pending: Default::default(),
            dbox_shown: false,
            conv_history: Vec::new(),
            dbox_tab: 0,
            dbox_scroll: [0; 2],
            dbox_drag: None,
            video_text: None,
            clock: 0,
            live_rand: None,
            lang_strings: None,
            smol_trace_picks: Default::default(),
            smol_trace_will: Default::default(),
            dbox_seq: None,
            media_busy: false,
            dbox_reshow_waiting: false,
            hotspots: Vec::new(),
            pending_entry: None,
            vp_of_view,
            background: None,
            layouts: Vec::new(),
            layouts_as_view: Vec::new(),
            pending_children: Vec::new(),
            dev: Default::default(),
            px: Default::default(),
            pc: Default::default(),
            ai: Default::default(),
            notes_ui: Default::default(),
            icons: Vec::new(),
            icons_active: Vec::new(),
            inv_assets: Vec::new(),
            explorables: Vec::new(),
            timer_subs: Vec::new(),
            pending_start: None,
            restore_mode: 0,
            restoring: false,
            restored_timers: Default::default(),
            engine_layouts: Default::default(),
            timer_ctx: None,
            engine_enables: Default::default(),
            curr_inv_var: None,
            curr_inv_none: 0,
            pda_addrs: Vec::new(),
            pda_index: 0,
            game_icon: false,
            save_from_menu: false,
            save_slot: 0,
            save_names: Default::default(),
            save_edit: None,
            save_name_unknown: false,
            restored_players: Default::default(),
            restored_pos: Default::default(),
            looping_players: Default::default(),
            played: Default::default(),
            live_players: Default::default(),
            timers_running: Vec::new(),
            engine_timer_stops: Default::default(),
            node_vars: (None, None, None),
            view_state_var: None,
            view_special_var: None,
            icons_off: Vec::new(),
            icon_objs: Vec::new(),
            icon_state1: Vec::new(),
            ev_icons_shown: false,
            char_views: Vec::new(),
            media_tail: HashMap::new(),
            cv_wrapper_regs: Vec::new(),
            cv_players: Vec::new(),
            teleport_cv: None,
            cv_click: Vec::new(),
            cv_assets: Default::default(),
            held: None,
            idea_drag: None,
            held_item: None,
            idea_answer_playing: false,
            h76_external: false,
            h76_seq: 0,
            h89_await: Vec::new(),
            h89_prefired: Vec::new(),
            h89_flushes: 0,
            out: Vec::new(),
        };
        w.ab_seed();
        if w.rules.h793_photo_album {
            w.pc_seed(); // H793: the save's photos
        }
        if w.rules.h621_ai_icon {
            w.ai_seed();
        }
        w
    }

    /// H65: if the gam is a save (has a VCGameState 0x57), take its start (Node, Location, ViewPoint, View) and restore
    /// mode for the next SetStartView. Returns the start, None for a new game's gam.
    pub fn load_saved_start(&mut self) -> Option<[u32; 4]> {
        if !self.rules.h65_load {
            return None;
        }
        let &(id, _) = self.gam.class(class::GAME_STATE)?.objects.first()?;
        let r = self.gam.read(class::GAME_STATE, id)?;
        let start = [r.u32("node"), r.u32("location"), r.u32("viewpoint"), r.u32("view")];
        if start.iter().any(|&x| x == 0) {
            return None; // SetStartView takes a saved start only when all four are nonzero
        }
        self.pending_start = Some(start);
        self.restore_mode = match r.field("mode") {
            Some(crate::hdb::Value::I16(v)) => *v,
            _ => 0,
        };
        Some(start)
    }

    /// H1601: the saved position (ms) a restored player of `asset` resumes at (taken once)
    pub fn take_restored_position(&mut self, asset: u32) -> Option<u32> {
        self.restored_pos.remove(&asset)
    }

    /// HYPOTHESIS H100: `asset` has a looping player — at its media end it is not removed.
    pub fn player_loops(&self, asset: u32) -> bool {
        self.rules.h100_player_mode && self.looping_players.contains(&asset)
    }

    /// H1585: `asset` has a player a save lists in l64 — a looping one, or (rule on) a type-1 / 7 one still playing
    /// (fo-196-end.x l64 = [0xda7e] from action 0x10d0e, type 1 mode 1, started 126.906, saved 156.2–191.3;
    /// fo-090-end.x 0x6f60 from 0x8e78, type 7 mode 1)
    pub fn player_saved(&self, asset: u32) -> bool {
        self.player_loops(asset) || (self.rules.h1585_save_played_players && self.played.contains(&asset))
    }

    /// H65: the restore window closes (the type-0x14 command with value 0).
    pub fn end_restore(&mut self) {
        if self.restoring {
            self.restoring = false;
            self.out.push(Out::Restore { on: false });
        }
    }

    // ---- data helpers ------------------------------------------------------------------------

    /// C15: VCEnabled flag of (class, objId); no record -> enabled.
    pub fn is_enabled(&self, cls: u32, id: u32) -> bool {
        self.enabled.get(&(cls, id)).map_or(true, |&f| f != 0)
    }

    /// C20: TriggerList of an owner from its HDB field. View f3c, VCAssetRef f3c & 0xffffff,
    /// VCExplorable f44, VCLocaton f6c, VCViewPoint f74, VCEmotionIcon/VCInventory f40, VCNav f30, VCInterfaceItem
    /// f48; VCTitle fa0 (HYPOTHESIS H56: the VCTitle reader's tag 0x6b, static; = the traced 0x12b -> 0x5bdd in
    /// fo-005..fo-009). HYPOTHESIS H57 (static registration sites; every traced value is 0): VCNode f68, VCEvidenceIcon
    /// f40; VCCursor (and a null owner) and VCIFaceLayout are registered with a literal 0. Any other class -> None.
    pub fn trigger_list_of(&self, owner: u32) -> Option<u32> {
        let h = self.h;
        if owner == 0 || h.has(class::CURSOR, owner) || h.has(class::IFACE_LAYOUT, owner) {
            return Some(0);
        }
        // H203 / H206: every icon class goes through the common icon base (TL = f40)
        if (self.rules.h203_action_icon_type3 && h.has(class::ACTION_ICON, owner))
            || (self.rules.h206_idea_icon_type3 && h.has(icons::IDEA_ICON_CLASS, owner))
        {
            return h.read(class::ACTION_ICON, owner).or_else(|| h.read(icons::IDEA_ICON_CLASS, owner)).map(|r| r.u32("f40"));
        }
        let fields: [(u32, &str, u32); 12] = [
            (class::VIEW, "f3c", u32::MAX),
            (class::ASSET_REF, "f3c", 0x00ff_ffff),
            (class::EXPLORABLE, "f44", u32::MAX),
            (class::LOCATION, "f6c", u32::MAX),
            (class::VIEWPOINT, "f74", u32::MAX),
            (class::EMOTION_ICON, "f40", u32::MAX),
            (class::INVENTORY, "f40", u32::MAX),
            // fo-009: nav-hotspot of VCNav 0x14987 registered TL 0x14989 = f30; VCInterfaceItem TL = f48 (16/16)
            (class::NAV, "f30", u32::MAX),
            (class::INTERFACE_ITEM, "f48", u32::MAX),
            (class::TITLE, "fa0", u32::MAX),
            (class::NODE, "f68", u32::MAX),
            (class::EVIDENCE_ICON, "f40", u32::MAX),
        ];
        for (cls, f, mask) in fields {
            if let Some(r) = h.read(cls, owner) {
                return Some(r.u32(f) & mask);
            }
        }
        // HYPOTHESIS H126: VCConversationList object registered with TL 0; VCConversation with f3c (fo-027 / fo-029:
        // 0x590e → 0x12628, 0x5914 → 0x1262c, 0x591a → 0x12630)
        if self.rules.h126_dialog_box {
            if h.has(class::CONVERSATION_LIST, owner) {
                return Some(0);
            }
            if let Some(r) = h.read(class::CONVERSATION, owner) {
                return Some(r.u32("f3c"));
            }
        }
        // HYPOTHESIS H74: VCCharView runtime object registered with f54 (fo-020 0x55e9 → 0x62da)
        if self.rules.h74_char_view {
            if let Some(r) = h.read(class::CHAR_VIEW, owner) {
                return Some(r.u32("f54"));
            }
        }
        // "every other class: TL 0" was REFUTED by fo-009 (VCNav, VCInterfaceItem): unknown classes have no rule
        None
    }

    pub fn nav_list(&self, view: u32) -> Vec<u32> {
        match self.h.read(class::VIEW, view) {
            Some(r) => self.h.list_of(class::NAV_LIST, r.u32("navList")),
            None => Vec::new(),
        }
    }

    /// HYPOTHESIS H50 (static): the ViewPoint whose list part contains the View (1657/1657 Views are in
    /// exactly one list).
    pub fn viewpoint_of(&self, view: u32) -> Option<u32> {
        self.vp_of_view.get(&view).copied()
    }

    /// C6: transition kind from the VCNav's HotSpot type (live globals 0x11c/0x11d/0x11e -> 1/2/3, else
    /// 0). The type is read from HotSpot.region.kind — HYPOTHESIS H49 for that field link (static;
    /// consistent with all HotSpots of C6).
    pub fn nav_kind(&self, vcnav: u32) -> u32 {
        let hs = self.h.read(class::NAV, vcnav).map(|r| r.u32("hotSpot")).unwrap_or(0);
        let t = match self.h.read(class::HOTSPOT, hs).and_then(|r| r.field("region").cloned()) {
            Some(Value::Region { kind, .. }) => kind,
            _ => 0,
        };
        match t {
            0x11c => 1,
            0x11d => 2,
            0x11e => 3,
            _ => 0,
        }
    }

    /// (trigger, event, ActionList) of the triggers in a TriggerList, in list order.
    fn triggers(&self, tl: u32) -> Vec<(u32, u8, u32)> {
        self.h
            .list_of(class::TRIGGER_LIST, tl)
            .into_iter()
            .filter_map(|t| self.h.read(class::TRIGGER, t).map(|r| (t, r.u32("fe8") as u8, r.u32("fe4"))))
            .collect()
    }

    // ---- registration and events (C19, C20) ---------------------------------------------------

    /// Register `owner`'s TriggerList (C20). Returns the TL, or None if the owner's TL rule is unknown
    /// (then nothing is registered — the caller may supply it with [`World::register_tl`]).
    pub fn register(&mut self, owner: u32) -> Option<u32> {
        // H74: the predicted media wrapper of a CharView registers the same owner with TL 0; the CharView object keeps its
        // own TL (its Mouse Click trigger), so the owner's registered TL is not replaced
        if let Some(i) = self.cv_wrapper_regs.iter().position(|&c| c == owner) {
            self.cv_wrapper_regs.remove(i);
            return Some(0);
        }
        let tl = self.trigger_list_of(owner)?;
        self.register_tl(owner, tl);
        Some(tl)
    }

    pub fn register_tl(&mut self, owner: u32, tl: u32) {
        self.registered.insert(owner, tl);
        if self.rules.h59_timer {
            self.timer_subs.retain(|x| x.0 != owner);
            let ids = self.timer_ids(tl);
            if !ids.is_empty() {
                self.timer_subs.push((owner, ids));
            }
        }
    }

    /// HYPOTHESIS H59 (static, at trigger registration, FUN_00480e70): for every Timer (7) trigger of the TriggerList,
    /// every action of its ActionList with a condition prefix whose lhs (rhs) operand is the event context (kind 4)
    /// subscribes the owner to the timer id = value of the other operand, kept with the owner.
    pub fn timer_ids(&self, tl: u32) -> Vec<u8> {
        let mut ids = Vec::new();
        for (_, ev, al) in self.triggers(tl) {
            if ev != event::TIMER {
                continue;
            }
            for a in self.h.list_of(class::ACTION_LIST, al) {
                let Some(c) = Action::load(self.h, a).and_then(|a| a.condition()) else { continue };
                let v = if c.lhs_kind == 4 {
                    self.vars.operand(c.rhs, c.rhs_kind)
                } else if c.rhs_kind == 4 {
                    self.vars.operand(c.lhs, c.lhs_kind)
                } else {
                    None
                };
                if let Some(v) = v {
                    if !ids.contains(&(v as u8)) {
                        ids.push(v as u8);
                    }
                }
            }
        }
        ids
    }

    /// HYPOTHESIS H59: registered owners subscribed to timer `id`, registration order (one command per expired id; each
    /// object fires Timer iff the id is in its set). Which objects the command visits is not decoded — every registered
    /// owner is assumed.
    pub fn timer_subscribers(&self, id: u8) -> Vec<u32> {
        self.timer_subs.iter().filter(|x| x.1.contains(&id)).map(|x| x.0).collect()
    }

    /// HYPOTHESIS H90: a Timer (7) fire for expired timer `id`: the timer command builds the event-context map {0: id}
    /// that the fire hands to the trigger.
    /// The next Timer fire is for expired timer `id` (consumed by that fire).
    pub fn timer_ctx_next(&mut self, id: u8) {
        self.timer_ctx = Some(id);
    }

    /// Fire `event` on `owner` (C19): every trigger of the owner's registered TriggerList whose event
    /// (fe8) matches posts its ActionList (fe4), in list order. An Activation runs as the owner's own
    /// queue command (fo-006: q_dispatch cmd 0 on the object, then fire_post event 8), so it starts a new
    /// dispatch for C24 ordering (fo-005/fo-006 contain no case that distinguishes this). Activation of the View being
    /// entered builds its nav-hotspots (C23).
    pub fn fire(&mut self, owner: u32, ev: u8) {
        if ev == event::DEACTIVATION {
            self.live_players.remove(&owner); // C61: the player is gone with its AssetRef's Deactivation
        }
        self.dev_fired(owner, ev); // H167: inventory objects alive
        self.h76_settle(); // H76
        self.h89_pump(); // H89: whatever was ahead has run
        self.h89_fired(owner, ev);
        if ev == event::ACTIVATION || (ev == event::DEACTIVATION && self.rules.h124_deact_own_dispatch) {
            self.begin_dispatch();
        }
        // H89: View cmd 0 posts View cmd 11 (the background) before its trigger actions
        if ev == event::ACTIVATION && self.h89_on() && self.pending_entry == Some(owner) {
            self.post_cmd(WCmd::ViewBuild(owner));
        }
        // H135: a Mouse Click on a line of the shown dialogue box: box hidden + line played before the fire
        let conv_click = ev == event::MOUSE_CLICK && self.rules.h135_conversation_click && self.dbox_line(owner);
        let conv_played = conv_click && self.conversation_clicked(owner);
        let timer_ctx = if ev == event::TIMER { self.timer_ctx.take() } else { None };
        // H146: a one-shot timer's expiry ends it (the timer list no longer holds it)
        if let Some(id) = timer_ctx {
            self.timers_running.retain(|x| !(x.0 == id && !x.1));
        }
        let mut tl = self.registered.get(&owner).copied().unwrap_or(0);
        // C30 / H65: while the restore window is open an Activation marks the object active but fires no
        // trigger (mode 1); mode 2 would fire only for object kinds 3, 4, 5, 6, 0x14 (never seen, kinds not modelled)
        if ev == event::ACTIVATION && self.restoring {
            if self.restore_mode != 1 {
                self.out.push(Out::Unimplemented { action: 0, what: format!("restore mode {} (only 1 is modelled)", self.restore_mode) });
            }
            tl = 0;
        }
        if tl != 0 {
            for (trigger, tev, al) in self.triggers(tl) {
                if tev == ev {
                    if self.rules.h90_event_ctx {
                        // H90: the event-context map is copied into the trigger before its locals are reset
                        let map = timer_ctx.map(|id| vec![(0, id as i32)]);
                        self.vars.set_event_ctx(trigger, map);
                    }
                    if self.rules.h73_trigger_locals {
                        self.vars.reset_locals(trigger);
                    }
                    for a in self.h.list_of(class::ACTION_LIST, al) {
                        self.post(Msg { action: a, trigger }, PostSrc::Event { owner, event: ev });
                    }
                }
            }
        }
        // HYPOTHESIS H68 (fo-017 155.83–156.27: a state-0 command for each of the three icons, right after the clicked
        // icon's 8 action posts; dispatched 185.60.. after the last of them, each followed by its Deactivation)
        if ev == event::MOUSE_CLICK && self.rules.h68_icon_click_off && self.icons.contains(&owner) {
            for icon in self.icons.clone() {
                self.icons_off.push(icon);
                self.out.push(Out::IconOff { icon });
            }
        }
        // C296: an ActionIcon click takes the ActionIcons out (state 0 + removal queued behind the click's actions)
        let is_action_icon = |w: &Self, o: u32| w.icon_objs.iter().any(|x| x.cls == class::ACTION_ICON && x.id == o);
        if ev == event::MOUSE_CLICK && self.rules.c296_action_icon_click_off && is_action_icon(self, owner) {
            let all: Vec<u32> = self.icon_objs.iter().filter(|x| x.cls == class::ACTION_ICON).map(|x| x.id).collect();
            for icon in all {
                if !self.icons_off.contains(&icon) {
                    self.icons_off.push(icon);
                    self.out.push(Out::IconOff { icon });
                }
            }
        }
        if ev == event::ACTIVATION && self.pending_entry == Some(owner) {
            self.view_activated(owner);
        }
        if ev == event::ACTIVATION {
            if let Some(i) = self.pending_children.iter().position(|&o| o == owner) {
                self.pending_children.remove(i);
                self.create_children(owner);
            }
        }
        // H126: the CharView's Deactivation removes its dialogue box (fo-029 158.27 Deact 0x590c → 158.45 removal of the
        // box → Deact 0x590e, 0x4503, 0x5914, 0x4505, 0x591a, 0x4507 159.89–161.51)
        // H207: the deactivation of an ActionIcon / EmotionIcon clears its VCEnabled flag
        if ev == event::DEACTIVATION
            && self.rules.h207_icon_deact_clears_flag
            && (self.h.has(class::ACTION_ICON, owner) || self.h.has(class::EMOTION_ICON, owner))
        {
            let cls = if self.h.has(class::ACTION_ICON, owner) { class::ACTION_ICON } else { class::EMOTION_ICON };
            self.enabled.insert((cls, owner), 0);
        }
        if ev == event::DEACTIVATION && self.dbox.as_ref().is_some_and(|d| d.0 == owner) {
            self.remove_dbox();
        }
        if conv_click && !conv_played {
            self.conversation_unplayed(owner);
        }
        // H139: end of a played line
        if ev == event::DEACTIVATION && self.rules.h138_conversation_enable {
            self.dbox_seq_deact(owner);
        }
        self.h76_fired(owner, ev); // H76
        self.h89_pump(); // H89
    }

    /// HYPOTHESIS H126 — DisplayDBox (FUN_00481660): a box of the same CharView → nothing; another box → removed first;
    /// then the CharView's f48 VCConversationList gets its runtime object (REG TL 0) and is posted (also at a Load).
    fn display_dbox(&mut self, cv: u32) {
        if self.dbox.as_ref().is_some_and(|d| d.0 == cv) {
            return;
        }
        if self.dbox.is_some() {
            self.remove_dbox();
        }
        let list = self.h.read(class::CHAR_VIEW, cv).map(|r| r.u32("f48")).unwrap_or(0);
        if list == 0 || !self.h.has(class::CONVERSATION_LIST, list) {
            return;
        }
        self.dbox = Some((cv, list, Vec::new()));
        self.dbox_shown = false;
        self.dbox_pending.push_back(list);
        self.out.push(Out::Reg { owner: list, tl: Some(0), cause: RegCause::DialogBox { char_view: cv } });
        self.out.push(Out::DboxPost { list });
    }

    /// HYPOTHESIS H126 — the dialogue box's command (cmd 0; no Activation fire): every enabled VCConversation of the list
    /// (C15) is built (REG TL f3c) and posted, then activated
    /// in list order (fo-027 576.76–577.25 REG 0x590e / 0x5914 / 0x591a of [0x106c2, 0x590e, 0x5914, 0x591a, 0x5df7, 0x5dfa,
    /// 0x884c]; the other four disabled). False if no box is pending.
    pub fn dbox_run(&mut self) -> bool {
        let Some(list) = self.dbox_pending.pop_front() else { return false };
        if self.dbox.as_ref().map(|d| d.1) != Some(list) {
            return true; // removed meanwhile
        }
        let convs: Vec<u32> = self
            .h
            .list_of(class::CONVERSATION_LIST, list)
            .into_iter()
            .filter(|&c| self.h.has(class::CONVERSATION, c) && self.is_enabled(class::CONVERSATION, c))
            .collect();
        for &c in &convs {
            let tl = self.trigger_list_of(c);
            self.out.push(Out::Reg { owner: c, tl, cause: RegCause::Conversation { list } });
        }
        for &c in &convs {
            self.pending_children.push(c);
            self.out.push(Out::Fire { owner: c, event: event::ACTIVATION });
        }
        if let Some(d) = &mut self.dbox {
            d.2 = convs.into_iter().map(|c| (c, None)).collect();
        }
        // H136: the command ends with the box's state-1 command (fo-027 580.50, fo-029 145.77); H922: a queue command
        // behind the Conversations' Activations (else approximated as at once)
        if self.rules.h922_dbox_show_queued {
            self.out.push(Out::DboxShowPost { list });
        } else {
            self.dbox_state(true);
        }
        true
    }

    /// H922: the box's queued state-1 command — the box still of `list` and not shown: state 1 (H136 / H700).
    pub fn dbox_show_cmd(&mut self, list: u32) {
        if self.dbox.as_ref().is_some_and(|d| d.1 == list) && !self.dbox_shown {
            self.dbox_state(true);
        }
    }

    /// HYPOTHESIS H126 — the box's removal (FUN_004bdc30) deactivates each built Conversation, then its asset.
    fn remove_dbox(&mut self) {
        if self.dbox_shown {
            self.dbox_state(false); // state 0 if active (fo-034 145.77, fo-029 158.65)
        }
        let Some((_, list, convs)) = self.dbox.take() else { return };
        self.dbox_pending.retain(|&l| l != list);
        for (c, a) in convs {
            self.pending_children.retain(|&o| o != c);
            self.out.push(Out::Fire { owner: c, event: event::DEACTIVATION });
            if let Some(a) = a {
                self.out.push(Out::Fire { owner: a, event: event::DEACTIVATION });
            }
        }
    }

    /// H1181: the runtime asset object of AssetRef `a` is held by an active icon other than `owner` (or a shown item)
    pub(super) fn icon_asset_held(&self, a: u32, owner: u32) -> bool {
        self.rules.h1181_shared_icon_asset
            && a != 0
            && (self.icons_active.iter().any(|x| x.0 != owner && x.1.contains(&a))
                || self.inv_assets.iter().any(|x| x.0 != owner && x.1.contains(&a))
                || self.layouts.iter().any(|l| l.1.iter().any(|x| x.1 == Some(a))))
    }

    /// HYPOTHESIS H58/H60: what the activation of a layout / item / emotion icon runtime object creates.
    fn create_children(&mut self, owner: u32) {
        let h = self.h;
        if h.has(class::IFACE_LAYOUT, owner) {
            // the layout's activation: Activation fired, then for every item of the layout's list that is enabled (C15
            // table, class 0x4f) an item object is built (REG TL f48); then every built item is posted
            let items: Vec<u32> =
                h.list_of(class::IFACE_LAYOUT, owner).into_iter().filter(|&i| self.is_enabled(class::INTERFACE_ITEM, i)).collect();
            for &i in &items {
                let tl = self.trigger_list_of(i);
                self.out.push(Out::Reg { owner: i, tl, cause: RegCause::IfaceItem { layout: owner } });
            }
            // H58 order: built items go into the layout's sorted container; the posts walk it
            let items = if self.rules.h58_item_order { self.item_container_order(items) } else { items };
            for &i in &items {
                self.pending_children.push(i);
                self.out.push(Out::Fire { owner: i, event: event::ACTIVATION });
            }
            if let Some(l) = self.layouts.iter_mut().find(|l| l.0 == owner) {
                l.1 = items.iter().map(|&i| (i, None)).collect();
            }
        } else if h.has(class::INTERFACE_ITEM, owner) {
            // the item's activation: asset = the first enabled element of the AssetRefList f44 (its object registers
            // it) → posted
            let list = h.read(class::INTERFACE_ITEM, owner).map(|r| r.u32("f44")).unwrap_or(0);
            let asset = self.first_enabled_asset(list);
            // H123: an existing asset object of that AssetRef gets one more holder and is only re-posted: no
            // registration, no Activation (fo-027 314.76 / 315.69 post of 0x4926's object)
            let shared = self.rules.h123_shared_asset_object
                && asset.is_some_and(|a| self.layouts.iter().any(|l| l.1.iter().any(|x| x.0 != owner && x.1 == Some(a))));
            if let Some(a) = asset.filter(|_| !shared) {
                let tl = self.trigger_list_of(a);
                self.out.push(Out::Reg { owner: a, tl, cause: RegCause::ObjectAsset { of: owner } });
                self.out.push(Out::Fire { owner: a, event: event::ACTIVATION });
            }
            for l in &mut self.layouts {
                if let Some(x) = l.1.iter_mut().find(|x| x.0 == owner) {
                    x.1 = asset;
                }
            }
        } else if h.has(class::EMOTION_ICON, owner)
            || h.has(class::INVENTORY, owner)
            || h.has(class::EVIDENCE_ICON, owner)
            || (self.rules.h203_action_icon_type3 && h.has(class::ACTION_ICON, owner))
            || (self.rules.h206_idea_icon_type3 && h.has(icons::IDEA_ICON_CLASS, owner))
        {
            // the icon activation (FUN_00470b40): the first enabled element of the AssetRefLists f30, f2c, f3c, in this
            // order — each registers its object, then each is posted.
            // H66: the VCInventory runtime object runs the same icon activation (fo-010 166.21 / 167.86, fo-013 136.92 /
            // 139.26: the EmotionIcon activation breakpoint hit for the inventory object); same ICON record layout
            // H71: the game-start EvidenceIcon object too (fo-010 64.27 Act 0xdb79 → REG/Act 0xdd7c of f30 0xdb7a)
            // H203 / H206: ActionIcon and IdeaIcon have the same activation
            let cls = [class::EMOTION_ICON, class::INVENTORY, class::EVIDENCE_ICON, class::ACTION_ICON, icons::IDEA_ICON_CLASS]
                .into_iter()
                .find(|&c| h.has(c, owner))
                .unwrap();
            let r = h.read(cls, owner);
            let lists: Vec<u32> = ["f30", "f2c", "f3c"].iter().map(|f| r.as_ref().map_or(0, |r| r.u32(f))).collect();
            let assets: Vec<u32> = lists.into_iter().filter_map(|l| self.first_enabled_asset(l)).collect();
            // H1181: an asset object another holder keeps alive is only re-posted (C151: the second fax 0xa186 does not
            // register 0xa182 / 0xa184 / 0xd545 again)
            let new: Vec<u32> = assets.iter().copied().filter(|&a| !self.icon_asset_held(a, owner)).collect();
            for &a in &new {
                let tl = self.trigger_list_of(a);
                self.out.push(Out::Reg { owner: a, tl, cause: RegCause::ObjectAsset { of: owner } });
            }
            for &a in &new {
                self.out.push(Out::Fire { owner: a, event: event::ACTIVATION });
            }
            if cls == class::INVENTORY {
                self.inv_assets.retain(|x| x.0 != owner);
                self.inv_assets.push((owner, assets.clone()));
            }
            // H203–H206: the other icon objects are tracked too (their removal deactivates the assets, as H61)
            let tracked = cls == class::EMOTION_ICON || self.icon_objs.iter().any(|o| o.id == owner);
            if tracked {
                self.icons_active.retain(|x| x.0 != owner);
                self.icons_active.push((owner, assets));
            }
        } else if h.has(class::CONVERSATION, owner) && self.rules.h126_dialog_box {
            // H126: the Conversation's cmd 11 builds its asset (first enabled of f2c; fo-027 577.83 cmd 11 → 577.95 REG
            // 0x4503)
            let list = h.read(class::CONVERSATION, owner).map(|r| r.u32("f2c")).unwrap_or(0);
            let asset = self.first_enabled_asset(list);
            if let Some(a) = asset {
                let tl = self.trigger_list_of(a);
                self.out.push(Out::Reg { owner: a, tl, cause: RegCause::ObjectAsset { of: owner } });
                self.out.push(Out::Fire { owner: a, event: event::ACTIVATION });
            }
            if let Some(d) = &mut self.dbox {
                if let Some(x) = d.2.iter_mut().find(|x| x.0 == owner) {
                    x.1 = asset;
                }
            }
        } else if h.has(class::CHAR_VIEW, owner) {
            // H74: the CharView's activation: Activation fired (the triggers' posts above), state-1 command (no trigger
            // effect), the conversation display data (no registration), then, unless the restore window is open (C30),
            // the wrapper (TL 0) is posted (fo-020: posts 190.66–196.93, REG 197.22, wrapper post 197.30)
            if !self.restoring {
                self.cv_wrapper_regs.push(owner);
                self.cv_players.push(owner);
                self.out.push(Out::Reg { owner, tl: Some(0), cause: RegCause::CharViewPlayer { char_view: owner } });
                self.out.push(Out::CharViewPlayer { char_view: owner });
            }
        }
    }

    /// HYPOTHESIS H74 — the command of a CharView's media wrapper runs (FUN_00480fe0; fo-020 215.97): no Activation
    /// fires; the first enabled AssetRef of the CharView's f50 list (C15) gets a media player (posted:
    /// [`Out::PlayerPost`], run by [`World::run_player`]); none → the wrapper is removed (fo-020: 0x4d22 was disabled by
    /// 0xe345 at 189.29, 0x4e89 / 0x4e8b disabled). False if no wrapper of `char_view` is pending.
    pub fn char_view_player(&mut self, char_view: u32) -> bool {
        let Some(i) = self.cv_players.iter().position(|&c| c == char_view) else { return false };
        self.cv_players.remove(i);
        let list = self.h.read(class::CHAR_VIEW, char_view).map(|r| r.u32("f50")).unwrap_or(0);
        let flagged = match self.cv_click.iter().position(|&c| c == char_view) {
            Some(i) => {
                self.cv_click.remove(i);
                true
            }
            None => false,
        };
        if let Some(asset) = self.first_enabled_asset(list) {
            self.cv_assets.push_back((asset, char_view));
            self.out.push(Out::PlayerPost { asset });
            if flagged {
                // H601: the wrapper lives on with the player; when it is removed is not traced
                self.out.push(Out::Unimplemented {
                    action: 0,
                    what: format!("H601: teleport click of CharView {char_view:#x} after its wrapper's player not modelled"),
                });
            }
        } else if flagged {
            // H601: none → the wrapper is removed (fo-068 160.561), its Deactivation queues a mouse command at the
            // centre of the CharView's f2c rect
            if let Some(Value::Region { rect, .. }) = self.h.read(class::CHAR_VIEW, char_view).and_then(|r| r.field("f2c").cloned()) {
                let (l, t, r, b) = (rect[0] as i32, rect[1] as i32, rect[2] as i32, rect[3] as i32);
                self.out.push(Out::MouseCommand { x: l + ((r - l) >> 1), y: t + ((b - t) >> 1) });
            }
        }
        true
    }

    /// HYPOTHESIS H66 — the start-icon builder (FUN_00477940), called by SetStartView after the View setup. Each created
    /// object is registered by its constructor (the common icon base: TL f40), posted and activated (an Activation
    /// command posted after it).
    /// 1. VCInventory 0x4b — new game: the class is enumerated (objId order assumed; fo-010 0x442b before 0x442f), each
    ///    enabled one (C15) is built; Load: the save's VCGameState la0 list (u32 array), no enabled check. Activated only
    ///    if the inventory preference is 'V' — always so in fo-010 / fo-013 / fo-014 / fo-015 (assumed).
    /// 2. every enabled VCEmotionIcon 0x48 (same object as PreloadEmIcon, H60), activated.
    /// 3. every enabled VCActionIcon 0x47, activated — TL rule not in [`World::trigger_list_of`] (none is enabled on the
    ///    traced routes), activation children not decoded.
    /// 4. Load only: VCEvidenceIcon 0x49 from the VCGameState's ldc list (8-byte records [id][active], activated iff
    ///    active ≠ 0) and l118 (u32 ids, kit icons, posted, not activated) — static only (both lists empty in the traced
    ///    saves).
    /// Not modelled (logged): the Load-only restore of the l64 list (12-byte records, media players posted) and of the
    /// l154 list (12-byte records → timer action messages), which run before step 1.
    fn start_icons(&mut self, action: u32, load: bool) {
        let h = self.h;
        let cause = RegCause::StartIcon { action, load };
        let mut created: Vec<(u32, u32, bool)> = Vec::new(); // (class, objId, activated)
        if load && self.rules.h67_saved_media_timers {
            self.restore_media_timers(action);
        } else if load {
            for f in ["l64", "l154"] {
                if self.saved_list(f, 12).map_or(false, |l| !l.is_empty()) {
                    self.out.push(Out::Unimplemented { action, what: format!("H66: Load restore of VCGameState {f} not modelled") });
                }
            }
        }
        if load {
            for id in self.saved_list("la0", 4).unwrap_or_default() {
                if h.has(class::INVENTORY, id[0]) {
                    created.push((class::INVENTORY, id[0], true));
                }
            }
        } else {
            created.extend(self.enabled_of(class::INVENTORY).into_iter().map(|id| (class::INVENTORY, id, true)));
        }
        created.extend(self.enabled_of(class::EMOTION_ICON).into_iter().map(|id| (class::EMOTION_ICON, id, true)));
        created.extend(self.enabled_of(class::ACTION_ICON).into_iter().map(|id| (class::ACTION_ICON, id, true)));
        let mut kit_ids: Vec<u32> = Vec::new();
        if load {
            for r in self.saved_list("ldc", 8).unwrap_or_default() {
                if h.has(class::EVIDENCE_ICON, r[0]) {
                    created.push((class::EVIDENCE_ICON, r[0], r[1] != 0));
                }
            }
            for r in self.saved_list("l118", 4).unwrap_or_default() {
                if h.has(class::EVIDENCE_ICON, r[0]) {
                    created.push((class::EVIDENCE_ICON, r[0], false));
                    kit_ids.push(r[0]);
                }
            }
        }
        for (cls, id, act) in created {
            let tl = self.trigger_list_of(id);
            self.out.push(Out::Reg { owner: id, tl, cause: cause.clone() });
            match cls {
                class::EMOTION_ICON => {
                    if !self.icons.contains(&id) {
                        self.icons.push(id);
                    }
                    self.pending_children.push(id);
                }
                class::INVENTORY => self.pending_children.push(id),
                // H203: the ActionIcon's activation builds its assets (the icon activation) and queues state 1
                class::ACTION_ICON if self.rules.h203_action_icon_type3 => {
                    self.icon_objs.push(icons::IconObj { cls, id, kit: false });
                    if act {
                        self.pending_children.push(id);
                    }
                }
                // H205: ldc (not kit, state 1 iff active) and l118 (kit) EvidenceIcons
                class::EVIDENCE_ICON if self.rules.h205_evidence_icon_type3 => {
                    // H702: kit = listed in l118 (was `!act`, which made an inactive ldc entry a kit icon)
                    let kit = if self.rules.h702_evidence_save { kit_ids.contains(&id) } else { !act };
                    self.icon_objs.push(icons::IconObj { cls, id, kit });
                    if act {
                        self.pending_children.push(id);
                    } else if self.rules.h702_evidence_save && !kit {
                        // H702: a built non-kit icon in state 0 (enabled, not yet displayed by DispEvIcons) is still an
                        // activated object (H205's Enable posts it) — activated here, no state command
                        self.pending_children.push(id);
                        self.out.push(Out::Fire { owner: id, event: event::ACTIVATION });
                    }
                }
                _ if act => self.out.push(Out::Unimplemented {
                    action,
                    what: format!("H66: activation of class {cls:#x} object {id:#x}: children not decoded"),
                }),
                _ => {}
            }
            if act {
                self.out.push(Out::Fire { owner: id, event: event::ACTIVATION });
                // H66: the state-1 command (H203 / H204 / H205 bookkeeping)
                let st = match cls {
                    class::EMOTION_ICON => self.rules.h204_emotion_icon_type3,
                    class::ACTION_ICON => self.rules.h203_action_icon_type3,
                    class::EVIDENCE_ICON => self.rules.h205_evidence_icon_type3,
                    _ => false,
                };
                if st {
                    self.icon_set_state(id, true);
                }
            }
        }
        // H126 (static, the start-icon builder's tail, load only): VCGameState f1a4 names the CharView whose dialogue
        // box was open → its box is rebuilt as by DisplayDBox (fo-029 129.30 REG 0x590d, posted; save fo-028-end.x
        // f1a4 = 0x590c)
        if load && self.rules.h126_dialog_box {
            let cv = self
                .gam
                .class(class::GAME_STATE)
                .and_then(|c| c.objects.first())
                .and_then(|&(id, _)| self.gam.read(class::GAME_STATE, id))
                .map_or(0, |r| r.u32("f1a4"));
            if cv != 0 && self.h.has(class::CHAR_VIEW, cv) {
                self.display_dbox(cv);
            }
        }
    }

    /// HYPOTHESIS H67 — the Load-only head of the start-icon builder (before the start icons): VCGameState f1a8 = a
    /// VCIFaceLayout to show (H732); l64: per 12-byte record [AssetRef][flag][position] a media player (flag = looping,
    /// position = start) is posted — its AssetRef is registered and activated when it runs (fo-013 134.30 REG 0x76bf TL
    /// 0, 135.22 Act); l154: per record [id][ms][periodic] a type-2 action message (blob byte 4 = id, the delay = ms, byte
    /// 5 bit 0x80 = periodic, mode 0) is built and posted (fo-013 116.01 → run 135.77 type 2 → timer armed 0x733b ms).
    fn restore_media_timers(&mut self, action: u32) {
        if let Some(r) = self.gam.class(class::GAME_STATE).and_then(|c| c.objects.first()).and_then(|&(id, _)| self.gam.read(class::GAME_STATE, id)) {
            if self.rules.h1603_flight_saved {
                // H1603: the flashlight globals from b190 / b191 / b192
                self.dev.flight = [r.u32("b190") as u8, r.u32("b191") as u8, r.u32("b192") as u8];
                self.dev.flight_known = true;
            }
            let layout = r.u32("f1a8");
            if layout != 0 && self.rules.h732_load_layout && self.h.has(class::IFACE_LAYOUT, layout) {
                // H732 (post-hoc fo-061 / fo-063 / fo-064): the layout is built (REG TL 0, right after the start View's
                // REG), posted and set as View like SetIFLAsView (H58 / H130)
                self.layout_cmd(action, layout, 0);
                if self.rules.h130_hit_chain && !self.layouts_as_view.contains(&layout) {
                    self.layouts_as_view.push(layout);
                }
            } else if layout != 0 {
                self.out.push(Out::Unimplemented { action, what: format!("H67: Load shows the saved VCIFaceLayout {layout:#x} (not modelled)") });
            }
        }
        for rec in self.saved_list("l64", 12).unwrap_or_default() {
            let asset = rec[0];
            if !self.h.has(class::ASSET_REF, asset) {
                self.out.push(Out::Unimplemented { action, what: format!("H67: saved player {asset:#x} is not an AssetRef") });
                continue;
            }
            self.restored_players.push_back((asset, action));
            self.restored_pos.insert(asset, rec[2]);
            if self.rules.h100_player_mode && rec[1] != 0 {
                self.looping_players.insert(asset);
            }
            self.out.push(Out::PlayerPost { asset });
        }
        for rec in self.saved_list("l154", 12).unwrap_or_default() {
            self.restored_timers.push_back((rec[0] as u8, rec[1], rec[2] != 0));
            self.post(Msg { action: 0, trigger: 0 }, PostSrc::Restore);
        }
    }

    /// HYPOTHESIS H67: the oldest restored media player's command runs (trace: the player's activation command
    /// dispatched; fo-013 134.10 → REG 0x76bf TL 0 134.30, Act 135.22): its AssetRef is
    /// registered and activated. False if no restored player is pending (a type-1 / type-7 player: already done at exec).
    pub fn run_player(&mut self) -> bool {
        let Some((asset, action)) = self.restored_players.pop_front() else {
            // H74: a CharView wrapper's player (same player class; untraced — no enabled f50 asset in fo-020)
            let Some((asset, char_view)) = self.cv_assets.pop_front() else { return false };
            let tl = self.trigger_list_of(asset);
            self.played.insert(asset);
            self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::CharViewAsset { char_view } });
            self.out.push(Out::Fire { owner: asset, event: event::ACTIVATION });
            return true;
        };
        let tl = self.trigger_list_of(asset);
        self.played.insert(asset);
        self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::RestoredPlayer { action } });
        self.out.push(Out::Fire { owner: asset, event: event::ACTIVATION });
        true
    }

    /// HYPOTHESIS H145 (H84 path): the media of `asset`'s player ended and the (non-looping) player was removed →
    /// the AssetRef's Deactivation, if the core played it. False otherwise (an asset the core did not play, a looping one)
    pub fn media_end(&mut self, asset: u32) -> bool {
        if !self.rules.h145_media_end_input || self.looping_players.contains(&asset) || !self.played.remove(&asset) {
            return false;
        }
        self.out.push(Out::Fire { owner: asset, event: event::DEACTIVATION });
        true
    }

    /// HYPOTHESIS H68: the state-0 command of `icon` posted by an icon click runs (trace: the state command dispatched
    /// on the icon): an active icon is deactivated, then its assets (as H61); the object leaves the "exists" list (removed
    /// at the click — whether this is what a later PreloadEmIcon sees is not traced). False if no such command is pending.
    pub fn icon_off(&mut self, icon: u32) -> bool {
        let Some(i) = self.icons_off.iter().position(|&x| x == icon) else { return false };
        self.icons_off.remove(i);
        // C296: an ActionIcon's state 0 + removal (bar_place 0, Deactivation, assets)
        if self.icon_objs.iter().any(|x| x.cls == class::ACTION_ICON && x.id == icon) {
            self.icon_remove(icon);
            return true;
        }
        self.icons.retain(|&x| x != icon);
        if self.rules.h204_emotion_icon_type3 {
            self.icon_set_state(icon, false); // H68's command is the state-0 command
        }
        if let Some(i) = self.icons_active.iter().position(|x| x.0 == icon) {
            let (icon, assets) = self.icons_active.remove(i);
            self.out.push(Out::Fire { owner: icon, event: event::DEACTIVATION });
            for a in assets {
                if !self.icon_asset_held(a, icon) {
                    self.out.push(Out::Fire { owner: a, event: event::DEACTIVATION });
                }
            }
        }
        true
    }

    /// HYPOTHESIS H69 — application start (traced fo-005 / fo-009 / fo-017 4.9–20.5 s): AssetRef 0xf3d6 is registered
    /// (TL 0) and posted; then, for each id of the exe's startup cursor list (`exetables` `cursors`, 12 ids) that exists
    /// as a VCCursor, a cursor object is registered (TL 0, H57) and posted, then an ownerless cursor. The queue then
    /// runs: Act 0xf3d6; per cursor the asset of the first enabled AssetRef of its f38 (an asset object that exists
    /// already is not registered or activated again: 0xea2e shares 0x116c with 0x11e), Act asset, then Act cursor.
    pub fn startup(&mut self) {
        const SOUND: u32 = 0xf3d6;
        let reg = |w: &mut Self, owner: u32| {
            let tl = w.trigger_list_of(owner);
            w.out.push(Out::Reg { owner, tl, cause: RegCause::Startup });
        };
        reg(self, SOUND);
        let cursors: Vec<u32> = self.h.exe.cursors.iter().copied().filter(|&c| self.h.has(class::CURSOR, c)).collect();
        for &c in &cursors {
            reg(self, c);
        }
        reg(self, 0);
        self.out.push(Out::Fire { owner: SOUND, event: event::ACTIVATION });
        let mut built = vec![SOUND];
        for c in cursors.into_iter().chain([0]) {
            let list = if c == 0 { 0 } else { self.h.read(class::CURSOR, c).map_or(0, |r| r.u32("f38")) };
            if let Some(a) = self.first_enabled_asset(list) {
                if !built.contains(&a) {
                    built.push(a);
                    reg(self, a);
                    self.out.push(Out::Fire { owner: a, event: event::ACTIVATION });
                }
            }
            self.out.push(Out::Fire { owner: c, event: event::ACTIVATION });
        }
    }

    /// HYPOTHESIS H71 — game start FUN_0048cd70 (new game: main menu "New"; Load: SaveLoadComit; static + fo-009/010/013..
    /// 017: the game-start media reset marks its entry). In order: the AI icon's update builds, unless it exists, the
    /// EvidenceIcon object 0xdb79 (named at startup, H69; the common icon base: TL f40) and posts it; the VCGameState
    /// object is registered (the save's class-0x57 record, or a new one — objId 0x14cb in every new-game trace; TL 0,
    /// never activated); then the main menu is removed and every VCTitle (class 0x27; a per-object check not decoded) is
    /// built (TL fa0) and posted. Queue: Act 0xdb79 (its f30/f2c/f3c assets: 0xdd7c), Act 0x12b (its trigger 0x66d6 → …
    /// SetStartView).
    /// C86: the values of the 24 RegPreferences variables (the host's Preferences as the game sees them); unknown ones left out
    pub fn pref_values(&self) -> Vec<(u32, i32)> {
        PREF_VARS.iter().filter_map(|&v| self.vars.get(v).map(|x| (v, x))).collect()
    }

    /// C86: the host's Preferences written into their variables (each keeps its type, as RegPreferences' read-back, H81
    /// state 5)
    pub fn set_prefs(&mut self, prefs: &[(u32, i32)]) {
        for &(v, x) in prefs {
            if let Some(t) = self.vars.typ(v) {
                self.vars.set_native(v, Some(x), t);
            }
        }
    }

    pub fn game_start(&mut self) {
        const ICON: u32 = 0xdb79;
        // H141 (static: the game start removes the app kinds 0x1e / 0x1f …; fo-030 82.19–89.45): the shown layouts (the
        // load screen) and their items / assets are removed with no traced Deactivation
        if self.rules.h141_game_start_layouts {
            let gone: Vec<u32> = self.layouts.drain(..).flat_map(|(l, items)| std::iter::once(l).chain(items.into_iter().map(|x| x.0))).collect();
            self.pending_children.retain(|o| !gone.contains(o));
        }
        if !self.game_icon && self.h.has(class::EVIDENCE_ICON, ICON) {
            self.game_icon = true;
            let tl = self.trigger_list_of(ICON);
            self.out.push(Out::Reg { owner: ICON, tl, cause: RegCause::GameStart });
            self.pending_children.push(ICON);
            self.out.push(Out::Fire { owner: ICON, event: event::ACTIVATION });
        }
        let state = self.gam.class(class::GAME_STATE).and_then(|c| c.objects.first()).map_or(0x14cb, |&(id, _)| id);
        self.out.push(Out::Reg { owner: state, tl: Some(0), cause: RegCause::GameStart });
        let titles: Vec<u32> = self.h.class(class::TITLE).map(|c| c.objects.iter().map(|&(id, _)| id).collect()).unwrap_or_default();
        for t in titles {
            let tl = self.trigger_list_of(t);
            self.out.push(Out::Reg { owner: t, tl, cause: RegCause::GameStart });
            self.out.push(Out::Fire { owner: t, event: event::ACTIVATION });
        }
    }

    /// Objects of `cls` in XFiles.hdb (objId order) that are enabled now (C15).
    fn enabled_of(&self, cls: u32) -> Vec<u32> {
        match self.h.class(cls) {
            Some(c) => c.objects.iter().map(|&(id, _)| id).filter(|&id| self.is_enabled(cls, id)).collect(),
            None => Vec::new(),
        }
    }

    /// A list blob of the save's VCGameState (H66): little-endian u32 records of `size` bytes (the blob's length / size
    /// records). None if the gam has no VCGameState or no such blob.
    fn saved_list(&self, field: &str, size: usize) -> Option<Vec<Vec<u32>>> {
        let &(id, _) = self.gam.class(class::GAME_STATE)?.objects.first()?;
        let r = self.gam.read(class::GAME_STATE, id)?;
        let b = r.blob(field)?;
        if b.len == 0 {
            return Some(Vec::new());
        }
        let bytes = self.gam.blob_bytes(b)?;
        Some(
            bytes
                .chunks_exact(size)
                .map(|c| c.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect())
                .collect(),
        )
    }

    /// First enabled AssetRef of an AssetRefList (not in the C15 table → enabled).
    fn first_enabled_asset(&self, list: u32) -> Option<u32> {
        if list == 0 {
            return None;
        }
        self.h.list_of(class::ASSET_REF_LIST, list).into_iter().find(|&x| self.is_enabled(class::ASSET_REF, x))
    }

    // ---- queue (C24) --------------------------------------------------------------------------

    fn post(&mut self, msg: Msg, src: PostSrc) {
        let at = if self.started { self.ins.min(self.queue.len()) } else { self.queue.len() };
        self.queue.insert(at, QItem::Msg(msg));
        if self.started {
            self.ins += 1;
        }
        self.h76_inserted(at); // H76
        self.out.push(Out::Post { msg, src });
    }

    /// HYPOTHESIS H201: a message the main loop posts from a native timer's command list goes
    /// to the back of the queue, not to the C24 insert point.
    pub(super) fn post_back(&mut self, msg: Msg, src: PostSrc) {
        let at = self.queue.len();
        self.queue.push(QItem::Msg(msg));
        self.h76_inserted(at); // H76
        self.out.push(Out::Post { msg, src });
    }

    /// A queue command starts running: later posts go to the front (C24).
    pub fn begin_dispatch(&mut self) {
        self.started = true;
        self.ins = 0;
    }

    /// The queued action messages, queue order (H89 commands left out).
    pub fn pending(&self) -> Vec<Msg> {
        self.queue.iter().filter_map(|q| if let QItem::Msg(m) = q { Some(*m) } else { None }).collect()
    }

    /// Dispatch the head of the queue (C24 FIFO + front insertion).
    pub fn next(&mut self) -> Option<Msg> {
        self.h76_settle(); // H76
        self.h89_pump(); // H89
        self.h89_flush_to(None);
        let QItem::Msg(m) = self.queue.first()?.clone() else { return None };
        self.queue.remove(0);
        self.h76_removed(0);
        self.begin_dispatch();
        Some(m)
    }

    /// Dispatch a specific action out of order (used by a driver to follow an external schedule).
    pub fn take(&mut self, action: u32) -> Option<Msg> {
        self.h76_settle(); // H76
        self.h89_pump(); // H89
        self.h89_flush_to(Some(action));
        let i = self.queue.iter().position(|q| matches!(q, QItem::Msg(m) if m.action == action))?;
        if self.h76_behind(i) {
            self.h76_release(); // H76: a message behind the held fires runs → they ran before it
        }
        let QItem::Msg(m) = self.queue.remove(i) else { unreachable!() };
        self.h76_removed(i);
        self.begin_dispatch();
        Some(m)
    }

    // ---- interpreter (C17, C21, H43 cases with confirmed rules) --------------------------------

    /// Condition of an action (C21): no prefix -> true; prefix -> H46 operand pair (C22).
    pub fn condition(&mut self, msg: &Msg) -> Option<bool> {
        let action = msg.action;
        let ctx = (self.rules.h73_trigger_locals || self.rules.h90_event_ctx).then_some(msg.trigger);
        if action == 0
            && (!self.restored_timers.is_empty()
                || !self.engine_layouts.is_empty()
                || !self.engine_enables.is_empty()
                || !self.engine_timer_stops.is_empty()
                || (self.rules.h623_ai_click && !self.px.engine_views.is_empty()))
        {
            // H67: a restored timer message has no condition prefix (fo-013 135.44 action_cond 0 → 1); H70 the same for
            // the menu's type-5 message (fo-017 48.71)
            self.out.push(Out::Cond { action, value: Some(true) });
            return Some(true);
        }
        let v = match Action::load(self.h, action) {
            Some(a) => match a.condition() {
                // kind 1 needs H73 (the trigger is passed for H90 too)
                Some(c) if !self.rules.h73_trigger_locals && (c.lhs_kind == 1 || c.rhs_kind == 1) => None,
                Some(c) => self.vars.condition_in(&c, ctx),
                None => Some(true),
            },
            None => None,
        };
        self.out.push(Out::Cond { action, value: v });
        v
    }

    /// Run an action whose condition held (interpreter FUN_00433540, switch on type & 0x7f).
    pub fn execute(&mut self, msg: Msg) {
        self.execute_msg(msg);
        self.h76_settle(); // H76: the last action ahead of held fires has run
        self.h89_pump(); // H89
    }

    fn execute_msg(&mut self, msg: Msg) {
        if msg.action == 0 {
            if let Some((id, ms, periodic)) = self.restored_timers.pop_front() {
                // H67: the restored type-2 message (mode 0): start timer `id` with the saved remaining ms
                self.out.push(Out::Exec { action: 0, typ: 2 });
                self.timers_running.retain(|x| x.0 != id);
                self.timers_running.push((id, periodic));
                self.out.push(Out::Timer { id, start: Some((ms, periodic)) });
                return;
            }
            if let Some((layout, mode)) = self.engine_layouts.pop_front() {
                // H70: the engine-built show runs as type 5 (fo-017 48.997 action_exec 0 type 5 → t5_layout)
                self.out.push(Out::Exec { action: 0, typ: 5 });
                self.layout_cmd(0, layout, mode);
                return;
            }
            if let Some(id) = self.engine_timer_stops.pop_front() {
                // H146: the engine-built stop runs as type 2 mode 1
                self.out.push(Out::Exec { action: 0, typ: 2 });
                self.timers_running.retain(|x| x.0 != id);
                self.out.push(Out::Timer { id, start: None });
                return;
            }
            if let Some((cls, target, state)) = self.engine_enables.pop_front() {
                // H97: the engine-built Enable / Disable (class, objId, state) runs as type 3
                self.out.push(Out::Exec { action: 0, typ: 3 });
                self.enable_target(0, target, cls, state);
                return;
            }
            if let Some(view) = self.px.engine_views.pop_front() {
                // H192: the engine-built Set View runs as type 4
                self.run_engine_view(view);
                return;
            }
        }
        let Some(a) = Action::load(self.h, msg.action) else {
            self.out.push(Out::Unimplemented { action: msg.action, what: "VCAction not in XFiles.hdb".into() });
            return;
        };
        self.out.push(Out::Exec { action: a.id, typ: a.typ });
        let body = a.body().to_vec();
        match a.kind() {
            0 => self.exec_variable(&a, &body, msg.trigger),
            1 => self.exec_play(&a, &body),
            2 if self.rules.h59_timer => self.exec_timer(&a, &body),
            3 => self.exec_enable(&a, &body),
            4 => self.exec_set_view(&a, &body),
            5 if self.rules.h58_iface_layout => self.exec_layout(&a, &body),
            6 => self.exec_proc(&a, &body, msg.trigger),
            7 if self.rules.h67_saved_media_timers => self.exec_play(&a, &body),
            t => self.out.push(Out::UnknownType { action: a.id, typ: t }),
        }
    }

    /// type 0: variable op on the operand pair after the optional prefix (H46, C22)
    fn exec_variable(&mut self, a: &Action, body: &[u8], trigger: u32) {
        let ctx = self.rules.h73_trigger_locals.then_some(trigger);
        let result = match Operands::parse(body) {
            Some(p) => self.vars.apply_in(&p, ctx),
            None => SetResult::UnknownOp(0xff),
        };
        self.out.push(Out::SetVar { action: a.id, result });
    }

    /// type 1: play an AssetRefList (C23: blob [u32 AssetRefList][u8]; the AssetRef is registered and
    /// activated). Which element plays is HYPOTHESIS H48 (first enabled).
    fn exec_play(&mut self, a: &Action, body: &[u8]) {
        let Some(list) = be32(body, 0) else {
            self.out.push(Out::Unimplemented { action: a.id, what: "type 1: short blob".into() });
            return;
        };
        if !self.rules.h48_play_first_enabled {
            self.out.push(Out::Unimplemented {
                action: a.id,
                what: format!("type 1: play AssetRefList {list:#x} (element choice = H48, disabled)"),
            });
            return;
        }
        let mode = body.get(4).copied().unwrap_or(1);
        if self.rules.h100_player_mode && mode != 1 && mode != 2 {
            // H100: mode 3 = stop the list's players (untraced), other modes: no case (mode 3 walks every element: an
            // existing player loses its looping mark and its movie is stopped; it then ends like a media end: removed
            // → Deactivation of the AssetRef (fo-025 0xd281 874.58 → removals 885.14 / 885.55, Deact 0xcc41 885.72, after
            // the trigger's queued actions; post-hoc)
            if mode == 3 {
                for x in self.h.list_of(class::ASSET_REF_LIST, list) {
                    let known = self.looping_players.remove(&x);
                    if known {
                        self.played.remove(&x); // H145: its Deactivation is the stop's (the media end that follows adds none)
                    }
                    self.out.push(Out::PlayerStop { asset: x, known });
                }
            }
            return;
        }
        let assets = self.h.list_of(class::ASSET_REF_LIST, list);
        if let Some(&asset) = assets.iter().find(|&&x| self.is_enabled(class::ASSET_REF, x)) {
            if self.rules.h100_player_mode {
                // H100: a player for the AssetRef exists → no new player (fo-023 0x1238a at 1033.6 / 1177.8)
                if self.looping_players.contains(&asset) {
                    return;
                }
                if mode == 2 {
                    self.looping_players.insert(asset);
                }
            }
            if self.rules.c61_one_player {
                // C61 (fo-076; fo-127 119.102: 0x8b1e plays 0x5bbe while its player from 114.916 still runs → no REG):
                // the AssetRef's player exists → no second one, whatever the mode
                if !self.live_players.insert(asset) {
                    return;
                }
            }
            let tl = self.trigger_list_of(asset);
            self.played.insert(asset);
            self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::PlayAsset { action: a.id } });
            // C23: each played AssetRef is registered and activated (fo-005 0x4d6a, fo-009 0x8c96: REG then Act)
            self.out.push(Out::Fire { owner: asset, event: event::ACTIVATION });
        }
    }

    /// HYPOTHESIS H59 — type 2 (FUN_00428700): blob `[u32 ms][u8 id][u8 flags]` (id = byte 4, mode = byte 5 & 0x7f,
    /// periodic = byte 5 & 0x80). A running timer with the same id is killed first (the timer list); mode 0 → new timer
    /// (a native multimedia timer of `ms`, periodic or one-shot); mode 1 → pending expirations of the id dropped.
    fn exec_timer(&mut self, a: &Action, body: &[u8]) {
        let (Some(ms), Some(&id), Some(&flags)) = (be32(body, 0), body.get(4), body.get(5)) else {
            self.out.push(Out::Unimplemented { action: a.id, what: "type 2: short blob".into() });
            return;
        };
        self.timers_running.retain(|x| x.0 != id);
        match flags & 0x7f {
            0 => {
                self.timers_running.push((id, flags & 0x80 != 0));
                self.out.push(Out::Timer { id, start: Some((ms, flags & 0x80 != 0)) })
            }
            1 => self.out.push(Out::Timer { id, start: None }),
            m => {
                self.out.push(Out::Timer { id, start: None });
                self.out.push(Out::Unimplemented { action: a.id, what: format!("type 2: mode {m}") });
            }
        }
    }

    /// HYPOTHESIS H58 — type 5 (FUN_0044ec80): blob `[u32 VCIFaceLayout][u8 mode]`.
    /// Mode 0: unless a runtime object of that layout exists, build one (REG TL 0) and post it; its activation builds
    /// the items ([`World::create_children`]). Mode 1: if it exists, its removal is posted — traced effect (fo-009):
    /// Deactivation of every item, each followed by its asset;
    /// the layout itself gets no Deactivation. A layout that is not shown: nothing (fo-009 0x14579 → 0x7f94).
    fn exec_layout(&mut self, a: &Action, body: &[u8]) {
        let (Some(layout), Some(&mode)) = (be32(body, 0), body.get(4)) else {
            self.out.push(Out::Unimplemented { action: a.id, what: "type 5: short blob".into() });
            return;
        };
        self.layout_cmd(a.id, layout, mode);
    }

    /// HYPOTHESIS H70 / H140: a menu opens the load screen (load mode, "return to the menu" set, show 0x142ca) or the
    /// save screen (save mode, "return to the menu" = `from_menu`, show 0x14269)
    pub fn menu_screen(&mut self, layout: u32, from_menu: bool) {
        if self.rules.h143_save_comit {
            self.save_from_menu = from_menu;
        }
        self.engine_layout(layout, 0);
    }

    /// H143 / H403: SaveLoadComit(arg). Save mode, arg ≠ 0: the typed name, else the
    /// slot's name; none → the game's message box and nothing else (not shown by the port: logged); else the save is
    /// written (`Out::SaveGame`; the "overwrite?" box is not modelled — answered yes) and the screen closes. Load mode,
    /// arg ≠ 0: a named slot is loaded (`Out::LoadGame`; the game start then drops the screen, H141). arg 0: the screen
    /// closes.
    fn save_comit(&mut self, action: u32, arg: u32) {
        let load = self.load_screen();
        let data = self.rules.h403_save_screen_data;
        if arg != 0 && data {
            let slot = self.save_slot % SAVE_SLOTS;
            if load {
                if !self.save_names[slot].is_empty() {
                    self.out.push(Out::LoadGame { slot });
                }
                return;
            }
            let typed = self.save_edit.clone().unwrap_or_default();
            let name = if typed.trim().is_empty() { self.save_names[slot].clone() } else { typed.trim().to_string() };
            if name.is_empty() && !self.save_name_unknown {
                self.out.push(Out::Unimplemented { action, what: "H403: save without a name (message box 0x20)".into() });
                return;
            }
            if !name.is_empty() {
                self.save_names[slot] = name.clone();
                self.out.push(Out::SaveGame { slot, name });
            }
            self.save_edit = None;
            self.save_name_unknown = false;
        }
        self.close_save_screen(if arg == 0 && load { LOAD_SCREEN } else { SAVE_SCREEN });
        if self.save_from_menu && data {
            self.out.push(Out::MainMenu { action });
        }
    }

    /// "The screen closes": a hide of the screen `on`; opened from the menu, hides of both screens follow
    /// (a hide of a screen not shown does nothing: fo-030 records the save screen hidden twice, H143).
    fn close_save_screen(&mut self, on: u32) {
        let from_menu: &[u32] = if self.save_from_menu { &[SAVE_SCREEN, LOAD_SCREEN] } else { &[] };
        for &screen in std::iter::once(&on).chain(from_menu) {
            self.engine_layout(screen, 1);
        }
    }

    /// H403: SaveKeyPress with the event's key: Tab / Down / Up move the selection,
    /// Enter / Esc commit / cancel, Select / Print Screen / Delete are ignored, any other key edits the save-mode buffer.
    fn save_key(&mut self, action: u32, key: u32) {
        let load = self.load_screen();
        match SaveKey::of(key) {
            SaveKey::Step(dir) => {
                if !load {
                    self.show_slot_texts();
                }
                self.save_slot = self.stepped_slot(dir, load);
                self.save_edit = None;
                self.save_bar_follow(); // H2280
            }
            SaveKey::Commit(arg) => self.save_comit(action, arg),
            SaveKey::Ignore => {}
            SaveKey::Edit if load => {}
            SaveKey::Edit => {
                let name = self.save_edit.get_or_insert_with(String::new);
                if key == SaveKey::BACKSPACE {
                    name.pop();
                } else if let Some(c) = procs::login_char(key).filter(|_| name.chars().count() < SaveKey::MAX_NAME) {
                    name.push(c);
                }
                self.show_selected_slot_text();
            }
        }
    }

    /// The slot one step from the selected one in direction `dir` (±1, wrapping). On the load screen the step goes on
    /// to the nearest slot with a name; with no named slot at all it is a single step.
    fn stepped_slot(&self, dir: isize, load: bool) -> usize {
        let at = |k: isize| (self.save_slot as isize + dir * k).rem_euclid(SAVE_SLOTS as isize) as usize;
        if !load {
            return at(1);
        }
        (1..=SAVE_SLOTS as isize).map(at).find(|&s| !self.save_names[s].is_empty()).unwrap_or(at(1))
    }

    /// The slot text items of the shown screen (`exetables`: the save or the load screen's list).
    fn slot_items(&self, load: bool) -> Vec<u32> {
        if load { self.h.exe.load_slot_items.clone() } else { self.h.exe.save_slot_items.clone() }
    }

    /// Every slot item of the save screen shows its text anew (H142).
    fn show_slot_texts(&mut self) {
        for item in self.slot_items(false) {
            self.item_text_asset(item);
        }
    }

    /// The selected save slot's item shows its text anew (H142 / H403).
    fn show_selected_slot_text(&mut self) {
        if let Some(&item) = self.h.exe.save_slot_items.get(self.save_slot % SAVE_SLOTS) {
            self.item_text_asset(item);
        }
    }

    /// The text a save / load slot item shows (H403): the typed buffer on the selected save slot, else the slot's name.
    pub fn save_slot_text(&self, slot: usize) -> String {
        match (&self.save_edit, slot == self.save_slot && !self.load_screen()) {
            (Some(b), true) => b.clone(),
            _ => self.save_names.get(slot).cloned().unwrap_or_default(),
        }
    }

    /// HYPOTHESIS H70: the engine posts a type-5 message for `layout` / `mode` (action id 0, not an act_post).
    pub fn engine_layout(&mut self, layout: u32, mode: u8) {
        self.engine_layouts.push_back((layout, mode));
        self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
    }

    /// Type 5 (H58) on `layout` with `mode`, run by action `action` (0 = an engine-built message, H70).
    fn layout_cmd(&mut self, action: u32, layout: u32, mode: u8) {
        let shown = self.layouts.iter().position(|l| l.0 == layout);
        match (mode, shown) {
            (0, None) if self.h.has(class::IFACE_LAYOUT, layout) => {
                self.layouts.push((layout, Vec::new()));
                self.pending_children.push(layout);
                let tl = self.trigger_list_of(layout);
                self.out.push(Out::Reg { owner: layout, tl, cause: RegCause::IfaceLayout { action } });
                self.out.push(Out::Fire { owner: layout, event: event::ACTIVATION });
            }
            (0, _) => {}
            (1, Some(i)) => {
                // H58 order: the layout's hide removes the items in container order — `items` is kept in that order
                // (show: `item_container_order`, Enable: `item_container_pos`)
                let (_, items) = self.layouts.remove(i);
                self.layouts_as_view.retain(|&l| l != layout);
                for (it, _) in &items {
                    self.px.item_rects.remove(it); // H179: the item object (and its moved rect) goes
                }
                self.pending_children.retain(|&o| o != layout && !items.iter().any(|x| x.0 == o));
                for (k, &(item, asset)) in items.iter().enumerate() {
                    self.out.push(Out::Fire { owner: item, event: event::DEACTIVATION });
                    if let Some(asset) = asset {
                        // H123: the shared asset object is deactivated when its last holder removes it (fo-027 378.80:
                        // Deact 0x4926 after the third item 0x8500's removal, none after 0x84ee's / 0x84f4's)
                        // (owner 0 = a text asset, H72 a / H102: its own object, never looked up)
                        let held = self.rules.h123_shared_asset_object
                            && asset != 0
                            && (items[k + 1..].iter().any(|x| x.1 == Some(asset))
                                || self.layouts.iter().any(|l| l.1.iter().any(|x| x.1 == Some(asset))));
                        if !held {
                            self.out.push(Out::Fire { owner: asset, event: event::DEACTIVATION });
                        }
                    }
                }
            }
            (1, None) => {}
            (m, _) => self.out.push(Out::Unimplemented { action, what: format!("type 5: mode {m}") }),
        }
    }

    /// HYPOTHESIS H58 order: InterfaceItem sort key (f4c & 0xff).
    pub fn item_key(&self, i: u32) -> u32 {
        self.h.read(class::INTERFACE_ITEM, i).map(|r| r.u32("f4c") & 0xff).unwrap_or(0)
    }

    /// HYPOTHESIS H58 order: insert position in the sorted container (FUN_004390e0):
    /// key ≥ the last element's → appended; smaller → before the first element whose key is ≥ its own.
    fn container_insert_pos(&self, order: &[u32], item: u32) -> usize {
        let k = self.item_key(item);
        match order.last() {
            Some(&last) if k < self.item_key(last) => order.iter().position(|&x| self.item_key(x) >= k).unwrap_or(order.len()),
            _ => order.len(),
        }
    }

    /// HYPOTHESIS H58 order: `items` inserted one by one (build order) into the layout's sorted container.
    fn item_container_order(&self, items: Vec<u32>) -> Vec<u32> {
        let mut v: Vec<u32> = Vec::new();
        for i in items {
            let p = self.container_insert_pos(&v, i);
            v.insert(p, i);
        }
        v
    }

    /// Where an Enable-built item of a shown layout goes (H58 order; without the rule at the end).
    fn item_container_pos(&self, layout: u32, item: u32) -> usize {
        let Some(l) = self.layouts.iter().find(|l| l.0 == layout) else { return 0 };
        if !self.rules.h58_item_order {
            return l.1.len();
        }
        let order: Vec<u32> = l.1.iter().map(|x| x.0).collect();
        self.container_insert_pos(&order, item)
    }

    /// Layouts shown (H58), with their items.
    pub fn layouts(&self) -> &[(u32, Vec<(u32, Option<u32>)>)] {
        &self.layouts
    }

    /// H130: shown `layout` set as View by SetIFLAsView (its own hit-test category).
    pub fn layout_as_view(&self, layout: u32) -> bool {
        self.layouts_as_view.contains(&layout)
    }

    /// type 3: Enable (C16; blob [u32 VCEnabled][u32][u8 state]). The state table is the one the nav
    /// filter reads (C15). VCNav targets in the current View's list gain/lose their nav-hotspot (C16);
    /// effects on other target classes are not confirmed and not modelled (state is still stored).
    fn exec_enable(&mut self, a: &Action, body: &[u8]) {
        let (Some(rec), Some(&state)) = (be32(body, 0), body.get(8)) else {
            self.out.push(Out::Unimplemented { action: a.id, what: "type 3: short blob".into() });
            return;
        };
        let Some(&(target, cls)) = self.enabled_rec.get(&rec) else {
            self.out.push(Out::Unimplemented { action: a.id, what: format!("type 3: VCEnabled {rec:#x} not in gam") });
            return;
        };
        self.enable_target(a.id, target, cls, state);
    }

    /// H1124 (e2e `--chained`, a segment boundary = save → Load): the Load's start procedures run RegViewState again (H80:
    /// its variable := 0, type 0; H1121: fo-132-end.x holds 'a', fo-133-end.x 0 as the core after a Load)
    pub fn load_reg_writes(&mut self) {
        if let Some(v) = self.view_state_var.filter(|_| self.rules.h80_reg_vars) {
            self.vars.set_native(v, Some(0), 0);
        }
    }

    /// H1122: a type-4 action message is still queued (the runner's queue, or the world's own in lock-step)
    fn set_view_queued_now(&self) -> bool {
        self.set_view_queued || self.queue.iter().any(|q| matches!(q, QItem::Msg(m) if self.is_set_view_action(m.action)))
    }

    /// H1122: the queue walk's test — the queued message's action is of type 4 (SetView)
    pub fn is_set_view_action(&self, action: u32) -> bool {
        action != 0 && self.h.read(0x41, action).is_some_and(|r| r.u32("type") & 0x7f == 4)
    }

    /// Type-3 effect on (`cls`, `target`) with `state`, run by action `action` (0 = an engine-built type-3 message, H97).
    fn enable_target(&mut self, action: u32, target: u32, cls: u32, state: u8) {
        let was = self.is_enabled(cls, target);
        self.enabled.insert((cls, target), state);
        if cls == class::ASSET_REF && !(self.rules.h1122_bg_skip_on_queued_set_view && self.set_view_queued_now()) {
            self.choose_background();
        }
        if cls == class::EXPLORABLE {
            self.enable_explorable(action, target, was, state != 0);
        }
        if cls == class::CHAR_VIEW {
            self.enable_char_view(target, was, state != 0);
        }
        // HYPOTHESIS H138: only on a flag change
        if cls == class::CONVERSATION && self.rules.h138_conversation_enable && was != (state != 0) {
            self.conversation_enable(target, state != 0);
        }
        if cls == class::INVENTORY && self.rules.h101_inventory_enable && was != (state != 0) {
            self.enable_inventory(action, target, state != 0);
        }
        // HYPOTHESIS H72 (b) (fo-017 72.28 Enable 0x142ed → 72.38 REG 0x142d3 TL f48, 72.91 Act, 73.20 REG
        // 0x14264 = its f44 asset, 73.76 Act): an enabled InterfaceItem of a shown layout gets its item object
        if cls == class::INTERFACE_ITEM && state != 0 && !was && self.rules.h72_save_screen {
            let layout = self
                .layouts
                .iter()
                .map(|l| l.0)
                .find(|&l| self.h.list_of(class::IFACE_LAYOUT, l).contains(&target));
            if let Some(layout) = layout {
                let built = self.layouts.iter().any(|l| l.1.iter().any(|x| x.0 == target));
                if !built {
                    let tl = self.trigger_list_of(target);
                    self.out.push(Out::Reg { owner: target, tl, cause: RegCause::IfaceItem { layout } });
                    self.pending_children.push(target);
                    self.out.push(Out::Fire { owner: target, event: event::ACTIVATION });
                    let pos = self.item_container_pos(layout, target);
                    if let Some(l) = self.layouts.iter_mut().find(|l| l.0 == layout) {
                        l.1.insert(pos.min(l.1.len()), (target, None));
                    }
                }
            }
        }
        // HYPOTHESIS H122 (static; post-hoc fo-030 0xe6bd 209.63 → removal of 0x855f 209.71, Deact 0x855f 210.06, asset
        // 0x4900 210.39; fo-029 0x1199f): a Disable that changes the flag removes the built item from every shown layout
        // containing it and removes its object → Deact item, then its asset
        if cls == class::INTERFACE_ITEM && state == 0 && was && self.rules.h122_iitem_disable {
            let mut removed = Vec::new();
            // H1500: the Disable finds only an item that has run its Activation (its cmd 11 lists it)
            let not_active = self.rules.h1500_iitem_disable_needs_active && self.pending_children.contains(&target);
            for l in self.layouts.iter_mut().filter(|_| !not_active) {
                if let Some(i) = l.1.iter().position(|x| x.0 == target) {
                    removed.push(l.1.remove(i));
                }
            }
            for (item, asset) in removed {
                self.pending_children.retain(|&o| o != item);
                self.out.push(Out::Fire { owner: item, event: event::DEACTIVATION });
                if let Some(asset) = asset {
                    // H123: the shared asset object stays while another shown item holds it
                    let held = self.rules.h123_shared_asset_object
                        && asset != 0
                        && self.layouts.iter().any(|l| l.1.iter().any(|x| x.1 == Some(asset)));
                    if !held {
                        self.out.push(Out::Fire { owner: asset, event: event::DEACTIVATION });
                    }
                }
            }
        }
        // HYPOTHESIS H203–H206 (classes 0x47–0x4a): only on a flag change
        let icon3 = was != (state != 0) && self.icon_type3(action, cls, target, state != 0);
        // HYPOTHESIS H61 (post-hoc fo-007/fo-008): Timer trigger 0x7a3f's 0x7a41/0x7a42/0x7a43 disable the icons 0x46fc/
        // 0x4700/0x6fde (VCEnabled 0x102/0x103/0x2d8); each is followed at once by Deact(icon), Deact(asset 1),
        // Deact(asset 2)
        let h204 = cls == class::EMOTION_ICON && self.rules.h204_emotion_icon_type3;
        if cls == class::EMOTION_ICON && state == 0 && self.rules.h61_icon_disable && !icon3 && !h204 {
            if let Some(i) = self.icons_active.iter().position(|x| x.0 == target) {
                let (icon, assets) = self.icons_active.remove(i);
                self.out.push(Out::Fire { owner: icon, event: event::DEACTIVATION });
                for a in assets {
                    if !self.icon_asset_held(a, icon) {
                        self.out.push(Out::Fire { owner: a, event: event::DEACTIVATION });
                    }
                }
            }
        }
        if cls != class::NAV {
            return;
        }
        if state == 0 && (self.rules.h76_entry_hotspot_order || self.h89_on()) {
            // H76: a built nav-hotspot (also one of a View being left, fo-021 276.80 Deact 0xd242 inside 0x159e9) is
            // removed and deactivated at once
            if let Some(i) = self.hotspots.iter().position(|&n| n == target) {
                self.hotspots.remove(i);
                self.out.push(Out::HotspotRemoved { vcnav: target });
                self.out.push(Out::Fire { owner: target, event: event::DEACTIVATION });
            }
            return;
        }
        let Some(view) = self.view else { return };
        if !self.nav_list(view).contains(&target) {
            return;
        }
        if state != 0 {
            if !self.hotspots.contains(&target) {
                self.build_hotspot(target, HotspotSrc::Enable { action: action });
            }
        } else if let Some(i) = self.hotspots.iter().position(|&n| n == target) {
            self.hotspots.remove(i);
            self.out.push(Out::HotspotRemoved { vcnav: target });
        }
    }

    /// HYPOTHESIS H101 (static FUN_0049bbc0, post-hoc fo-025: 0x7c6b 402.85 → REG 0x7afb 402.96, Act 415.74 → REG + Act
    /// 0x7af7 / 0x7af9 / 0xd53d).
    fn enable_inventory(&mut self, action: u32, target: u32, on: bool) {
        if !on {
            if self.rules.h199_inventory_disable {
                self.inventory_disable(target);
            } else {
                self.out.push(Out::Unimplemented { action, what: format!("H101: Disable of VCInventory {target:#x} not modelled") });
            }
            return;
        }
        if target == devices::PDA && self.rules.h167_pda_enable {
            self.pda_enable(action);
            return;
        }
        if target == 0x442b {
            self.out.push(Out::Unimplemented { action, what: "H101: Enable of VCInventory 0x442b not decoded".into() });
            return;
        }
        let tl = self.trigger_list_of(target);
        self.out.push(Out::Reg { owner: target, tl, cause: RegCause::InventoryEnable { action } });
        self.pending_children.push(target);
        self.out.push(Out::Fire { owner: target, event: event::ACTIVATION });
    }

    /// HYPOTHESIS H55 (post-hoc fo-005..fo-009): an Enable turning on a member of the current View's f38 list registers it
    /// (fo-005 0xf493 → REG 0xac5d, Act at once); a Disable of an active one deactivates it at once (fo-009 0x15a10 →
    /// Deact 0x13f2e). Before the View's activation nothing (the activation picks the enabled members up).
    fn enable_explorable(&mut self, action: u32, target: u32, was: bool, on: bool) {
        // H76: while a transition's tail is held, the old View's Explorables are still active (fo-009 322.50 Disable
        // 0x15a10 → 322.71 Deact 0x13f2e, posted by the old background's Deactivation)
        let leaving = (self.rules.h76_entry_hotspot_order || self.h89_on()) && !on && self.explorables.contains(&target);
        if !self.rules.h55_explorable || (self.pending_entry.is_some() && !leaving) {
            return;
        }
        let Some(view) = self.view else { return };
        if on && !was && !self.explorables.contains(&target) && self.explorable_list(view).contains(&target) {
            self.register_explorable(target, Some(action));
            self.out.push(Out::Fire { owner: target, event: event::ACTIVATION });
        } else if !on {
            if let Some(i) = self.explorables.iter().position(|&x| x == target) {
                self.explorables.remove(i);
                self.out.push(Out::Fire { owner: target, event: event::DEACTIVATION });
            }
        }
    }

    /// View.f38 (VCExplorableList) members, list order.
    pub fn explorable_list(&self, view: u32) -> Vec<u32> {
        match self.h.read(class::VIEW, view) {
            Some(r) => self.h.list_of(class::EXPLORABLE_LIST, r.u32("f38")),
            None => Vec::new(),
        }
    }

    fn register_explorable(&mut self, x: u32, action: Option<u32>) {
        self.explorables.push(x);
        let tl = self.trigger_list_of(x);
        self.out.push(Out::Reg { owner: x, tl, cause: RegCause::Explorable { action } });
    }

    /// VCExplorables registered and active in the current View (H55).
    pub fn explorables(&self) -> &[u32] {
        &self.explorables
    }

    /// type 4: Set View (C18; blob [View][Node][Location][ViewPoint]); the current View -> no transition.
    fn exec_set_view(&mut self, a: &Action, body: &[u8]) {
        let (Some(view), vp) = (be32(body, 0), be32(body, 12)) else {
            self.out.push(Out::Unimplemented { action: a.id, what: "type 4: short blob".into() });
            return;
        };
        if self.rules.h860_set_view_location {
            let h = self.h;
            let node = be32(body, 4).filter(|&n| h.has(class::NODE, n));
            let loc = be32(body, 8).filter(|&l| h.has(class::LOCATION, l));
            let node_ch = node.is_some() && self.node.is_some() && self.node != node;
            let loc_ch = node_ch || (loc.is_some() && self.location.is_some() && self.location != loc);
            if loc_ch && loc.is_some() {
                self.h76_release();
                self.h89_flush_all();
                let old = [self.viewpoint, self.location, self.node];
                if node_ch {
                    let n = node.unwrap();
                    let tl = self.trigger_list_of(n);
                    self.out.push(Out::Reg { owner: n, tl, cause: RegCause::TravelNode { action: a.id } });
                }
                let l = loc.unwrap();
                let tl = self.trigger_list_of(l);
                self.out.push(Out::Reg { owner: l, tl, cause: RegCause::TravelLocation { action: a.id } });
                self.travel = Some([old[0], old[1], if node_ch { old[2] } else { None }, if node_ch { node } else { None }, loc]);
                self.px.keep_players = true;
                if node_ch {
                    self.node = node;
                }
                self.location = loc;
                // H860: a changed Location rebuilds the ViewPoint even with the same objId
                self.viewpoint = None;
                let vp = vp.or_else(|| self.viewpoint_of(view));
                self.transition(view, 0, TransSrc::SetView { action: a.id }, vp, &[]);
                return;
            }
        }
        if self.view != Some(view) {
            self.transition(view, 0, TransSrc::SetView { action: a.id }, vp, &[]);
        }
    }

    /// type 6: named procedure (C25). Only procedures whose effect is trace-backed are run.
    /// H96: the PDA address list = enabled AssetRefs of InterfaceItem 0xb0fb's f44 list, in list order.
    fn pda_collect(&mut self) {
        let list = self.h.read(class::INTERFACE_ITEM, 0xb0fb).map(|r| r.u32("f44")).unwrap_or(0);
        self.pda_addrs = if list == 0 { Vec::new() } else { self.h.list_of(class::ASSET_REF_LIST, list) }
            .into_iter()
            .filter(|&x| self.is_enabled(class::ASSET_REF, x))
            .collect();
    }

    /// H96: show the current address card in InterfaceItem 0x89f8 if its object exists: same asset → nothing; else the
    /// old asset is removed (Deact) and the new one built (REG TL 0, posted). An empty list shows a text asset (owner 0,
    /// as H72 a).
    fn pda_show(&mut self) {
        const ITEM: u32 = 0x89f8;
        let new = self.pda_addrs.get(self.pda_index).copied();
        let Some(slot) = self.layouts.iter_mut().flat_map(|l| l.1.iter_mut()).find(|x| x.0 == ITEM) else { return };
        let old = slot.1;
        if new.is_some() && old == new {
            return;
        }
        slot.1 = new;
        match new {
            Some(asset) => {
                let tl = self.trigger_list_of(asset);
                self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::ObjectAsset { of: ITEM } });
                if let Some(o) = old {
                    self.out.push(Out::Fire { owner: o, event: event::DEACTIVATION });
                }
                self.out.push(Out::Fire { owner: asset, event: event::ACTIVATION });
            }
            None => {
                if let Some(o) = old {
                    self.out.push(Out::Fire { owner: o, event: event::DEACTIVATION });
                }
                self.out.push(Out::Reg { owner: 0, tl: Some(0), cause: RegCause::ObjectAsset { of: ITEM } });
                self.out.push(Out::Fire { owner: 0, event: event::ACTIVATION });
            }
        }
    }

    /// H72 / H95: load mode = the load screen 0x142ca is shown
    fn load_screen(&self) -> bool {
        self.layouts.iter().any(|l| l.0 == LOAD_SCREEN)
    }

    /// H2280: the selection bar of the shown save / load screen moved to `save_slot` (its rect: the bar's picture and
    /// hit rect follow, H179's `item_rects`): top-left at x 194, y 164 + 58 per slot, the size of its HDB region. A bar
    /// item that is not built is left alone.
    fn save_bar_follow(&mut self) {
        const LEFT: i16 = 194;
        const FIRST_ROW_TOP: i16 = 164;
        const ROW_PITCH: i16 = 58;
        if !self.rules.h2280_save_slot_bar {
            return;
        }
        let bar = if self.load_screen() { 0x14578 } else { 0x14572 };
        if !self.layouts.iter().any(|l| l.1.iter().any(|x| x.0 == bar)) {
            return;
        }
        let Some(crate::hdb::Value::Region { rect, .. }) =
            self.h.read(class::INTERFACE_ITEM, bar).and_then(|r| r.field("region").cloned())
        else {
            return;
        };
        let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
        let (x, y) = (LEFT, FIRST_ROW_TOP + ROW_PITCH * (self.save_slot % SAVE_SLOTS) as i16);
        self.px.item_rects.insert(bar, [x, y, x + w, y + h]);
    }

    /// H148: SetIsNode3Done — the RegNode3Vars result variable := "node 3 done": the current node is 3, VCVariable
    /// 0x1402 is 1, the first bound variable is 1 and the second exceeds the threshold the third selects.
    /// Needs the RegCurrNode / RegNode3Vars bindings and 0x1402; unknown inputs → unknown result.
    fn set_node3_done(&mut self) {
        const GATE: u32 = 0x1402;
        let (Some(node), Some([met, count, mode, result])) = (self.node_vars.0, self.node_vars.1) else { return };
        if !self.vars.has(GATE) {
            return;
        }
        let read = |v: u32| self.vars.get(v);
        let done = (|| {
            let [node, gate, met, count, mode] = [read(node)?, read(GATE)?, read(met)?, read(count)?, read(mode)?];
            let threshold = match mode {
                1 => Some(2),
                0 => Some(1),
                _ => None,
            };
            Some(node == 3 && gate == 1 && met == 1 && threshold.is_some_and(|t| count > t))
        })();
        self.vars.set_native(result, done.map(i32::from), 2);
    }

    /// H148: SetIsNode6Done — the RegNode6Vars result variable := the current node is 6 and its first variable is 1.
    fn set_node6_done(&mut self) {
        let (Some(node), Some([met, result])) = (self.node_vars.0, self.node_vars.2) else { return };
        let done = self.vars.get(node).zip(self.vars.get(met)).map(|(n, m)| n == 6 && m == 1);
        self.vars.set_native(result, done.map(i32::from), 2);
    }

    /// H97 / C52: PDAFlipAddr on the address card list — nothing with fewer than two cards.
    fn pda_flip(&mut self, flip: PdaFlip) {
        /// the two cards PDAFlipAddr(2) / (3) enable / disable (engine-built type-3 messages)
        const SWITCHED_CARDS: [u32; 2] = [0xa79a, 0xa79c];
        let n = self.pda_addrs.len();
        if n < 2 {
            return;
        }
        match flip {
            PdaFlip::Refresh => self.pda_collect(),
            PdaFlip::Switch(on) => {
                for card in SWITCHED_CARDS {
                    self.engine_enables.push_back((class::ASSET_REF, card, on as u8));
                    self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
                }
            }
            PdaFlip::Turn(dir) => {
                self.pda_index = (self.pda_index as isize + dir).rem_euclid(n as isize) as usize;
                self.pda_show();
            }
        }
    }

    /// H93 / H1042 / H1603: SetFLightProps(n, c1, c2) stores the flashlight's dark mode (n = 2), wide beam (c1 = 'O') and
    /// brightness class (c2: 'B' 0, 'D' 2, else 1). With the flashlight value (8) current and an object held, H736 selects
    /// the held item again (off: logged).
    fn set_flashlight_props(&mut self, action: u32, args: &[u32]) {
        const FLASHLIGHT_VALUE: i32 = 8;
        let arg = |i: usize| args.get(i).copied().unwrap_or(0);
        let brightness = match char::from_u32(arg(2) & 0xff) {
            Some('B') => 0,
            Some('D') => 2,
            _ => 1,
        };
        self.dev.flight = [u8::from(arg(0) == 2), u8::from(arg(1) == 'O' as u32), brightness];
        self.dev.flight_known = true;
        if self.curr_inv_var.and_then(|v| self.vars.get(v)) != Some(FLASHLIGHT_VALUE) {
            return;
        }
        if !self.rules.h736_flight_reselect {
            self.out.push(Out::Unimplemented { action, what: "SetFLightProps: current inventory = 8".into() });
        } else if self.dev.held.is_some() {
            self.dev.requests.push(devices::DevReq::ReselectHeld);
        }
    }

    fn item_text_asset(&mut self, item: u32) {
        let Some(slot) = self.layouts.iter_mut().flat_map(|l| l.1.iter_mut()).find(|x| x.0 == item) else { return };
        let old = slot.1.replace(0);
        if let Some(o) = old {
            self.out.push(Out::Fire { owner: o, event: event::DEACTIVATION });
        }
        self.out.push(Out::Reg { owner: 0, tl: Some(0), cause: RegCause::ObjectAsset { of: item } });
        self.out.push(Out::Fire { owner: 0, event: event::ACTIVATION });
    }

    fn exec_proc(&mut self, a: &Action, body: &[u8], trigger: u32) {
        let Some(p) = ProcCall::parse(body) else {
            self.out.push(Out::Unimplemented { action: a.id, what: "type 6: unparsable blob".into() });
            return;
        };
        self.ab_native_state(&p.name, &p.args); // H186 / H193 (`procs.rs`)
        // H165–H174 (`devices.rs`)
        if self.device_proc(a.id, &p.name, &p.args) {
            return;
        }
        if self.pc_proc(a.id, trigger, &p.name, &p.args) {
            return; // H570–H578 (`computer.rs`)
        }
        match (p.name.as_str(), p.args.as_slice()) {
            // fo-005/fo-006 0x13e55: DoActionList(trigger 0x12144, AL 0x12146) posted AL with that trigger
            ("DoActionList", &[trigger, al, ..]) => {
                for act in self.h.list_of(class::ACTION_LIST, al) {
                    self.post(Msg { action: act, trigger }, PostSrc::Proc { action: a.id });
                }
            }
            // fo-005/fo-006 0xcdbe: SetStartView(Node, Location, ViewPoint, View) -> trig_register of the
            // Node, the Location and the ViewPoint, then transition_new(View, kind 0)
            ("SetStartView", &[node, location, vp, view]) => {
                // H65: a pending saved start replaces the four arguments and opens the restore window
                // (type-0x14 command, value 1, posted before the activations)
                let mut load = false;
                let [node, location, vp, view] = match self.pending_start.take() {
                    Some(saved) if self.rules.h65_load => {
                        self.restoring = true;
                        load = true;
                        self.out.push(Out::Restore { on: true });
                        saved
                    }
                    _ => [node, location, vp, view],
                };
                let tl = self.trigger_list_of(node);
                self.out.push(Out::Reg { owner: node, tl, cause: RegCause::StartNode });
                let tl = self.trigger_list_of(location);
                self.out.push(Out::Reg { owner: location, tl, cause: RegCause::StartLocation });
                self.node = Some(node);
                self.location = Some(location);
                // fo-005, fo-006, fo-009: Act(Node), Act(Location) follow the View's registration, before Act(ViewPoint)
                self.transition(view, 0, TransSrc::StartView { action: a.id }, Some(vp), &[node, location]);
                if self.rules.h66_start_icons {
                    self.start_icons(a.id, load);
                }
            }
            // H125 (static FUN_005068b0; C40 fo-027 386.26..388.00, fo-028): the four objects are looked up, the current
            // View / ViewPoint / Location / Node get their removal commands (in that order), the new Node / Location /
            // ViewPoint are built and the View's transition object (kind 0); their posts go to a list the swap object
            // appends to the queue from the main loop once the queue has run empty — after it removed the media players
            // (fo-027 452.32 removal of 0x76bf's player). The variable (5th argument) is read for the disc number and
            // written back only if changed — the swap never changes it
            ("SwapDisc", &[node, location, vp, view, ..]) if self.rules.h125_swap_disc => {
                self.h76_release(); // an older step's held fires run first (as in `transition`)
                self.h89_flush_all();
                let old = [self.viewpoint, self.location, self.node];
                let tl = self.trigger_list_of(node);
                self.out.push(Out::Reg { owner: node, tl, cause: RegCause::TravelNode { action: a.id } });
                let tl = self.trigger_list_of(location);
                self.out.push(Out::Reg { owner: location, tl, cause: RegCause::TravelLocation { action: a.id } });
                self.node = Some(node);
                self.location = Some(location);
                // the current ViewPoint object is removed even if the new one has the same objId: a new one is built
                self.viewpoint = None;
                self.travel = Some([old[0], old[1], old[2], Some(node), Some(location)]);
                self.transition(view, 0, TransSrc::SwapDisc { action: a.id }, Some(vp), &[]);
            }
            // H126 (static; post-hoc fo-027 576.05 / fo-028): DisplayDBox(CharView)
            ("DisplayDBox", &[cv, ..]) if self.rules.h126_dialog_box => self.display_dbox(cv),
            // H52 (static FUN_0047d090): PDAEscape(VCVariable) — value == 1 → posts (trigger 0x1270c, AL 0x1270d), else
            // (0x12704, 0x1270a); the ids are constants in the code
            ("PDAEscape", &[var, ..]) => match self.vars.get(var) {
                Some(v) => {
                    let (trigger, al) = if v == 1 { (0x1270c, 0x1270d) } else { (0x12704, 0x1270a) };
                    for act in self.h.list_of(class::ACTION_LIST, al) {
                        self.post(Msg { action: act, trigger }, PostSrc::Proc { action: a.id });
                    }
                }
                None => self.out.push(Out::Unimplemented { action: a.id, what: format!("PDAEscape: variable {var:#x} unknown") }),
            },
            // H60 (static FUN_0049c1f0): unless an EmotionIcon object with that objId exists, build one (REG TL f40) and
            // post it
            // H72 (a) (fo-017 63.24–65.43): every slot item of the shown screen (load or save list, `exetables`) that is
            // on screen gets a new ownerless text asset (REG TL 0, posted, Act), its old asset dropped first (the traced
            // slot items have none)
            ("SaveLoadInit", _) if self.rules.h72_save_screen => {
                let load = self.load_screen();
                if self.rules.h403_save_screen_data {
                    // H403: buffer cleared, slot 0; load mode: the first named slot
                    self.save_edit = None;
                    self.save_name_unknown = false;
                    self.save_slot =
                        if load { (0..SAVE_SLOTS).find(|&i| !self.save_names[i].is_empty()).unwrap_or(0) } else { 0 };
                    if load {
                        self.save_bar_follow(); // H2280 (save mode: only the caret 0x14281 moves, not modelled)
                    }
                }
                for item in self.slot_items(load) {
                    if self.rules.h142_save_slot_text {
                        // H142: the asset is held by the item → replaced by the next text, removed by the hide
                        self.item_text_asset(item);
                    } else if self.layouts.iter().any(|l| l.1.iter().any(|x| x.0 == item)) {
                        self.out.push(Out::Reg { owner: 0, tl: Some(0), cause: RegCause::ObjectAsset { of: item } });
                        self.out.push(Out::Fire { owner: 0, event: event::ACTIVATION });
                    }
                }
            }
            // H102 (static FUN_00468f00; post-hoc fo-025 809.2: REG owner 0 809.74 / 810.01,
            // Act 810.83 / 811.31; the hide of 0x3b99 Deacts them right after 0xc68c (835.55) and 0xc67b (836.19))
            ("LoginInit", &[0x43, ..]) if self.rules.h102_login_init => {
                if self.vars.has(0x1352) {
                    for item in [0xc67b, 0xc68c] {
                        self.item_text_asset(item);
                    }
                }
            }
            ("LoginInit", &[c, ..]) if self.rules.h102_login_init && c != 0x50 => {}
            ("PreloadEmIcon", &[icon, ..]) if self.rules.h60_preload_em_icon => {
                if !self.icons.contains(&icon) && self.h.has(class::EMOTION_ICON, icon) {
                    self.icons.push(icon);
                    self.pending_children.push(icon);
                    let tl = self.trigger_list_of(icon);
                    self.out.push(Out::Reg { owner: icon, tl, cause: RegCause::EmotionIcon { action: a.id } });
                    self.out.push(Out::Fire { owner: icon, event: event::ACTIVATION });
                }
            }
            // H58 (static FUN_0041afe0): finds the layout's runtime object and marks it as View; no registration, event
            // or post follows in fo-009 (334.88, 388.20). H130: the mark moves the layout's items to the as-View hit-test
            // category (after the other layouts'); only a shown layout is found
            ("SetIFLAsView", &[layout, ..]) if self.rules.h58_iface_layout => {
                if self.rules.h130_hit_chain
                    && self.layouts.iter().any(|l| l.0 == layout)
                    && !self.layouts_as_view.contains(&layout)
                {
                    self.layouts_as_view.push(layout);
                }
            }
            // H80 (static; the procedure name table's handlers, FUN_00472c50): every Reg* handler resolves its arguments
            // as VCVariables. RegCurrInvVar(a, b): a set to b's value (type 1), a bound, that value kept as the "no item"
            // default
            ("RegCurrInvVar", &[a, b, ..]) if self.rules.h80_reg_vars => {
                if self.vars.has(a) && self.vars.has(b) {
                    let v = self.vars.get(b);
                    self.vars.set_native(a, v, 1);
                }
                self.curr_inv_var = self.vars.has(a).then_some(a);
                self.curr_inv_none = self.vars.get(a).unwrap_or(0); // the "no item" default
            }
            // H80: RegViewState(a): a set to 0 (type 0), a bound
            ("RegViewState", &[a, ..]) if self.rules.h80_reg_vars => {
                self.vars.set_native(a, Some(0), 0);
                self.view_state_var = Some(a);
            }
            // H80: pure bindings of VCVariables to native globals (refcount +1, no variable write, no event):
            // RegCurrNode (1), RegDayNight (empty handler), RegNode2Vars (3), RegNode3Vars (4), RegNode4Vars (2),
            // RegNode5Vars (3), RegNode6Vars (2), RegLink2Vars (2), RegLink3Vars (2), RegShanksNode2 (20),
            // RegShanksNode3 (18), RegShanksNode6 (5), RegUberVars (3), RegViewSpecial (1), RegCookEmail (2). The native
            // code that later reads/writes them is not modelled
            // H148: the bindings SetIsNode3Done / SetIsNode6Done read (each binds only when every variable exists)
            // H1420: RegViewSpecial binds the variable (bind only)
            ("RegViewSpecial", &[a, ..]) if self.rules.h1420_binoc_view_swap && self.rules.h80_reg_vars => {
                self.view_special_var = self.vars.has(a).then_some(a);
            }
            ("RegCurrNode", &[a, ..]) if self.rules.h148_node_done && self.rules.h80_reg_vars => {
                if self.vars.has(a) {
                    self.node_vars.0 = Some(a);
                }
            }
            ("RegNode3Vars", &[a, b, c, d, ..]) if self.rules.h148_node_done && self.rules.h80_reg_vars => {
                if [a, b, c, d].iter().all(|&v| self.vars.has(v)) {
                    self.node_vars.1 = Some([a, b, c, d]);
                }
            }
            ("RegNode6Vars", &[a, b, ..]) if self.rules.h148_node_done && self.rules.h80_reg_vars => {
                if self.vars.has(a) && self.vars.has(b) {
                    self.node_vars.2 = Some([a, b]);
                }
            }
            ("SetIsNode3Done", _) if self.rules.h148_node_done => self.set_node3_done(),
            ("SetIsNode6Done", _) if self.rules.h148_node_done => self.set_node6_done(),
            // H146
            ("StopAllTimers", _) if self.rules.h146_stop_all_timers => {
                for (id, _) in self.timers_running.clone() {
                    self.engine_timer_stops.push_back(id);
                    self.post(Msg { action: 0, trigger: 0 }, PostSrc::Engine);
                }
            }
            // H147: random value
            ("RandGen", &[var, n, ..]) if self.rules.h147_rand_gen => {
                let v = self.live_rand_value(0, n as i32);
                self.vars.set_native(var, v, 1);
            }
            ("RandGen", &[var, ..]) if self.rules.h147_rand_gen => {
                self.vars.set_native(var, None, 1);
            }
            // H149
            ("DispEvIcons", &[n, ..]) if self.rules.h149_disp_ev_icons && self.rules.h205_evidence_icon_type3 => self.disp_ev_icons(n),
            ("DispEvIcons", _) if self.rules.h149_disp_ev_icons => {}
            ("RegCurrNode" | "RegDayNight" | "RegNode2Vars" | "RegNode3Vars" | "RegNode4Vars" | "RegNode5Vars"
            | "RegNode6Vars" | "RegLink2Vars" | "RegLink3Vars" | "RegShanksNode2" | "RegShanksNode3"
            | "RegShanksNode6" | "RegUberVars" | "RegViewSpecial" | "RegCookEmail", _)
                if self.rules.h80_reg_vars => {}
            // H81 (the preferences object, FUN_004ccda0): its state 0 (and a cleared flag) → the 24 variables' values
            // are written to the preferences (no variable change); state 5 → read back into the variables (not
            // modelled); state 0 with the flag set → another path (not decoded)
            ("RegPreferences", _) if self.rules.h81_reg_preferences => {}
            // H82 (FUN_0050a570): if the held inventory item is the argument (or the argument is 0): only when a
            // held-item object exists it is deactivated and removed and the selection cleared. The core holds no item
            ("DeselectInv", _) if self.rules.h82_deselect_inv => {}
            // H153 (`icons.rs`): OnInvClick(var) → RegCurrInvVar's variable set to var's value (held object not modelled)
            ("OnInvClick", &[v, ..]) if self.rules.h153_on_inv_click => self.on_inv_click(v),
            // H83 (FUN_0040be40): a fade call with the variable's value on the runtime objects of the app's kinds 4, 0xc,
            // 0xe, 0xf, 0x12, 0x1f, 0x22, 0x10, 0x1b — no registration / event / post in fo-021 (481.90). Static (H83
            // update): each posts a type-21 command; its handler adds / removes a display filter — traced as command
            // dispatches only (fo-021 482.63–483.03, 5 targets)
            ("FadeBack", _) if self.rules.h83_fade_back => {}
            // H91 (FUN_0046b6b0): the three arguments as VCVariables are bound to the shootout's three variable slots, the
            // six shootout globals zeroed, then each variable set to 0 as type 2 (the variable's high type bit kept);
            // three random permutations of 1..3 are drawn — native state, not modelled
            ("InitSmShootout", args) if self.rules.h91_init_sm_shootout => {
                for &v in args.iter().take(3) {
                    if self.vars.has(v) {
                        self.vars.set_native(v, Some(0), 2);
                    }
                }
            }
            // H92: SetLanguage(text AssetRef, string variable): the variable set to the asset's text (only if it
            // differs; strings are not modelled — 0xf9d is type 3); SendEmail (the app's e-mail list); EmailFlushMess
            // (e-mail list flush); StartVideoText(text, day var, time var, flag var): a text overlay object + a native
            // timer (fo-010 139.03); the variables are rewritten only if the overlay changed them — it only reads them,
            // so no change; RollHdCredits: a native timer (fo-010 303.91). No registration / event / post between proc6
            // and the action's removal in any trace
            ("StartVideoText", &[text, day, time, flag, ..]) if self.rules.h92_native_display_procs => {
                // H308: display record only (StartVideoText returns early when a variable argument is 0)
                if day != 0 && time != 0 && flag != 0 {
                    let v = |x: u32| self.vars.get(x).unwrap_or(0);
                    let serial = self.video_text.as_ref().map_or(0, |t| t.serial) + 1;
                    self.video_text =
                        Some(VideoText { action: a.id, asset: text, day: v(day), time: v(time), flag: v(flag), serial, at: self.clock });
                }
            }
            // H650: SetLanguage(asset, var) (FUN_0048b680): var set to the AssetRef's text, written only when it differs;
            // 0x10dec = SetLanguage(0x10dea = XfilesT.dll 1091 'english', 0xf9d)
            ("SetLanguage", &[asset, var, ..]) if self.rules.h650_string_compare => {
                match procs::computer::asset_string(self, asset) {
                    Some(t) if self.vars.set_text(var, &procs::computer::to_cp1251(&t)) => {}
                    _ => self.out.push(Out::Unimplemented {
                        action: a.id,
                        what: format!("H650: SetLanguage text of {asset:#x} / variable {var:#x} unknown"),
                    }),
                }
            }
            ("SetLanguage" | "SendEmail" | "EmailFlushMess" | "StartVideoText" | "RollHdCredits", _)
                if self.rules.h92_native_display_procs => {}
            // H93 / H736 / H1042
            ("SetFLightProps", args) if self.rules.h93_flight_props => self.set_flashlight_props(a.id, args),
            // H94: SetAIList(trigger, action list) / ClearAIList (FUN_00469160): binds the VCTrigger (class 0x51) /
            // VCActionList (0x42), then the AI icon's update: the EvidenceIcon object (built and posted only if missing)
            // gets state 0/1 by whether the list's action applies. SetAIGlow: the icon's glow + two native values.
            // fo-007/fo-010/fo-023: no lifecycle event of 0xdb79 follows
            ("SetAIList" | "ClearAIList" | "SetAIGlow", args) if self.rules.h94_ai_list => {
                if self.rules.h621_ai_icon {
                    self.ai_proc(p.name.as_str(), args); // H621 / H622 (ai.rs)
                }
            }
            // H95: SetIItemFocus: the InterfaceItem's object gets the keyboard focus; SaveSlotSelect: load mode and a
            // non-empty slot → the slot selected, marker item 0x14578 moved; SaveLoadComit: arg ≠ 0 in load mode → the
            // Load (H65). Save mode / cancel not traced
            ("SetIItemFocus", _) if self.rules.h95_save_screen_procs => {}
            // H142 (save mode; post-hoc fo-030 457.37: the three slot texts shown anew, Deact / Act pairs 458.74–460.61)
            ("SaveSlotSelect", &[slot, ..]) if self.rules.h142_save_slot_text && !self.load_screen() => {
                self.show_slot_texts();
                self.save_slot = slot as usize % SAVE_SLOTS;
                if self.rules.h403_save_screen_data {
                    self.save_edit = None;
                }
                self.save_bar_follow(); // H2280
            }
            // H403: SaveKeyPress with the key of the event (H174): navigation, Enter / Esc, editing
            ("SaveKeyPress", _) if self.rules.h403_save_screen_data && self.dev.key.is_some() => {
                let key = self.dev.key.unwrap_or(0);
                self.save_key(a.id, key);
            }
            // H142 (save mode, key unknown; post-hoc fo-030 462.37 ×10: the selected slot's text shown anew each time) —
            // the bot's keys (8 × Backspace, "30") are character keys
            ("SaveKeyPress", _) if self.rules.h142_save_slot_text && !self.load_screen() => {
                self.save_name_unknown = true;
                self.show_selected_slot_text();
            }
            // H143 (static; post-hoc fo-030 496.48–496.72: the four engine-built hides posted)
            ("SaveLoadComit", &[arg, ..]) if self.rules.h143_save_comit && (arg == 0 || !self.load_screen()) => {
                self.save_comit(a.id, arg);
            }
            // H95 / H403: load mode — SaveSlotSelect on a named slot selects it; SaveLoadComit(1) loads it
            ("SaveSlotSelect", &[slot, ..])
                if self.rules.h403_save_screen_data && self.load_screen() => {
                let slot = slot as usize % SAVE_SLOTS;
                if !self.save_names[slot].is_empty() {
                    self.save_slot = slot;
                    self.save_bar_follow(); // H2280
                }
            }
            ("SaveLoadComit", &[1, ..]) if self.rules.h403_save_screen_data && self.load_screen() => {
                self.save_comit(a.id, 1);
            }
            ("SaveSlotSelect", _) | ("SaveLoadComit", &[1, ..])
                if self.rules.h95_save_screen_procs && self.layouts.iter().any(|l| l.0 == 0x142ca) => {}
            // H96 (FUN_004ad130): the address list = the enabled AssetRefs of InterfaceItem 0xb0fb's AssetRefList, and
            // its count; non-empty → the current entry shown in item 0x89f8, else a text asset (as H72 a). fo-023 568.56:
            // old asset removed, REG 0xb09e TL 0, posted; Deact 0x89f6 569.30, Act 0xb09e 569.78
            ("PDAInitAddr", _) if self.rules.h96_pda_init_addr => {
                self.pda_collect();
                self.pda_show();
            }
            // H97 / C52 (fo-023 603.79, fo-039)
            ("PDAFlipAddr", &[mode, ..]) if self.rules.h97_pda_flip_addr => self.pda_flip(PdaFlip::of(mode)),
            // H175–H198 (`procs.rs`)
            _ if self.ab_proc(a.id, trigger, &p.name, &p.args) => {}
            _ => self.out.push(Out::Unimplemented { action: a.id, what: format!("type 6: {}{:x?}", p.name, p.args) }),
        }
    }

    // ---- navigation (C1–C3, C6, C15, C16, C23) ------------------------------------------------

    /// Click on a nav-hotspot (C1–C3): dest = the VCNav's destView, kind from its HotSpot (C6).
    pub fn click_nav(&mut self, vcnav: u32) {
        let Some(r) = self.h.read(class::NAV, vcnav) else {
            self.out.push(Out::Unimplemented { action: 0, what: format!("nav click: VCNav {vcnav:#x} unknown") });
            return;
        };
        let dest = r.u32("destView");
        if self.rules.h75_nav_same_view && self.view == Some(dest) {
            return; // H75: same View, no transition object
        }
        let kind = self.nav_kind(vcnav);
        let vp = self.viewpoint_of(dest);
        self.transition(dest, kind, TransSrc::Nav { vcnav }, vp, &[]);
        self.h89_pump(); // H89
    }

    /// transition_new(dest, kind) and the lifecycle of C23. `vp` = the ViewPoint of `dest` if known; `pre_act` =
    /// owners activated after the View's registration and before the ViewPoint (SetStartView: Node, Location).
    fn transition(&mut self, dest: u32, kind: u32, src: TransSrc, vp: Option<u32>, pre_act: &[u32]) {
        self.h76_release(); // H76: a new step runs whatever an older one still holds first
        self.h89_flush_all(); // H89: the same fallback
        let vp_changes = vp.is_some() && vp != self.viewpoint;
        if vp_changes {
            let vp = vp.unwrap();
            let tl = self.trigger_list_of(vp);
            self.out.push(Out::Reg { owner: vp, tl, cause: RegCause::ViewPoint });
        }
        self.out.push(Out::Transition { view: dest, kind, src });
        let tl = self.trigger_list_of(dest);
        self.out.push(Out::Reg { owner: dest, tl, cause: RegCause::TransitionView });
        // C23: Deactivation(old View) ... Deactivation of old VCNavs and old ViewPoint, Activation(new
        // ViewPoint), Activation(new View)
        for &o in pre_act {
            self.out.push(Out::Fire { owner: o, event: event::ACTIVATION });
        }
        // H53 background of the old View: deactivated right after the View (fo-005 0x115bc, fo-009 0x97c0, 0x9840, …)
        let old_bg = self.background.take();
        let vp_new = if vp_changes { vp } else { None };
        if self.h89_on() {
            // H89: the old View's Deactivation (+ its background's, A1) is one command, the rest of the exit another
            let deacts: Vec<(u32, u8)> =
                self.view.into_iter().chain(old_bg).map(|o| (o, event::DEACTIVATION)).collect();
            self.view = Some(dest);
            self.pending_entry = Some(dest);
            if !deacts.is_empty() {
                self.post_cmd(WCmd::Fires(deacts));
            }
            self.post_cmd(WCmd::ExitTail { dest, vp: vp_new });
            return;
        }
        if let Some(old) = self.view {
            self.out.push(Out::Fire { owner: old, event: event::DEACTIVATION });
        }
        if let Some(bg) = old_bg {
            self.out.push(Out::Fire { owner: bg, event: event::DEACTIVATION });
        }
        self.view = Some(dest);
        self.pending_entry = Some(dest);
        if self.h76_hold_exit(dest, vp_new, old_bg) {
            return;
        }
        self.exit_tail(dest, vp_new);
    }

    /// C23 after the old View's (and background's) Deactivation: old nav-hotspots, old Explorables, ViewPoint change,
    /// Activation of the new View.
    fn exit_tail(&mut self, dest: u32, vp: Option<u32>) {
        for n in std::mem::take(&mut self.hotspots) {
            self.fire_cmd(n, event::DEACTIVATION);
        }
        // H88 (static, the View's exit FUN_00438770): removal of the app lists kind 5, 6, 0x14, 0x1d in that
        // order → CharViews before Explorables; without H88 the earlier assumption (Explorables first) is kept
        if self.rules.h88_exit_remove_order {
            self.exit_char_views();
        }
        // H55: the old View's active Explorables after its nav-hotspots (fo-005 137.85 0x15620, 0x15621; fo-009 327.19)
        for x in std::mem::take(&mut self.explorables) {
            self.fire_cmd(x, event::DEACTIVATION);
        }
        // H74: the old View's CharViews after its nav-hotspots (fo-020 239.93 after 0x31c / 0x31e 239.39 / 239.65)
        self.exit_char_views();
        let travel = self.travel.take();
        if let Some(vp) = vp {
            let old_vp = match travel {
                Some(t) => t[0],
                None => self.viewpoint,
            };
            if let Some(old) = old_vp {
                self.fire_cmd(old, event::DEACTIVATION);
            }
            if let Some([_, old_loc, old_node, node, loc]) = travel {
                self.travel_tail(old_loc, old_node, node, loc);
            }
            self.fire_cmd(vp, event::ACTIVATION);
            self.viewpoint = Some(vp);
        }
        self.fire_cmd(dest, event::ACTIVATION);
    }

    /// HYPOTHESIS H125 — the rest of a SwapDisc travel after the old ViewPoint's Deactivation (fo-027: Deact 0x2b2 390.67,
    /// Location 0x12f 390.96 → its 26 posts run, Node 0x12d 451.62, player 0x76bf 452.96 (every media player removed
    /// once the queue is empty), then the swap object's list: Act Node 0x6af 455.01, Location 0x2a21 455.39 → its
    /// 46 posts run, ViewPoint 0x2a23 493.21 (then the View, after the Location's video, C28: 500.73).
    fn travel_tail(&mut self, old_loc: Option<u32>, old_node: Option<u32>, node: Option<u32>, loc: Option<u32>) {
        for o in [old_loc, old_node].into_iter().flatten() {
            self.fire_cmd(o, event::DEACTIVATION);
        }
        // H183: a CharTeleport travel keeps the players (no player removal)
        let keep = std::mem::take(&mut self.px.keep_players);
        let mut players: Vec<u32> = if keep { Vec::new() } else { self.looping_players.drain().collect() };
        players.sort_unstable();
        for p in players {
            self.fire_cmd(p, event::DEACTIVATION);
        }
        // H1060: the played (non-looping) players still alive go too
        if !keep && self.rules.h1060_travel_stops_played {
            self.out.push(Out::TravelStopPlayers);
        }
        for o in [node, loc].into_iter().flatten() {
            self.fire_cmd(o, event::ACTIVATION);
        }
    }

    /// H74: deactivate (and forget) the old View's CharViews.
    fn exit_char_views(&mut self) {
        self.cv_click.clear();
        for c in std::mem::take(&mut self.char_views) {
            self.pending_children.retain(|&o| o != c);
            self.cv_wrapper_regs.retain(|&o| o != c); // runner mode: nothing consumes the wrapper's registration
            self.fire_cmd(c, event::DEACTIVATION);
        }
    }

    /// H74: build CharView `c` of `view` (TL f54) and post its activation.
    fn build_char_view(&mut self, c: u32, view: u32) {
        self.char_views.push(c);
        self.pending_children.push(c);
        let tl = self.trigger_list_of(c);
        self.out.push(Out::Reg { owner: c, tl, cause: RegCause::CharView { view } });
        self.fire_cmd(c, event::ACTIVATION);
    }

    /// HYPOTHESIS H87 (static FUN_00492170; no traced case): a changed CharView flag. Enable: the current View's f30 list;
    /// the member with that objId is built and posted. Disable: the kind-6 runtime object with that id gets a state-0
    /// command if active and is removed — as a built nav-hotspot (kind 5, H76 (c)), its Deactivation is modelled at once
    fn enable_char_view(&mut self, target: u32, was: bool, on: bool) {
        if !self.rules.h74_char_view || !self.rules.h87_char_view_enable || was == on {
            return;
        }
        if on {
            // before the View's activation (pending entry) the activation itself builds the enabled members
            let Some(view) = self.view.filter(|_| self.pending_entry.is_none()) else { return };
            let list = self.h.read(class::VIEW, view).map(|r| r.u32("f30")).unwrap_or(0);
            if self.h.list_of(class::CHAR_VIEW_LIST, list).contains(&target) && !self.char_views.contains(&target) {
                self.build_char_view(target, view);
            }
        } else if let Some(i) = self.char_views.iter().position(|&c| c == target) {
            self.char_views.remove(i);
            self.pending_children.retain(|&o| o != target);
            self.cv_wrapper_regs.retain(|&o| o != target);
            self.out.push(Out::Fire { owner: target, event: event::DEACTIVATION });
        }
    }

    /// The new View was activated: build nav-hotspots of its enabled VCNavs (C15, C23).
    fn view_activated(&mut self, view: u32) {
        self.pending_entry = None;
        // H74: the View activation: after the View's Activation, every enabled (C15, class 0x2d) member of View.f30
        // (VCCharViewList 0x2e) is built (TL f54) and posted — before the nav-hotspots (f34) and the Explorables (f38)
        // of the same function
        // (fo-020 186.41: 0x55e9 of [0x55e9, 0x14ec0], 0x14ec0 disabled in the save; Act 190.53 after the View's posts)
        if self.rules.h74_char_view {
            let list = self.h.read(class::VIEW, view).map(|r| r.u32("f30")).unwrap_or(0);
            let cvs: Vec<u32> = self
                .h
                .list_of(class::CHAR_VIEW_LIST, list)
                .into_iter()
                .filter(|&c| self.h.has(class::CHAR_VIEW, c) && self.is_enabled(class::CHAR_VIEW, c))
                .collect();
            // H601: the View's teleport target CharView gets the teleport-click flag
            let target = self.teleport_cv.take().filter(|t| t.0 == view).map(|t| t.1);
            for c in cvs {
                if self.rules.h601_teleport_click && target == Some(c) {
                    self.cv_click.push(c);
                }
                self.build_char_view(c, view);
            }
        }
        let mark = self.out.len();
        for n in self.nav_list(view) {
            if self.is_enabled(class::NAV, n) && !self.hotspots.contains(&n) {
                self.build_hotspot(n, HotspotSrc::ViewEntry { view });
            }
        }
        // H55: enabled members of View.f38 after the nav-hotspots' registrations, before the background (fo-005 138.66,
        // fo-009 303.97, 328.73..329.04); their Activations follow the hotspots' (fo-005 139.58)
        if self.rules.h55_explorable {
            let xs: Vec<u32> =
                self.explorable_list(view).into_iter().filter(|&x| self.is_enabled(class::EXPLORABLE, x)).collect();
            for &x in &xs {
                self.register_explorable(x, None);
            }
            for &x in &xs {
                self.fire_cmd(x, event::ACTIVATION);
            }
        }
        if self.h89_on() {
            return; // H89: the background was built by View cmd 11 (`WCmd::ViewBuild`), queued ahead
        }
        // C23 step 8: the background (View.f2c) after the nav-hotspots
        let bg = self.background;
        self.choose_background();
        self.h76_hold_entry(mark, bg);
    }

    /// HYPOTHESIS H53: (re)choose the current View's background — first enabled AssetRef of View.f2c.
    fn choose_background(&mut self) {
        if !self.rules.h53_background || self.pending_entry.is_some() {
            return; // before C23 step 6 (View activation) the new View has no background yet
        }
        let Some(view) = self.view else { return };
        let list = self.h.read(class::VIEW, view).map(|r| r.u32("f2c")).unwrap_or(0);
        let pick = self.h.list_of(class::ASSET_REF_LIST, list).into_iter().find(|&x| self.is_enabled(class::ASSET_REF, x));
        if pick.is_some() && pick != self.background {
            // fo-006/fo-009: the previous background is deactivated first (Deact 0x432 → REG 0x6453 → Act 0x6453)
            if let Some(old) = self.background {
                self.out.push(Out::Fire { owner: old, event: event::DEACTIVATION });
            }
            let asset = pick.unwrap();
            let tl = self.trigger_list_of(asset);
            self.out.push(Out::Reg { owner: asset, tl, cause: RegCause::Background { view } });
            self.fire_cmd(asset, event::ACTIVATION); // H89: its cmd 0, posted behind its cmd 11
        } else if pick.is_none() && self.rules.h1220_bg_none_deact {
            // H1220 (fo-168 183.799: Disable of the playing background 0x5268 with no other member of View 0x1b33's f2c
            // list enabled → removed → its Deactivation at once, before the trigger's next action)
            if let Some(old) = self.background {
                self.out.push(Out::Fire { owner: old, event: event::DEACTIVATION });
            }
        }
        self.background = pick;
    }

    /// nav-hotspot: registered with TL 0 (C20) and activated (C23).
    fn build_hotspot(&mut self, vcnav: u32, src: HotspotSrc) {
        self.hotspots.push(vcnav);
        let entry = matches!(src, HotspotSrc::ViewEntry { .. });
        self.out.push(Out::Hotspot { vcnav, src });
        let tl = self.trigger_list_of(vcnav);
        self.out.push(Out::Reg { owner: vcnav, tl, cause: RegCause::NavHotspot });
        if entry {
            self.fire_cmd(vcnav, event::ACTIVATION); // H89: posted by View cmd 0
        } else {
            self.out.push(Out::Fire { owner: vcnav, event: event::ACTIVATION });
        }
    }
}
