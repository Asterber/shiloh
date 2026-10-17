//! Editing a NeoAccess 5.x database (XFiles.gam / a `.x` save): class directory, objId index trees, the class list,
//! the free list and 8-aligned appends — what the save builder ([`crate::save::build`]) needs to make a save without
//! a save of the original as template.
//!
//! Derived in part from Zeffuro/x-files-pc-enhancement-patch, `src/saves/database.h` (MIT License, Copyright (c) 2026
//! Zeffuro; licence text in `port/LICENSE-THIRD-PARTY`): the container checks ([`Db::validate`]: header 5 / 0x501,
//! 43-byte class-directory entries, index leaves of at most 32 `[mark][objId]` entries flagged 0x80, interior nodes of
//! 4-byte child marks, record sizes of the save classes), the 8-aligned append ([`Db::alloc`]) and the index rebuild
//! ([`Db::rebuild_index`], 264-byte leaves / 136-byte interior nodes). Rewritten in Rust, generalised to trees of any
//! depth; not a transliteration. Everything else here (the class list of class 1, adding a class, the free list,
//! in-place re-keying) is ours, decoded from a byte comparison of XFiles.gam with the original's Temp.gam / saves
//! (RE_NOTES H470–H476) — all **hypotheses**, static, not traced.
//!
//! Offsets of a class-directory entry are ours (`p` = where [`crate::hdb::Hdb`] starts an entry): p+8 u16 (class 1:
//! number of classes − 1), p+10 u8 number of indexes (1), p+11 class id, p+15 record count, p+19 / p+23 two marks
//! (class 1: p+19 = root of the class list), then per index 16 bytes: u32 7, u32 root, u32 class id, 4-byte tag. Zeffuro
//! counts the same entry from p+7 (`entry+0 == 1`, `+4` id, `+8` count, `+24` root, `+28` id).
use std::collections::BTreeMap;

/// Index leaf / interior node capacities and sizes (Zeffuro database.h; the original's 0x46 tree agrees: leaves of
/// ≤ 0x1f entries, H471).
pub const LEAF_CAP: usize = 32;
pub const LEAF_SIZE: usize = 8 + 8 * LEAF_CAP;
pub const NODE_CAP: usize = 32;
pub const NODE_SIZE: usize = 8 + 4 * NODE_CAP;
/// H472: the class list (class 1, p+19) — leaves of at most 8 u32 class ids (40 bytes), interior nodes as above.
pub const CLASS_LEAF_CAP: usize = 8;
pub const CLASS_LEAF_SIZE: usize = 8 + 4 * CLASS_LEAF_CAP;
/// Size of a class-directory entry with one index.
pub const DIR_ENTRY: u32 = 43;

/// Node flag byte 0 (stored as written; the reader bit-reverses it, so 0x80 = fLeaf): 0x80 leaf, 0x40 root. The
/// original's files carry more bits in both flag bytes (0xca31, 0x8b70, 0x5230 …); Zeffuro writes 0x80 / 0xc0 / 0x40
/// and 0 — so does this module (H471: only 0x80 / 0x40 matter on load — untested on the original, check AZ1).
pub const F_LEAF: u8 = 0x80;
pub const F_ROOT: u8 = 0x40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// file offset of the entry (the reader's `p`)
    pub at: u32,
    pub cls: u32,
    pub count: u32,
    /// root of the first (objId) index, 0 = none
    pub root: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Db {
    pub d: Vec<u8>,
}

fn err<T>(s: impl Into<String>) -> Result<T, String> {
    Err(s.into())
}

impl Db {
    pub fn new(d: Vec<u8>) -> Db {
        Db { d }
    }

