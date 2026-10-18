//! Reader for the installer archive of a Gentee CreateInstall setup program (`Setup.exe`). Port feature: on a disc
//! where the game's own files (XFiles.hdb, XFiles.gam, XFiles.exe, the DLLs, the fonts) exist only inside the
//! installer, [`crate::vfs`] reads them straight out of it, in memory, with nothing installed and nothing written.
//!
//! Layout, as found on such a disc (CreateInstall of 2008; worked out from the setup program's loader and checked by
//! decoding every entry of the archive; the PPMd coder itself follows the Gentee open source project, see
//! [`crate::ppmd`]):
//!
//! - The setup program is a small loader (a PE file) followed by its packed runtime DLL and the archive. The archive
//!   starts with the 31-byte header: `GEA\0`, u16 header size (31), u32, u16, u32 archive size (from `GEA`), three
//!   bytes, u32 *moved* size, u32 data size, u32 volume size (-1 = one volume). All numbers little-endian.
//! - After the header: the file list, one PPMd stream (below). The data area takes the last *data size* bytes of
//!   the archive.
//! - The data area holds the entries' packed data back to back in list order (the *logical* stream), except that
//!   its last *moved* bytes are stored first: logical offset L is at L + moved when L < data − moved, else at
//!   L − (data − moved).
//! - File list entry: u16 entry size, u32 packed size, u32 group, u16, u8, u16 size of the rest, u32 file size, u8,
//!   u32 attributes, u64 FILETIME, then tagged fields: 0x02 + 8 bytes (version), 0x04 + the name (NUL-terminated,
//!   `\` separates folders), which ends the entry.
//! - Packed data of an entry: byte 0 = 0: stored, u32 size + the bytes. Else a PPMd stream: 4 bytes `memory (MB)`,
//!   `order`, `block size (× 102400 bytes)`, `flags` (bit 0: blocks after the first continue the model, bit 1: one
//!   block, bit 2: a fifth byte = the number of blocks); then blocks, each u32 length (bit 31 = stored as is) + data;
//!   without a block count the stream ends at a zero length.
use crate::ppmd::Model;
use std::io;
use std::sync::{Arc, Mutex};

/// Random-access bytes: a file on disk, or a file inside a disc image.
#[allow(clippy::len_without_is_empty)]
pub trait Source: Send + Sync {
    fn len(&self) -> u64;
    /// Up to `buf.len()` bytes at `off`; the count read (short only at the end).
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize>;
}

/// A file on disk as a [`Source`].
pub struct FileSource {
    file: Mutex<std::fs::File>,
    len: u64,
}

impl FileSource {
    pub fn open(p: &std::path::Path) -> io::Result<FileSource> {
        let file = std::fs::File::open(p)?;
        let len = file.metadata()?.len();
        Ok(FileSource { file: Mutex::new(file), len })
    }
}

impl Source for FileSource {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
        f.seek(SeekFrom::Start(off))?;
        let mut n = 0;
        while n < buf.len() {
            match f.read(&mut buf[n..])? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    }
}

/// A file of a disc image as a [`Source`].
pub struct IsoSource(pub Arc<crate::iso::Image>, pub usize);

impl Source for IsoSource {
    fn len(&self) -> u64 {
        self.0.node(self.1).size
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read_at(self.1, off, buf)
    }
}

/// Bytes in memory as a [`Source`] (tests).
impl Source for Vec<u8> {
    fn len(&self) -> u64 {
        self.as_slice().len() as u64
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        let s = self.as_slice();
        let o = (off as usize).min(s.len());
        let n = buf.len().min(s.len() - o);
        buf[..n].copy_from_slice(&s[o..o + n]);
        Ok(n)
    }
}

