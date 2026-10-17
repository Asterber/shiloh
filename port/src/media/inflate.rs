//! zlib / DEFLATE decompression (RFC 1950 / RFC 1951, public specs) — for QuickTime's compressed movie header
//! (`moov` → `cmov` → `dcom` 'zlib' + `cmvd`, H2210: XN/25949.XMV, the only such file of the game).

/// Bit reader, LSB first (RFC 1951 3.1.1).
struct Bits<'a> {
    b: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl Bits<'_> {
    fn need(&mut self, n: u32) -> Option<()> {
        while self.nbits < n {
            let byte = *self.b.get(self.pos)?;
            self.pos += 1;
            self.bit |= (byte as u32) << self.nbits;
            self.nbits += 8;
        }
        Some(())
    }
    fn get(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Some(v)
    }
    fn align(&mut self) {
        let r = self.nbits % 8;
        self.bit >>= r;
        self.nbits -= r;
    }
}

/// Canonical Huffman code: counts per length and the symbols in code order.
struct Huff {
    counts: [u16; 16],
    syms: Vec<u16>,
}

impl Huff {
    fn new(lens: &[u8]) -> Huff {
        let mut counts = [0u16; 16];
        for &l in lens {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut syms = vec![0u16; lens.len()];
        for (s, &l) in lens.iter().enumerate() {
            if l != 0 {
                syms[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Huff { counts, syms }
    }
    fn decode(&self, r: &mut Bits) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= r.get(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return self.syms.get((index + code - first) as usize).copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289,
    16385, 24577,
];
const DEXT: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn codes(r: &mut Bits, out: &mut Vec<u8>, lit: &Huff, dist: &Huff) -> Option<()> {
    loop {
        let s = lit.decode(r)? as usize;
        if s < 256 {
            out.push(s as u8);
        } else if s == 256 {
            return Some(());
        } else {
            let i = s - 257;
            let len = *LBASE.get(i)? as usize + r.get(*LEXT.get(i)? as u32)? as usize;
            let d = dist.decode(r)? as usize;
            let dd = *DBASE.get(d)? as usize + r.get(*DEXT.get(d)? as u32)? as usize;
            if dd > out.len() {
                return None;
            }
            let from = out.len() - dd;
            for k in 0..len {
                out.push(out[from + k]);
            }
        }
    }
}

/// Raw DEFLATE stream → bytes; None on a malformed stream.
pub fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut r = Bits { b: data, pos: 0, bit: 0, nbits: 0 };
    let mut out = Vec::new();
    loop {
        let last = r.get(1)?;
        match r.get(2)? {
            0 => {
                r.align();
                let len = r.get(16)? as usize;
                let nlen = r.get(16)? as usize;
                if len != !nlen & 0xffff || r.nbits != 0 {
                    return None;
                }
                out.extend_from_slice(r.b.get(r.pos..r.pos + len)?);
                r.pos += len;
            }
            1 => {
                let mut l = [0u8; 288];
                l[..144].fill(8);
                l[144..256].fill(9);
                l[256..280].fill(7);
                l[280..].fill(8);
                codes(&mut r, &mut out, &Huff::new(&l), &Huff::new(&[5u8; 30]))?;
            }
            2 => {
                let hlit = r.get(5)? as usize + 257;
                let hdist = r.get(5)? as usize + 1;
                let hclen = r.get(4)? as usize + 4;
                const ORD: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &o in ORD.iter().take(hclen) {
                    cl[o] = r.get(3)? as u8;
                }
                let ch = Huff::new(&cl);
                let mut lens = Vec::with_capacity(hlit + hdist);
                while lens.len() < hlit + hdist {
                    match ch.decode(&mut r)? {
                        s @ 0..=15 => lens.push(s as u8),
                        16 => {
                            let p = *lens.last()?;
                            for _ in 0..3 + r.get(2)? {
                                lens.push(p);
                            }
                        }
                        17 => lens.extend(std::iter::repeat(0).take(3 + r.get(3)? as usize)),
                        _ => lens.extend(std::iter::repeat(0).take(11 + r.get(7)? as usize)),
                    }
                }
                if lens.len() != hlit + hdist {
                    return None;
                }
                codes(&mut r, &mut out, &Huff::new(&lens[..hlit]), &Huff::new(&lens[hlit..]))?;
            }
            _ => return None,
        }
        if last == 1 {
            return Some(out);
        }
    }
}

/// zlib stream (2-byte header, DEFLATE, Adler-32) → bytes; None on a bad header, stream or checksum.
pub fn zlib(data: &[u8]) -> Option<Vec<u8>> {
    let (cmf, flg) = (*data.first()?, *data.get(1)?);
    if cmf & 0x0f != 8 || ((cmf as u16) << 8 | flg as u16) % 31 != 0 || flg & 0x20 != 0 {
        return None;
    }
    let out = inflate(&data[2..])?;
    if let Some(c) = data.len().checked_sub(4).and_then(|o| data.get(o..)) {
        let want = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        let (mut a, mut b) = (1u32, 0u32);
        for &x in &out {
            a = (a + x as u32) % 65521;
            b = (b + a) % 65521;
        }
        if (b << 16 | a) != want {
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_and_fixed_blocks() {
        // zlib.compress(b"hello hello hello", 9) and a stored block (level 0) of b"abc"
        let fixed = [0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00, 0x3a, 0x2e, 0x06, 0x7d];
        assert_eq!(zlib(&fixed).unwrap(), b"hello hello hello");
        let stored = [0x78, 0x01, 0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c', 0x02, 0x4d, 0x01, 0x27];
        assert_eq!(zlib(&stored).unwrap(), b"abc");
    }
}
