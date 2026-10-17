//! Save games: the `.x` file the original writes (a byte copy of Temp.gam, NeoAccess — RE_NOTES E9 / H65) and the
//! port's writer for it.
//!
//! HYPOTHESIS H400 (static file comparison of the original's saves, not traced): between two saves
//! of one session the original changes records **in place** (fixed-size VCEnabled flags, VCVariable values, the
//! VCGameState scalars; `cmp` of fo-028-end.x / fo-029-end.x: 14 bytes, all inside records 0x46 / 0x53 / 0x57) and stores
//! variable-length blobs (VCGameState lists, the data of 0x56 / 0x58 / 0x5a..0x5c) either in a free chunk taken from the
//! NeoAccess free list (header +0x04 → node: CNeoPersist header, u16 count, count × [u32 mark][u32 length]) or appended at
//! the end of the file (fo-026-end.x: l64 at 156664 = the old end, la0 right after it; the header does not hold the file
//! length). The port's writer [`write`] therefore patches a **base** file the original wrote: every understood field is
//! overwritten from a [`SaveState`]; a blob whose new bytes fit its old place is written there (length updated), a longer
//! one is appended at the end (the old place leaks; the free list is left untouched, so it stays consistent), an empty
//! one becomes mark 0 / length 0 (what the original's empty lists hold). Not reproduced: the original's allocator
//! choices (they depend on the whole session's write history) — the output is byte-identical to the base only when the
//! state is the base's own ([`SaveState::from_gam`]; test `round_trip_every_save`).
//!
//! HYPOTHESIS H401: a game started from XFiles.gam (no VCGameState / 0x56..0x5c classes: the original adds 6 classes and
//! their index trees to Temp.gam at game start, FUN_0048ecd0) is saved over a **template**: a save of the original
//! ([`find_template`]); every VCEnabled flag and VCVariable of the session overwrites the template's, so only the
//! fields this module does not understand come from the template (VCGameState f4c..f60, b190..b196, f198, b19c, f1a0,
//! mode; the VCString records). Without any template the port cannot write a save (reported, not guessed).
//!
//! HYPOTHESIS H470 (static comparison of XFiles.gam with Temp.gam and every save, NeoAccess
//! container logic partly from Zeffuro's MIT `database.h`, see `crate::neodb` and `port/LICENSE-THIRD-PARTY`):
//! [`build`] makes the save of a game started from XFiles.gam **without a template** — it adds the game start's six
//! classes the way the original lays them out, then runs the same writer ([`write_with`]). The front uses it when
//! `Rules::h470_save_builder` is on (default); H401's template remains the fallback with the rule off. A session whose
//! gam is already a save keeps AU's in-place path ([`write`]).
use crate::hdb::{Hdb, Kind};
use crate::neodb::Db;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub const ENABLED: u32 = 0x46;
pub const VARIABLE: u32 = 0x53;
pub const GAME_STATE: u32 = 0x57;
pub const STRING: u32 = 0x50;
/// VCConversationHistory, VCPDANotes, VCEmail, VCEmailRead, VCEmailPending (named by VCGameState f38..f48)
pub const DATA_CLASSES: [(u32, &str); 5] = [(0x56, "f38"), (0x58, "f3c"), (0x5a, "f40"), (0x5b, "f44"), (0x5c, "f48")];

/// What a save holds, as far as decoded (field meanings: H65 / H66 / H67 / H126 / H178 / H186 / H262).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveState {
    /// VCEnabled flags: (target class, target objId) → flag (C15)
    pub enabled: Vec<((u32, u32), u8)>,
    /// VCVariable: (objId, value, type & 0x7f) (H46; the type's 0x80 bit is kept from the base)
    pub vars: Vec<(u32, i32, u8)>,
    /// VCGameState +0x28..+0x34: Node, Location, ViewPoint, View (H65)
    pub start: [u32; 4],
    /// la0: VCInventory objects (H66)
    pub inventories: Vec<u32>,
    /// l64: media players [AssetRef][flag][position] (H67)
    pub players: Vec<[u32; 3]>,
    /// ldc: EvidenceIcons [id][active] (H66)
    pub evidence: Vec<[u32; 2]>,
    /// l118: evidence-kit icons (H66)
    pub kit: Vec<u32>,
    /// l154: running timers [id][remaining ms][periodic] (H67)
    pub timers: Vec<[u32; 3]>,
    /// f1a4: the CharView whose dialogue box was shown (H126)
    pub char_view: u32,
    /// data of 0x56 / 0x58 / 0x5a / 0x5b / 0x5c: AssetRef / Conversation ids, little-endian (H178 / H186 / H262)
    pub conv_history: Vec<u32>,
    pub notes: Vec<u32>,
    pub inbox: Vec<u32>,
    pub read: Vec<u32>,
    pub pending: Vec<u32>,
    /// the VCGameState fields not decoded (kept from the base; [`GsExtra::default`] for a new game, H476)
    pub extra: GsExtra,
    /// H475: the text of every string variable (type & 0x7f == 3): (VCVariable objId, VCString text with its NUL).
    /// The variable's value is the VCString's objId — an allocation detail the writer chooses ([`write`]).
    pub strings: Vec<(u32, Vec<u8>)>,
    /// H982: the VCPhoto records (class 0x59), objId order (the objIds are an allocation detail the writer chooses)
    pub photos: Vec<Photo>,
    /// C190 (fo-163): header +0x10 of the file the state was read from — the objId counter. The original keeps the
    /// loaded file's counter (ids used and gone stay used): fo-163 loaded a port save with counter 0x14d4 (photo
    /// 0x14d3) and saved 0x14d4. The writer never writes a lower counter (0 = none).
    pub next_id: u32,
}