fn bad(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// One file of the archive.
#[derive(Clone, Debug)]
pub struct Entry {
    /// path inside the archive, folders separated by `/`
    pub name: String,
    pub size: u64,
    /// packed size and offset in the logical data stream
    pub packed: u64,
    pub offset: u64,
    pub group: u32,
    /// whether the data is stored as is (no decoding needed)
    pub stored: bool,
}

/// An opened installer archive.
pub struct Setup {
    src: Box<dyn Source>,
    /// where the data area starts in the source
    data_at: u64,
    data_size: u64,
    moved: u64,
    pub entries: Vec<Entry>,
    /// the last decoded (not stored) entry: index and bytes
    cache: Mutex<Option<(usize, Arc<Vec<u8>>)>>,
}

/// How far into the setup program the archive header is looked for (the loader and its packed DLL take ~64 KB).
const SCAN: u64 = 4 << 20;

/// Decode a PPMd stream (props + blocks) of at most `limit` bytes from `input`; returns the bytes and the length of
/// the stream read.
pub fn decode_stream(input: &[u8], limit: usize) -> io::Result<(Vec<u8>, usize)> {
    if input.len() < 4 {
        return Err(bad("installer stream: too short"));
    }
    let (mem, order, blk, flags) = (input[0] as u32, input[1] as u32, input[2] as usize, input[3]);
    if mem == 0 || mem > 256 || !(2..=16).contains(&order) {
        return Err(bad(format!("installer stream: unknown parameters {:02x?}", &input[..4])));
    }
    let mut at = 4;
    let count: Option<u32> = if flags & 2 != 0 {
        Some(1)
    } else if flags & 4 != 0 {
        at = 5;
        Some(*input.get(4).ok_or_else(|| bad("installer stream: too short"))? as u32)
    } else {
        None
    };
    let block_limit = blk * 102400 + 16;
    let mut model: Option<Model> = None;
    let mut out = Vec::new();
    let mut n = 0u32;
    let mut prev_stored = false;
    loop {
        if count.is_some_and(|c| n >= c) || out.len() >= limit {
            break;
        }
        let hdr = input.get(at..at + 4).ok_or_else(|| bad("installer stream: cut short"))?;
        let v = le32(hdr, 0);
        if count.is_none() && v == 0 {
            at += 4;
            break;
        }
        let len = (v & 0x7FFF_FFFF) as usize;
        let data = input.get(at + 4..at + 4 + len).ok_or_else(|| bad("installer stream: cut short"))?;
        at += 4 + len;
        if v & 0x8000_0000 != 0 {
            out.extend_from_slice(data);
            prev_stored = true;
        } else {
            let m = model.get_or_insert_with(|| Model::new(mem));
            let o = if n > 0 && flags & 1 != 0 && !prev_stored { 1 } else { order };
            let b = m.decode(data, o, block_limit).map_err(|e| bad(format!("installer stream: {e}")))?;
            out.extend_from_slice(&b);
            prev_stored = false;
        }
        n += 1;
    }
    out.truncate(limit);
    Ok((out, at))
}

/// Parse the decoded file list.
fn parse_list(b: &[u8]) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut o = 0usize;
    let mut offset = 0u64;
    while o + 2 <= b.len() {
        let n = le16(b, o) as usize;
        if n == 0 {
            break;
        }
        let r = b.get(o..o + n).ok_or_else(|| bad("installer file list: entry cut short"))?;
        if n < 33 {
            return Err(bad("installer file list: entry too short"));
        }
        let packed = le32(r, 2) as u64;
        let group = le32(r, 6);
        let size = le32(r, 15) as u64;
        // tagged fields after the FILETIME
        let mut t = 32;
        let mut name = None;
        while t < n {
            match r[t] {
                0x02 => t += 9,
                0x04 => {
                    let s = &r[t + 1..];
                    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                    // names are in the installer's ANSI code page; game file names are ASCII
                    name = Some(s[..end].iter().map(|&c| c as char).collect::<String>().replace('\\', "/"));
                    break;
                }
                _ => break,
            }
        }
        let name = name.ok_or_else(|| bad("installer file list: entry without a name"))?;
        out.push(Entry { name, size, packed, offset, group, stored: false });
        offset += packed;
        o += n;
    }
    Ok(out)
}

impl Setup {
    /// Open the archive of a setup program; Err when `src` is not a Gentee setup program with an archive.
    pub fn open(src: Box<dyn Source>) -> io::Result<Setup> {
        let len = src.len();
        let mut mz = [0u8; 2];
        if src.read_at(0, &mut mz)? < 2 || &mz != b"MZ" {
            return Err(bad("not a Windows program"));
        }
        let mut head = vec![0u8; SCAN.min(len) as usize];
        let got = src.read_at(0, &mut head)?;
        head.truncate(got);
        // the archive header: `GEA\0`, header size 31, its archive size reaching the end of the file
        let mut at = None;
        let mut i = 0;
        while let Some(p) = head[i..].windows(4).position(|w| w == b"GEA\0") {
            let o = i + p;
            if o + 31 <= head.len() && le16(&head, o + 4) == 31 {
                let total = le32(&head, o + 12) as u64;
                let data = le32(&head, o + 23) as u64;
                if o as u64 + total == len && data < total {
                    at = Some(o);
                    break;
                }
            }
            i = o + 1;
        }
        let at = at.ok_or_else(|| bad("no installer archive (not a Gentee setup program, or a multi-volume one)"))?;
        let h = &head[at..at + 31];
        let total = le32(h, 12) as u64;
        let moved = le32(h, 19) as u64;
        let data_size = le32(h, 23) as u64;
        if moved > data_size {
            return Err(bad("installer archive: bad header"));
        }
        let list_at = at as u64 + 31;
        let list_len = (total - data_size).saturating_sub(31) as usize;
        let mut lb = vec![0u8; list_len];
        if src.read_at(list_at, &mut lb)? < list_len {
            return Err(bad("installer archive: file list cut short"));
        }
        let (list, _) = decode_stream(&lb, 16 << 20)?;
        let mut entries = parse_list(&list)?;
        let sum: u64 = entries.iter().map(|e| e.packed).sum();
        if sum != data_size {
            return Err(bad(format!("installer archive: the entries take {sum} bytes, the data area {data_size}")));
        }
        let mut s = Setup {
            src,
            data_at: at as u64 + total - data_size,
            data_size,
            moved,
            entries: Vec::new(),
            cache: Mutex::new(None),
        };
        for e in &mut entries {
            let mut b = [0u8; 5];
            s.read_logical(e.offset, &mut b)?;
            e.stored = b[0] == 0;
            if e.stored && (le32(&b, 1) as u64 != e.size || e.packed != e.size + 5) {
                return Err(bad(format!("installer archive: {}: stored size does not match", e.name)));
            }
        }
        s.entries = entries;
        Ok(s)
    }

