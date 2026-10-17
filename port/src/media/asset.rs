//! VCAssetRef `asset` blob (HDB field f28) → file references.
//!
//! Layout determined from the original's asset-entry chooser (FUN_00450220, static) and its token
//! readers:
//! - `pstr`: `[u8 n][n bytes]`;
//! - `num`: a `pstr` holding a decimal number;
//! - `lstr`: `[pstr = decimal length L][L bytes]`;
//! - body: `lstr path`; if `f40 & 7 == 3`: `num, num`; `lstr`; then while bytes remain:
//!   `num variant, lstr file, num start, num end`.
//!
//! The chooser returns the entry whose variant == the requested one (`-1` = the first entry); the clip lookup asks
//! for `-1` when the install mode (NAV1.NMV present → 2) is 2 and the asset is not of the two special kinds (not
//! decoded). Path separators inside names are 0x7f.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetEntry {
    pub variant: u32,
    /// file name relative to the install dir, 0x7f replaced by '/'
    pub file: String,
    /// start / end (milliseconds of movie time — HYPOTHESIS: fits every FO asset seen: navm.nmv still frames
    /// at whole seconds (R17/H17), XN loop 7000..7090 inside a 7.3 s movie)
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetBlob {
    /// authoring path (e.g. `Navs/Field OfficeR/02/014-Fo-2n_door_closed.mov`)
    pub path: String,
    /// the two numbers present when `f40 & 7 == 3`
    pub range: Option<(u32, u32)>,
    pub entries: Vec<AssetEntry>,
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn pstr(&mut self) -> Option<&'a [u8]> {
        let n = *self.b.get(self.p)? as usize;
        let s = self.b.get(self.p + 1..self.p + 1 + n)?;
        self.p += 1 + n;
        Some(s)
    }
    fn num(&mut self) -> Option<u32> {
        let s = self.pstr()?;
        if s.is_empty() {
            return Some(0);
        }
        std::str::from_utf8(s).ok()?.trim().parse().ok()
    }
    fn lstr(&mut self) -> Option<String> {
        let n = self.num()? as usize;
        let s = self.b.get(self.p..self.p + n)?;
        self.p += n;
        Some(s.iter().map(|&c| if c == 0x7f { '/' } else { c as char }).collect())
    }
}

impl AssetBlob {
    pub fn parse(blob: &[u8], f40: u32) -> Option<AssetBlob> {
        let mut r = Rd { b: blob, p: 0 };
        let path = r.lstr()?;
        let range = if f40 & 7 == 3 { Some((r.num()?, r.num()?)) } else { None };
        r.lstr()?;
        let mut entries = Vec::new();
        while r.p < blob.len() {
            // a trailing empty token ends the list (e.g. 0x4d64 ends with 00); variant "0" is a real entry (0x8c96)
            if blob[r.p] == 0 {
                break;
            }
            let Some(variant) = r.num() else { break };
            let (Some(file), Some(start), Some(end)) = (r.lstr(), r.num(), r.num()) else { break };
            entries.push(AssetEntry { variant, file, start, end });
        }
        Some(AssetBlob { path, range, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_and_nav_blobs() {
        // 0x4d64 (SC03C.mov), f40 = 1
        let mut b = b"\x0235Video\x7fNode 1\x7fField Office\x7fSC03C.mov\x010\x011\x0212XV\x7f19812.xmv\x010\x010\x00".to_vec();
        let a = AssetBlob::parse(&b, 1).unwrap();
        assert_eq!(a.path, "Video/Node 1/Field Office/SC03C.mov");
        assert_eq!(a.entries, vec![AssetEntry { variant: 1, file: "XV/19812.xmv".into(), start: 0, end: 0 }]);
        // f40 & 7 == 3: two numbers after the path (0x115bc)
        b = b"\x0242Navs\x7fLoopsR\x7fField Office\x7f1n_cookleaves.mov\x047000\x047090\x010\x011\x0212XN\x7f71100.xmv\x047000\x047090".to_vec();
        let a = AssetBlob::parse(&b, 3).unwrap();
        assert_eq!(a.range, Some((7000, 7090)));
        assert_eq!(a.entries[0].file, "XN/71100.xmv");
        assert_eq!((a.entries[0].start, a.entries[0].end), (7000, 7090));
        // variant 0 (0x8c96 cabinetA.mov)
        b = b"\x0238Video\x7fNode 1\x7fField Office\x7fcabinetA.mov\x010\x010\x0212XV\x7f35990.xmv\x010\x010".to_vec();
        let a = AssetBlob::parse(&b, 1).unwrap();
        assert_eq!(a.entries, vec![AssetEntry { variant: 0, file: "XV/35990.xmv".into(), start: 0, end: 0 }]);
    }
}