/// H982 (the first saves of the original with a photo, `data/bot/fo-127-end.x` .. `fo-134-end.x`, after C108's sedan
/// photo; static ctor FUN_0061c620): a VCPhoto record is 32 bytes — header (6, `ce ce 00 00 00 01` in the original's
/// file), the data blob (mark, length: a QuickDraw PICT v2 80×60 thumbnail, 2804 bytes), f64 the picture AssetRef
/// (0x1372e), f68 (0x269, not decoded), rect f6c (0,0,300,240 = TakePhoto's capture rect), f94 (2), f95 (downloaded,
/// 0). Kept byte for byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Photo {
    /// the 32-byte record with its blob reference (bytes 6..14) zeroed
    pub rec: [u8; 32],
    /// the data blob (thumbnail)
    pub thumb: Vec<u8>,
}

/// H982: the fixed parts of the one VCPhoto record the original wrote (fo-127-end.x): header, f68, rect, f94.
const PHOTO_TEMPLATE: [u8; 32] =
    [0xce, 0xce, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02, 0x69, 0, 0, 0, 0, 0x01, 0x2c, 0, 0xf0, 2, 0];

impl Photo {
    pub const CLASS: u32 = 0x59;
    /// f64: the picture AssetRef
    pub fn picture(&self) -> u32 {
        u32::from_be_bytes(self.rec[14..18].try_into().unwrap())
    }
    /// f95: downloaded to the album (H793)
    pub fn downloaded(&self) -> bool {
        self.rec[31] != 0
    }
    pub fn set_downloaded(&mut self, on: bool) {
        self.rec[31] = on as u8;
    }
    /// f68: TakePhoto's picture index (0x269) or a camera shot's video time in 1/600 s (C306)
    pub fn f68(&self) -> u32 {
        u32::from_be_bytes(self.rec[18..22].try_into().unwrap())
    }
    /// rect: the crop of the picture / video frame (C306; TakePhoto's 0,0,300,240)
    pub fn rect(&self) -> [i16; 4] {
        [0, 1, 2, 3].map(|k| i16::from_be_bytes([self.rec[22 + 2 * k], self.rec[23 + 2 * k]]))
    }
    /// C306: a camera shot's f68 / rect (the thumbnail is the caller's)
    pub fn set_frame(&mut self, f68: u32, rect: [i16; 4]) {
        self.rec[18..22].copy_from_slice(&f68.to_be_bytes());
        for (k, v) in rect.iter().enumerate() {
            self.rec[22 + 2 * k..24 + 2 * k].copy_from_slice(&v.to_be_bytes());
        }
    }
    /// HYPOTHESIS H982: a photo the port took (TakePhoto) — the original's record with this picture, f95 = `downloaded`,
    /// no thumbnail yet: C130 (fo-154) — the original freezes on a VCPhoto without its thumbnail blob, so the writer
    /// never writes it so (`Session::save_state` puts the picture's 80×60 thumbnail in, [`write`] a black one as the
    /// last resort, H1101)
    pub fn new(picture: u32, downloaded: bool) -> Photo {
        let mut rec = PHOTO_TEMPLATE;
        rec[14..18].copy_from_slice(&picture.to_be_bytes());
        rec[31] = downloaded as u8;
        Photo { rec, thumb: Vec::new() }
    }

    /// H1101: the thumbnail blob of a photo of `pic` — `pic` scaled to 80×60 (H793's nearest pixel; TakePhoto's
    /// thumbnail size, C124) as a DirectBitsRect PICT v2 ([`crate::media::pict::encode`]); None = black
    pub fn thumbnail(pic: Option<&crate::media::pict::Picture>) -> Vec<u8> {
        use crate::media::pict;
        let black = pict::Picture { width: 80, height: 60, rgb: vec![0; 80 * 60 * 3], alpha: None };
        pict::encode(&pic.map_or(black, |p| pict::scale(p, 80, 60)))
    }
}

/// The VCGameState fields no reader decodes (RE_NOTES H65 / H401): +0x4c..+0x60, +0x190..+0x196, +0x198, +0x19c,
/// +0x1a0, +0x1a8, mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GsExtra {
    pub f4c: [u32; 6],
    pub b190: [u8; 7],
    pub f198: u32,
    pub b19c: u8,
    pub f1a0: u32,
    pub f1a8: u32,
    pub mode: u16,
}

/// HYPOTHESIS H476 (static, the 48 files of the original in data/): a new game's VCGameState holds the values every save
/// shares — f4c 1 (2 in later saves), f50..f5c 0 (f54 / f58 vary), f60 0xffffffff, b190..b196 [0, 0, 1, 0, 0, 0, 0],
/// mode 1 — and 0 in the fields that vary between saves (f198, b19c, f1a0, f1a8). No trace of the game start exists.
impl Default for GsExtra {
    fn default() -> GsExtra {
        GsExtra {
            f4c: [1, 0, 0, 0, 0, 0xffff_ffff],
            b190: [0, 0, 1, 0, 0, 0, 0],
            f198: 0,
            b19c: 0,
            f1a0: 0,
            f1a8: 0,
            mode: 1,
        }
    }
}

const GS_U32: [&str; 6] = ["f4c", "f50", "f54", "f58", "f5c", "f60"];
const GS_U8: [&str; 7] = ["b190", "b191", "b192", "b193", "b194", "b195", "b196"];

impl GsExtra {
    fn read(g: &Hdb, gs: u32) -> Option<GsExtra> {
        let o = |n: &str| offset(g, GAME_STATE, gs, n).map(|x| x.1);
        let mut e = GsExtra::default();
        for (i, n) in GS_U32.iter().enumerate() {
            e.f4c[i] = g.be32(o(n)?);
        }
        for (i, n) in GS_U8.iter().enumerate() {
            e.b190[i] = g.byte(o(n)?);
        }
        e.f198 = g.be32(o("f198")?);
        e.b19c = g.byte(o("b19c")?);
        e.f1a0 = g.be32(o("f1a0")?);
        e.f1a8 = g.be32(o("f1a8")?);
        e.mode = g.be16(o("mode")?);
        Some(e)
    }
}

