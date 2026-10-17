//! Scheduler block matrix (C26, fo-008; static layout H47; addresses in RE_NOTES).
//!
//! The scheduler does not dispatch a command of type T to a target of kind K while the counter of cell (T, K) is not
//! zero (a row-wide change hits every column of the row). The counters start at zero and, on every traced path, only
//! a media player's state change moves them.
//!
//! Not modelled (no trace): the other counter writers RE_NOTES H47 lists (0 hits on the traced paths). Which media
//! are video and whether a media has the extra hold are inputs here ([`media_inputs_h51`]).

/// Number of target kinds per row.
pub const KINDS: usize = 0x27;
/// Row-wide pseudo kind: +1 / −1 on every column of the row.
pub const ROW: u32 = 0x29;
/// Number of command types (rows).
pub const TYPES: usize = 23;

/// Target kinds (C26).
pub mod kind {
    pub const TRANSITION: u32 = 3;
    /// a View's runtime object (C28: a transition's View Activation is a type-0 command on it)
    pub const VIEW: u32 = 4;
    pub const K4: u32 = VIEW;
    pub const NAV: u32 = 5;
    pub const ACTION_MSG: u32 = 0x17;
    pub const MEDIA_PLAYER: u32 = 0x1b;
}

/// Command types (C24).
pub mod cmd {
    pub const RUN: u32 = 0;
    pub const C10: u32 = 10;
    pub const C11: u32 = 11;
}

/// One counter change: +1 (`inc`) or −1 on a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellOp {
    pub typ: u32,
    pub kind: u32,
    pub inc: bool,
}

/// C26 / H51: the cells a video's player holds while it plays, in the order the traces record them (fo-007..fo-009
/// `blk_inc` / `blk_dec`, 48 / 48 ops, `sched_replay.rs`): first the extra-hold cells (only for a media with the
/// AssetRef's 0x40 flag; never on the FO path), then the cells every video holds.
const EXTRA_HOLD_CELLS: [(u32, u32); 2] = [(cmd::RUN, kind::MEDIA_PLAYER), (cmd::C11, kind::MEDIA_PLAYER)];
const VIDEO_CELLS: [(u32, u32); 4] =
    [(cmd::C10, kind::MEDIA_PLAYER), (cmd::RUN, kind::VIEW), (cmd::C11, kind::VIEW), (cmd::C10, kind::VIEW)];

#[derive(Clone, Debug)]
pub struct Blocks {
    m: [[i32; KINDS]; TYPES],
    /// the media state last applied by [`Blocks::media`]
    media_state: u32,
}

impl Default for Blocks {
    fn default() -> Self {
        Blocks { m: [[0; KINDS]; TYPES], media_state: 0 }
    }
}

impl Blocks {
    /// Every counter back to zero (startup).
    pub fn reset(&mut self) {
        self.m = [[0; KINDS]; TYPES];
    }

    /// One counter change; kind [`ROW`] changes the whole row.
    pub fn apply(&mut self, op: CellOp) {
        let d = if op.inc { 1 } else { -1 };
        let row = &mut self.m[op.typ as usize];
        if op.kind == ROW {
            row.iter_mut().for_each(|c| *c += d);
        } else {
            row[op.kind as usize] += d;
        }
    }

    pub fn get(&self, typ: u32, kind: u32) -> i32 {
        self.m[typ as usize][kind as usize]
    }

    /// A command (typ, kind) must wait.
    pub fn blocked(&self, typ: u32, kind: u32) -> bool {
        let (t, k) = (typ as usize, kind as usize);
        t < TYPES && k < KINDS && self.m[t][k] != 0
    }

    /// Any counter negative (never seen in the original).
    pub fn negative(&self) -> bool {
        self.m.iter().flatten().any(|&c| c < 0)
    }

    /// The player is already in `state` (a switch to it changes nothing).
    pub fn media_state_is(&self, state: u32) -> bool {
        self.media_state == state
    }

