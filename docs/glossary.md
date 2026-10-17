# Glossary

Words used in the code, in the project's research notes (not in this repository) and in commit messages. Most
engine terms are class names of the VirtualCinema engine; in the data they usually carry a `VC` prefix (`VCView`,
`VCAssetRef`), which the code and the notes often drop. Class ids are in `port/src/world/mod.rs` (`mod class`).
Objects are named by their object id in hex: "View 0x2b3", "AssetRef 0xfba5".

## Game files

| Term | Meaning |
|---|---|
| **HDB** | `XFiles.hdb`, the static game database: every View, Trigger, Action, AssetRef, text reference. NeoAccess 5.x format, big-endian. Read by `port/src/hdb.rs`. Only the HDB in `InstalledGame/` is the one the game loads. |
| **GAM** | `XFiles.gam`, the initial mutable state in the same format: variables, VCEnabled flags. At game start the original copies it to `Temp.gam` and adds the game-state classes. |
| **save, `.x`** | A save of the original: a byte copy of `Temp.gam`. Same format again, so `hdb.rs` reads it. `slots.txt` names the slots. See [architecture.md](architecture.md#5-saves). |
| **XV/** | Movies (`.xmv`, QuickTime): Views, dialogue lines, cut scenes. A movie may come with a `.HOT` hotspot file of the same name. |
| **XS/** | Sound: music (`.mus`), ambient and effect sounds (`.amv`), some `.DMV`. QuickTime containers with a sound track. |
| **XG/** | Graphics: `.pff` picture containers (icons, interface pictures) and some movies. |
| **XN/** | Further movie files, opened by name like XV ones. |
| **XT/** | Texts (`.xtx`): documents, notes, e-mails, help shown in close-ups. |
| **NMV** | `navm.nmv`, `NAV1.NMV`..`NAV7.NMV`: banks of navigation movies (turns, walks). |
| **PFF** | Picture container: `"PFF "`, a count, then QuickDraw PICT pictures. `x.pff`, `X7.PFF`, `XG/*.pff`. Read by `media/pict.rs`. |
| **XFilesT.dll** etc. | Resource DLLs: string tables (`T`), cursors, and the subliminal-message word banks (`S`). Read by `pe.rs`. |
| **QDMC** | QDesign Music, a sound codec the port does not decode; `xf-prep` converts those tracks once with your ffmpeg. |

## World structure

| Term | Meaning |
|---|---|
| **Node** | A large story section (roughly a day or a place group: "Node 6" = day 4). Travelling between Nodes is a SwapDisc action. |
| **Location** | A place inside a Node (an office, a warehouse). |
| **ViewPoint** | A standing position inside a Location; it has one or more Views. |
| **View** | One screen the player looks at: a background (still or movie), nav-hotspots, Explorables, CharViews. A View change is a "transition". |
| **VCNav, nav-hotspot** | A clickable region of a View that leads to another View (its target is the VCNav's destination field). At run time each VCNav gets a nav-hotspot object. |
| **Explorable** | A clickable object in a View that is not navigation (a drawer, a door, a piece of evidence). Listed per View. |
| **CharView** | A character in a View. Clicking it opens the dialogue box with that character's Conversations. |
| **Conversation** | One dialogue line or exchange the player can pick, with the AssetRefs that play for it. Grouped in a VCConversationList. VCConversationHistory holds the ones asked. |
| **dialogue box** | The talk panel at the bottom with its rows, TALK / HISTORY tabs and icons (`world/dbox.rs`). |
| **EmotionIcon** | The icons in the dialogue box that pick a tone of answer. |
| **IdeaIcon** | An icon in the dialogue box for an idea or piece of evidence the player can raise; its VCIdeaMap names the Conversation that plays. |
| **ActionIcon** | An icon the engine places in the interface for an action, e.g. a hint such as "go to the wheelhouse". |
| **AI icon** | The "artificial intuition" icon at the top right (an EvidenceIcon). It lights up or spins near something important and gives a hint on click. Exists only while the Artificial Intuition preference is on. |
| **Inventory** | VCInventory objects: items the player carries, shown in the strip at the bottom. "Held" = picked up on the cursor. |
| **layout, VCIFaceLayout** | An interface screen drawn over or instead of the View: the PDA, the travel map, a drawer close-up, the menu. Shown and hidden by type-5 actions. A layout "set as View" replaces the View for clicks. |
| **IItem, VCInterfaceItem** | One element of a layout: a picture, a button, a text field. Has its own TriggerList and AssetRefs. |
| **PDA** | The player's handheld: map, notes, address book. |

## Logic

| Term | Meaning |
|---|---|
| **owner** | Any object that can receive events (View, VCNav, Explorable, IItem, icon, AssetRef ...). |
| **TriggerList, Trigger** | An owner's list of Triggers. A Trigger = one event code + an ActionList. When an owner is activated its TriggerList is "registered"; then events on it post its Triggers' actions. |
| **event** | Mouse Click (0), Double Click (1), Mouse Move (2), Key Press (3), Roll Into (4), Roll Out Of (5), Drop (6), Timer (7), Activation (8), Deactivation (9). `world::event`. |
| **Activation / Deactivation** | The lifecycle events of an object: shown / registered, and removed. Many story effects hang on a View's or AssetRef's Activation. |
| **ActionList, VCAction** | A list of actions. An action has a type byte (1 play, 2 timer, 3 enable / disable, 4 set View, 5 show / hide layout, 6 procedure call, ...), an optional condition prefix and operands (`world/action.rs`). |
| **post** | Putting an action on the command queue. "Posts" in a trace = action ids the original queued. |
| **procedure** | A type-6 action calls a native function of the engine by name (`SwapDisc`, `RandGen`, `EnablePDANote`, ...). Implemented in `world/procs.rs`. |
| **VCVariable** | A game variable (bool, int, enum, string), id in hex, name like `iUberParanoia` or `Pref_bArtificialIntuition`. `Pref_*` variables mirror the player's preferences. |
| **VCEnabled** | A record in the GAM holding the enabled flag of another object (a VCNav, an AssetRef, an IItem ...). Disabled objects are skipped: a disabled VCNav gives no hotspot, a disabled AssetRef is not played. Type-3 actions flip them. |
| **AssetRef** | A reference to a piece of media: which file (movie, sound, picture, text, string resource), which part (start / end), and how to play it. |
| **AssetRefList** | A list of AssetRefs; "play the list" usually plays the first enabled one. A View's background is chosen this way. |
| **Timer** | A type-2 action: `[ms][id][mode]`, one-shot or periodic. On expiry the Timer event fires on every owner subscribed to that id. |
| **Uber tracks** | Three story counters (`iUberParanoia`, `iUberXTrack`, `iUberLoss`). They steer some content, e.g. which word bank the subliminal messages use. |
| **command queue** | The original's single queue of commands (action messages, lifecycle fires, clicks). Posts made while a command runs go before older pending commands. Modelled by `Runner`. |
| **media block, scheduler** | While a video plays, some commands (a View's Activation) wait. `world/sched.rs`. |
| **restore window** | During a Load, Activations fire no triggers until the restore has run (H65). |
| **Rules** | `world::Rules`: one boolean per behaviour that is a hypothesis or newly confirmed. See [architecture.md](architecture.md#6-rules-flags). |
| **Session / Runner / World** | The front, the closed-loop driver and the engine core. See [architecture.md](architecture.md). |

## Research ids

Code comments cite these ids from the project's research notes (not part of this repository).

| Id | Meaning |
|---|---|
| **C123** | Confirmed fact. Always names the trace that shows it (`fo-NNN`, often with a time or step). Only a trace of the original running can make something confirmed. |
| **H1234** | Hypothesis: from a static reading of the code or data, from the port's behaviour, or from an observation that was not a controlled trace. A hypothesis that a later trace checked gets a bracketed note: `[CONFIRMED (C299, fo-250)]` or `[REFUTED ...]`. |
| **R12, REFUTED** | A refuted hypothesis or an H entry marked **REFUTED** in place. Refuted entries are never deleted; they say why and give the source. **FLIP** = refuted and later reversed. |
| **E5** | An environment fact about running the original (CrossOver, the tracer, input). |
| **Q7** | An open question. Answered ones say `[ANSWERED: ...]`. |
| **A1** | An approximation in the Runner, observed but not derived from code (`world/run.rs` header). |

Checks run on the original also get a short label in the notes (two letters and a digit, e.g. "HB3"); the label is
only a name for the planned run, the result is the C / H entry and the trace.

### Traces

`fo-NNN` names one traced run of the original, numbered in order (the first runs traced the Field Office; the prefix
stayed). The traces are part of the research notes and are not in this repository.

A `Rules` field's name gives its id (`h2083_ai_icon_pref` → H2083, `c296_...` → C296), and its doc comment names the
trace.