fn le_list(b: &[u8], size: usize) -> Vec<Vec<u32>> {
    b.chunks_exact(size)
        .map(|c| c.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect())
        .collect()
}

fn le_bytes<const N: usize>(v: &[[u32; N]]) -> Vec<u8> {
    v.iter().flat_map(|r| r.iter().flat_map(|x| x.to_le_bytes())).collect()
}

fn le_ids(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn blob_of(g: &Hdb, cls: u32, id: u32, field: &str) -> Vec<u8> {
    g.read(cls, id)
        .and_then(|r| r.blob(field))
        .filter(|b| b.len > 0)
        .and_then(|b| g.blob_bytes(b))
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

/// The VCGameState record's objId (one per save).
pub fn game_state_id(g: &Hdb) -> Option<u32> {
    g.class(GAME_STATE)?.objects.first().map(|&(id, _)| id)
}

impl SaveState {
    /// The state a gam holds (every decoded field); for XFiles.gam the VCGameState parts are empty.
    pub fn from_gam(g: &Hdb) -> SaveState {
        let mut st = SaveState::default();
        st.next_id = if g.d.len() >= 0x14 { g.be32(0x10) } else { 0 };
        st.strings = strings_of(g).into_iter().map(|(v, (_, t))| (v, t)).collect();
        if let Some(c) = g.class(ENABLED) {
            for &(id, _) in &c.objects {
                if let Some(r) = g.read(ENABLED, id) {
                    st.enabled.push(((r.u32("f2c"), r.u32("f28")), r.u32("f2d") as u8));
                }
            }
        }
        if let Some(c) = g.class(VARIABLE) {
            for &(id, _) in &c.objects {
                if let Some(r) = g.read(VARIABLE, id) {
                    st.vars.push((id, r.u32("f38") as i32, (r.u32("f41") & 0x7f) as u8));
                }
            }
        }
        if let Some(c) = g.class(Photo::CLASS) {
            for &(_, mark) in &c.objects {
                if mark as usize + 32 > g.d.len() {
                    continue;
                }
                let mut rec: [u8; 32] = g.d[mark as usize..mark as usize + 32].try_into().unwrap();
                let (m, l) = (g.be32(mark + 6), g.be32(mark + 10));
                let thumb = g.d.get(m as usize..(m + l) as usize).filter(|_| l > 0).map(<[u8]>::to_vec).unwrap_or_default();
                rec[6..14].fill(0);
                st.photos.push(Photo { rec, thumb });
            }
        }
        let Some(gs) = game_state_id(g) else { return st };
        let Some(r) = g.read(GAME_STATE, gs) else { return st };
        st.start = [r.u32("node"), r.u32("location"), r.u32("viewpoint"), r.u32("view")];
        let list = |f: &str, size: usize| le_list(&blob_of(g, GAME_STATE, gs, f), size);
        st.inventories = list("la0", 4).into_iter().map(|v| v[0]).collect();
        st.players = list("l64", 12).into_iter().map(|v| [v[0], v[1], v[2]]).collect();
        st.evidence = list("ldc", 8).into_iter().map(|v| [v[0], v[1]]).collect();
        st.kit = list("l118", 4).into_iter().map(|v| v[0]).collect();
        st.timers = list("l154", 12).into_iter().map(|v| [v[0], v[1], v[2]]).collect();
        st.char_view = r.u32("f1a4");
        st.extra = GsExtra::read(g, gs).unwrap_or_default();
        let data = |cls: u32, f: &str| -> Vec<u32> {
            le_list(&blob_of(g, cls, r.u32(f), "data"), 4).into_iter().map(|v| v[0]).collect()
        };
        st.conv_history = data(0x56, "f38");
        st.notes = data(0x58, "f3c");
        st.inbox = data(0x5a, "f40");
        st.read = data(0x5b, "f44");
        st.pending = data(0x5c, "f48");
        st
    }
}

/// H475: string variables of `g`: VCVariable objId → (VCString objId, text); text empty when the VCString is missing
/// or its blob is 0 / 0.
fn strings_of(g: &Hdb) -> std::collections::BTreeMap<u32, (u32, Vec<u8>)> {
    let mut m = std::collections::BTreeMap::new();
    if let Some(c) = g.class(VARIABLE) {
        for &(id, _) in &c.objects {
            if let Some(r) = g.read(VARIABLE, id) {
                if r.u32("f41") & 0x7f == 3 {
                    let s = r.u32("f38");
                    m.insert(id, (s, blob_of(g, STRING, s, "text")));
                }
            }
        }
    }
    m
}

/// What [`write`] could not store (and what [`build`] added).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteReport {
    /// enabled flags of objects without a VCEnabled record in the base (the original would add a record)
    pub missing_enabled: Vec<(u32, u32)>,
    /// variables without a VCVariable record in the base
    pub missing_vars: Vec<u32>,
    /// blobs appended at the end of the file (did not fit their old place)
    pub appended: Vec<String>,
    /// string variables whose text changed: (variable, new VCString objId) (H475)
    pub strings: Vec<(u32, u32)>,
    /// string variables of the state that are not string variables of the base
    pub missing_strings: Vec<u32>,
    /// VCEnabled records added by [`build`] (H477): (target class, target objId)
    pub added_enabled: Vec<(u32, u32)>,
}

/// How [`write_with`] stores what does not fit in place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WriteMode {
    /// appends start at a multiple of 8 (every file of the original is 8-aligned, H470); AU's default path keeps the
    /// unaligned append (pre-registered check AU1 predicts its exact file size)
    pub align: bool,
    /// a VCEnabled flag without a record gets a new record + rebuilt 0x46 index (H477) instead of a report
    pub add_missing: bool,
}

struct Patch {
    db: Db,
    mode: WriteMode,
    rep: WriteReport,
}

