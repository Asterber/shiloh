//! X-Files Game port core.
//!
//! - [`hdb`]: structural reader for XFiles.hdb / XFiles.gam (NeoAccess 5.x, big-endian); field order and
//!   widths determined from the original's readers (RE_NOTES §8).
//! - [`world`]: headless engine core (triggers/events, queue, action interpreter, transitions) on
//!   trace-confirmed rules; hypotheses are switchable ([`world::Rules`]).
pub mod e2e;
pub mod exetables;
pub mod front;
pub mod gpu;
pub mod hdb;
pub mod io;
pub mod iso;
pub mod media;
pub mod neodb;
pub mod notices;
pub mod pe;
pub mod replay;
pub mod save;
pub mod sched_replay;
pub mod vfs;
pub mod world;
