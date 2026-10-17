//! Win32 PE resources: string tables (RT_STRING = 6) and monochrome cursors (RT_GROUP_CURSOR = 12 / RT_CURSOR = 1).
//!
//! HYPOTHESIS H260 (static): an AssetRef of kind 0 (f40 & 7 == 0) is a string resource:
//! the asset loader (FUN_004553f0) reads the blob's two Pascal strings (resource type "6" = RT_STRING, id) and loads
//! the string with LoadStringA from the resource module the game opened at startup, the one it checks to be
//! "XfilesT.dll". E.g. AssetRef 0xe231 = blob "6" / "937" = XfilesT.dll string 937 (PswdConnect's user name).
use std::collections::HashMap;
use std::path::Path;

fn u16le(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32le(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

/// The resource section of a PE image: (file offset of the root directory, rva → file offset map).
struct Rsrc<'a> {
    d: &'a [u8],
    base: usize,
    secs: Vec<(usize, usize, usize)>,
}

impl<'a> Rsrc<'a> {
    fn new(d: &'a [u8]) -> Option<Rsrc<'a>> {
        let pe = u32le(d, 0x3c)? as usize;
        let nsec = u16le(d, pe + 6)? as usize;
        let opt = u16le(d, pe + 20)? as usize;
        // optional header: data directory 2 (resources) at +96 (PE32)
        let rsrc_rva = u32le(d, pe + 24 + 96 + 16)? as usize;
        let mut secs = Vec::new();
        for i in 0..nsec {
            let s = pe + 24 + opt + 40 * i;
            let (vs, va, rs, ro) = (u32le(d, s + 8)?, u32le(d, s + 12)?, u32le(d, s + 16)?, u32le(d, s + 20)?);
            secs.push((va as usize, vs.max(rs) as usize, ro as usize));
        }
        let mut r = Rsrc { d, base: 0, secs };
        r.base = r.off(rsrc_rva)?;
        Some(r)
    }
    fn off(&self, rva: usize) -> Option<usize> {
        self.secs.iter().find(|&&(va, sz, _)| va <= rva && rva < va + sz).map(|&(va, _, ro)| rva - va + ro)
    }
    /// (name/id, offset) entries of the directory at `p`
    fn entries(&self, p: usize) -> Vec<(u32, u32)> {
        let d = self.d;
        let n = u16le(d, p + 12).unwrap_or(0) as usize + u16le(d, p + 14).unwrap_or(0) as usize;
        (0..n).filter_map(|i| Some((u32le(d, p + 16 + 8 * i)?, u32le(d, p + 20 + 8 * i)?))).collect()
    }
    fn sub(&self, o: u32) -> usize {
        self.base + (o & 0x7fff_ffff) as usize
    }
    /// The data of resource (type, id), first language found
    fn data(&self, ty: u32, id: u32) -> Option<&'a [u8]> {
        let (_, o1) = self.entries(self.base).into_iter().find(|&(t, o)| t == ty && o & 0x8000_0000 != 0)?;
        let (_, o2) = self.entries(self.sub(o1)).into_iter().find(|&(i, o)| i == id && o & 0x8000_0000 != 0)?;
        let &(_, o3) = self.entries(self.sub(o2)).first()?;
        let leaf = self.base + o3 as usize;
        let (rva, size) = (u32le(self.d, leaf)? as usize, u32le(self.d, leaf + 4)? as usize);
        let p = self.off(rva)?;
        self.d.get(p..p + size)
    }
}

/// Every RT_STRING entry of a PE image: id → text (UTF-16 decoded; the first language found per block).
pub fn string_table(d: &[u8]) -> Option<HashMap<u32, String>> {
    let r = Rsrc::new(d)?;
    let mut out = HashMap::new();
    for (ty, o1) in r.entries(r.base) {
        if ty != 6 || o1 & 0x8000_0000 == 0 {
            continue;
        }
        for (block, o2) in r.entries(r.sub(o1)) {
            let Some(&(_, o3)) = r.entries(r.sub(o2)).first() else { continue };
            let leaf = r.base + o3 as usize;
            let Some(mut p) = u32le(d, leaf).and_then(|x| r.off(x as usize)) else { continue };
            for k in 0..16u32 {
                let Some(n) = u16le(d, p) else { break };
                p += 2;
                let n = n as usize;
                if n > 0 {
                    let w: Vec<u16> = (0..n).filter_map(|i| u16le(d, p + 2 * i)).collect();
                    out.insert((block - 1) * 16 + k, String::from_utf16_lossy(&w));
                }
                p += 2 * n;
            }
        }
    }
    Some(out)
}

/// A monochrome Win32 cursor (RT_CURSOR): per pixel the AND mask and the XOR bit, row-major top-down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WinCursor {
    pub width: usize,
    pub height: usize,
    pub hot: (i32, i32),
    pub and: Vec<bool>,
    pub xor: Vec<bool>,
}