    pub fn be32(&self, o: u32) -> u32 {
        self.d.get(o as usize..o as usize + 4).map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn be16(&self, o: u32) -> u16 {
        self.d.get(o as usize..o as usize + 2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
    }
    fn in_range(&self, o: u32, n: u32) -> bool {
        (o as u64 + n as u64) <= self.d.len() as u64
    }
    pub fn put32(&mut self, o: u32, v: u32) {
        self.d[o as usize..o as usize + 4].copy_from_slice(&v.to_be_bytes());
    }
    pub fn put16(&mut self, o: u32, v: u16) {
        self.d[o as usize..o as usize + 2].copy_from_slice(&v.to_be_bytes());
    }
    pub fn put(&mut self, o: u32, b: &[u8]) {
        self.d[o as usize..o as usize + b.len()].copy_from_slice(b);
    }

    /// Zeffuro `allocate`: `n` zero bytes at the end of the file rounded up to 8; unlike Zeffuro the size is rounded
    /// up too, so the file stays a multiple of 8 bytes long, as every file of the original is (48 / 48, H470).
    pub fn alloc(&mut self, n: usize) -> u32 {
        let at = (self.d.len() + 7) & !7;
        self.d.resize(at + ((n + 7) & !7), 0);
        at as u32
    }

    /// A node header: flags (byte 0, byte 1 = 0), version 1, count.
    fn node_header(&mut self, at: u32, flag: u8, count: usize) {
        self.d[at as usize] = flag;
        self.d[at as usize + 1] = 0;
        self.put32(at + 2, 1);
        self.put16(at + 6, count as u16);
    }

    fn is_leaf(&self, at: u32) -> bool {
        self.d.get(at as usize).is_some_and(|b| b & F_LEAF != 0)
    }

    /// The class directory (header +8, one root-leaf node: entries in file order).
    pub fn dir(&self) -> Result<Vec<DirEntry>, String> {
        let root = self.be32(8);
        if !self.in_range(root, 8) || !self.is_leaf(root) {
            return err("class directory: root is not a leaf");
        }
        let mut p = root + 8;
        let mut v = Vec::new();
        for _ in 0..self.be16(root + 6) {
            if !self.in_range(p, 27) {
                return err("class directory out of range");
            }
            let nidx = self.d[p as usize + 10] as u32;
            v.push(DirEntry {
                at: p,
                cls: self.be32(p + 11),
                count: self.be32(p + 15),
                root: if nidx > 0 { self.be32(p + 31) } else { 0 },
            });
            p += 27 + 16 * nidx;
        }
        Ok(v)
    }

    pub fn entry(&self, cls: u32) -> Option<DirEntry> {
        self.dir().ok()?.into_iter().find(|e| e.cls == cls)
    }

    /// Leaf offsets of the tree at `root`, in order (depth ≤ 8, as Zeffuro checks).
    pub fn leaves(&self, root: u32) -> Result<Vec<u32>, String> {
        let mut out = Vec::new();
        self.walk(root, 0, &mut out)?;
        Ok(out)
    }
    fn walk(&self, at: u32, depth: u32, out: &mut Vec<u32>) -> Result<(), String> {
        if at == 0 {
            return Ok(());
        }
        if depth > 8 || !self.in_range(at, 8) {
            return err(format!("tree node {at} out of range / too deep"));
        }
        if self.is_leaf(at) {
            out.push(at);
            return Ok(());
        }
        for i in 0..self.be16(at + 6) as u32 {
            self.walk(self.be32(at + 8 + 4 * i), depth + 1, out)?;
        }
        Ok(())
    }

    /// (objId, record mark) of a class index, file order.
    pub fn index(&self, root: u32) -> Result<Vec<(u32, u32)>, String> {
        let mut v = Vec::new();
        for l in self.leaves(root)? {
            for i in 0..self.be16(l + 6) as u32 {
                v.push((self.be32(l + 12 + 8 * i), self.be32(l + 8 + 8 * i)));
            }
        }
        Ok(v)
    }

    /// Records of class `cls` by objId.
    pub fn records(&self, cls: u32) -> Result<BTreeMap<u32, u32>, String> {
        let e = self.entry(cls).ok_or(format!("class {cls:#x} missing"))?;
        Ok(self.index(e.root)?.into_iter().collect())
    }

    /// H473: rewrite the entries of a class index in the existing leaves (same count per leaf; interior nodes hold
    /// only child marks, no keys, so any sorted spread over the same leaves is a valid tree). This is what the
    /// original did when VCString 0xf9c became 0x14d1 (XFiles.gam → Temp.gam: the 0x50 leaf rewritten in place).
    pub fn rewrite_index(&mut self, cls: u32, entries: &BTreeMap<u32, u32>) -> Result<(), String> {
        let e = self.entry(cls).ok_or(format!("class {cls:#x} missing"))?;
        let leaves = self.leaves(e.root)?;
        let total: usize = leaves.iter().map(|&l| self.be16(l + 6) as usize).sum();
        if total != entries.len() {
            return err("rewrite_index: count differs (use rebuild_index)");
        }
        let mut it = entries.iter();
        for l in leaves {
            for i in 0..self.be16(l + 6) as u32 {
                let (&id, &mark) = it.next().expect("counted");
                self.put32(l + 8 + 8 * i, mark);
                self.put32(l + 12 + 8 * i, id);
            }
        }
        Ok(())
    }

    /// Zeffuro `rebuild_index`, any depth: new leaves of ≤ 32 entries appended (264 bytes each), interior nodes of ≤ 32
    /// children (136 bytes) above them until one root; the class entry's count and root updated. The old nodes leak
    /// (as Zeffuro's; the original's free list is not touched).
    pub fn rebuild_index(&mut self, cls: u32, entries: &BTreeMap<u32, u32>) -> Result<u32, String> {
        let e = self.entry(cls).ok_or(format!("class {cls:#x} missing"))?;
        if entries.is_empty() {
            return err("rebuild_index: no records");
        }
        let v: Vec<(u32, u32)> = entries.iter().map(|(&i, &m)| (i, m)).collect();
        let mut level: Vec<u32> = Vec::new();
        for chunk in v.chunks(LEAF_CAP) {
            let at = self.alloc(LEAF_SIZE);
            self.node_header(at, F_LEAF, chunk.len());
            for (i, &(id, mark)) in chunk.iter().enumerate() {
                self.put32(at + 8 + 8 * i as u32, mark);
                self.put32(at + 12 + 8 * i as u32, id);
            }
            level.push(at);
        }
        while level.len() > 1 {
            let mut up = Vec::new();
            for chunk in level.chunks(NODE_CAP) {
                let at = self.alloc(NODE_SIZE);
                self.node_header(at, 0, chunk.len());
                for (i, &c) in chunk.iter().enumerate() {
                    self.put32(at + 8 + 4 * i as u32, c);
                }
                up.push(at);
            }
            level = up;
        }
        let root = level[0];
        self.d[root as usize] |= F_ROOT;
        self.put32(e.at + 15, v.len() as u32);
        self.put32(e.at + 31, root);
        Ok(root)
    }

    /// GUESS (H982, no file of the original shows an empty class): the class keeps a new empty root leaf, count 0.
    pub fn empty_index(&mut self, cls: u32) -> Result<(), String> {
        let e = self.entry(cls).ok_or(format!("class {cls:#x} missing"))?;
        let at = self.alloc(LEAF_SIZE);
        self.node_header(at, F_LEAF | F_ROOT, 0);
        self.put32(e.at + 15, 0);
        self.put32(e.at + 31, at);
        Ok(())
    }

    /// Header +0x10: the next objId to hand out (H470: 0x14cb in XFiles.gam, 0x14d2 / 0x14d9 in the saves).
    pub fn next_id(&self) -> u32 {
        self.be32(0x10)
    }
    pub fn take_id(&mut self) -> u32 {
        let id = self.next_id();
        self.put32(0x10, id + 1);
        id
    }

    /// H472: the class list — class ids in creation order (XFiles.gam: 0x46 0x53 0x50; saves: + 0x57 0x56 0x58 0x5a
    /// 0x5b 0x5c), a tree at class 1's p+19.
    pub fn class_list(&self) -> Result<Vec<u32>, String> {
        let c1 = self.entry(1).ok_or("class 1 missing")?;
        let mut v = Vec::new();
        for l in self.leaves(self.be32(c1.at + 19))? {
            for i in 0..self.be16(l + 6) as u32 {
                v.push(self.be32(l + 8 + 4 * i));
            }
        }
        Ok(v)
    }

    /// H472: append a class id to the class list as the original did for 0x5c (Temp.gam): the last leaf takes it while
    /// it has fewer than 8; a full root leaf gets a new interior root (136 bytes, flag 0x40) and a new leaf (40 bytes)
    /// holding the id, allocated in that order; a full leaf under an interior root gets a sibling leaf.
    fn class_list_push(&mut self, cls: u32) -> Result<(), String> {
        let c1 = self.entry(1).ok_or("class 1 missing")?;
        let root = self.be32(c1.at + 19);
        let leaves = self.leaves(root)?;
        let last = *leaves.last().ok_or("class list empty")?;
        let n = self.be16(last + 6) as usize;
        if n < CLASS_LEAF_CAP {
            self.put32(last + 8 + 4 * n as u32, cls);
            self.put16(last + 6, n as u16 + 1);
            return Ok(());
        }
        if self.is_leaf(root) {
            let node = self.alloc(NODE_SIZE);
            let leaf = self.alloc(CLASS_LEAF_SIZE);
            self.node_header(leaf, F_LEAF, 1);
            self.put32(leaf + 8, cls);
            self.node_header(node, F_ROOT, 2);
            self.put32(node + 8, root);
            self.put32(node + 12, leaf);
            self.d[root as usize] &= !F_ROOT;
            self.put32(c1.at + 19, node);
            return Ok(());
        }
        let k = self.be16(root + 6) as usize;
        if !self.is_leaf(self.be32(root + 8)) || k >= NODE_CAP {
            return err("class list deeper than two levels: not supported");
        }
        let leaf = self.alloc(CLASS_LEAF_SIZE);
        self.node_header(leaf, F_LEAF, 1);
        self.put32(leaf + 8, cls);
        self.put32(root + 8 + 4 * k as u32, leaf);
        self.put16(root + 6, k as u16 + 1);
        Ok(())
    }

    /// H470 / H472: add class `cls` holding one record `rec` with objId `id`, in the original's order (Temp.gam):
    /// class-list entry, then a 264-byte root leaf, then the record, both appended 8-aligned; the class-directory entry
    /// goes in class-id order into the directory node (the entries after it move up 43 bytes; the room after the last
    /// one must be zero — XFiles.gam has zeros from 39044 to 40840); header +0x0c and class 1's p+8 count it. Returns the record's mark.
    pub fn add_class(&mut self, cls: u32, id: u32, rec: &[u8]) -> Result<u32, String> {
        let dir = self.dir()?;
        if dir.iter().any(|e| e.cls == cls) {
            return err(format!("class {cls:#x} exists"));
        }
        let last = dir.last().ok_or("empty class directory")?;
        let end = last.at + 27 + 16 * self.d[last.at as usize + 10] as u32;
        if !self.in_range(end, DIR_ENTRY) || self.d[end as usize..(end + DIR_ENTRY) as usize].iter().any(|&b| b != 0) {
            return err("no free room in the class directory node");
        }
        // H470: the directory is kept in class-id order (saves: 0x56 before 0x57, though 0x57 is created first)
        let at = dir.iter().skip(1).find(|e| e.cls > cls).map_or(end, |e| e.at);
        self.d.copy_within(at as usize..end as usize, (at + DIR_ENTRY) as usize);
        self.d[at as usize..(at + DIR_ENTRY) as usize].fill(0);
        self.class_list_push(cls)?;
        let leaf = self.alloc(LEAF_SIZE);
        let mark = self.alloc(rec.len());
        self.put(mark, rec);
        self.node_header(leaf, F_LEAF | F_ROOT, 1);
        self.put32(leaf + 8, mark);
        self.put32(leaf + 12, id);
        // the entry: p+10 one index, p+11 class, p+15 count, p+19/p+23 0, index: 7, root, class, "ID  "
        self.d[at as usize + 10] = 1;
        self.put32(at + 11, cls);
        self.put32(at + 15, 1);
        self.put32(at + 27, 7);
        self.put32(at + 31, leaf);
        self.put32(at + 35, cls);
        self.put(at + 39, b"ID  ");
        let root = self.be32(8);
        let n = self.be16(root + 6) + 1;
        self.put16(root + 6, n);
        self.put32(0x0c, n as u32);
        let c1 = self.entry(1).ok_or("class 1 missing")?;
        self.put16(c1.at + 8, n - 1);
        Ok(mark)
    }

    /// The NeoAccess free list (header +4: node, u16 count, count × [mark][length]) (H400).
    pub fn free_list(&self) -> Vec<(u32, u32)> {
        let f = self.be32(4);
        if f == 0 || !self.in_range(f, 8) {
            return Vec::new();
        }
        (0..self.be16(f + 6) as u32).map(|i| (self.be32(f + 8 + 8 * i), self.be32(f + 12 + 8 * i))).collect()
    }

    /// H474: take the first free chunk of exactly `n` rounded up to 8 bytes (XFiles.gam → Temp.gam: 'english\0' went to
    /// the first chunk, 0x14de0 / 8, and the list lost it); None if there is none.
    pub fn free_take(&mut self, n: usize) -> Option<u32> {
        let want = ((n + 7) & !7) as u32;
        let list = self.free_list();
        let i = list.iter().position(|&(m, l)| l == want && m != 0)?;
        let f = self.be32(4);
        let mark = list[i].0;
        let rest: Vec<(u32, u32)> = list.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &x)| x).collect();
        for (j, &(m, l)) in rest.iter().enumerate() {
            self.put32(f + 8 + 8 * j as u32, m);
            self.put32(f + 12 + 8 * j as u32, l);
        }
        let k = rest.len() as u32;
        self.put32(f + 8 + 8 * k, 0);
        self.put32(f + 12 + 8 * k, 0);
        self.put16(f + 6, k as u16);
        Some(mark)
    }

