//! Read-only reader for CD / DVD images in the ISO 9660 format (ECMA-119), with the Joliet extension (UCS-2 names in
//! a Supplementary Volume Descriptor). Port feature: the game can be started from the user's own disc images
//! ([`crate::vfs`]). Written from the public ECMA-119 description.
//!
//! - Image containers: plain 2048-byte sectors (`.iso`, Alcohol `.mdf` of a DVD) and raw 2352-byte CD sectors (Mode 1:
//!   user data at byte 16; Mode 2 Form 1 / XA: at byte 24), also with 96 bytes of sub-channel per sector (2448). The
//!   layout is found by looking for the `CD001` volume descriptor at sector 16.
//! - The directory tree is read once when the image is opened (a few thousand records on a game disc). File contents
//!   are read on demand by offset; nothing is copied into memory beyond what a read asks for.
//! - Names: Joliet when present, else the primary tree with the `;1` version and a trailing dot removed. Rock Ridge
//!   and UDF are not read (game discs of 1998 carry ISO 9660 names; the EN DVD is an ISO 9660 / UDF bridge).
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Logical sector (block) size of the file system.
pub const BLOCK: u64 = 2048;

/// One node of the directory tree.
#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub is_dir: bool,
    /// file size in bytes (for a directory: the size of its directory extent)
    pub size: u64,
    /// the file's extents: (first logical block, byte length); several for a multi-extent file
    extents: Vec<(u32, u64)>,
    /// children of a directory (indices into the node table)
    pub children: Vec<usize>,
    /// lower-case name → child index
    by_name: HashMap<String, usize>,
}

/// An opened image.
pub struct Image {
    pub path: PathBuf,
    /// the volume identifier of the primary volume descriptor (trailing spaces removed)
    pub label: String,
    /// whether the names come from a Joliet tree
    pub joliet: bool,
    nodes: Vec<Node>,
    file: Mutex<File>,
    /// bytes per physical sector in the image file, and the offset of the 2048 user bytes in it
    sector: u64,
    data_off: u64,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}, {:?}, {} nodes)", self.path.display(), self.label, self.nodes.len())
    }
}

fn err(m: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, m.into())
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// The sector layouts tried, (bytes per sector, offset of the user data).
const LAYOUTS: [(u64, u64); 5] = [(2048, 0), (2352, 16), (2352, 24), (2448, 16), (2448, 24)];