    /// Bytes of the logical data stream at `l` (crossing the moved part as needed).
    fn read_logical(&self, mut l: u64, buf: &mut [u8]) -> io::Result<usize> {
        let split = self.data_size - self.moved;
        let mut done = 0;
        while done < buf.len() && l < self.data_size {
            let (phys, room) = if l < split { (l + self.moved, split - l) } else { (l - split, self.data_size - l) };
            let n = (room as usize).min(buf.len() - done);
            let k = self.src.read_at(self.data_at + phys, &mut buf[done..done + n])?;
            done += k;
            l += k as u64;
            if k < n {
                break;
            }
        }
        Ok(done)
    }

    /// The entry index of `name` (folders by `/`, letter case ignored).
    pub fn find(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.name.eq_ignore_ascii_case(name))
    }

    fn decoded(&self, i: usize) -> io::Result<Arc<Vec<u8>>> {
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((ci, b)) = c.as_ref() {
            if *ci == i {
                return Ok(b.clone());
            }
        }
        let e = &self.entries[i];
        let mut raw = vec![0u8; e.packed as usize];
        if self.read_logical(e.offset, &mut raw)? < raw.len() {
            return Err(bad(format!("installer archive: {}: data cut short", e.name)));
        }
        let (b, _) = decode_stream(&raw, e.size as usize)?;
        if b.len() as u64 != e.size {
            return Err(bad(format!("installer archive: {}: decoded {} of {} bytes", e.name, b.len(), e.size)));
        }
        let b = Arc::new(b);
        *c = Some((i, b.clone()));
        Ok(b)
    }

    /// Up to `buf.len()` bytes of entry `i` at `off`.
    pub fn read_at(&self, i: usize, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        let e = &self.entries[i];
        if off >= e.size {
            return Ok(0);
        }
        let n = ((e.size - off) as usize).min(buf.len());
        if e.stored {
            return self.read_logical(e.offset + 5 + off, &mut buf[..n]);
        }
        let b = self.decoded(i)?;
        buf[..n].copy_from_slice(&b[off as usize..off as usize + n]);
        Ok(n)
    }

    /// The whole entry `i`.
    pub fn read(&self, i: usize) -> io::Result<Vec<u8>> {
        let mut b = vec![0u8; self.entries[i].size as usize];
        let n = self.read_at(i, 0, &mut b)?;
        if n < b.len() {
            return Err(bad(format!("installer archive: {}: data cut short", self.entries[i].name)));
        }
        Ok(b)
    }
}

