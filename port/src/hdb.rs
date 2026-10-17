//! Structural reader for XFiles.hdb / XFiles.gam (NeoAccess 5.x database, big-endian).
//!
//! Field order and widths determined from the original's readers (RE_NOTES §8) and checked against traces
//! (C13, C14, C17, C19, C20). No heuristics.
//!
//! File header: +0x08 root mark (CNeoInode of the class tree), +0x0c number of classes, +0x14
//! database version. Every record starts with the CNeoPersist header (FUN_00413910): u16 flags
//! (each byte bit-reversed; bit0 = fLeaf) + u32 vers. Tree nodes (FUN_004138e0) add u16 count,
//! payload at +8.

use std::fmt;
use std::path::Path;

/// Field kinds of the per-class schemas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    U32,
    U16,
    I16,
    U8,
    Bool,
    Blob,
    Rect,
    Point,
    Region,
    List,
    U32x2,
    U32x3,
}

/// PersistentList part (FUN_00413a50, 26 bytes): CNeoIDList tree at `mark`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ListPart {
    pub count: u32,
    pub mark: u32,
    pub cls: u32,
    pub tag: [u8; 4],
    pub npto: u32,
    pub nprn: u8,
    pub npdu: u8,
    pub id: u32,
}

/// Blob reference (FUN_00413990): mark + length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BlobRef {
    pub mark: u32,
    pub len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// u32 / u16 / u8 / bool values (widened).
    Int(u32),
    I16(i16),
    Blob(BlobRef),
    Rect([i16; 4]),
    Point([i16; 2]),
    Region {
        kind: u32,
        rect: [i16; 4],
    },
    List(ListPart),
    Arr(Vec<u32>),
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: &'static str,
    pub kind: Kind,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct Record {
    pub cls: u32,
    pub id: u32,
    pub mark: u32,
    pub end: u32,
    pub flags: u16,
    pub vers: u32,
    pub fields: Vec<Field>,
}