    pub fn is_zero(&self) -> bool {
        self.m.iter().flatten().all(|&c| c == 0)
    }

    /// A media player's state change (C26): a video entering a non-zero state raises its cells, entering state 0
    /// lowers them; the same state again or a non-video media changes nothing. `video` / `extra_hold` = the media's
    /// inputs ([`media_inputs_h51`]). Returns the counter changes in the traced order and applies them.
    pub fn media(&mut self, state: u32, video: bool, extra_hold: bool) -> Vec<CellOp> {
        if self.media_state == state || !video {
            return Vec::new();
        }
        let inc = state != 0;
        let extra: &[(u32, u32)] = if extra_hold { &EXTRA_HOLD_CELLS } else { &[] };
        let ops: Vec<CellOp> = extra.iter().chain(&VIDEO_CELLS).map(|&(typ, kind)| CellOp { typ, kind, inc }).collect();
        for &op in &ops {
            self.apply(op);
        }
        self.media_state = if inc { state } else { 0 };
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_raise_lower_balances() {
        let mut b = Blocks::default();
        let up = b.media(1, true, false);
        assert_eq!(up.len(), 4);
        assert!(b.blocked(cmd::RUN, kind::VIEW) && b.blocked(cmd::C10, kind::MEDIA_PLAYER));
        assert!(!b.blocked(cmd::RUN, kind::ACTION_MSG) && !b.blocked(cmd::RUN, kind::MEDIA_PLAYER));
        assert!(b.media(1, true, false).is_empty()); // same state: no change
        assert_eq!(b.media(0, true, false).len(), 4);
        assert!(b.is_zero() && !b.negative());
        assert!(b.media(1, false, false).is_empty()); // non-video media: nothing
        // the extra hold comes first and also blocks the player's own run commands
        let up = b.media(1, true, true);
        assert_eq!(up.len(), 6);
        assert_eq!((up[0].typ, up[0].kind), (cmd::RUN, kind::MEDIA_PLAYER));
        assert!(b.blocked(cmd::RUN, kind::MEDIA_PLAYER));
        b.media(0, true, true);
        assert!(b.is_zero());
    }
}

/// HYPOTHESIS H51 (static + post-hoc on fo-008, 5/5; confirmed by fo-009, C27): the media-block inputs of
/// [`Blocks::media`] from the AssetRef record: `video` = the file kind of the names in the `asset` blob (by extension,
/// the exe's table `Hdb::exe.ext_kinds`) is a video kind (0x16 NMV / 0x17 MOV-XMV-MOOV); None if the names disagree or
/// none is found; `extra_hold` = the AssetRef's `f40` & 0x40.
pub fn media_inputs_h51(h: &crate::hdb::Hdb, asset_ref: u32) -> Option<(bool, bool)> {
    let r = h.read(crate::world::class::ASSET_REF, asset_ref)?;
    let blob = h.blob_bytes(r.blob("asset")?)?;
    let mut kinds = blob.split(|&c| c < 0x20 || c == 0x7f).filter_map(|tok| {
        let s = std::str::from_utf8(tok).ok()?;
        let ext = s.rsplit_once('.')?.1.to_ascii_uppercase();
        h.exe.ext_kinds.iter().find(|(e, _)| *e == ext).map(|&(_, k)| k)
    });
    let k = kinds.next()?;
    let vk = |k: u32| k == 0x16 || k == 0x17;
    let mut mixed_video = false;
    for x in kinds {
        if x != k {
            // H1560 (C270, fo-220 57.825: the nav movie 0x7109 = '005-36s.mov' + '8nav7.nmv' raises the cells (0,4)
            // (11,4) (10,4) at its player's state 1): a MOV / NMV pair is a video kind either way
            if vk(k) && vk(x) {
                mixed_video = true;
                continue;
            }
            return None;
        }
    }
    Some((mixed_video || vk(k), r.u32("f40") & 0x40 != 0))
}