impl Patch {
    fn put32(&mut self, o: u32, v: u32) {
        self.db.put32(o, v);
    }
    fn get32(&self, o: u32) -> u32 {
        self.db.be32(o)
    }
    fn append(&mut self, n: usize) -> u32 {
        if self.mode.align {
            self.db.alloc(n)
        } else {
            let end = self.db.d.len();
            self.db.d.resize(end + n, 0);
            end as u32
        }
    }
    /// A blob field at `o` (mark, length) gets `bytes` (H400 placement; `free` = try the free list before appending,
    /// H474).
    fn blob(&mut self, o: u32, bytes: &[u8], what: &str, free: bool) {
        let (mark, len) = (self.get32(o), self.get32(o + 4));
        if bytes.is_empty() {
            self.put32(o, 0);
            self.put32(o + 4, 0);
            return;
        }
        let n = bytes.len() as u32;
        let at = if mark != 0 && n <= len && (mark + len) as usize <= self.db.d.len() {
            mark
        } else if let Some(m) = free.then(|| self.db.free_take(bytes.len())).flatten() {
            m
        } else {
            self.rep.appended.push(what.to_string());
            self.append(bytes.len())
        };
        self.db.put(at, bytes);
        self.put32(o, at);
        self.put32(o + 4, n);
    }
}

fn offset(g: &Hdb, cls: u32, id: u32, name: &str) -> Option<(Kind, u32)> {
    g.field_offsets(cls, id)?.into_iter().find(|f| f.0 == name).map(|f| (f.1, f.2))
}

/// Write `st` over `base` (a save written by the original, H400 / H401): the new file's bytes and what could not be
/// stored. Err if the base has no VCGameState (XFiles.gam: use [`build`]).
pub fn write(base: &Hdb, st: &SaveState) -> Result<(Vec<u8>, WriteReport), String> {
    write_with(base, st, WriteMode::default())
}

/// [`write`] with a [`WriteMode`].
pub fn write_with(base: &Hdb, st: &SaveState, mode: WriteMode) -> Result<(Vec<u8>, WriteReport), String> {
    let gs = game_state_id(base).ok_or("the base gam has no VCGameState (0x57): use save::build (H470)")?;
    let mut p = Patch { db: Db::new(base.d.clone()), mode, rep: WriteReport::default() };
    // VCEnabled flags (f2d) by target
    let mut flag_at: HashMap<(u32, u32), u32> = HashMap::new();
    if let Some(c) = base.class(ENABLED) {
        for &(id, _) in &c.objects {
            if let (Some(r), Some((_, o))) = (base.read(ENABLED, id), offset(base, ENABLED, id, "f2d")) {
                flag_at.insert((r.u32("f2c"), r.u32("f28")), o);
            }
        }
    }
    let mut add: Vec<((u32, u32), u8)> = Vec::new();
    for &(key, flag) in &st.enabled {
        match flag_at.get(&key) {
            Some(&o) => p.db.d[o as usize] = flag,
            None if mode.add_missing && key.0 <= 0xff => add.push((key, flag)),
            None => p.rep.missing_enabled.push(key),
        }
    }
    // VCVariable value (f38) and type (f41 & 0x7f, the 0x80 bit kept)
    for &(id, value, typ) in &st.vars {
        match (offset(base, VARIABLE, id, "f38"), offset(base, VARIABLE, id, "f41")) {
            (Some((_, v)), Some((_, t))) => {
                p.put32(v, value as u32);
                p.db.d[t as usize] = (p.db.d[t as usize] & 0x80) | (typ & 0x7f);
            }
            _ => p.rep.missing_vars.push(id),
        }
    }
    // VCGameState
    let f = |name: &str| offset(base, GAME_STATE, gs, name).map(|x| x.1).ok_or(format!("VCGameState field {name}"));
    for (name, v) in ["node", "location", "viewpoint", "view"].iter().zip(st.start) {
        p.put32(f(name)?, v);
    }
    p.put32(f("f1a4")?, st.char_view);
    for (n, v) in GS_U32.iter().zip(st.extra.f4c) {
        p.put32(f(n)?, v);
    }
    for (n, v) in GS_U8.iter().zip(st.extra.b190) {
        p.db.d[f(n)? as usize] = v;
    }
    p.put32(f("f198")?, st.extra.f198);
    p.db.d[f("b19c")? as usize] = st.extra.b19c;
    p.put32(f("f1a0")?, st.extra.f1a0);
    p.put32(f("f1a8")?, st.extra.f1a8);
    p.db.put16(f("mode")?, st.extra.mode);
    let inv: Vec<[u32; 1]> = st.inventories.iter().map(|&x| [x]).collect();
    let kit: Vec<[u32; 1]> = st.kit.iter().map(|&x| [x]).collect();
    p.blob(f("l64")?, &le_bytes(&st.players), "VCGameState l64", false);
    p.blob(f("la0")?, &le_bytes(&inv), "VCGameState la0", false);
    p.blob(f("ldc")?, &le_bytes(&st.evidence), "VCGameState ldc", false);
    p.blob(f("l118")?, &le_bytes(&kit), "VCGameState l118", false);
    p.blob(f("l154")?, &le_bytes(&st.timers), "VCGameState l154", false);
    // the data records named by the VCGameState
    let r = base.read(GAME_STATE, gs).ok_or("VCGameState unreadable")?;
    for ((cls, field), ids) in DATA_CLASSES.iter().zip([&st.conv_history, &st.notes, &st.inbox, &st.read, &st.pending])
    {
        let id = r.u32(field);
        match offset(base, *cls, id, "data") {
            Some((_, o)) => p.blob(o, &le_ids(ids), &format!("{cls:#x} data"), false),
            None if ids.is_empty() => {}
            None => return Err(format!("class {cls:#x} record {id:#x} missing in the base")),
        }
    }
    write_strings(&mut p, base, st)?;
    if SaveState::from_gam(base).photos != st.photos {
        write_photos(&mut p, &st.photos)?;
    }
    if !add.is_empty() {
        add_enabled(&mut p, &add)?;
    }
    // C190: the counter the state's session reached (ids used and gone stay used)
    if st.next_id > p.db.next_id() {
        p.db.put32(0x10, st.next_id);
    }
    Ok((p.db.d, p.rep))
}