/// The cursor of group `group_id` (RT_GROUP_CURSOR = 12) in a PE image: the group's entry of width `want` (else the first),
/// its RT_CURSOR (1) data = u16 hotspot x, y + a 1-bit DIB (height = 2 × the cursor's: XOR rows then AND rows, bottom-up).
pub fn cursor(d: &[u8], group_id: u32, want: usize) -> Option<WinCursor> {
    let r = Rsrc::new(d)?;
    let g = r.data(12, group_id)?;
    let n = u16le(g, 4)? as usize;
    let pick = (0..n).find(|&i| u16le(g, 6 + 14 * i) == Some(want as u16)).unwrap_or(0);
    let cid = u16le(g, 6 + 14 * pick + 12)? as u32;
    let c = r.data(1, cid)?;
    let hot = (u16le(c, 0)? as i32, u16le(c, 2)? as i32);
    let bi = c.get(4..)?;
    let hs = u32le(bi, 0)? as usize;
    let w = u32le(bi, 4)? as usize;
    let h = u32le(bi, 8)? as usize / 2;
    if u16le(bi, 14)? != 1 {
        return None; // colour cursors are not used by the game
    }
    let stride = w.div_ceil(32) * 4;
    let xor0 = hs + 8; // 2-entry palette
    let and0 = xor0 + stride * h;
    let bit = |o: usize, x: usize, y: usize| -> Option<bool> { Some(bi.get(o + y * stride + x / 8)? >> (7 - x % 8) & 1 == 1) };
    let (mut and, mut xor) = (Vec::with_capacity(w * h), Vec::with_capacity(w * h));
    for row in 0..h {
        let y = h - 1 - row; // bottom-up
        for x in 0..w {
            and.push(bit(and0, x, y)?);
            xor.push(bit(xor0, x, y)?);
        }
    }
    Some(WinCursor { width: w, height: h, hot, and, xor })
}

/// The string table of `XfilesT.dll` next to the HDB (file name matched case-insensitively); empty if absent.
pub fn text_dll_strings(dir: &Path) -> HashMap<u32, String> {
    let Ok(rd) = crate::io::read_dir(dir) else { return HashMap::new() };
    for e in rd {
        if e.name.eq_ignore_ascii_case("XfilesT.dll") {
            if let Some(t) = crate::io::read(&e.path).ok().and_then(|d| string_table(&d)) {
                return t;
            }
        }
    }
    HashMap::new()
}

#[cfg(test)]
mod tests {
    #[test]
    fn xfilest_string_937() {
        let d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
        if !d.join("XFiles.hdb").exists() {
            return;
        }
        let t = super::text_dll_strings(&d);
        // the login's user name (compared by FNV-1a 64 so the game's text is not spelled out)
        let fnv = |s: &str| s.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
        assert_eq!(t.get(&937).map(|s| fnv(s)), Some(0x5d13_aa09_53d3_c454));
    }

    /// H331: XFiles.exe cursor group 0x81 = a small crosshair (hot spot (7,7)), group 0x84 = a gunsight (hot (16,15))
    #[test]
    fn xfiles_exe_cursors() {
        let d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame/XFiles.exe");
        let Ok(b) = std::fs::read(d) else { return };
        let c = super::cursor(&b, 0x81, 32).expect("group 0x81");
        assert_eq!((c.width, c.height, c.hot), (32, 32, (7, 7)));
        // the white vertical bar through the hot spot, outlined black; centre pixel black
        let at = |x: usize, y: usize| (c.and[y * 32 + x], c.xor[y * 32 + x]);
        assert_eq!(at(7, 4), (false, true));
        assert_eq!(at(6, 4), (false, false));
        assert_eq!(at(7, 7), (false, false));
        assert_eq!(at(20, 20), (true, false));
        assert_eq!(c.and.iter().filter(|&&a| !a).count(), 33);
        let g = super::cursor(&b, 0x84, 32).expect("group 0x84");
        assert_eq!(g.hot, (16, 15));
    }
}