impl Image {
    /// Open an image file; Err when it is not an ISO 9660 image.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Image> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let len = file.metadata()?.len();
        let mut found = None;
        for (sector, off) in LAYOUTS {
            let at = 16 * sector + off;
            if at + 6 > len {
                continue;
            }
            let mut b = [0u8; 6];
            file.seek(SeekFrom::Start(at))?;
            file.read_exact(&mut b)?;
            if &b[1..6] == b"CD001" {
                found = Some((sector, off));
                break;
            }
        }
        let (sector, data_off) = found.ok_or_else(|| err(format!("{}: not an ISO 9660 image", path.display())))?;
        let mut img = Image {
            path,
            label: String::new(),
            joliet: false,
            nodes: Vec::new(),
            file: Mutex::new(file),
            sector,
            data_off,
        };
        // the volume descriptors: sector 16 up to the terminator (type 255)
        let (mut primary, mut joliet) = (None, None);
        for i in 16..16 + 64 {
            let d = img.block(i)?;
            if &d[1..6] != b"CD001" {
                break;
            }
            match d[0] {
                1 if primary.is_none() => primary = Some(d),
                // Joliet: a supplementary descriptor with the UCS-2 escape sequence %/@, %/C or %/E
                2 if d[88] == b'%' && d[89] == b'/' && matches!(d[90], b'@' | b'C' | b'E') => joliet = Some(d),
                255 => break,
                _ => {}
            }
        }
        let pvd = primary.ok_or_else(|| err(format!("{}: no primary volume descriptor", img.path.display())))?;
        if le16(&pvd, 128) as u64 != BLOCK {
            return Err(err(format!(
                "{}: logical block size {} (only 2048 is read)",
                img.path.display(),
                le16(&pvd, 128)
            )));
        }
        img.label = String::from_utf8_lossy(&pvd[40..72]).trim_end_matches([' ', '\0']).to_string();
        img.joliet = joliet.is_some();
        let vd = joliet.unwrap_or(pvd);
        img.read_tree(&vd[156..190])?;
        Ok(img)
    }

    /// The 2048 user bytes of logical block `lba`.
    fn block(&self, lba: u64) -> std::io::Result<Vec<u8>> {
        let mut b = vec![0; BLOCK as usize];
        let n = self.read_blocks(lba, 0, &mut b)?;
        if n < b.len() {
            return Err(err(format!("{}: block {lba} past the end", self.path.display())));
        }
        Ok(b)
    }

    /// Up to `buf.len()` user bytes starting `skip` bytes into logical block `lba` (crossing blocks as needed).
    fn read_blocks(&self, lba: u64, skip: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if self.sector == BLOCK {
            f.seek(SeekFrom::Start(lba * BLOCK + skip))?;
            return read_full(&mut *f, buf);
        }
        // raw sectors: 2048 user bytes per physical sector
        let mut done = 0;
        let mut pos = lba * BLOCK + skip;
        while done < buf.len() {
            let (blk, within) = (pos / BLOCK, pos % BLOCK);
            let n = ((BLOCK - within) as usize).min(buf.len() - done);
            f.seek(SeekFrom::Start(blk * self.sector + self.data_off + within))?;
            let k = read_full(&mut *f, &mut buf[done..done + n])?;
            done += k;
            if k < n {
                break;
            }
            pos += n as u64;
        }
        Ok(done)
    }

    /// Read the whole directory tree from the root directory record `root`.
    fn read_tree(&mut self, root: &[u8]) -> std::io::Result<()> {
        let rec = parse_record(root, self.joliet).ok_or_else(|| err("bad root directory record"))?;
        self.nodes.push(Node {
            name: String::new(),
            is_dir: true,
            size: rec.len,
            extents: vec![(rec.lba, rec.len)],
            children: Vec::new(),
            by_name: HashMap::new(),
        });
        // breadth-first; `seen` guards against directory loops in a damaged image
        let mut queue = vec![0usize];
        let mut seen = std::collections::HashSet::new();
        seen.insert(rec.lba);
        while let Some(di) = queue.pop() {
            let (lba, len) = self.nodes[di].extents[0];
            let mut raw = vec![0; len as usize];
            let n = self.read_blocks(lba as u64, 0, &mut raw)?;
            raw.truncate(n);
            let mut kids: Vec<(Rec, String)> = Vec::new();
            let mut o = 0usize;
            while o < raw.len() {
                let rl = raw[o] as usize;
                if rl == 0 {
                    // records never cross a block boundary: the rest of this block is padding
                    o = (o / BLOCK as usize + 1) * BLOCK as usize;
                    continue;
                }
                if o + rl > raw.len() || rl < 34 {
                    break;
                }
                if let Some(r) = parse_record(&raw[o..o + rl], self.joliet) {
                    if !r.dot {
                        let name = r.name.clone();
                        kids.push((r, name));
                    }
                }
                o += rl;
            }
            // a multi-extent file is several records of the same name, the last one without the flag
            let mut i = 0;
            while i < kids.len() {
                let (r, name) = &kids[i];
                let mut extents = vec![(r.lba, r.len)];
                let mut more = r.multi;
                let is_dir = r.dir;
                i += 1;
                while more && i < kids.len() && kids[i].1 == *name {
                    extents.push((kids[i].0.lba, kids[i].0.len));
                    more = kids[i].0.multi;
                    i += 1;
                }
                let size = extents.iter().map(|e| e.1).sum();
                let idx = self.nodes.len();
                let key = name.to_lowercase();
                if self.nodes[di].by_name.contains_key(&key) {
                    continue;
                }
                self.nodes.push(Node {
                    name: name.clone(),
                    is_dir,
                    size,
                    extents: extents.clone(),
                    children: Vec::new(),
                    by_name: HashMap::new(),
                });
                self.nodes[di].children.push(idx);
                self.nodes[di].by_name.insert(key, idx);
                if is_dir && seen.insert(extents[0].0) {
                    queue.push(idx);
                }
            }
        }
        Ok(())
    }

    /// The root directory's node index.
    pub fn root(&self) -> usize {
        0
    }

    pub fn node(&self, i: usize) -> &Node {
        &self.nodes[i]
    }

    /// The child of directory `dir` named `name` (letter case ignored).
    pub fn child(&self, dir: usize, name: &str) -> Option<usize> {
        self.nodes.get(dir)?.by_name.get(&name.to_lowercase()).copied()
    }

    /// The node at `rel` (components separated by `/` or given as a path; letter case ignored) below `dir`.
    pub fn lookup(&self, dir: usize, rel: &Path) -> Option<usize> {
        let mut at = dir;
        for c in rel.components() {
            match c {
                std::path::Component::Normal(n) => at = self.child(at, &n.to_string_lossy())?,
                std::path::Component::CurDir => {}
                _ => return None,
            }
        }
        Some(at)
    }

    /// Up to `buf.len()` bytes of file `i` at `off`; the count read (short only at the end of the file).
    pub fn read_at(&self, i: usize, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = &self.nodes[i];
        let mut done = 0usize;
        let mut base = 0u64;
        for &(lba, len) in &n.extents {
            if done == buf.len() {
                break;
            }
            let pos = off + done as u64;
            if pos < base + len {
                let within = pos - base;
                let want = ((len - within) as usize).min(buf.len() - done);
                let k = self.read_blocks(lba as u64, within, &mut buf[done..done + want])?;
                done += k;
                if k < want {
                    break;
                }
            }
            base += len;
        }
        Ok(done)
    }

    /// The whole file `i`.
    pub fn read(&self, i: usize) -> std::io::Result<Vec<u8>> {
        let mut b = vec![0; self.nodes[i].size as usize];
        let n = self.read_at(i, 0, &mut b)?;
        b.truncate(n);
        Ok(b)
    }
}