impl Record {
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|f| f.name == name).map(|f| &f.value)
    }
    /// Integer field (u32/u16/u8/bool); 0 if absent or not an integer.
    pub fn u32(&self, name: &str) -> u32 {
        match self.field(name) {
            Some(Value::Int(v)) => *v,
            _ => 0,
        }
    }
    pub fn blob(&self, name: &str) -> Option<BlobRef> {
        match self.field(name) {
            Some(Value::Blob(b)) => Some(*b),
            _ => None,
        }
    }
    pub fn list(&self) -> Option<&ListPart> {
        match self.field("list") {
            Some(Value::List(l)) => Some(l),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Class {
    pub id: u32,
    pub count: u32,
    pub mark1: u32,
    pub mark2: u32,
    /// root of the first (objId) index, 0 if none
    pub index_root: u32,
    /// (objId, mark), sorted by objId; duplicate ids keep the last index entry (dict semantics of hdb.py)
    pub objects: Vec<(u32, u32)>,
}

/// One field of a class schema: name, kind, and the app version the field needs (`app > min`).
type FieldSpec = (&'static str, Kind, Option<u32>);

const LIST: FieldSpec = ("list", Kind::List, None);
const ICON: &[FieldSpec] = &[
    ("f28", Kind::U32, None),
    ("f2c", Kind::U32, None),
    ("f30", Kind::U32, None),
    ("f34", Kind::U32, None),
    ("f3c", Kind::U32, None),
    ("f40", Kind::U32, None),
    ("f38", Kind::U32, Some(3)),
];
const DATA: &[FieldSpec] = &[("data", Kind::Blob, None)];

/// classId -> fields; the class name in comment (RE_NOTES §8).
pub fn schema(cls: u32) -> Option<&'static [FieldSpec]> {
    use Kind::*;
    Some(match cls {
        0x1e => &[LIST], // PersistentList
        // VCTitle: tags 0x65 list (app version > 1), 0x66 blob, 0x68 u32, 0x67 blob, 0x69 blob, 0x6a f9c,
        // 0x6b fa0 (TriggerList), 0x6c fa4, 0x6d fa6
        0x27 => &[
            ("list", List, Some(1)),
            ("f64", Blob, None),
            ("f84", U32, None),
            ("f74", Blob, None),
            ("f8c", Blob, None),
            ("f9c", U32, None),
            ("fa0", U32, None),
            ("fa4", U16, None),
            ("fa6", U16, None),
        ],
        0x28 => &[LIST, ("f64", U32, None), ("f68", U32, None)], // VCNode
        0x29 => &[LIST, ("f64", U32, None), ("f68", U32, None), ("f6c", U32, None), ("f74", U8, None)], // VCLocaton
        0x2a => &[LIST, ("f64", Point, None), ("f70", U32, None), ("f74", U32, None)], // VCViewPoint
        0x2b => &[
            ("f28", U32, None),
            ("f2c", U32, None),
            ("f30", U32, None),
            ("navList", U32, None),
            ("f38", U32, None),
            ("f3c", U32, None),
        ], // VCView
        0x2c => &[("f28", U32, None)],                           // VCCharacter
        0x2d => &[
            ("f28", U32, None),
            ("f2c", Region, None),
            ("f48", U32, None),
            ("f4c", U32, None),
            ("f50", U32, None),
            ("f54", U32, None),
        ], // VCCharView
        0x2e => &[LIST],                                         // VCCharViewList
        0x2f => &[("name", Blob, None), ("f38", Blob, None)],    // VCName
        0x31 => &[
            ("f28", U32, None),
            ("f2c", U32, None),
            ("f34", U32, None),
            ("f3c", U32, None),
            ("f30", U32, Some(2)),
            ("f38", U32, Some(2)),
            ("f40", Bool, Some(3)),
        ], // VCConversation
        0x32 => &[LIST],                                         // VCConversationList
        0x33 => &[("destView", U32, None), ("hotSpot", U32, None), ("f30", U32, None)], // VCNav
        0x34 => &[LIST],                                         // VCNav_List
        0x35 => &[("asset", Blob, None), ("f38", U32, None), ("f3c", U32, None), ("f40", U8, None), ("f41", U8, None)], // VCAssetRef
        0x36 => &[LIST],                                                              // VCAssetRefList
        0x37 => &[("region", Region, None), ("f44", U32, None)],                      // VCExplorable
        0x38 => &[LIST],                                                              // VCExplorableList
        0x39 => &[("region", Region, None), ("f44", U32, None)],                      // VCHotSpot
        0x3a => &[LIST],                                                              // VCHotSpotList
        0x3b => &[LIST],                                                              // VCStdHotSpotList
        0x3c => &[("f28", U32, None), ("f2c", U8, None)],                             // VCAssetCategory
        0x3d => &[("hotPoint", Point, None), ("f34", U32, None), ("f38", U32, None)], // VCCursor
        0x3e => &[("cursors", U32x2, None)], // VCDefaultCursors (array len: HYPOTHESIS)
        0x40 => &[("hotSpots", U32x3, None)], // VCDefaultHotSpots (array len: HYPOTHESIS)
        0x41 => &[("data", Blob, None), ("type", U8, None)], // VCAction
        0x42 => &[LIST],                     // VCActionList
        0x43 => DATA,                        // VCBlob
        0x44 => &[("f28", U32, None)],       // VCPlayer
        0x46 => &[("f28", U32, None), ("f2d", U8, None), ("f2c", U8, None)], // VCEnabled
        0x47..=0x4b => ICON,                 // icons
        0x4c => &[("f28", U32, None), ("f2c", U32, None)], // VCIdeaMap
        0x4d => &[LIST],                     // VCIdeaMapList
        0x4e => &[LIST, ("f64", U32, None), ("f68", U8, None)], // VCIFaceLayout
        0x4f => &[
            ("region", Region, None),
            ("f44", U32, None),
            ("f48", U32, None),
            ("f4c", U16, None),
            ("f50", U16, Some(4)),
        ], // VCInterfaceItem
        0x50 => &[("text", Blob, None)],     // VCString
        0x51 => &[("fe4", U32, None), ("fe8", U8, None)], // VCTrigger
        0x52 => &[LIST],                     // VCTriggerList
        0x53 => &[("name", Blob, None), ("f38", U32, None), ("f3c", U32, None), ("f40", U8, None), ("f41", U8, None)], // VCVariable
        0x54 => &[("f28", U32, None), ("f2c", U32, None), ("f30", U32, None)], // VCStdAction
        0x55 => &[LIST],                                                       // VCStdActionList
        0x56 => DATA,                                                          // VCConversationHistory
        // VCGameState (tags 0x65..0x85; RE_NOTES H65): Node, Location, ViewPoint, View (SetStartView's
        // pending start, FUN_00486a10), f38..f48 = ids of the 0x56/0x58/0x5a/0x5b/0x5c records, 5 blobs (mark +
        // length), then the restore mode (read in FUN_00607a00).
        0x57 => &[
            ("node", U32, None),
            ("location", U32, None),
            ("viewpoint", U32, None),
            ("view", U32, None),
            ("f38", U32, None),
            ("f3c", U32, None),
            ("f40", U32, None),
            ("f44", U32, None),
            ("f48", U32, None),
            ("f4c", U32, None),
            ("f50", U32, None),
            ("f54", U32, None),
            ("f58", U32, None),
            ("f5c", U32, None),
            ("f60", U32, None),
            ("l64", Blob, None),
            ("la0", Blob, None),
            ("ldc", Blob, None),
            ("l118", Blob, None),
            ("l154", Blob, None),
            ("b190", U8, None),
            ("b191", U8, None),
            ("b192", U8, None),
            ("b193", U8, None),
            ("b194", U8, None),
            ("b195", U8, None),
            ("b196", U8, None),
            ("f198", U32, None),
            ("b19c", U8, None),
            ("f1a0", U32, None),
            ("f1a4", U32, None),
            ("f1a8", U32, None),
            ("mode", I16, None),
        ],
        0x58 => DATA, // VCPDANotes
        0x59 => &[
            ("data", Blob, None),
            ("f64", U32, None),
            ("f68", U32, None),
            ("rect", Rect, None),
            ("f94", U8, None),
            ("f95", U8, None),
        ], // VCPhoto
        0x5a..=0x5c => DATA, // VCEmail*
        _ => return None,
    })
}

#[derive(Debug)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub struct Hdb {
    pub d: Vec<u8>,
    pub root: u32,
    pub nclasses: u32,
    pub version: u32,
    /// HYPOTHESIS (RE_NOTES §8): app schema version = header u32 @0.
    pub app_version: u32,
    /// classes in class-tree order
    pub classes: Vec<Class>,
    /// HYPOTHESIS H260: string resources of XfilesT.dll beside an opened `.hdb` (kind-0 AssetRefs, `crate::pe`)
    pub res_strings: std::collections::HashMap<u32, String>,
    /// The content tables of the `XFiles.exe` beside an opened `.hdb` ([`crate::exetables`]; empty without one)
    pub exe: crate::exetables::ExeTables,
}

fn rev8(b: u8) -> u8 {
    b.reverse_bits()
}

impl Hdb {
    pub fn open(path: impl AsRef<Path>) -> Result<Hdb, Error> {
        let p = path.as_ref();
        let d = crate::io::read(p).map_err(|e| Error(format!("{}: {e}", p.display())))?;
        let mut h = Hdb::from_bytes(d);
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hdb")) {
            h.res_strings = crate::pe::text_dll_strings(p.parent().unwrap_or(Path::new(".")));
            h.exe = crate::exetables::ExeTables::beside(p.parent().unwrap_or(Path::new(".")));
        }
        Ok(h)
    }

    pub fn from_bytes(d: Vec<u8>) -> Hdb {
        let mut h = Hdb {
            d,
            root: 0,
            nclasses: 0,
            version: 0,
            app_version: 0,
            classes: Vec::new(),
            res_strings: Default::default(),
            exe: Default::default(),
        };
        h.app_version = h.be32(0);
        h.root = h.be32(8);
        h.nclasses = h.be32(0xc);
        h.version = h.be32(0x14);
        let mut classes = Vec::new();
        for leaf in h.leaves(h.root) {
            let n = h.be16(leaf + 6);
            let mut p = leaf + 8;
            for _ in 0..n {
                let nidx = h.byte(p + 10) as u32;
                let mut c = Class {
                    id: h.be32(p + 11),
                    count: h.be32(p + 15),
                    mark1: h.be32(p + 19),
                    mark2: h.be32(p + 23),
                    index_root: 0,
                    objects: Vec::new(),
                };
                let q = p + 27;
                if nidx > 0 {
                    c.index_root = h.be32(q + 4);
                }
                classes.push(c);
                p = q + 16 * nidx;
            }
        }
        for c in &mut classes {
            let mut objs: Vec<(u32, u32)> = Vec::new();
            if c.index_root != 0 {
                for leaf in h.leaves(c.index_root) {
                    let n = h.be16(leaf + 6);
                    for i in 0..n as u32 {
                        let mark = h.be32(leaf + 8 + 8 * i);
                        let id = h.be32(leaf + 12 + 8 * i);
                        objs.push((id, mark));
                    }
                }
            }
            // stable sort, then keep the LAST entry of each duplicate id (hdb.py dict semantics)
            objs.sort_by_key(|&(id, _)| id);
            let mut out: Vec<(u32, u32)> = Vec::with_capacity(objs.len());
            for o in objs {
                match out.last_mut() {
                    Some(last) if last.0 == o.0 => *last = o,
                    _ => out.push(o),
                }
            }
            c.objects = out;
        }
        h.classes = classes;
        h
    }

    pub fn be32(&self, o: u32) -> u32 {
        let o = o as usize;
        match self.d.get(o..o + 4) {
            Some(b) => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            None => 0,
        }
    }
    pub fn be16(&self, o: u32) -> u16 {
        let o = o as usize;
        match self.d.get(o..o + 2) {
            Some(b) => u16::from_be_bytes([b[0], b[1]]),
            None => 0,
        }
    }
    pub fn byte(&self, o: u32) -> u8 {
        self.d.get(o as usize).copied().unwrap_or(0)
    }
    /// CNeoPersist flags: each byte stored bit-reversed; bit0 = fLeaf.
    pub fn flags(&self, o: u32) -> u16 {
        if o as usize + 2 > self.d.len() {
            return 0;
        }
        rev8(self.byte(o)) as u16 | (rev8(self.byte(o + 1)) as u16) << 8
    }

    /// Leaf node offsets of the NeoAccess tree at `mark`, in order.
    pub fn leaves(&self, mark: u32) -> Vec<u32> {
        let mut out = Vec::new();
        self.walk(mark, 0, &mut out);
        out
    }
    fn walk(&self, mark: u32, depth: u32, out: &mut Vec<u32>) {
        if mark == 0 || mark as usize + 8 > self.d.len() || depth > 32 {
            return;
        }
        if self.flags(mark) & 1 != 0 {
            out.push(mark);
            return;
        }
        let n = self.be16(mark + 6) as u32;
        for i in 0..n {
            self.walk(self.be32(mark + 8 + 4 * i), depth + 1, out);
        }
    }

    pub fn class(&self, cls: u32) -> Option<&Class> {
        self.classes.iter().find(|c| c.id == cls)
    }

    /// record mark of (cls, id)
    pub fn find(&self, cls: u32, id: u32) -> Option<u32> {
        let c = self.class(cls)?;
        c.objects.binary_search_by_key(&id, |&(i, _)| i).ok().map(|i| c.objects[i].1)
    }

    pub fn has(&self, cls: u32, id: u32) -> bool {
        self.find(cls, id).is_some()
    }

    /// Parse one record per the class schema.
    pub fn read(&self, cls: u32, id: u32) -> Option<Record> {
        let s = schema(cls)?;
        let mark = self.find(cls, id)?;
        if mark as usize + 6 > self.d.len() {
            return None;
        }
        let mut r = Record {
            cls,
            id,
            mark,
            end: 0,
            flags: self.flags(mark),
            vers: self.be32(mark + 2),
            fields: Vec::with_capacity(s.len()),
        };
        let mut p = mark + 6;
        for &(name, kind, min) in s {
            if let Some(m) = min {
                if self.app_version <= m {
                    continue;
                }
            }
            let (value, np) = self.read_value(p, kind);
            p = np;
            r.fields.push(Field { name, kind, value });
        }
        r.end = p;
        Some(r)
    }

    /// File offset and kind of every field of record (cls, id), schema order (the positions [`Hdb::read`] reads from;
    /// used by the save writer, `crate::save`).
    pub fn field_offsets(&self, cls: u32, id: u32) -> Option<Vec<(&'static str, Kind, u32)>> {
        let s = schema(cls)?;
        let mark = self.find(cls, id)?;
        let mut p = mark + 6;
        let mut v = Vec::with_capacity(s.len());
        for &(name, kind, min) in s {
            if min.is_some_and(|m| self.app_version <= m) {
                continue;
            }
            v.push((name, kind, p));
            p = self.read_value(p, kind).1;
        }
        Some(v)
    }

    fn rect(&self, p: u32) -> [i16; 4] {
        [0, 1, 2, 3].map(|i| self.be16(p + 2 * i) as i16)
    }

    fn read_value(&self, p: u32, kind: Kind) -> (Value, u32) {
        match kind {
            Kind::U32 => (Value::Int(self.be32(p)), p + 4),
            Kind::U16 => (Value::Int(self.be16(p) as u32), p + 2),
            Kind::I16 => (Value::I16(self.be16(p) as i16), p + 2),
            Kind::U8 | Kind::Bool => (Value::Int(self.byte(p) as u32), p + 1),
            Kind::Blob => (Value::Blob(BlobRef { mark: self.be32(p), len: self.be32(p + 4) }), p + 8),
            Kind::Rect => (Value::Rect(self.rect(p)), p + 8),
            Kind::Point => (Value::Point([self.be16(p) as i16, self.be16(p + 2) as i16]), p + 4),
            Kind::Region => (Value::Region { kind: self.be32(p), rect: self.rect(p + 4) }, p + 12),
            Kind::List => {
                let l = ListPart {
                    count: self.be32(p),
                    mark: self.be32(p + 4),
                    cls: self.be32(p + 8),
                    tag: [self.byte(p + 12), self.byte(p + 13), self.byte(p + 14), self.byte(p + 15)],
                    npto: self.be32(p + 16),
                    nprn: self.byte(p + 20),
                    npdu: self.byte(p + 21),
                    id: self.be32(p + 22),
                };
                (Value::List(l), p + 26)
            }
            Kind::U32x2 | Kind::U32x3 => {
                let n = if kind == Kind::U32x2 { 2 } else { 3 };
                ((Value::Arr((0..n).map(|i| self.be32(p + 4 * i)).collect())), p + 4 * n)
            }
        }
    }

    /// Contents of a list part: the CNeoIDList tree at `list.mark`.
    pub fn idlist(&self, list: &ListPart) -> Vec<u32> {
        let mut out = Vec::new();
        if list.mark == 0 {
            return out;
        }
        for leaf in self.leaves(list.mark) {
            let n = self.be16(leaf + 6) as u32;
            for i in 0..n {
                out.push(self.be32(leaf + 8 + 4 * i));
            }
        }
        out
    }

    /// Elements of the list object (cls, id) (classes whose schema starts with a list part).
    pub fn list_of(&self, cls: u32, id: u32) -> Vec<u32> {
        match self.read(cls, id) {
            Some(r) => r.list().map(|l| self.idlist(l)).unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// Raw bytes [mark, mark+len), None if out of range.
    pub fn bytes(&self, mark: u32, len: u32) -> Option<&[u8]> {
        self.d.get(mark as usize..mark as usize + len as usize)
    }

    pub fn blob_bytes(&self, b: BlobRef) -> Option<&[u8]> {
        self.bytes(b.mark, b.len)
    }
}