/// HYPOTHESIS H475 (static: XFiles.gam / Temp.gam / the 47 saves): a string variable's value is the objId of a VCString
/// (0x50) holding its text. When the text changes the original gives the VCString a **new objId** (header +0x10) in
/// the **same record** (same mark; the 0x50 index leaf rewritten in place, H473) and the variable the new id:
/// ` scGameLanguage` 0xf9d: VCString 0xf9c '' → 0x14d1 'english' (every save), IFL_scPhoneNumber 0xa3e: 0xa3d '' →
/// 0x14d8 (a 7-digit number; 4 saves, ids 0x14d2..0x14d7 used and gone). The text goes to the old blob when it fits, else a
/// free-list chunk (H474), else the end. A text equal to the base's keeps the base's VCString.
fn write_strings(p: &mut Patch, base: &Hdb, st: &SaveState) -> Result<(), String> {
    let old = strings_of(base);
    let mut recs: Option<std::collections::BTreeMap<u32, u32>> = None;
    for (var, text) in &st.strings {
        let Some((sid, otext)) = old.get(var) else {
            p.rep.missing_strings.push(*var);
            continue;
        };
        let (_, vo) = offset(base, VARIABLE, *var, "f38").ok_or("VCVariable f38")?;
        if otext == text {
            p.put32(vo, *sid);
            continue;
        }
        let recs = match &mut recs {
            Some(r) => r,
            None => recs.insert(p.db.records(STRING)?),
        };
        let new = p.db.take_id();
        let mark = match recs.remove(sid) {
            Some(m) => m,
            None => {
                // no record to reuse: a new one (H475 extension, not seen in the original's files)
                let m = p.db.alloc(14);
                p.db.put(m, &[0xc0, 0, 0, 0, 0, 1]);
                m
            }
        };
        recs.insert(new, mark);
        p.blob(mark + 6, text, &format!("VCString {new:#x} text"), true);
        p.put32(vo, new);
        p.rep.strings.push((*var, new));
    }
    if let Some(recs) = recs {
        let n = p.db.entry(STRING).map_or(0, |e| e.count) as usize;
        if n == recs.len() {
            p.db.rewrite_index(STRING, &recs)?;
        } else {
            p.db.rebuild_index(STRING, &recs)?;
        }
    }
    Ok(())
}

/// H982: the VCPhoto class holds exactly `photos`: the base's records are reused in objId order (their marks), new
/// ones get the next objId and a 32-byte record (fo-127-end.x: the photo is objId 0x14d2 = header +0x10 at the time), the
/// class is added when the base has none (after the game-start classes: the original's class list ends in 0x59).
fn write_photos(p: &mut Patch, photos: &[Photo]) -> Result<(), String> {
    let exists = p.db.entry(Photo::CLASS).is_some();
    let old: Vec<(u32, u32)> = if exists { p.db.records(Photo::CLASS)?.into_iter().collect() } else { Vec::new() };
    let mut recs = BTreeMap::new();
    for (i, ph) in photos.iter().enumerate() {
        let (id, mark) = match old.get(i) {
            Some(&(id, m)) => {
                let blob = (p.get32(m + 6), p.get32(m + 10));
                p.db.put(m, &ph.rec);
                p.put32(m + 6, blob.0);
                p.put32(m + 10, blob.1);
                (id, m)
            }
            None if !exists && i == 0 => {
                let id = p.db.take_id();
                (id, p.db.add_class(Photo::CLASS, id, &ph.rec)?)
            }
            None => {
                let id = p.db.take_id();
                let m = p.db.alloc(32);
                p.db.put(m, &ph.rec);
                (id, m)
            }
        };
        // C130 (fo-154): a VCPhoto without its thumbnail blob freezes the original (PhotoEnlarge) — never written so
        let black;
        let thumb = if ph.thumb.is_empty() {
            black = Photo::thumbnail(None);
            &black
        } else {
            &ph.thumb
        };
        p.blob(mark + 6, thumb, &format!("VCPhoto {id:#x} thumbnail"), true);
        recs.insert(id, mark);
    }
    if !exists && recs.len() <= 1 {
        return Ok(());
    }
    if recs.is_empty() {
        // GUESS (no file of the original shows it): the last photo deleted → the class stays with an empty root leaf
        p.db.empty_index(Photo::CLASS)?;
        return Ok(());
    }
    if exists && recs.len() == old.len() {
        p.db.rewrite_index(Photo::CLASS, &recs)?;
    } else {
        p.db.rebuild_index(Photo::CLASS, &recs)?;
    }
    Ok(())
}

/// HYPOTHESIS H477 (no file of the original shows it): a VCEnabled flag of an object without a record gets a new
/// record (12 bytes: header, f28 target objId, f2d flag, f2c target class) with the next objId, appended, and the 0x46
/// index rebuilt ([`Db::rebuild_index`], Zeffuro's algorithm).
fn add_enabled(p: &mut Patch, add: &[((u32, u32), u8)]) -> Result<(), String> {
    let mut recs = p.db.records(ENABLED)?;
    for &((cls, obj), flag) in add {
        let id = p.db.take_id();
        let m = p.db.alloc(12);
        p.db.put(m, &[0xc0, 0, 0, 0, 0, 1]);
        p.db.put32(m + 6, obj);
        p.db.d[m as usize + 10] = flag;
        p.db.d[m as usize + 11] = cls as u8;
        recs.insert(id, m);
        p.rep.added_enabled.push((cls, obj));
    }
    p.db.rebuild_index(ENABLED, &recs)?;
    Ok(())
}