fn read_full(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// A directory record (ECMA-119 9.1).
struct Rec {
    lba: u32,
    len: u64,
    dir: bool,
    multi: bool,
    /// the `.` / `..` entries (identifier 0x00 / 0x01)
    dot: bool,
    name: String,
}

fn parse_record(r: &[u8], joliet: bool) -> Option<Rec> {
    if r.len() < 34 {
        return None;
    }
    let lba = le32(r, 2);
    let len = le32(r, 10) as u64;
    let flags = r[25];
    let nl = r[32] as usize;
    let id = r.get(33..33 + nl)?;
    let dot = nl == 1 && (id[0] == 0 || id[0] == 1);
    let mut name = if dot {
        String::new()
    } else if joliet {
        let u: Vec<u16> = id.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        id.iter().map(|&b| b as char).collect()
    };
    if flags & 2 == 0 {
        // file: drop the version number (`;1`) and a dot left before it (`NAME.;1`)
        if let Some(p) = name.rfind(';') {
            name.truncate(p);
        }
        if name.ends_with('.') {
            name.pop();
        }
    }
    if !dot && (name.is_empty() || name.contains(['/', '\0'])) {
        return None;
    }
    Some(Rec { lba, len, dir: flags & 2 != 0, multi: flags & 0x80 != 0, dot, name })
}

/// Whether the file at `p` looks like a disc image this reader can open (the `CD001` descriptor at sector 16 in one
/// of the known sector layouts).
pub fn is_image(p: &Path) -> bool {
    let Ok(mut f) = File::open(p) else { return false };
    let Ok(m) = f.metadata() else { return false };
    if !m.is_file() {
        return false;
    }
    LAYOUTS.iter().any(|&(sector, off)| {
        let mut b = [0u8; 6];
        f.seek(SeekFrom::Start(16 * sector + off)).is_ok() && f.read_exact(&mut b).is_ok() && &b[1..6] == b"CD001"
    })
}

/// Builder of small ISO 9660 images in memory (tests and examples): files given by path, directories made as needed.
/// Level-1-style upper-case names in the primary tree; with `joliet`, also a Joliet tree with the names as given.
pub mod build {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Dir {
        dirs: BTreeMap<String, Dir>,
        files: BTreeMap<String, Vec<u8>>,
    }

    impl Dir {
        fn add(&mut self, path: &str, data: Vec<u8>) {
            match path.split_once('/') {
                Some((d, rest)) => self.dirs.entry(d.to_string()).or_default().add(rest, data),
                None => {
                    self.files.insert(path.to_string(), data);
                }
            }
        }
    }

    /// `files`: (path with `/`, bytes). `label`: the volume identifier. `sector`: 2048, or 2352 for a raw Mode 1 image.
    pub fn image(label: &str, files: &[(&str, &[u8])], joliet: bool, sector: usize) -> Vec<u8> {
        let mut root = Dir::default();
        for (p, d) in files {
            root.add(p, d.to_vec());
        }
        let mut img = Img { blocks: vec![vec![0; 2048]; 16], joliet: false };
        // descriptors: 16 = primary, 17 = Joliet (optional), then the terminator
        let n_vd = if joliet { 3 } else { 2 };
        for _ in 0..n_vd {
            img.blocks.push(vec![0; 2048]);
        }
        let file_lbas = img.place_files(&root);
        let (proot, psize) = img.place_dir(&root, None, &file_lbas, "");
        let jroot = joliet.then(|| {
            img.joliet = true;
            let r = img.place_dir(&root, None, &file_lbas, "");
            img.joliet = false;
            r
        });
        let total = img.blocks.len() as u32;
        img.blocks[16] = descriptor(1, label, total, proot, psize, false);
        if let Some((jr, js)) = jroot {
            img.blocks[17] = descriptor(2, label, total, jr, js, true);
        }
        let mut term = vec![0u8; 2048];
        term[0] = 255;
        term[1..6].copy_from_slice(b"CD001");
        term[6] = 1;
        img.blocks[16 + n_vd - 1] = term;
        match sector {
            2048 => img.blocks.concat(),
            _ => img
                .blocks
                .iter()
                .flat_map(|b| {
                    let mut s = vec![0u8; sector];
                    s[1..11].fill(0xff);
                    s[15] = 1;
                    s[16..16 + 2048].copy_from_slice(b);
                    s
                })
                .collect(),
        }
    }

    struct Img {
        blocks: Vec<Vec<u8>>,
        joliet: bool,
    }

    impl Img {
        /// The file contents, each in its own blocks; key = full path.
        fn place_files(&mut self, root: &Dir) -> BTreeMap<String, (u32, u32)> {
            let mut out = BTreeMap::new();
            fn walk(img: &mut Img, d: &Dir, pre: &str, out: &mut BTreeMap<String, (u32, u32)>) {
                for (n, data) in &d.files {
                    let lba = img.blocks.len() as u32;
                    for c in data.chunks(2048) {
                        let mut b = c.to_vec();
                        b.resize(2048, 0);
                        img.blocks.push(b);
                    }
                    out.insert(format!("{pre}{n}"), (lba, data.len() as u32));
                }
                for (n, sub) in &d.dirs {
                    walk(img, sub, &format!("{pre}{n}/"), out);
                }
            }
            walk(self, root, "", &mut out);
            out
        }

        fn ident(&self, name: &str, dir: bool) -> Vec<u8> {
            if self.joliet {
                let n = if dir { name.to_string() } else { format!("{name};1") };
                n.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
            } else {
                let up = name.to_ascii_uppercase();
                if dir {
                    up.into_bytes()
                } else if up.contains('.') {
                    format!("{up};1").into_bytes()
                } else {
                    format!("{up}.;1").into_bytes()
                }
            }
        }

        /// Place directory `d` (children first), returning (lba, size) of its extent.
        fn place_dir(
            &mut self,
            d: &Dir,
            parent: Option<(u32, u32)>,
            files: &BTreeMap<String, (u32, u32)>,
            pre: &str,
        ) -> (u32, u32) {
            // the extent's size depends only on the record lengths: compute it first, reserve, then fill
            let mut recs: Vec<(Vec<u8>, bool)> = Vec::new();
            for n in d.dirs.keys() {
                recs.push((self.ident(n, true), true));
            }
            for n in d.files.keys() {
                recs.push((self.ident(n, false), false));
            }
            let rec_len = |id: &[u8]| (33 + id.len() + 1) & !1;
            let mut size = 0usize;
            let mut used = 2 * 34;
            for (id, _) in &recs {
                let l = rec_len(id);
                if used + l > 2048 {
                    size += 2048;
                    used = 0;
                }
                used += l;
            }
            size += 2048;
            let lba = self.blocks.len() as u32;
            for _ in 0..size / 2048 {
                self.blocks.push(vec![0; 2048]);
            }
            let me = (lba, size as u32);
            let parent = parent.unwrap_or(me);
            // the sub-directories' own extents
            let mut sub_at = BTreeMap::new();
            for (n, sub) in &d.dirs {
                sub_at.insert(n.clone(), self.place_dir(sub, Some(me), files, &format!("{pre}{n}/")));
            }
            let mut raw = Vec::new();
            let push = |raw: &mut Vec<u8>, r: Vec<u8>| {
                if raw.len() % 2048 + r.len() > 2048 {
                    raw.resize((raw.len() / 2048 + 1) * 2048, 0);
                }
                raw.extend(r);
            };
            push(&mut raw, record(&[0], me.0, me.1, true));
            push(&mut raw, record(&[1], parent.0, parent.1, true));
            let mut i = 0;
            for n in d.dirs.keys() {
                let (l, s) = sub_at[n];
                push(&mut raw, record(&recs[i].0, l, s, true));
                i += 1;
            }
            for n in d.files.keys() {
                let (l, s) = files[&format!("{pre}{n}")];
                push(&mut raw, record(&recs[i].0, l, s, false));
                i += 1;
            }
            raw.resize(size, 0);
            for (k, c) in raw.chunks(2048).enumerate() {
                self.blocks[lba as usize + k] = c.to_vec();
            }
            me
        }
    }

    fn both32(v: u32) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[..4].copy_from_slice(&v.to_le_bytes());
        b[4..].copy_from_slice(&v.to_be_bytes());
        b
    }

    fn record(id: &[u8], lba: u32, size: u32, dir: bool) -> Vec<u8> {
        let len = (33 + id.len() + 1) & !1;
        let mut r = vec![0u8; len];
        r[0] = len as u8;
        r[2..10].copy_from_slice(&both32(lba));
        r[10..18].copy_from_slice(&both32(size));
        r[25] = if dir { 2 } else { 0 };
        r[28..32].copy_from_slice(&[1, 0, 0, 1]);
        r[32] = id.len() as u8;
        r[33..33 + id.len()].copy_from_slice(id);
        r
    }

    fn descriptor(kind: u8, label: &str, total: u32, root: u32, root_size: u32, joliet: bool) -> Vec<u8> {
        let mut d = vec![0u8; 2048];
        d[0] = kind;
        d[1..6].copy_from_slice(b"CD001");
        d[6] = 1;
        d[8..40].fill(b' ');
        d[40..72].fill(b' ');
        d[40..40 + label.len().min(32)].copy_from_slice(&label.as_bytes()[..label.len().min(32)]);
        d[80..88].copy_from_slice(&both32(total));
        if joliet {
            d[88..91].copy_from_slice(b"%/E");
        }
        d[120..124].copy_from_slice(&[1, 0, 0, 1]);
        d[124..128].copy_from_slice(&[1, 0, 0, 1]);
        d[128..132].copy_from_slice(&[0, 8, 8, 0]);
        d[156..190].copy_from_slice(&record(&[0], root, root_size, true));
        d[881] = 1;
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("xf-iso-{tag}-{}.iso", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn files() -> Vec<(&'static str, Vec<u8>)> {
        let big: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
        vec![
            ("XFiles.hdb", b"hdb!".to_vec()),
            ("XV/12345.xmv", big),
            ("XV/12345.HOT", b"HSPT".to_vec()),
            ("A/B/c.txt", vec![]),
        ]
    }

    fn check(img: &Image, joliet: bool) {
        assert_eq!(img.label, "XF_TEST");
        assert_eq!(img.joliet, joliet);
        let r = img.root();
        let h = img.lookup(r, Path::new("xfiles.HDB")).unwrap();
        assert_eq!(img.read(h).unwrap(), b"hdb!");
        if joliet {
            assert_eq!(img.node(h).name, "XFiles.hdb");
        } else {
            assert_eq!(img.node(h).name, "XFILES.HDB");
        }
        let m = img.lookup(r, Path::new("xv/12345.XMV")).unwrap();
        let want = &files()[1].1;
        assert_eq!(img.node(m).size, 5000);
        assert_eq!(img.read(m).unwrap(), *want);
        let mut b = vec![0; 100];
        // a range across a block boundary, and one at the end of the file
        assert_eq!(img.read_at(m, 2000, &mut b).unwrap(), 100);
        assert_eq!(b, want[2000..2100]);
        assert_eq!(img.read_at(m, 4950, &mut b).unwrap(), 50);
        assert_eq!(b[..50], want[4950..]);
        let xv = img.lookup(r, Path::new("XV")).unwrap();
        assert!(img.node(xv).is_dir);
        assert_eq!(img.node(xv).children.len(), 2);
        let c = img.lookup(r, Path::new("a/b/C.TXT")).unwrap();
        assert_eq!(img.read(c).unwrap(), b"");
        assert!(img.lookup(r, Path::new("nope")).is_none());
        assert!(img.lookup(r, Path::new("../XV")).is_none());
    }

    #[test]
    fn synthetic_images() {
        let f = files();
        let fs: Vec<(&str, &[u8])> = f.iter().map(|(p, d)| (*p, d.as_slice())).collect();
        for (joliet, sector) in [(false, 2048), (true, 2048), (true, 2352), (false, 2352)] {
            let p = tmp(&format!("{joliet}-{sector}"), &build::image("XF_TEST", &fs, joliet, sector));
            assert!(is_image(&p));
            let img = Image::open(&p).unwrap();
            check(&img, joliet);
            let _ = std::fs::remove_file(&p);
        }
        let p = tmp("not", b"plain bytes");
        assert!(!is_image(&p) && Image::open(&p).is_err());
        let _ = std::fs::remove_file(&p);
    }

    /// A directory whose records fill more than one block.
    #[test]
    fn large_directory() {
        let names: Vec<String> = (0..300).map(|i| format!("XV/{:05}.xmv", i)).collect();
        let fs: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), n.as_bytes())).collect();
        let p = tmp("large", &build::image("XF_TEST", &fs, false, 2048));
        let img = Image::open(&p).unwrap();
        let xv = img.lookup(img.root(), Path::new("XV")).unwrap();
        assert_eq!(img.node(xv).children.len(), 300);
        let f = img.lookup(img.root(), Path::new("xv/00299.XMV")).unwrap();
        assert_eq!(img.read(f).unwrap(), b"XV/00299.xmv");
        let _ = std::fs::remove_file(&p);
    }
}
