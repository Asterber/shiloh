//! QuickTime 'rpza' (Apple Video, "Road Pizza") decoder, RGB555 → RGB24.
//!
//! Clean-room implementation. Sources used: the MultimediaWiki page "Apple RPZA"
//! (<https://wiki.multimedia.cx/index.php/Apple_RPZA>, itself based on Roberto Togni's
//! "Description of the Apple Video (rpza) Codec"). No decoder source code (ffmpeg or other) was consulted.
//!
//! Format summary: a frame is `[0xe1][u24 length]` followed by opcodes that walk the 4×4 blocks left→right,
//! top→bottom. Upper three bits of an opcode byte select the operation, the low five give `count - 1`:
//! `100` skip (block keeps the previous frame), `101` one colour, `110` a colour pair + 2-bit index maps.
//! A byte with bit 7 clear is the high half of a colour; the following byte decides between a one-block
//! colour-pair block (its bit 7 set) and a block of 16 literal colours.
//! In the FO data: the dialogue EmotionIcon movies XG/27698, 28647, 28649.xmv (80×60).

pub struct Rpza {
    pub width: usize,
    pub height: usize,
    /// the frame as RGB555 words, `cols*4` wide, `rows*4` high
    canvas: Vec<u16>,
    cols: usize,
    rows: usize,
    pub rgb: Vec<u8>,
}

/// Big-endian byte cursor over one frame; reads past the end yield `None`.
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn byte(&mut self) -> Option<u8> {
        let v = *self.data.get(self.at)?;
        self.at += 1;
        Some(v)
    }
    fn word(&mut self) -> Option<u16> {
        let hi = self.byte()?;
        let lo = self.byte()?;
        Some(u16::from_be_bytes([hi, lo]))
    }
    fn peek(&self) -> Option<u8> {
        self.data.get(self.at).copied()
    }
    fn more(&self) -> bool {
        self.at < self.data.len()
    }
}

/// Mix two 5-bit-per-channel colours channel by channel: `(wa*a + wb*b) / 32`.
fn mix(a: u16, b: u16, wa: u16, wb: u16) -> u16 {
    let mut out = 0;
    for shift in [0u16, 5, 10] {
        let ca = (a >> shift) & 31;
        let cb = (b >> shift) & 31;
        out |= ((wa * ca + wb * cb) >> 5) << shift;
    }
    out
}

/// The 4-entry palette of a colour-pair block, indexed by the 2-bit pixel codes.
fn quad(a: u16, b: u16) -> [u16; 4] {
    [b, mix(a, b, 11, 21), mix(a, b, 21, 11), a]
}

impl Rpza {
    pub fn new(width: usize, height: usize) -> Rpza {
        let cols = width.div_ceil(4);
        let rows = height.div_ceil(4);
        Rpza { width, height, canvas: vec![0; cols * 4 * rows * 4], cols, rows, rgb: vec![0; width * height * 3] }
    }

    /// The current frame as RGB555 (bit 15 clear), `width × height`.
    pub fn rgb555(&self) -> Vec<u16> {
        let stride = self.cols * 4;
        let mut out = Vec::with_capacity(self.width * self.height);
        for y in 0..self.height {
            out.extend_from_slice(&self.canvas[y * stride..y * stride + self.width]);
        }
        out
    }

    /// Write pixel `i` (0..16, raster order inside the block) of block number `blk`.
    fn put(&mut self, blk: usize, i: usize, c: u16) {
        let stride = self.cols * 4;
        let (bx, by) = (blk % self.cols, blk / self.cols);
        let (x, y) = (bx * 4 + (i & 3), by * 4 + (i >> 2));
        self.canvas[y * stride + x] = c & 0x7fff;
    }

    fn paint_indexed(&mut self, blk: usize, pal: &[u16; 4], rows: [u8; 4]) {
        for (r, bits) in rows.iter().enumerate() {
            for col in 0..4 {
                let code = (bits >> (6 - 2 * col)) & 3;
                self.put(blk, r * 4 + col, pal[code as usize]);
            }
        }
    }

    /// Decode one frame (`b` = one sample) into `self.rgb`.
    pub fn decode(&mut self, b: &[u8]) -> Result<(), String> {
        if b.len() < 4 {
            return Err(format!("rpza: frame of {} bytes", b.len()));
        }
        let declared = (b[1] as usize) << 16 | (b[2] as usize) << 8 | b[3] as usize;
        let end = declared.min(b.len());
        let mut cur = Cursor { data: &b[..end.max(4)], at: 4 };
        let total = self.cols * self.rows;
        let mut blk = 0usize;
        while blk < total && cur.more() {
            let op = cur.byte().unwrap();
            if op & 0x80 == 0 {
                // a colour word starts here (op is its high byte)
                let lo = cur.byte().ok_or("rpza: truncated colour")?;
                let a = u16::from_be_bytes([op, lo]);
                if cur.peek().ok_or("rpza: truncated block")? & 0x80 != 0 {
                    let bcol = cur.word().ok_or("rpza: truncated colour")?;
                    let pal = quad(a, bcol);
                    let rows = self.four(&mut cur)?;
                    self.paint_indexed(blk, &pal, rows);
                } else {
                    self.put(blk, 0, a);
                    for i in 1..16 {
                        let c = cur.word().ok_or("rpza: truncated 16-colour block")?;
                        self.put(blk, i, c);
                    }
                }
                blk += 1;
                continue;
            }
            let n = (op & 0x1f) as usize + 1;
            match op >> 5 {
                0b100 => blk += n,
                0b101 => {
                    let c = cur.word().ok_or("rpza: truncated fill")?;
                    for _ in 0..n {
                        if blk >= total {
                            break;
                        }
                        for i in 0..16 {
                            self.put(blk, i, c);
                        }
                        blk += 1;
                    }
                }
                0b110 => {
                    let a = cur.word().ok_or("rpza: truncated colour")?;
                    let bcol = cur.word().ok_or("rpza: truncated colour")?;
                    let pal = quad(a, bcol);
                    for _ in 0..n {
                        if blk >= total {
                            break;
                        }
                        let rows = self.four(&mut cur)?;
                        self.paint_indexed(blk, &pal, rows);
                        blk += 1;
                    }
                }
                _ => return Err(format!("rpza: unknown opcode {op:#04x}")),
            }
        }
        self.to_rgb();
        Ok(())
    }

    fn four(&self, cur: &mut Cursor) -> Result<[u8; 4], String> {
        let mut r = [0u8; 4];
        for v in r.iter_mut() {
            *v = cur.byte().ok_or("rpza: truncated index map")?;
        }
        Ok(r)
    }

    fn to_rgb(&mut self) {
        let stride = self.cols * 4;
        let expand = |c: u16| -> u8 {
            let c = (c & 31) as u8;
            c << 3 | c >> 2
        };
        for y in 0..self.height {
            for x in 0..self.width {
                let p = self.canvas[y * stride + x];
                let o = (y * self.width + x) * 3;
                self.rgb[o] = expand(p >> 10);
                self.rgb[o + 1] = expand(p >> 5);
                self.rgb[o + 2] = expand(p);
            }
        }
    }
}