    /// Zeffuro's `Container` checks (database.h), re-implemented, with our class set: header 5 / 0x501; the class
    /// directory a root leaf whose count = header +0x0c, class 1 first with p+8 = count − 1; every other entry one index
    /// whose class id repeats; every index: nodes of version 1 and 1..=32 entries, objIds strictly ascending and unique
    /// over the file, record marks unique, records of version 1, the class count = the entries; header +0x10 > every
    /// objId; record sizes 0x46 12, 0x53 24 (name blob non-empty, NUL-terminated), 0x57 132 (5 blobs of 4-byte
    /// multiples), 0x59 32, the others 14 (data blob a 4-byte multiple except 0x50); every blob in the file.
    pub fn validate(&self) -> Result<(), String> {
        if self.be32(0) != 5 || self.be32(0x14) != 0x501 {
            return err("header: not 5 / 0x501");
        }
        let root = self.be32(8);
        let dir = self.dir()?;
        if self.be32(root + 2) != 1 || dir.len() as u32 != self.be32(0x0c) || !(4..=11).contains(&dir.len()) {
            return err("class directory: count / version");
        }
        if dir[0].cls != 1 || self.be16(dir[0].at + 8) as usize != dir.len() - 1 {
            return err("class directory: class 1");
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut marks = std::collections::BTreeSet::new();
        for e in &dir[1..] {
            let ok_cls = matches!(e.cls, 0x46 | 0x50 | 0x53 | 0x56..=0x5c);
            if !ok_cls || self.d[e.at as usize + 10] != 1 || self.be32(e.at + 35) != e.cls || self.be32(e.at + 27) != 7
            {
                return err(format!("class entry {:#x}", e.cls));
            }
            let mut prev = 0u32;
            let mut n = 0u32;
            let mut nodes = std::collections::BTreeSet::new();
            self.check_nodes(e.root, 0, &mut nodes)?;
            for (id, mark) in self.index(e.root)? {
                if id <= prev || !ids.insert(id) || !marks.insert(mark) || self.be32(mark + 2) != 1 {
                    return err(format!("class {:#x}: objId {id:#x} / mark {mark}", e.cls));
                }
                prev = id;
                n += 1;
                self.check_record(e.cls, mark).map_err(|s| format!("class {:#x} record {id:#x}: {s}", e.cls))?;
            }
            if n != e.count {
                return err(format!("class {:#x}: count {} != {n} entries", e.cls, e.count));
            }
        }
        if ids.last().is_some_and(|&m| self.next_id() <= m) {
            return err("header +0x10 not above every objId");
        }
        Ok(())
    }

    fn check_nodes(&self, at: u32, depth: u32, seen: &mut std::collections::BTreeSet<u32>) -> Result<(), String> {
        if depth > 8 || !self.in_range(at, 8) || !seen.insert(at) || self.be32(at + 2) != 1 {
            return err(format!("index node {at}"));
        }
        let n = self.be16(at + 6) as u32;
        let leaf = self.is_leaf(at);
        if (n == 0 && !(depth == 0 && leaf)) || n as usize > LEAF_CAP || !self.in_range(at, 8 + n * if leaf { 8 } else { 4 }) {
            return err(format!("index node {at}: {n} entries"));
        }
        if !leaf {
            for i in 0..n {
                self.check_nodes(self.be32(at + 8 + 4 * i), depth + 1, seen)?;
            }
        }
        Ok(())
    }

    fn blob(&self, at: u32) -> Result<(u32, u32), String> {
        let (m, l) = (self.be32(at), self.be32(at + 4));
        if (m == 0) != (l == 0) || !self.in_range(m, l) {
            return err(format!("blob {m}/{l}"));
        }
        Ok((m, l))
    }

    fn check_record(&self, cls: u32, at: u32) -> Result<(), String> {
        let size = match cls {
            0x46 => 12,
            0x53 => 24,
            0x57 => 132,
            0x59 => 32,
            _ => 14,
        };
        if !self.in_range(at, size) {
            return err("record out of range");
        }
        match cls {
            0x46 => {}
            0x53 => {
                let (m, l) = self.blob(at + 6)?;
                if l == 0 || l > 1024 || self.d[(m + l - 1) as usize] != 0 {
                    return err("variable name");
                }
            }
            0x57 => {
                for i in 0..5 {
                    if self.blob(at + 66 + 8 * i)?.1 % 4 != 0 {
                        return err("VCGameState list length");
                    }
                }
            }
            0x59 => {
                self.blob(at + 6)?;
            }
            _ => {
                let (_, l) = self.blob(at + 6)?;
                if cls != 0x50 && l % 4 != 0 {
                    return err("data length");
                }
            }
        }
        Ok(())
    }
}
