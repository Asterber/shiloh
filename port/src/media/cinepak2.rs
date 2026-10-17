//! Cinepak ('cvid') decoder, RGB24 output.
//!
//! Clean-room implementation. Sources used: Dr. Tim Ferguson, "Cinepak (CVID) stream format for AVI and QT"
//! (2001, mirrored at <https://multimedia.cx/mirror/cinepak.txt>) and the MultimediaWiki page "Cinepak"
//! (<https://wiki.multimedia.cx/index.php/Cinepak>: relative strip coordinates, the inferred YUV→RGB matrix).
//! No decoder source code (ffmpeg or other) was consulted. The game plays .xmv through QuickTime (C9).
//!
//! Layout: frame header (10 bytes), then per strip a 12-byte header and a list of chunks `[u16 id][u16 size]`.
//! Codebook chunk ids are read as bit fields of `0x2X00`: X&1 selective update, X&2 V1 (else V4), X&4 8-bit
//! luma-only entries (else 12-bit Y0..Y3,U,V). Vector chunks: 0x3000 V1/V4 by flag bit, 0x3100 skip/V1/V4,
//! 0x3200 V1 only. Each strip slot keeps its own codebook pair across frames.

#[derive(Clone, Copy, Default)]
struct Vector {
    luma: [u8; 4],
    cb: i8,
    cr: i8,
}

type Book = [Vector; 256];

#[derive(Clone)]
struct Books {
    v1: Box<Book>,
    v4: Box<Book>,
}

impl Books {
    fn blank() -> Books {
        Books { v1: Box::new([Vector::default(); 256]), v4: Box::new([Vector::default(); 256]) }
    }
}

pub struct Cinepak {
    pub width: usize,
    pub height: usize,
    /// RGB24, top-down; persists between frames (inter frames update it)
    pub rgb: Vec<u8>,
    strips: Vec<Books>,
}

/// Big-endian reads that return 0 past the end of the buffer.
fn get(b: &[u8], at: usize, n: usize) -> u32 {
    let mut v = 0u32;
    for k in 0..n {
        v = v << 8 | *b.get(at + k).unwrap_or(&0) as u32;
    }
    v
}

/// MSB-first reader of 32-bit flag words embedded in a byte stream.
struct Flags {
    word: u32,
    left: u32,
}

impl Flags {
    fn new() -> Flags {
        Flags { word: 0, left: 0 }
    }
    /// Next flag bit, refilling from `data[*pos..]` when the current word is used up.
    fn bit(&mut self, data: &[u8], pos: &mut usize) -> Option<bool> {
        if self.left == 0 {
            if *pos + 4 > data.len() {
                return None;
            }
            self.word = get(data, *pos, 4);
            *pos += 4;
            self.left = 32;
        }
        self.left -= 1;
        Some(self.word >> self.left & 1 == 1)
    }
}