/// The data classes in the order the original adds them at game start (Temp.gam's layout, H470).
const START_CLASSES: [u32; 5] = [0x56, 0x58, 0x5a, 0x5b, 0x5c];
/// ` scGameLanguage` (H475 / H92 SetLanguage)
pub const VAR_GAME_LANGUAGE: u32 = 0xf9d;

/// HYPOTHESIS H470 (static: XFiles.gam vs Temp.gam and the 47 saves, every one with the same structure; FUN_0048ecd0
/// named by H401, not traced): a save **built from XFiles.gam** without any save of the original — the game start's
/// classes added as the original lays them out (VCGameState 0x57 objId 0x14cb, then 0x56 / 0x58 / 0x5a / 0x5b / 0x5c
/// = 0x14cc..0x14d0, each a 264-byte root leaf + its record appended 8-aligned; the class list grows to 9 entries with
/// an interior root, H472; header +0x0c 4 → 10, +0x10 0x14cb → 0x14d1), then `st` written over it with
/// [`write_with`] (aligned appends, missing VCEnabled records added). A new game's empty ` scGameLanguage` becomes
/// 'english' (every save of the original holds it; H92 SetLanguage sets it at game start — strings are not modelled
/// in the core). The result is checked with [`Db::validate`] (Zeffuro's container checks).
pub fn build(gam: &Hdb, st: &SaveState) -> Result<(Vec<u8>, WriteReport), String> {
    if game_state_id(gam).is_some() {
        return Err("build: the gam already holds a VCGameState (use save::write)".into());
    }
    let mut db = Db::new(gam.d.clone());
    let gs = db.take_id();
    let data: Vec<u32> = START_CLASSES.iter().map(|_| db.take_id()).collect();
    let mut rec = vec![0u8; 132];
    rec[..6].copy_from_slice(&[0xc0, 0, 0, 0, 0, 1]);
    for (i, id) in data.iter().enumerate() {
        rec[22 + 4 * i..26 + 4 * i].copy_from_slice(&id.to_be_bytes());
    }
    db.add_class(GAME_STATE, gs, &rec)?;
    for (&cls, &id) in START_CLASSES.iter().zip(&data) {
        let mut r = vec![0u8; 14];
        r[..6].copy_from_slice(&[0xc0, 0, 0, 0, 0, 1]);
        db.add_class(cls, id, &r)?;
    }
    let base = Hdb::from_bytes(db.d);
    let mut st = st.clone();
    if let Some(t) = st.strings.iter_mut().find(|s| s.0 == VAR_GAME_LANGUAGE).filter(|s| s.1.is_empty()) {
        t.1 = b"english\0".to_vec();
    }
    let (bytes, rep) = write_with(&base, &st, WriteMode { align: true, add_missing: true })?;
    let out = Db::new(bytes);
    out.validate().map_err(|e| format!("built save fails the container checks: {e}"))?;
    Ok((out.d, rep))
}

/// A gam that can serve as the base of a save (has a VCGameState, H401).
pub fn is_save(path: &Path) -> bool {
    Hdb::open(path).is_ok_and(|g| game_state_id(&g).is_some())
}

/// H401: the newest save of the original in `dirs` (`*.x`, then Temp.gam) — the template for a game started from
/// XFiles.gam.
pub fn find_template(dirs: &[&Path]) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ext_x = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("x"));
            if !(ext_x || name.eq_ignore_ascii_case("Temp.gam")) || !is_save(&p) {
                continue;
            }
            let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().map_or(true, |b| t > b.0) {
                best = Some((t, p));
            }
        }
    }
    best.map(|b| b.1)
}

/// HYPOTHESIS H402 (static, the original's save screen FUN_00515dd0, not traced): the save screen has 3 slots whose
/// names live in the preferences (16 bytes per slot); a slot whose `<name>.x` file is missing shows empty (the
/// screen clears it); a save writes `<name>.x` next to XFiles.exe. The port keeps the names in
/// `<dir>/slots.txt` (one line per slot) and the files in `<dir>/<name>.x`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slots {
    pub dir: PathBuf,
    pub names: [String; 3],
}

impl Slots {
    pub fn open(dir: &Path) -> Slots {
        let mut s = Slots { dir: dir.to_path_buf(), names: Default::default() };
        if let Some(t) = crate::io::read(&dir.join("slots.txt")).ok().and_then(|b| String::from_utf8(b).ok()) {
            for (i, l) in t.lines().take(3).enumerate() {
                s.names[i] = l.trim().to_string();
            }
        }
        // FUN_00515dd0: a name without its file is cleared
        for n in &mut s.names {
            if !n.is_empty() && !crate::io::is_file(&s.dir.join(format!("{n}.x"))) {
                n.clear();
            }
        }
        s
    }

    pub fn path(&self, slot: usize) -> Option<PathBuf> {
        let n = self.names.get(slot)?;
        (!n.is_empty()).then(|| self.dir.join(format!("{n}.x")))
    }

    /// Store `bytes` as slot `slot` named `name`; the slot list is saved.
    pub fn store(&mut self, slot: usize, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        let name = sanitize(name);
        if name.is_empty() || slot >= 3 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty save name"));
        }
        // through xf::io: std::fs natively (temp file + rename), xf-serve's /saves/ in the web front
        let p = self.dir.join(format!("{name}.x"));
        crate::io::write(&p, bytes)?;
        // FUN_00516ae0: a typed name that another slot holds is that slot's overwrite — the other slot keeps it too;
        // the port drops the duplicate so one file has one slot
        for (i, n) in self.names.iter_mut().enumerate() {
            if i != slot && *n == name {
                n.clear();
            }
        }
        self.names[slot] = name;
        crate::io::write(&self.dir.join("slots.txt"), (self.names.join("\n") + "\n").as_bytes())?;
        Ok(p)
    }
}