/// Builder of small setup programs in memory, in the layout described at the top (tests and examples): `files` =
/// (name, group, bytes, packed with PPMd); `moved` = how many bytes of the data stream's end go first.
pub fn build(files: &[(&str, u32, &[u8], bool)], moved: usize) -> Vec<u8> {
    use crate::ppmd::encode;
    let ppmd = |b: &[u8], mem: u8, order: u8, flags: u8| {
        let mut m = Model::new(mem as u32);
        let p = encode(&mut m, b, order as u32);
        let mut v = vec![mem, order, 20, flags];
        v.extend_from_slice(&(p.len() as u32).to_le_bytes());
        v.extend(p);
        v
    };
    let mut list = Vec::new();
    let mut stream = Vec::new();
    for (name, group, data, pack) in files {
        let packed = if *pack {
            ppmd(data, 1, 6, 2)
        } else {
            let mut v = vec![0];
            v.extend_from_slice(&(data.len() as u32).to_le_bytes());
            v.extend_from_slice(data);
            v
        };
        let mut rest = Vec::new();
        rest.extend_from_slice(&(data.len() as u32).to_le_bytes());
        rest.push(1);
        rest.extend_from_slice(&0x20u32.to_le_bytes());
        rest.extend_from_slice(&0x01c7_e0c9_d7fb_cc00u64.to_le_bytes());
        if name.ends_with(".exe") {
            rest.push(2);
            rest.extend_from_slice(&[0, 0, 1, 0, 0, 0, 3, 0]);
        }
        rest.push(4);
        rest.extend_from_slice(name.replace('/', "\\").as_bytes());
        rest.push(0);
        let mut e = Vec::new();
        e.extend_from_slice(&((13 + 2 + rest.len()) as u16).to_le_bytes());
        e.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        e.extend_from_slice(&group.to_le_bytes());
        e.extend_from_slice(&1u16.to_le_bytes());
        e.push(0xFF);
        e.extend_from_slice(&((rest.len() + 2) as u16).to_le_bytes());
        e.extend(rest);
        list.extend(e);
        stream.extend(packed);
    }
    let list = ppmd(&list, 1, 4, 3);
    let moved = moved.min(stream.len());
    let split = stream.len() - moved;
    let mut data = stream[split..].to_vec();
    data.extend_from_slice(&stream[..split]);
    let total = 31 + list.len() + data.len();
    let mut out = b"MZ".to_vec();
    out.resize(0x1234, 0x90);
    out.extend_from_slice(b"GEA\0");
    out.extend_from_slice(&31u16.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&[1, 1, 0]);
    out.extend_from_slice(&(moved as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend(list);
    out.extend(data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ppmd::tests::sample;

    #[test]
    fn synthetic_setup() {
        let hdb = sample(70_000, 11);
        let exe = sample(30_000, 12);
        let bmp = sample(40_000, 13);
        let files: [(&str, u32, &[u8], bool); 4] = [
            ("XFiles.hdb", 1, &hdb, false),
            ("Fonts/Jrn.ttr", 1, b"font", true),
            ("XFiles.exe", 1, &exe, true),
            ("SETUP.BMP", 2, &bmp, true),
        ];
        // the moved part: none, inside SETUP.BMP, into XFiles.hdb (stored, read across the cut), everything
        for moved in [0, 1000, 75_000, usize::MAX] {
            let s = Setup::open(Box::new(build(&files, moved))).unwrap();
            assert_eq!(s.entries.len(), 4);
            for (i, (name, group, data, pack)) in files.iter().enumerate() {
                let e = &s.entries[i];
                assert_eq!((e.name.as_str(), e.group, e.size, e.stored), (*name, *group, data.len() as u64, !pack));
                assert!(s.read(i).unwrap() == *data, "{name} moved {moved}");
            }
            let x = s.find("xfiles.EXE").unwrap();
            let mut b = vec![0u8; 100];
            assert_eq!(s.read_at(x, 29_950, &mut b).unwrap(), 50);
            assert_eq!(b[..50], exe[29_950..]);
            let h = s.find("XFILES.HDB").unwrap();
            assert_eq!(s.read_at(h, 69_000, &mut b).unwrap(), 100);
            assert_eq!(b[..], hdb[69_000..69_100]);
        }
    }

    #[test]
    fn not_a_setup() {
        assert!(Setup::open(Box::new(b"plain bytes".to_vec())).is_err());
        let mut mz = b"MZ".to_vec();
        mz.resize(5000, 0);
        assert!(Setup::open(Box::new(mz)).is_err());
        // a damaged file list
        let mut s = build(&[("XFiles.hdb", 1, b"hdb", false)], 0);
        let at = s.windows(4).position(|w| w == b"GEA\0").unwrap() + 31 + 8;
        s[at + 2] ^= 0x55;
        s[at + 5] ^= 0xAA;
        assert!(Setup::open(Box::new(s)).is_err());
    }

    #[test]
    fn stream_blocks() {
        // solid stream (flag bit 0) without a block count: packed, packed continuing the model, stored, packed
        // with a new model (a stored block ends the model chain); ends at a zero length
        let (a, b, c) = (sample(5000, 21), sample(5000, 22), sample(5000, 23));
        let enc = crate::ppmd::encode;
        let mut m = Model::new(1);
        let pa = enc(&mut m, &a, 5);
        let pb = enc(&mut m, &b, 1);
        let pc = enc(&mut Model::new(1), &c, 5);
        let mut v = vec![1u8, 5, 1, 1];
        for p in [&pa, &pb] {
            v.extend_from_slice(&(p.len() as u32).to_le_bytes());
            v.extend(p);
        }
        v.extend_from_slice(&(0x8000_0003u32).to_le_bytes());
        v.extend_from_slice(b"abc");
        v.extend_from_slice(&(pc.len() as u32).to_le_bytes());
        v.extend(&pc);
        v.extend_from_slice(&0u32.to_le_bytes());
        let (out, used) = decode_stream(&v, 1 << 20).unwrap();
        assert_eq!(used, v.len());
        let want = [&a[..], &b[..], b"abc", &c[..]].concat();
        assert!(out == want);
    }
}