fn clamp(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// Cinepak colour: R = Y + 2V, G = Y − U/2 − V, B = Y + 2U (U, V signed).
fn to_rgb(y: u8, u: i8, v: i8) -> [u8; 3] {
    let (y, u, v) = (y as i32, u as i32, v as i32);
    [clamp(y + 2 * v), clamp(y - u / 2 - v), clamp(y + 2 * u)]
}

impl Cinepak {
    pub fn new(width: usize, height: usize) -> Cinepak {
        Cinepak { width, height, rgb: vec![0; width * height * 3], strips: Vec::new() }
    }

    fn plot(&mut self, x: usize, y: usize, c: [u8; 3]) {
        if x < self.width && y < self.height {
            let o = (y * self.width + x) * 3;
            self.rgb[o..o + 3].copy_from_slice(&c);
        }
    }

    /// Paint the 4×4 block at (x, y) from one V1 vector (each luma value covers a 2×2 square).
    fn block_v1(&mut self, x: usize, y: usize, e: Vector) {
        for dy in 0..4 {
            for dx in 0..4 {
                let l = e.luma[(dy / 2) * 2 + dx / 2];
                self.plot(x + dx, y + dy, to_rgb(l, e.cb, e.cr));
            }
        }
    }

    /// Paint the 4×4 block at (x, y) from four V4 vectors, one per 2×2 quadrant.
    fn block_v4(&mut self, x: usize, y: usize, q: [Vector; 4]) {
        for (k, e) in q.iter().enumerate() {
            let (qx, qy) = (x + (k & 1) * 2, y + (k >> 1) * 2);
            for p in 0..4 {
                self.plot(qx + (p & 1), qy + (p >> 1), to_rgb(e.luma[p], e.cb, e.cr));
            }
        }
    }

    /// Decode one frame into `self.rgb`.
    pub fn decode(&mut self, f: &[u8]) -> Result<(), String> {
        if f.len() < 10 {
            return Err(format!("cvid: frame of {} bytes", f.len()));
        }
        let flags = f[0];
        let declared = get(f, 1, 3) as usize;
        let frame = &f[..declared.clamp(10, f.len())];
        let nstrips = get(frame, 8, 2) as usize;
        let mut pos = 10usize;
        let mut prev_bottom = 0usize;
        for s in 0..nstrips {
            if pos + 12 > frame.len() {
                break;
            }
            let size = get(frame, pos + 2, 2) as usize;
            let mut top = get(frame, pos + 4, 2) as usize;
            let left = get(frame, pos + 6, 2) as usize;
            let mut bottom = get(frame, pos + 8, 2) as usize;
            let right = get(frame, pos + 10, 2) as usize;
            if top == 0 && s > 0 {
                // relative form: the stored bottom is the strip height
                top = prev_bottom;
                bottom += top;
            }
            prev_bottom = bottom;
            let end = (pos + size.max(12)).min(frame.len());
            if self.strips.len() <= s {
                self.strips.push(Books::blank());
            }
            if s > 0 && flags & 1 == 0 {
                self.strips[s] = self.strips[s - 1].clone();
            }
            self.strip(s, &frame[pos + 12..end], (left, top, right, bottom))?;
            pos = end;
        }
        Ok(())
    }

    fn strip(&mut self, s: usize, mut data: &[u8], area: (usize, usize, usize, usize)) -> Result<(), String> {
        while data.len() >= 4 {
            let id = get(data, 0, 2);
            let size = (get(data, 2, 2) as usize).clamp(4, data.len());
            let body = &data[4..size];
            match id >> 8 {
                0x20..=0x27 => self.codebook(s, (id >> 8) as u8 & 7, body),
                0x30 => self.vectors(s, body, area, false, true),
                0x31 => self.vectors(s, body, area, true, true),
                0x32 => self.vectors(s, body, area, false, false),
                _ => {}
            }
            data = &data[size..];
        }
        Ok(())
    }

    /// Load (or selectively update) a codebook. `kind` bits: 1 selective, 2 V1, 4 luma-only.
    fn codebook(&mut self, s: usize, kind: u8, body: &[u8]) {
        let entry_len = if kind & 4 != 0 { 4 } else { 6 };
        let selective = kind & 1 != 0;
        let books = &mut self.strips[s];
        let book = if kind & 2 != 0 { &mut books.v1 } else { &mut books.v4 };
        let mut pos = 0usize;
        let mut flags = Flags::new();
        for slot in book.iter_mut() {
            if selective {
                match flags.bit(body, &mut pos) {
                    Some(true) => {}
                    Some(false) => continue,
                    None => break,
                }
            }
            if pos + entry_len > body.len() {
                break;
            }
            let e = &body[pos..pos + entry_len];
            *slot = Vector {
                luma: [e[0], e[1], e[2], e[3]],
                cb: if entry_len == 6 { e[4] as i8 } else { 0 },
                cr: if entry_len == 6 { e[5] as i8 } else { 0 },
            };
            pos += entry_len;
        }
    }

    /// Walk the strip's blocks. `skippable`: a leading flag bit selects skip/code; `choose`: a flag bit selects
    /// V4 (1) or V1 (0), otherwise every block is V1.
    fn vectors(&mut self, s: usize, body: &[u8], area: (usize, usize, usize, usize), skippable: bool, choose: bool) {
        let (left, top, right, bottom) = area;
        let mut pos = 0usize;
        let mut flags = Flags::new();
        let mut y = top;
        while y < bottom {
            let mut x = left;
            while x < right {
                if skippable {
                    match flags.bit(body, &mut pos) {
                        Some(true) => {}
                        Some(false) => {
                            x += 4;
                            continue;
                        }
                        None => return,
                    }
                }
                let v4 = if choose {
                    match flags.bit(body, &mut pos) {
                        Some(b) => b,
                        None => return,
                    }
                } else {
                    false
                };
                if v4 {
                    if pos + 4 > body.len() {
                        return;
                    }
                    let b = &self.strips[s].v4;
                    let q = [
                        b[body[pos] as usize],
                        b[body[pos + 1] as usize],
                        b[body[pos + 2] as usize],
                        b[body[pos + 3] as usize],
                    ];
                    pos += 4;
                    self.block_v4(x, y, q);
                } else {
                    let Some(&i) = body.get(pos) else { return };
                    pos += 1;
                    let e = self.strips[s].v1[i as usize];
                    self.block_v1(x, y, e);
                }
                x += 4;
            }
            y += 4;
        }
    }
}