/// A save name as a file name: the characters FUN_00486e60 lets one type (digits, A–Z, space, "-(),"), at most 21.
pub fn sanitize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " -(),".contains(*c))
        .map(|c| c.to_ascii_uppercase())
        .take(21)
        .collect::<String>()
        .trim()
        .to_string()
}

/// A file name the save slots use in their directory ([`Slots`]): `slots.txt`, or `<name>.x` with `name` as
/// [`sanitize`] leaves it (non-empty, ≤ 21 of `A–Z 0–9 space -(),`, no outer spaces) — never a path, never `..`.
/// xf-serve's `/saves/` accepts only these.
pub fn is_slot_file(file: &str) -> bool {
    file == "slots.txt" || file.strip_suffix(".x").is_some_and(|n| !n.is_empty() && sanitize(n) == n)
}

/// The default saves directory of the port: `$XF_SAVES`, else `~/Library/Application Support/xf-port/saves` (macOS),
/// `$XDG_DATA_HOME/xf-port/saves` / `~/.local/share/xf-port/saves`, else `./saves` — never inside the game data.
pub fn default_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("XF_SAVES") {
        return d.into();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        if let Some(h) = &home {
            return h.join("Library/Application Support/xf-port/saves");
        }
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(x).join("xf-port/saves");
    }
    match home {
        Some(h) => h.join(".local/share/xf-port/saves"),
        None => PathBuf::from("saves"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saves() -> Vec<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data");
        let mut v = Vec::new();
        for d in ["bot", "InstalledGame"] {
            if let Ok(rd) = std::fs::read_dir(root.join(d)) {
                v.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "x")));
            }
        }
        v.sort();
        v
    }

    /// H400: read(write(x)) == x byte for byte for every save of the original in data/ (the writer changes nothing it
    /// does not understand, and everything it understands is written back unchanged).
    #[test]
    fn round_trip_every_save() {
        let all = saves();
        if all.is_empty() {
            eprintln!("no saves in data/: skipped");
            return;
        }
        for p in all {
            let g = Hdb::open(&p).unwrap();
            let st = SaveState::from_gam(&g);
            let (bytes, rep) = write(&g, &st).unwrap();
            assert_eq!(rep, WriteReport::default(), "{}", p.display());
            assert!(bytes == g.d, "{}: bytes differ", p.display());
            assert_eq!(SaveState::from_gam(&Hdb::from_bytes(bytes)), st);
        }
    }

    /// A changed state is read back as written (in place and appended blobs).
    #[test]
    fn changed_state_reads_back() {
        let Some(p) = saves().into_iter().next() else { return };
        let g = Hdb::open(&p).unwrap();
        let mut st = SaveState::from_gam(&g);
        st.start = [1, 2, 3, 4];
        st.inventories = vec![0x442b, 0x442f, 0x441e, 0x4427, 0x7afb];
        st.timers = vec![[0x9d59, 1234, 0]];
        st.players = vec![];
        st.conv_history = vec![0x5697, 0x57a0];
        if let Some(v) = st.vars.first_mut() {
            v.1 = 77;
        }
        if let Some(e) = st.enabled.first_mut() {
            e.1 ^= 1;
        }
        let (bytes, rep) = write(&g, &st).unwrap();
        assert!(rep.missing_enabled.is_empty() && rep.missing_vars.is_empty());
        let back = SaveState::from_gam(&Hdb::from_bytes(bytes));
        assert_eq!(back, st);
    }

    fn xfiles_gam() -> Option<Hdb> {
        Hdb::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame/XFiles.gam")).ok()
    }

    #[test]
    fn xfiles_gam_needs_the_builder() {
        let Some(g) = xfiles_gam() else { return };
        assert!(write(&g, &SaveState::from_gam(&g)).is_err());
        assert!(build(&g, &SaveState::from_gam(&g)).is_ok());
    }

    /// (d) Zeffuro's container checks (re-implemented in `Db::validate`) hold for every file of the original:
    /// XFiles.gam, Temp.gam and every save.
    #[test]
    fn originals_pass_the_container_checks() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
        let mut all = saves();
        all.push(root.join("XFiles.gam"));
        all.push(root.join("Temp.gam"));
        for p in all.into_iter().filter(|p| p.exists()) {
            let d = std::fs::read(&p).unwrap();
            Db::new(d).validate().unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        }
    }

    /// What two states must agree on (the variable's value of a string variable is a VCString objId — compared by
    /// text in `strings`).
    fn semantic(mut st: SaveState) -> SaveState {
        st.next_id = 0; // the counter moves with every id handed out (an allocation detail; header checked apart)
        let strs: std::collections::HashSet<u32> = st.strings.iter().map(|s| s.0).collect();
        for v in &mut st.vars {
            if strs.contains(&v.0) {
                v.1 = 0;
            }
        }
        st
    }

    /// H470 (c): for every save of the original, a save built from XFiles.gam alone with that save's state reads back
    /// as the same state (every VCEnabled flag, VCVariable, VCGameState field, list, data record, string text), passes
    /// the container checks, and its game-start records sit where the original put them (same marks, same objIds,
    /// same class list).
    #[test]
    fn built_save_equals_every_save() {
        let Some(gam) = xfiles_gam() else { return };
        for p in saves() {
            let g = Hdb::open(&p).unwrap();
            let st = SaveState::from_gam(&g);
            let (bytes, rep) = build(&gam, &st).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert!(rep.missing_enabled.is_empty() && rep.missing_vars.is_empty() && rep.missing_strings.is_empty());
            assert!(rep.added_enabled.is_empty(), "{}", p.display());
            let b = Hdb::from_bytes(bytes.clone());
            assert_eq!(semantic(SaveState::from_gam(&b)), semantic(st.clone()), "{}", p.display());
            assert_eq!(bytes.len() % 8, 0);
            let (o, n) = (Db::new(g.d.clone()), Db::new(bytes));
            for cls in [GAME_STATE, 0x56, 0x58, 0x5a, 0x5b, 0x5c] {
                assert_eq!(o.records(cls).unwrap(), n.records(cls).unwrap(), "{}: class {cls:#x}", p.display());
            }
            assert_eq!(o.class_list().unwrap(), n.class_list().unwrap());
            // the class directory node byte for byte (entries in class-id order) — except the VCPhoto index root (H982:
            // the original put the photo's record / leaf in free-list chunks of its own session's history, fo-127-end.x
            // record 156680 = fo-126-end.x's free chunk (156680, 40); not reproducible from XFiles.gam)
            let r = o.be32(8) as usize;
            let mask = |db: &Db| {
                let mut v = db.d[r..r + 8 + 11 * 43].to_vec();
                if let Some(e) = db.entry(Photo::CLASS) {
                    let at = e.at as usize + 31 - r;
                    v[at..at + 4].fill(0);
                }
                v
            };
            assert_eq!(mask(&o), mask(&n), "{}: class directory", p.display());
            assert_eq!(o.entry(1).map(|e| o.be32(e.at + 19)), n.entry(1).map(|e| n.be32(e.at + 19)));
            assert_eq!(o.d[..0x0c + 4], n.d[..0x0c + 4]);
            // header +0x10: the state's counter (C190: ids used and gone stay used — the 0x14d8 phone saves, fo-163)
            assert_eq!(o.next_id(), n.next_id(), "{}", p.display());
            // the VCString objIds: 0x14d1 after the language string; the 0x14d8 phone saves differ (ids used and gone)
            if !st.strings.iter().any(|s| s.0 == 0xa3e && !s.1.is_empty()) {
                assert_eq!(o.records(STRING).unwrap(), n.records(STRING).unwrap());
            }
        }
    }

    /// H470 / H475 / H476: a new game's save from XFiles.gam: 10 classes, the language string 0x14d1 'english', next
    /// objId 0x14d2 (as every save of the original without a typed phone number), the new-game VCGameState values.
    #[test]
    fn new_game_build() {
        let Some(gam) = xfiles_gam() else { return };
        let st = SaveState::from_gam(&gam);
        assert_eq!(st.extra, GsExtra::default());
        let (bytes, rep) = build(&gam, &st).unwrap();
        assert_eq!(rep.strings, vec![(VAR_GAME_LANGUAGE, 0x14d1)]);
        let db = Db::new(bytes.clone());
        assert_eq!(db.be32(0x0c), 10);
        assert_eq!(db.next_id(), 0x14d2);
        let b = Hdb::from_bytes(bytes);
        let back = SaveState::from_gam(&b);
        assert!(back.strings.contains(&(VAR_GAME_LANGUAGE, b"english\0".to_vec())));
        assert_eq!(game_state_id(&b), Some(0x14cb));
        // the built file is a base for the in-place writer: round trip byte-identical
        let (again, rep2) = write(&b, &back).unwrap();
        assert_eq!(rep2, WriteReport::default());
        assert!(again == b.d);
    }

    /// H477: a flag of an object without a VCEnabled record gets a record (rebuilt 0x46 index, 3116 records).
    #[test]
    fn build_adds_missing_enabled() {
        let Some(gam) = xfiles_gam() else { return };
        let mut st = SaveState::from_gam(&gam);
        st.enabled.push(((0x35, 0x7fff_0001), 1));
        let (bytes, rep) = build(&gam, &st).unwrap();
        assert_eq!(rep.added_enabled, vec![(0x35, 0x7fff_0001)]);
        let b = Hdb::from_bytes(bytes);
        assert_eq!(b.class(ENABLED).unwrap().objects.len(), 3116);
        assert!(SaveState::from_gam(&b).enabled.contains(&((0x35, 0x7fff_0001), 1)));
    }

    /// H475: a changed string text gets a new VCString objId in the old record, in the in-place writer too.
    #[test]
    fn changed_string_gets_a_new_id() {
        let Some(p) = saves().into_iter().next() else { return };
        let g = Hdb::open(&p).unwrap();
        let mut st = SaveState::from_gam(&g);
        let next = g.be32(0x10);
        let s = st.strings.iter_mut().find(|s| s.0 == 0xa3e).unwrap();
        s.1 = b"5551234\0".to_vec();
        let (bytes, rep) = write(&g, &st).unwrap();
        assert_eq!(rep.strings, vec![(0xa3e, next)]);
        let b = Hdb::from_bytes(bytes);
        assert_eq!(semantic(SaveState::from_gam(&b)), semantic(st));
        Db::new(b.d.clone()).validate().unwrap();
    }

    #[test]
    fn names() {
        assert_eq!(sanitize(" ab/c..12 "), "ABC12");
        assert_eq!(sanitize("x".repeat(40).as_str()).len(), 21);
        for ok in ["slots.txt", "GAME 1.x", "A.x", "(X)-1, 2.x"] {
            assert!(is_slot_file(ok), "{ok}");
        }
        for bad in ["", ".x", "game.x", "../A.x", "A/B.x", "A.X", " A.x", "A .x", "SLOTS.TXT", "a.txt", "A.x.tmp", "A..x", &format!("{}.x", "A".repeat(22))] {
            assert!(!is_slot_file(bad), "{bad}");
        }
    }

    #[test]
    fn slots_store_and_reopen() {
        let d = std::env::temp_dir().join(format!("xf-slots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut s = Slots::open(&d);
        assert_eq!(s.path(0), None);
        s.store(1, "game 1", b"abc").unwrap();
        let s2 = Slots::open(&d);
        assert_eq!(s2.names[1], "GAME 1");
        assert_eq!(std::fs::read(s2.path(1).unwrap()).unwrap(), b"abc");
        std::fs::remove_file(s2.path(1).unwrap()).unwrap();
        assert_eq!(Slots::open(&d).names[1], ""); // file gone → slot empty (FUN_00515dd0)
        let _ = std::fs::remove_dir_all(&d);
    }
}
