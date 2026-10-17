//! HYPOTHESIS H1280 (static decode — NOT traced): a movie's hotspot file `<movie dir>/<base name>.HOT`
//! (read by FUN_00445e30): the tag `HSPT`, a u32 count, then `count` records of 20 bytes,
//! all little-endian — u32 id, u16 left, top, right, bottom (movie pixels), u32 t0, u32 t1 (ms of movie time; the player
//! arms a callback at each). A movie without the file has no hotspots.

/// One record of a `.HOT` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotspot {
    /// what a click does (FUN_004b4470's switch, H1282): 221 / 222 / 223 = the Smolnikoff goons
    pub id: u32,
    /// left, top, right, bottom in movie pixels (the View movie is drawn at (20, 90))
    pub rect: [i16; 4],
    /// active window, ms of movie time
    pub t0: u32,
    pub t1: u32,
}

/// Parse a `.HOT` file; None if the tag is not `HSPT` or the file is shorter than its count says.
pub fn parse(b: &[u8]) -> Option<Vec<Hotspot>> {
    if b.get(..4)? != b"HSPT" {
        return None;
    }
    let u32_at = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
    let u16_at = |o: usize| b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as i16);
    // the original reads only the count's first byte; the other three are 0 in every file of the game
    let n = *b.get(4)? as usize;
    (0..n)
        .map(|i| {
            let o = 8 + 20 * i;
            Some(Hotspot {
                id: u32_at(o)?,
                rect: [u16_at(o + 4)?, u16_at(o + 6)?, u16_at(o + 8)?, u16_at(o + 10)?],
                t0: u32_at(o + 12)?,
                t1: u32_at(o + 16)?,
            })
        })
        .collect()
}

/// The `.HOT` file name of a movie file name (`XV/47749.xmv` → `XV/47749.HOT`).
pub fn hot_name(movie: &str) -> String {
    match movie.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.HOT"),
        None => format!("{movie}.HOT"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_record() {
        // XV/47749.HOT (0xf33c's goon movie): id 223, (406,105)–(447,152), 733..2133 ms
        let b = [
            0x48, 0x53, 0x50, 0x54, 1, 0, 0, 0, 0xdf, 0, 0, 0, 0x96, 1, 0x69, 0, 0xbf, 1, 0x98, 0, 0xdd, 2, 0, 0, 0x55,
            8, 0, 0,
        ];
        assert_eq!(parse(&b), Some(vec![Hotspot { id: 223, rect: [406, 105, 447, 152], t0: 733, t1: 2133 }]));
        assert_eq!(parse(&b[..27]), None);
        assert_eq!(parse(b"HSPX\0\0\0\0"), None);
        assert_eq!(parse(b"HSPT\0\0\0\0"), Some(vec![]));
        assert_eq!(hot_name("XV/47749.xmv"), "XV/47749.HOT");
    }
}
